//! I caratteri dei documenti d'ufficio: quelli scritti nel file (Calibri, Arial, Times New Roman…) presi
//! dal sistema, con i sostituti a misure uguali quando mancano (Carlito per Calibri, Liberation per
//! Arial e Times) e, in fondo, il Titillium che Cedro porta con sé. Grassetto e corsivo che il sistema
//! non ha si disegnano finti (tratto più spesso, testo inclinato), come fanno Word e LibreOffice.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

use skrifa::instance::{LocationRef, Size};
use skrifa::raw::types::Tag;
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use resvg::usvg::fontdb;

static REGULAR: &[u8] = include_bytes!("../../assets/fonts/TitilliumWeb-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../../assets/fonts/TitilliumWeb-Bold.ttf");

pub type FontId = usize;

/// Un carattere pronto per misurare e per finire nel PDF.
pub struct Face {
    data: Arc<Vec<u8>>,
    index: u32,
    /// Il nome PostScript, senza spazi: quello che il PDF scrive.
    pub name: String,
    /// La famiglia trovata (per i PDF senza caratteri incorporati).
    pub family: String,
    upem: f32,
    /// Misure verticali in millesimi di em: sopra la riga, sotto la riga (positivo), interlinea in più.
    pub ascent: f32,
    pub descent: f32,
    pub gap: f32,
    /// L'interlinea in più che Word aggiunge alla riga (external leading di Windows).
    pub leading: f32,
    pub cap_height: f32,
    pub bbox: [f32; 4],
    pub italic_angle: f32,
    /// Grassetto e corsivo disegnati finti: il sistema non ha la versione vera.
    pub synth_bold: bool,
    pub synth_italic: bool,
    pub serif: bool,
    pub fixed: bool,
    /// Lo stile chiesto (vero o finto).
    pub bold: bool,
    pub italic: bool,
    /// carattere → (glifo, larghezza in millesimi di em); `None` se il font non ha il carattere.
    cache: RefCell<HashMap<char, Option<(u16, f32)>>>,
}

impl Face {
    fn load(data: Arc<Vec<u8>>, index: u32, family: &str) -> Option<Face> {
        let font = FontRef::from_index(&data, index).ok()?;
        // Solo contorni TrueType: i CFF non entrano nel PDF/A-1 come CID TrueType.
        font.table_data(Tag::new(b"glyf"))?;
        let upem = font.head().ok()?.units_per_em() as f32;
        let k = 1000.0 / upem;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        // Word e LibreOffice spaziano le righe con le misure «win» dell'OS/2 (Calibri: 1,22 em).
        let (ascent, descent, gap) = match font.os2() {
            Ok(os2) if os2.us_win_ascent() > 0 => (os2.us_win_ascent() as f32 * k, os2.us_win_descent() as f32 * k, 0.0),
            _ => (metrics.ascent * k, -metrics.descent * k, metrics.leading * k),
        };
        // Come GDI: lineGap dell'hhea meno quello che le misure «win» hanno già in più.
        let leading = match (font.os2(), font.hhea()) {
            (Ok(os2), Ok(hh)) if os2.us_win_ascent() > 0 => {
                let win = os2.us_win_ascent() as f32 + os2.us_win_descent() as f32;
                let hhea = hh.ascender().to_i16() as f32 - hh.descender().to_i16() as f32;
                (hh.line_gap().to_i16() as f32 - (win - hhea)).max(0.0) * k
            }
            _ => 0.0,
        };
        let cap_height = metrics.cap_height.map_or(ascent * 0.7, |c| c * k);
        let bbox = metrics.bounds.map_or([-200.0, -300.0, 1200.0, 1000.0], |b| [b.x_min * k, b.y_min * k, b.x_max * k, b.y_max * k]);
        let italic_angle = font.post().map(|p| p.italic_angle().to_f64() as f32).unwrap_or(0.0);
        let name = font
            .localized_strings(skrifa::string::StringId::POSTSCRIPT_NAME)
            .english_or_first()
            .map(|s| s.chars().filter(|c| c.is_ascii_graphic() && !"()<>[]{}/%#".contains(*c)).collect::<String>())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| family.chars().filter(|c| c.is_ascii_alphanumeric()).collect());
        let fixed = font.post().map(|p| p.is_fixed_pitch() != 0).unwrap_or(false);
        let serif = font.os2().map(|o| matches!(o.s_family_class() >> 8, 1..=5 | 7)).unwrap_or(false);
        Some(Face {
            data,
            index,
            name,
            family: family.to_string(),
            upem,
            ascent,
            descent,
            gap,
            leading,
            cap_height,
            bbox,
            italic_angle,
            synth_bold: false,
            synth_italic: false,
            serif,
            fixed,
            bold: false,
            italic: false,
            cache: RefCell::new(HashMap::new()),
        })
    }

    fn font(&self) -> FontRef<'_> {
        FontRef::from_index(&self.data, self.index).expect("font già letto")
    }

    /// Il glifo e la larghezza (millesimi di em) di un carattere, se il font lo ha.
    pub fn glyph(&self, c: char) -> Option<(u16, f32)> {
        if let Some(v) = self.cache.borrow().get(&c) {
            return *v;
        }
        let font = self.font();
        let found = font.charmap().map(c).filter(|g| g.to_u32() != 0).map(|g| {
            let w = font.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(g).unwrap_or(0.0);
            (g.to_u32() as u16, w * 1000.0 / self.upem)
        });
        self.cache.borrow_mut().insert(c, found);
        found
    }

    pub fn has(&self, c: char) -> bool {
        self.glyph(c).is_some()
    }

    /// La larghezza di un glifo (millesimi di em).
    pub fn glyph_width(&self, gid: u16) -> f32 {
        self.font().glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(GlyphId::new(gid as u32)).unwrap_or(0.0) * 1000.0 / self.upem
    }

    /// Larghezza in punti di un testo (i caratteri che il font non ha valgono come il quadratino).
    pub fn width(&self, s: &str, size: f32) -> f32 {
        let notdef = || self.glyph_width(0);
        s.chars().map(|c| self.glyph(c).map_or_else(notdef, |g| g.1)).sum::<f32>() * size / 1000.0
    }

    /// Altezza della riga «singola» in punti.
    pub fn line(&self, size: f32) -> f32 {
        (self.ascent + self.descent + self.gap) * size / 1000.0
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn index(&self) -> u32 {
        self.index
    }
}

/// I caratteri del sistema, cercati una volta sola (gli stessi dell'SVG).
pub fn system() -> Arc<fontdb::Database> {
    crate::imgfmt::system_fonts()
}

/// famiglia in minuscolo → facce.
fn families() -> &'static HashMap<String, Vec<fontdb::ID>> {
    static INDEX: OnceLock<HashMap<String, Vec<fontdb::ID>>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let db = system();
        let mut map: HashMap<String, Vec<fontdb::ID>> = HashMap::new();
        for f in db.faces() {
            for (name, _) in &f.families {
                map.entry(name.to_lowercase()).or_default().push(f.id);
            }
        }
        map
    })
}

/// I sostituti con le stesse misure (o quasi) dei caratteri di Office, nell'ordine in cui si cercano.
fn substitutes(family: &str) -> &'static [&'static str] {
    match family {
        "calibri" | "calibri light" => &["Carlito", "Liberation Sans", "Arimo", "DejaVu Sans"],
        "cambria" | "cambria math" => &["Caladea", "Liberation Serif", "Tinos", "DejaVu Serif"],
        "arial" | "helvetica" | "arial unicode ms" | "helvetica neue" => &["Liberation Sans", "Arimo", "Nimbus Sans", "DejaVu Sans"],
        "arial narrow" => &["Liberation Sans Narrow", "Nimbus Sans Narrow", "Arial", "Liberation Sans"],
        "times new roman" | "times" => &["Liberation Serif", "Tinos", "Nimbus Roman", "DejaVu Serif"],
        "courier new" | "courier" => &["Liberation Mono", "Cousine", "Nimbus Mono PS", "DejaVu Sans Mono"],
        "georgia" => &["Gelasio", "DejaVu Serif", "Liberation Serif"],
        "verdana" | "tahoma" => &["DejaVu Sans", "Liberation Sans"],
        "segoe ui" | "segoe ui light" | "segoe ui semibold" => &["Selawik", "Open Sans", "Noto Sans", "Liberation Sans"],
        "aptos" | "aptos narrow" | "aptos display" => &["Calibri", "Carlito", "Liberation Sans", "DejaVu Sans"],
        "garamond" | "book antiqua" | "palatino linotype" | "palatino" => &["EB Garamond", "TeX Gyre Pagella", "Liberation Serif", "DejaVu Serif"],
        "century gothic" => &["TeX Gyre Adventor", "URW Gothic", "Liberation Sans"],
        "consolas" | "lucida console" => &["Inconsolata", "Liberation Mono", "DejaVu Sans Mono"],
        "trebuchet ms" | "gill sans mt" | "franklin gothic book" => &["DejaVu Sans", "Liberation Sans"],
        _ => &[],
    }
}

/// Caratteri a cui chiedere i simboli che il font del testo non ha.
const FALLBACK: &[&str] = &["Segoe UI Symbol", "Arial", "Liberation Sans", "DejaVu Sans", "Cambria Math", "Noto Sans", "Noto Sans Symbols2", "Noto Sans Math", "Microsoft YaHei", "Malgun Gothic", "Yu Gothic", "MS Gothic", "Noto Sans CJK SC", "Droid Sans Fallback"];

/// Che genere è un nome di carattere che non si trova: con le grazie, a passo fisso o bastoni.
fn generic(family: &str) -> &'static [&'static str] {
    let f = family.to_lowercase();
    if f.contains("mono") || f.contains("courier") || f.contains("consol") || f.contains("code") || f.contains("typewriter") {
        &["Courier New", "Liberation Mono", "Cousine", "DejaVu Sans Mono"]
    } else if f.contains("serif") && !f.contains("sans") || f.contains("times") || f.contains("roman") || f.contains("garamond") || f.contains("book") || f.contains("cambria") || f.contains("georgia") || f.contains("bodoni") || f.contains("palatino") {
        &["Times New Roman", "Liberation Serif", "Tinos", "DejaVu Serif"]
    } else {
        &["Arial", "Liberation Sans", "Arimo", "DejaVu Sans"]
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    family: String,
    bold: bool,
    italic: bool,
}

/// L'insieme dei caratteri usati da un documento.
pub struct Fonti {
    pub faces: Vec<Face>,
    by_key: HashMap<Key, FontId>,
    /// Le facce già aperte, per file e indice: lo stesso Calibri non si legge due volte.
    loaded: HashMap<(fontdb::ID, bool, bool), FontId>,
    fallback: Vec<FontId>,
    fallback_ready: bool,
    /// Solo il Titillium di Cedro, niente caratteri del sistema (per le prove: risultati uguali ovunque).
    builtin_only: bool,
}

impl Default for Fonti {
    fn default() -> Self {
        Fonti::new()
    }
}

impl Fonti {
    pub fn new() -> Fonti {
        Fonti { faces: Vec::new(), by_key: HashMap::new(), loaded: HashMap::new(), fallback: Vec::new(), fallback_ready: false, builtin_only: false }
    }

    /// Solo i caratteri di Cedro: le prove danno lo stesso risultato su ogni PC.
    pub fn builtin() -> Fonti {
        Fonti { builtin_only: true, ..Fonti::new() }
    }

    pub fn face(&self, id: FontId) -> &Face {
        &self.faces[id]
    }

    /// Il carattere per una famiglia e uno stile; mai un errore: in fondo c'è il Titillium.
    pub fn get(&mut self, family: &str, bold: bool, italic: bool) -> FontId {
        let key = Key { family: family.trim().to_lowercase(), bold, italic };
        if let Some(id) = self.by_key.get(&key) {
            return *id;
        }
        let id = self.find(&key.family, bold, italic).unwrap_or_else(|| self.builtin_face(bold, italic));
        // Caladea ha le larghezze di Cambria ma righe più alte: le misure verticali sono quelle di Cambria.
        if key.family == "cambria" && self.faces[id].family.starts_with("Caladea") {
            let f = &mut self.faces[id];
            (f.ascent, f.descent, f.gap, f.leading) = (950.0, 222.0, 0.0, 0.0);
        }
        self.by_key.insert(key, id);
        id
    }

    fn find(&mut self, family: &str, bold: bool, italic: bool) -> Option<FontId> {
        if self.builtin_only {
            return None;
        }
        if let Some(id) = self.from_system(family, bold, italic) {
            return Some(id);
        }
        for s in substitutes(family).iter().chain(generic(family)) {
            if let Some(id) = self.from_system(&s.to_lowercase(), bold, italic) {
                return Some(id);
            }
        }
        None
    }

    fn from_system(&mut self, family: &str, bold: bool, italic: bool) -> Option<FontId> {
        let ids = families().get(family)?;
        let db = system();
        // La faccia più vicina: peso giusto, poi stile giusto, poi larghezza normale.
        let want = if bold { 700i32 } else { 400 };
        let mut best: Vec<&fontdb::FaceInfo> = ids.iter().filter_map(|id| db.face(*id)).collect();
        best.sort_by_key(|f| {
            let italic_face = f.style != fontdb::Style::Normal;
            ((f.weight.0 as i32 - want).abs() / 50, (italic_face != italic) as i32, (f.stretch != fontdb::Stretch::Normal) as i32)
        });
        for info in best {
            let italic_face = info.style != fontdb::Style::Normal;
            let synth_bold = bold && info.weight.0 < 600;
            let synth_italic = italic && !italic_face;
            if let Some(id) = self.loaded.get(&(info.id, synth_bold, synth_italic)) {
                return Some(*id);
            }
            let Some(data) = db.with_face_data(info.id, |d, _| d.to_vec()) else { continue };
            let family_name = info.families.first().map(|f| f.0.clone()).unwrap_or_default();
            let Some(mut face) = Face::load(Arc::new(data), info.index, &family_name) else { continue };
            face.synth_bold = synth_bold;
            face.synth_italic = synth_italic;
            face.bold = bold;
            face.italic = italic;
            self.faces.push(face);
            let id = self.faces.len() - 1;
            self.loaded.insert((info.id, synth_bold, synth_italic), id);
            return Some(id);
        }
        None
    }

    fn builtin_face(&mut self, bold: bool, italic: bool) -> FontId {
        let key = Key { family: "\u{0}titillium".into(), bold, italic };
        if let Some(id) = self.by_key.get(&key) {
            return *id;
        }
        let data = if bold { BOLD } else { REGULAR };
        let mut face = Face::load(Arc::new(data.to_vec()), 0, "Titillium Web").expect("Titillium incluso");
        face.synth_italic = italic;
        face.bold = bold;
        face.italic = italic;
        self.faces.push(face);
        let id = self.faces.len() - 1;
        self.by_key.insert(key, id);
        id
    }

    fn prepare_fallback(&mut self) {
        if self.fallback_ready {
            return;
        }
        self.fallback_ready = true;
        if !self.builtin_only {
            for name in FALLBACK {
                if let Some(id) = self.from_system(&name.to_lowercase(), false, false) {
                    self.fallback.push(id);
                }
            }
        }
        let t = self.builtin_face(false, false);
        self.fallback.push(t);
    }

    /// Spezza un testo nei pezzi che ogni carattere sa disegnare: i simboli che `id` non ha passano a un
    /// altro font che li ha (con lo stesso grassetto e corsivo, finti se serve).
    pub fn runs(&mut self, id: FontId, text: &str) -> Vec<(FontId, String)> {
        let mut out: Vec<(FontId, String)> = Vec::new();
        for c in text.chars() {
            let f = if c.is_whitespace() || self.faces[id].has(c) { id } else { self.cover(id, c) };
            match out.last_mut() {
                Some((last, s)) if *last == f => s.push(c),
                _ => out.push((f, c.to_string())),
            }
        }
        out
    }

    fn cover(&mut self, id: FontId, c: char) -> FontId {
        self.prepare_fallback();
        let (bold, italic) = (self.faces[id].bold, self.faces[id].italic);
        for i in 0..self.fallback.len() {
            let f = self.fallback[i];
            if self.faces[f].has(c) {
                if !bold && !italic {
                    return f;
                }
                // Lo stesso font di riserva, con lo stile del testo intorno.
                let family = self.faces[f].family.to_lowercase();
                let styled = self.get(&family, bold, italic);
                return if self.faces[styled].has(c) { styled } else { f };
            }
        }
        id
    }

    /// Larghezza in punti di un testo, con i simboli presi dai font di riserva.
    pub fn width(&mut self, id: FontId, text: &str, size: f32) -> f32 {
        if text.chars().all(|c| c.is_whitespace() || self.faces[id].has(c)) {
            return self.faces[id].width(text, size);
        }
        self.runs(id, text).iter().map(|(f, s)| self.faces[*f].width(s, size)).sum()
    }
}

/// Glifi usati da un font del PDF, con il carattere che rappresentano (per copiare il testo).
#[derive(Default)]
pub struct Used {
    pub glyphs: BTreeMap<u16, char>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_fonts_measure_and_fall_back() {
        let mut f = Fonti::builtin();
        let a = f.get("Calibri", false, false);
        let b = f.get("Calibri", true, false);
        let i = f.get("Calibri", false, true);
        assert_ne!(a, b);
        assert!(f.face(i).synth_italic && !f.face(a).synth_italic);
        let w = f.width(a, "Comune", 11.0);
        assert!(w > 20.0 && w < 60.0, "{w}");
        assert!(f.width(b, "Comune", 11.0) > w);
        assert!(f.face(a).line(11.0) > 11.0);
        // Lo stesso carattere richiesto due volte è lo stesso.
        assert_eq!(f.get("calibri ", false, false), a);
        let runs = f.runs(a, "ab");
        assert_eq!(runs, vec![(a, "ab".to_string())]);
    }

    #[test]
    fn office_names_find_their_substitutes() {
        assert!(substitutes("calibri").contains(&"Carlito"));
        assert!(generic("Courier Prime").contains(&"Courier New"));
        assert!(generic("Book Antiqua X").contains(&"Times New Roman"));
        assert!(generic("Segoe Fancy").contains(&"Arial"));
    }
}
