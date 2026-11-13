//! Il foglio di calcolo com'è nel file, per stamparlo: valori e testi, caratteri, sfondi, bordi,
//! allineamenti, larghezze delle colonne e altezze delle righe, celle unite, righe e colonne nascoste,
//! area di stampa, titoli da ripetere, orientamento, adattamento, margini, intestazione e piè di pagina.
//! Qui il modello e la lettura degli .xlsx/.xlsm; Calc (.ods) e il vecchio .xls sono in `ods.rs` e `xls.rs`.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};

use super::pagina::Rgb;

/// Righe e colonne al massimo: un foglio con un milione di righe si stamperebbe in migliaia di pagine.
pub const MAX_ROWS: u32 = 200_000;
pub const MAX_COLS: u32 = 2_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Valore {
    Vuoto,
    Numero(f64),
    Testo(String),
    Bool(bool),
    Errore(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cella {
    pub valore: Valore,
    pub stile: u32,
    /// Il testo già formattato dal programma (Calc lo scrive nel file): vale più del formato.
    pub mostra: Option<String>,
    /// Un collegamento della cella.
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterAcross,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VAlign {
    Top,
    Center,
    #[default]
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bordo {
    pub width: f32,
    pub color: Rgb,
    pub dash: super::pagina::Dash,
    pub double: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Carattere {
    pub nome: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Rgb,
}

impl Default for Carattere {
    fn default() -> Self {
        Carattere { nome: "Calibri".into(), size: 11.0, bold: false, italic: false, underline: false, strike: false, color: (0, 0, 0) }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Stile {
    pub font: Carattere,
    pub fill: Option<Rgb>,
    /// sinistra, destra, sopra, sotto
    pub bordi: [Option<Bordo>; 4],
    pub h: HAlign,
    pub v: VAlign,
    pub wrap: bool,
    pub indent: u8,
    pub shrink: bool,
    /// Gradi: 90 in su, -90 in giù; 255 testo in colonna.
    pub rotation: i16,
    pub numfmt: String,
}

/// Un'area: prima riga, prima colonna, ultima riga, ultima colonna (comprese, da 0).
pub type Area = (u32, u32, u32, u32);

#[derive(Clone, Debug)]
pub struct Stampa {
    pub area: Option<Area>,
    pub titoli_righe: Option<(u32, u32)>,
    pub titoli_colonne: Option<(u32, u32)>,
    /// `Some(true)` orizzontale, `Some(false)` verticale, `None` non detto.
    pub orizzontale: Option<bool>,
    /// Larghezza e altezza della carta in punti (verticale).
    pub carta: Option<(f32, f32)>,
    /// Percentuale.
    pub scala: u32,
    /// Adatta a N pagine in larghezza e M in altezza (0 = quante servono).
    pub adatta: Option<(u32, u32)>,
    /// sinistra, destra, sopra, sotto, intestazione, piè (punti).
    pub margini: [f32; 6],
    pub centra_o: bool,
    pub centra_v: bool,
    pub griglia: bool,
    pub intestazioni: bool,
    pub testa: String,
    pub piede: String,
    pub salti_righe: Vec<u32>,
    pub salti_colonne: Vec<u32>,
    /// Prima in giù e poi a destra (come Excel), o il contrario.
    pub giu_poi_destra: bool,
    pub primo_numero: Option<u32>,
}

impl Default for Stampa {
    fn default() -> Self {
        // I margini «Normali» di Excel: 0,7" ai lati, 0,75" sopra e sotto, 0,3" per intestazione e piè.
        Stampa {
            area: None,
            titoli_righe: None,
            titoli_colonne: None,
            orizzontale: None,
            carta: None,
            scala: 100,
            adatta: None,
            margini: [50.4, 50.4, 54.0, 54.0, 21.6, 21.6],
            centra_o: false,
            centra_v: false,
            griglia: false,
            intestazioni: false,
            testa: String::new(),
            piede: String::new(),
            salti_righe: Vec::new(),
            salti_colonne: Vec::new(),
            giu_poi_destra: true,
            primo_numero: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Foglio {
    pub nome: String,
    pub nascosto: bool,
    pub colore: Option<Rgb>,
    pub celle: BTreeMap<(u32, u32), Cella>,
    /// Larghezza in punti e nascosta, per colonna.
    pub colonne: HashMap<u32, (Option<f32>, bool, u32)>,
    pub col_default: f32,
    /// Altezza in punti e nascosta, per riga.
    pub righe: HashMap<u32, (Option<f32>, bool, Option<u32>)>,
    pub riga_default: f32,
    pub unioni: Vec<Area>,
    pub stampa: Stampa,
    /// Le immagini del foglio: dove stanno (riga, colonna, scostamento in punti) e quanto sono grandi.
    pub immagini: Vec<(u32, u32, f32, f32, f32, f32, crate::pdfdoc::DocImage)>,
}

impl Foglio {
    pub fn new(nome: &str) -> Foglio {
        Foglio {
            nome: nome.to_string(),
            nascosto: false,
            colore: None,
            celle: BTreeMap::new(),
            colonne: HashMap::new(),
            col_default: 48.0,
            righe: HashMap::new(),
            riga_default: 15.0,
            unioni: Vec::new(),
            stampa: Stampa::default(),
            immagini: Vec::new(),
        }
    }

    pub fn col_width(&self, c: u32) -> f32 {
        match self.colonne.get(&c) {
            Some((_, true, _)) => 0.0,
            Some((Some(w), _, _)) => *w,
            _ => self.col_default,
        }
    }

    pub fn row_hidden(&self, r: u32) -> bool {
        self.righe.get(&r).is_some_and(|x| x.1)
    }

    pub fn col_hidden(&self, c: u32) -> bool {
        self.colonne.get(&c).is_some_and(|x| x.1)
    }

    /// Lo stile delle celle vuote: quello della riga, se c'è, poi quello della colonna.
    pub fn empty_style(&self, r: u32, c: u32) -> u32 {
        if let Some((_, _, Some(s))) = self.righe.get(&r) {
            return *s;
        }
        self.colonne.get(&c).map_or(0, |x| x.2)
    }

    /// Le celle con qualcosa da stampare: un valore o un aspetto (sfondo, bordo).
    pub fn used(&self, stili: &[Stile]) -> Option<Area> {
        let mut area: Option<Area> = None;
        fn grow(area: &mut Option<Area>, r: u32, c: u32) {
            *area = Some(match *area {
                None => (r, c, r, c),
                Some((r1, c1, r2, c2)) => (r1.min(r), c1.min(c), r2.max(r), c2.max(c)),
            });
        }
        for ((r, c), cell) in &self.celle {
            let visible = match &cell.valore {
                Valore::Vuoto => stili.get(cell.stile as usize).is_some_and(|s| s.fill.is_some() || s.bordi.iter().any(Option::is_some)),
                Valore::Testo(t) => !t.is_empty(),
                _ => true,
            };
            if visible {
                grow(&mut area, *r, *c);
            }
        }
        for (r1, c1, r2, c2) in &self.unioni {
            if area.is_some() {
                grow(&mut area, *r1, *c1);
                grow(&mut area, *r2, *c2);
            }
        }
        for (r, c, ..) in &self.immagini {
            grow(&mut area, *r, *c);
        }
        area
    }
}

pub struct Cartella {
    pub fogli: Vec<Foglio>,
    pub stili: Vec<Stile>,
    pub attivo: usize,
    pub date1904: bool,
}

impl Cartella {
    pub fn stile(&self, i: u32) -> &Stile {
        static DEFAULT: std::sync::OnceLock<Stile> = std::sync::OnceLock::new();
        self.stili.get(i as usize).unwrap_or_else(|| DEFAULT.get_or_init(Stile::default))
    }
}

/// La tavolozza dei colori numerati di Excel (64 voci).
pub fn indexed_color(i: usize) -> Option<Rgb> {
    const P: [u32; 64] = [
        0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF,
        0x800000, 0x008000, 0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, 0x9999FF, 0x993366, 0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF,
        0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99, 0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99,
        0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600, 0x666699, 0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399, 0x333333,
    ];
    P.get(i).map(|v| ((v >> 16) as u8, (v >> 8) as u8, *v as u8))
}

pub fn hex_color(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    // ARGB di Excel o RGB.
    let s = if s.len() == 8 { &s[2..] } else { s };
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// La sfumatura di Excel (tint): verso il bianco se positiva, verso il nero se negativa, sulla luminosità HSL.
pub fn tint(c: Rgb, t: f64) -> Rgb {
    if t == 0.0 {
        return c;
    }
    let (r, g, b) = (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let mut l = (max + min) / 2.0;
    let d = max - min;
    let (h, s) = if d == 0.0 {
        (0.0, 0.0)
    } else {
        let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        let h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } / 6.0;
        (h, s)
    };
    l = if t < 0.0 { l * (1.0 + t) } else { l * (1.0 - t) + t };
    let hue = |p: f64, q: f64, mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (hue(p, q, h + 1.0 / 3.0), hue(p, q, h), hue(p, q, h - 1.0 / 3.0))
    };
    let to = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    (to(r), to(g), to(b))
}

/// Il riferimento di una cella («AB12») come (riga, colonna) da 0.
pub fn cell_ref(r: &str) -> Option<(u32, u32)> {
    let r = r.trim().replace('$', "");
    let letters: String = r.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits = &r[letters.len()..];
    let mut c = 0u32;
    for b in letters.bytes() {
        c = c.checked_mul(26)?.checked_add((b.to_ascii_uppercase() - b'A' + 1) as u32)?;
    }
    let row: u32 = digits.parse().ok()?;
    (c > 0 && row > 0).then(|| (row - 1, c - 1))
}

fn col_letters(r: &str) -> Option<u32> {
    let mut c = 0u32;
    for b in r.trim().replace('$', "").bytes() {
        if !b.is_ascii_alphabetic() {
            return None;
        }
        c = c.checked_mul(26)?.checked_add((b.to_ascii_uppercase() - b'A' + 1) as u32)?;
    }
    (c > 0).then(|| c - 1)
}

/// Un'area scritta come «A1:D20», «'Foglio 1'!$A$1:$D$20», «$1:$2» (righe) o «$A:$B» (colonne).
pub fn parse_area(s: &str) -> Option<Area> {
    let s = s.rsplit('!').next()?.trim();
    let (a, b) = s.split_once(':').unwrap_or((s, s));
    if let (Some(x), Some(y)) = (cell_ref(a), cell_ref(b)) {
        return Some((x.0.min(y.0), x.1.min(y.1), x.0.max(y.0), x.1.max(y.1)));
    }
    if let (Ok(x), Ok(y)) = (a.replace('$', "").parse::<u32>(), b.replace('$', "").parse::<u32>()) {
        return Some((x.saturating_sub(1), 0, y.saturating_sub(1), MAX_COLS));
    }
    if let (Some(x), Some(y)) = (col_letters(a), col_letters(b)) {
        return Some((0, x, MAX_ROWS, y));
    }
    None
}

/// La carta dal numero di Excel (`paperSize`), in punti.
pub fn paper(n: u32) -> Option<(f32, f32)> {
    Some(match n {
        1 => (612.0, 792.0),
        5 => (612.0, 1008.0),
        8 => (842.0, 1191.0),
        9 => (595.0, 842.0),
        11 => (420.0, 595.0),
        _ => return None,
    })
}

// ------------------------------------------------------------------ lettura xlsx

const MAX_XML: u64 = 512 << 20;

fn voce(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, nome: &str) -> Option<String> {
    let f = zip.by_name(nome).ok()?;
    let mut s = String::new();
    f.take(MAX_XML).read_to_string(&mut s).ok()?;
    Some(s)
}

fn voce_bytes(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, nome: &str) -> Option<Vec<u8>> {
    let f = zip.by_name(nome).ok()?;
    let mut v = Vec::new();
    f.take(MAX_XML).read_to_end(&mut v).ok()?;
    Some(v)
}

fn parse(xml: &str) -> Result<roxmltree::Document<'_>, String> {
    roxmltree::Document::parse_with_options(xml, roxmltree::ParsingOptions { nodes_limit: u32::MAX, ..Default::default() }).map_err(|e| format!("foglio non letto: {e}"))
}

fn attr<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

fn child<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.children().find(|c| c.tag_name().name() == name)
}

fn truthy(v: Option<&str>) -> bool {
    matches!(v, Some("1" | "true" | "on"))
}

/// I colori del tema (theme1.xml), nell'ordine dei numeri di Excel: 0 sfondo 1, 1 testo 1, 2 sfondo 2…
fn theme_colors(xml: &str) -> Vec<Rgb> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    let Some(scheme) = doc.descendants().find(|n| n.tag_name().name() == "clrScheme") else { return Vec::new() };
    let get = |name: &str| -> Rgb {
        scheme
            .children()
            .find(|n| n.tag_name().name() == name)
            .and_then(|n| n.children().find(|c| c.is_element()))
            .and_then(|c| attr(c, "lastClr").or_else(|| attr(c, "val")))
            .and_then(hex_color)
            .unwrap_or((0, 0, 0))
    };
    ["lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"].iter().map(|n| get(n)).collect()
}

fn xlsx_color(n: roxmltree::Node, theme: &[Rgb]) -> Option<Rgb> {
    if truthy(attr(n, "auto")) {
        return None;
    }
    let base = if let Some(rgb) = attr(n, "rgb") {
        hex_color(rgb)?
    } else if let Some(t) = attr(n, "theme").and_then(|v| v.parse::<usize>().ok()) {
        *theme.get(t)?
    } else if let Some(i) = attr(n, "indexed").and_then(|v| v.parse::<usize>().ok()) {
        if i == 64 {
            return None;
        }
        indexed_color(i)?
    } else {
        return None;
    };
    Some(tint(base, attr(n, "tint").and_then(|v| v.parse().ok()).unwrap_or(0.0)))
}

fn border_of(n: roxmltree::Node, theme: &[Rgb]) -> Option<Bordo> {
    use super::pagina::Dash;
    let style = attr(n, "style")?;
    let color = child(n, "color").and_then(|c| xlsx_color(c, theme)).unwrap_or((0, 0, 0));
    let (width, dash, double) = match style {
        "hair" => (0.25, Dash::Solid, false),
        "thin" => (0.5, Dash::Solid, false),
        "medium" => (1.0, Dash::Solid, false),
        "thick" => (1.5, Dash::Solid, false),
        "dashed" => (0.5, Dash::Dashed, false),
        "dotted" => (0.5, Dash::Dotted, false),
        "mediumDashed" | "dashDot" | "dashDotDot" | "mediumDashDot" | "mediumDashDotDot" | "slantDashDot" => (if style.starts_with("medium") || style.starts_with("slant") { 1.0 } else { 0.5 }, Dash::Dashed, false),
        "double" => (0.5, Dash::Solid, true),
        "none" => return None,
        _ => (0.5, Dash::Solid, false),
    };
    Some(Bordo { width, color, dash, double })
}

struct XlsxStyles {
    stili: Vec<Stile>,
    default_font: Carattere,
}

fn xlsx_styles(xml: &str, theme: &[Rgb], l: &super::numfmt::Locale) -> XlsxStyles {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return XlsxStyles { stili: vec![Stile::default()], default_font: Carattere::default() } };
    let mut fmts: HashMap<u32, String> = HashMap::new();
    for f in doc.descendants().filter(|n| n.has_tag_name("numFmt")) {
        if let (Some(id), Some(code)) = (attr(f, "numFmtId").and_then(|v| v.parse().ok()), attr(f, "formatCode")) {
            fmts.insert(id, code.to_string());
        }
    }
    let section = |name: &str| doc.descendants().find(|n| n.tag_name().name() == name);
    let fonts: Vec<Carattere> = section("fonts")
        .map(|s| {
            s.children()
                .filter(|n| n.has_tag_name("font"))
                .map(|f| {
                    let val = |name: &str| child(f, name).map(|n| attr(n, "val").unwrap_or("1"));
                    Carattere {
                        nome: val("name").unwrap_or("Calibri").to_string(),
                        size: val("sz").and_then(|v| v.parse().ok()).unwrap_or(11.0),
                        bold: val("b").is_some_and(|v| v != "0" && v != "false"),
                        italic: val("i").is_some_and(|v| v != "0" && v != "false"),
                        underline: val("u").is_some_and(|v| v != "none"),
                        strike: val("strike").is_some_and(|v| v != "0" && v != "false"),
                        color: child(f, "color").and_then(|c| xlsx_color(c, theme)).unwrap_or((0, 0, 0)),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let fills: Vec<Option<Rgb>> = section("fills")
        .map(|s| {
            s.children()
                .filter(|n| n.has_tag_name("fill"))
                .map(|f| {
                    if let Some(p) = child(f, "patternFill") {
                        let kind = attr(p, "patternType").unwrap_or("none");
                        if kind == "none" {
                            return None;
                        }
                        let fg = child(p, "fgColor").and_then(|c| xlsx_color(c, theme));
                        let bg = child(p, "bgColor").and_then(|c| xlsx_color(c, theme));
                        return if kind == "solid" { fg.or(bg) } else { fg.or(bg).map(|c| tint(c, 0.5)) };
                    }
                    // Sfumatura: il primo colore.
                    f.descendants().find(|n| n.tag_name().name() == "color").and_then(|c| xlsx_color(c, theme))
                })
                .collect()
        })
        .unwrap_or_default();
    let borders: Vec<[Option<Bordo>; 4]> = section("borders")
        .map(|s| {
            s.children()
                .filter(|n| n.has_tag_name("border"))
                .map(|b| {
                    let side = |names: &[&str]| names.iter().find_map(|n| child(b, n)).and_then(|n| border_of(n, theme));
                    [side(&["left", "start"]), side(&["right", "end"]), side(&["top"]), side(&["bottom"])]
                })
                .collect()
        })
        .unwrap_or_default();
    let default_font = fonts.first().cloned().unwrap_or_default();
    let stili: Vec<Stile> = section("cellXfs")
        .map(|s| {
            s.children()
                .filter(|n| n.has_tag_name("xf"))
                .map(|xf| {
                    let num = |a: &str| attr(xf, a).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                    let fmt_id = num("numFmtId") as u32;
                    let al = child(xf, "alignment");
                    let a = |name: &str| al.and_then(|n| attr(n, name));
                    Stile {
                        font: fonts.get(num("fontId")).cloned().unwrap_or_else(|| default_font.clone()),
                        fill: fills.get(num("fillId")).copied().flatten(),
                        bordi: borders.get(num("borderId")).copied().unwrap_or([None; 4]),
                        h: match a("horizontal") {
                            Some("left") => HAlign::Left,
                            Some("center") => HAlign::Center,
                            Some("right") => HAlign::Right,
                            Some("fill") => HAlign::Fill,
                            Some("justify" | "distributed") => HAlign::Justify,
                            Some("centerContinuous") => HAlign::CenterAcross,
                            _ => HAlign::General,
                        },
                        v: match a("vertical") {
                            Some("top") => VAlign::Top,
                            Some("center" | "justify" | "distributed") => VAlign::Center,
                            _ => VAlign::Bottom,
                        },
                        wrap: truthy(a("wrapText")),
                        indent: a("indent").and_then(|v| v.parse().ok()).unwrap_or(0),
                        shrink: truthy(a("shrinkToFit")),
                        rotation: a("textRotation").and_then(|v| v.parse::<i16>().ok()).map_or(0, |r| if r > 90 && r <= 180 { 90 - r } else { r }),
                        numfmt: fmts.get(&fmt_id).cloned().unwrap_or_else(|| super::numfmt::builtin(fmt_id, l)),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let stili = if stili.is_empty() { vec![Stile { font: default_font.clone(), ..Default::default() }] } else { stili };
    XlsxStyles { stili, default_font }
}

fn rich_text(n: roxmltree::Node) -> String {
    let mut s = String::new();
    for d in n.descendants() {
        if d.tag_name().name() == "t" && !d.ancestors().any(|a| a.tag_name().name() == "rPh") {
            s.push_str(d.text().unwrap_or(""));
        }
    }
    s
}

/// La larghezza in punti di una colonna di Excel (in «caratteri» della cifra più larga del carattere normale).
pub fn col_points(chars: f64, mdw: f64) -> f32 {
    let px = (((256.0 * chars + (128.0 / mdw).trunc()) / 256.0) * mdw).trunc();
    (px * 0.75) as f32
}

/// La colonna normale da `baseColWidth`: i caratteri più il margine di 5 pixel, arrotondati in su
/// a 8 pixel come fa Excel (Calibri 11: 8 caratteri → 64 pixel, i soliti «8,43»).
fn base_points(chars: f64, mdw: f64) -> f32 {
    (((chars * mdw + 5.0) / 8.0).ceil() * 8.0 * 0.75) as f32
}

/// La cifra più larga del carattere normale, in pixel a 96 dpi (Calibri 11: 7).
pub fn max_digit_px(fonts: &mut super::font::Fonti, f: &Carattere) -> f64 {
    let id = fonts.get(&f.nome, false, false);
    let w = ('0'..='9').map(|c| fonts.face(id).glyph(c).map_or(500.0, |g| g.1)).fold(0.0f32, f32::max);
    ((w * f.size * 96.0 / 72.0 / 1000.0) as f64).round().max(1.0)
}

/// Legge un .xlsx/.xlsm per stamparlo.
pub fn leggi_xlsx(bytes: &[u8], fonts: &mut super::font::Fonti) -> Result<Cartella, String> {
    let l = super::numfmt::locale();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "il file non è un foglio Excel leggibile".to_string())?;
    let wb = voce(&mut zip, "xl/workbook.xml").ok_or("cartella di lavoro non trovata nel file")?;
    let rels = voce(&mut zip, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    let theme_path = {
        let d = roxmltree::Document::parse(&rels).ok();
        d.and_then(|d| d.descendants().find(|n| attr(*n, "Type").is_some_and(|t| t.ends_with("/theme"))).and_then(|n| attr(n, "Target")).map(|t| format!("xl/{}", t.trim_start_matches('/').trim_start_matches("xl/"))))
            .unwrap_or_else(|| "xl/theme/theme1.xml".into())
    };
    let theme = voce(&mut zip, &theme_path).map(|t| theme_colors(&t)).unwrap_or_default();
    let shared: Vec<String> = voce(&mut zip, "xl/sharedStrings.xml")
        .and_then(|s| parse(&s).ok().map(|d| d.root_element().children().filter(|n| n.has_tag_name("si")).map(rich_text).collect()))
        .unwrap_or_default();
    let styles = voce(&mut zip, "xl/styles.xml").map(|s| xlsx_styles(&s, &theme, l)).unwrap_or(XlsxStyles { stili: vec![Stile::default()], default_font: Carattere::default() });
    let mdw = max_digit_px(fonts, &styles.default_font);
    let default_row = {
        let id = fonts.get(&styles.default_font.nome, false, false);
        // Calibri 11 → 15 punti: la riga di Excel è l'altezza del carattere arrotondata al pixel.
        ((fonts.face(id).line(styles.default_font.size) * 96.0 / 72.0).ceil() + 1.0) * 0.75
    };
    let wb_doc = parse(&wb)?;
    let date1904 = wb_doc.descendants().any(|n| n.has_tag_name("workbookPr") && truthy(attr(n, "date1904")));
    let attivo = wb_doc.descendants().find(|n| n.has_tag_name("workbookView")).and_then(|n| attr(n, "activeTab")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let rel_doc = roxmltree::Document::parse(&rels).ok();
    // Area di stampa e titoli: nomi definiti, per foglio.
    let mut names: HashMap<(usize, &str), String> = HashMap::new();
    for d in wb_doc.descendants().filter(|n| n.has_tag_name("definedName")) {
        let (Some(name), Some(sheet)) = (attr(d, "name"), attr(d, "localSheetId").and_then(|v| v.parse::<usize>().ok())) else { continue };
        if name == "_xlnm.Print_Area" || name == "_xlnm.Print_Titles" {
            names.insert((sheet, name), d.text().unwrap_or("").to_string());
        }
    }
    let mut fogli = Vec::new();
    for (index, s) in wb_doc.descendants().filter(|n| n.has_tag_name("sheet")).enumerate() {
        let nome = attr(s, "name").unwrap_or("Foglio");
        let id = attr(s, "id").unwrap_or("");
        let target = rel_doc
            .as_ref()
            .and_then(|d| d.descendants().find(|n| n.has_tag_name("Relationship") && attr(*n, "Id") == Some(id)))
            .and_then(|n| attr(n, "Target"))
            .unwrap_or("");
        if target.is_empty() || target.contains("chartsheets/") {
            continue;
        }
        let path = match target.strip_prefix('/') {
            Some(abs) => abs.to_string(),
            None => format!("xl/{target}"),
        };
        let Some(xml) = voce(&mut zip, &path) else { continue };
        let mut f = Foglio::new(nome);
        f.nascosto = matches!(attr(s, "state"), Some("hidden" | "veryHidden"));
        f.col_default = base_points(8.0, mdw);
        f.riga_default = default_row as f32;
        sheet_xlsx(&xml, &shared, &theme, &styles.stili, mdw, &mut f)?;
        if let Some(a) = names.get(&(index, "_xlnm.Print_Area")) {
            // Più aree separate da virgole: si stampa quella che le contiene tutte.
            let areas: Vec<Area> = a.split(',').filter_map(parse_area).collect();
            if let Some(first) = areas.first() {
                f.stampa.area = Some(areas.iter().fold(*first, |x, y| (x.0.min(y.0), x.1.min(y.1), x.2.max(y.2), x.3.max(y.3))));
            }
        }
        if let Some(t) = names.get(&(index, "_xlnm.Print_Titles")) {
            for part in t.split(',') {
                if let Some(a) = parse_area(part) {
                    if a.3 == MAX_COLS {
                        f.stampa.titoli_righe = Some((a.0, a.2));
                    } else if a.2 == MAX_ROWS {
                        f.stampa.titoli_colonne = Some((a.1, a.3));
                    }
                }
            }
        }
        // Le immagini del foglio (non i grafici).
        let sheet_rels = {
            let (dir, file) = path.rsplit_once('/').unwrap_or(("", &path));
            format!("{dir}/_rels/{file}.rels")
        };
        if let Some(rels) = voce(&mut zip, &sheet_rels) {
            images_xlsx(&mut zip, &xml, &rels, &path, mdw, &mut f);
        }
        fogli.push(f);
    }
    if fogli.is_empty() {
        return Err("nessun foglio nel file".into());
    }
    Ok(Cartella { fogli, stili: styles.stili, attivo, date1904 })
}

fn sheet_xlsx(xml: &str, shared: &[String], theme: &[Rgb], stili: &[Stile], mdw: f64, f: &mut Foglio) -> Result<(), String> {
    let doc = parse(xml)?;
    let root = doc.root_element();
    if let Some(pr) = child(root, "sheetPr") {
        f.colore = child(pr, "tabColor").and_then(|c| xlsx_color(c, theme));
        if child(pr, "pageSetUpPr").is_some_and(|p| truthy(attr(p, "fitToPage"))) {
            f.stampa.adatta = Some((1, 1));
        }
    }
    if let Some(fmt) = child(root, "sheetFormatPr") {
        if let Some(w) = attr(fmt, "defaultColWidth").and_then(|v| v.parse::<f64>().ok()) {
            f.col_default = col_points(w, mdw);
        } else if let Some(b) = attr(fmt, "baseColWidth").and_then(|v| v.parse::<f64>().ok()) {
            f.col_default = base_points(b, mdw);
        }
        if let Some(h) = attr(fmt, "defaultRowHeight").and_then(|v| v.parse::<f32>().ok()) {
            f.riga_default = h;
        }
    }
    if let Some(cols) = child(root, "cols") {
        for c in cols.children().filter(|n| n.has_tag_name("col")) {
            let (Some(min), Some(max)) = (attr(c, "min").and_then(|v| v.parse::<u32>().ok()), attr(c, "max").and_then(|v| v.parse::<u32>().ok())) else { continue };
            let w = attr(c, "width").and_then(|v| v.parse::<f64>().ok()).map(|w| col_points(w, mdw));
            let hidden = truthy(attr(c, "hidden"));
            let style = attr(c, "style").and_then(|v| v.parse().ok()).unwrap_or(0);
            for i in min.saturating_sub(1)..max.min(MAX_COLS) {
                f.colonne.insert(i, (w, hidden, style));
            }
        }
    }
    if let Some(data) = child(root, "sheetData") {
        let mut next_row = 0u32;
        for row in data.children().filter(|n| n.has_tag_name("row")) {
            let r = attr(row, "r").and_then(|v| v.parse::<u32>().ok()).map_or(next_row, |v| v.saturating_sub(1));
            next_row = r + 1;
            if r >= MAX_ROWS {
                break;
            }
            let ht = attr(row, "ht").and_then(|v| v.parse::<f32>().ok()).filter(|_| truthy(attr(row, "customHeight")) || true);
            let hidden = truthy(attr(row, "hidden"));
            let row_style = if truthy(attr(row, "customFormat")) { attr(row, "s").and_then(|v| v.parse().ok()) } else { None };
            if ht.is_some() || hidden || row_style.is_some() {
                f.righe.insert(r, (ht, hidden, row_style));
            }
            let mut next_col = 0u32;
            for c in row.children().filter(|n| n.has_tag_name("c")) {
                let col = attr(c, "r").and_then(cell_ref).map_or(next_col, |x| x.1);
                next_col = col + 1;
                if col >= MAX_COLS {
                    continue;
                }
                let stile: u32 = attr(c, "s").and_then(|v| v.parse().ok()).unwrap_or(0);
                let v = child(c, "v").and_then(|n| n.text()).unwrap_or("");
                let valore = match attr(c, "t").unwrap_or("n") {
                    "s" => v.trim().parse::<usize>().ok().and_then(|i| shared.get(i)).map_or(Valore::Vuoto, |s| Valore::Testo(s.clone())),
                    "inlineStr" => child(c, "is").map_or(Valore::Vuoto, |n| Valore::Testo(rich_text(n))),
                    "b" => Valore::Bool(v.trim() == "1"),
                    "e" => Valore::Errore(v.to_string()),
                    "str" | "d" => Valore::Testo(v.to_string()),
                    _ if v.trim().is_empty() => Valore::Vuoto,
                    _ => v.trim().parse::<f64>().map_or_else(|_| Valore::Testo(v.to_string()), Valore::Numero),
                };
                if valore == Valore::Vuoto && stile == 0 {
                    continue;
                }
                f.celle.insert((r, col), Cella { valore, stile, mostra: None, link: None });
            }
        }
    }
    if let Some(m) = child(root, "mergeCells") {
        for mc in m.children().filter(|n| n.has_tag_name("mergeCell")) {
            if let Some(a) = attr(mc, "ref").and_then(parse_area) {
                f.unioni.push(a);
            }
        }
    }
    let st = &mut f.stampa;
    if let Some(p) = child(root, "printOptions") {
        st.centra_o = truthy(attr(p, "horizontalCentered"));
        st.centra_v = truthy(attr(p, "verticalCentered"));
        st.griglia = truthy(attr(p, "gridLines"));
        st.intestazioni = truthy(attr(p, "headings"));
    }
    if let Some(m) = child(root, "pageMargins") {
        let g = |a: &str, d: f32| attr(m, a).and_then(|v| v.parse::<f32>().ok()).map_or(d, |v| v * 72.0);
        st.margini = [g("left", 50.4), g("right", 50.4), g("top", 54.0), g("bottom", 54.0), g("header", 21.6), g("footer", 21.6)];
    }
    if let Some(p) = child(root, "pageSetup") {
        st.orizzontale = attr(p, "orientation").map(|o| o == "landscape");
        st.carta = attr(p, "paperSize").and_then(|v| v.parse().ok()).and_then(paper).or(Some((595.0, 842.0)));
        st.scala = attr(p, "scale").and_then(|v| v.parse().ok()).unwrap_or(100).clamp(10, 400);
        if st.adatta.is_some() {
            let w = attr(p, "fitToWidth").and_then(|v| v.parse().ok()).unwrap_or(1);
            let h = attr(p, "fitToHeight").and_then(|v| v.parse().ok()).unwrap_or(1);
            st.adatta = Some((w, h));
        }
        st.giu_poi_destra = attr(p, "pageOrder") != Some("overThenDown");
        if truthy(attr(p, "useFirstPageNumber")) {
            st.primo_numero = attr(p, "firstPageNumber").and_then(|v| v.parse().ok());
        }
    }
    if let Some(hf) = child(root, "headerFooter") {
        st.testa = child(hf, "oddHeader").and_then(|n| n.text()).unwrap_or("").to_string();
        st.piede = child(hf, "oddFooter").and_then(|n| n.text()).unwrap_or("").to_string();
    }
    let breaks = |name: &str| -> Vec<u32> {
        child(root, name)
            .map(|b| b.children().filter(|n| n.has_tag_name("brk")).filter_map(|n| attr(n, "id").and_then(|v| v.parse().ok())).collect())
            .unwrap_or_default()
    };
    st.salti_righe = breaks("rowBreaks");
    st.salti_colonne = breaks("colBreaks");
    let _ = stili;
    Ok(())
}

/// Le immagini ancorate alle celle (xl/drawings), con la loro misura.
fn images_xlsx(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, sheet_xml: &str, rels: &str, sheet_path: &str, mdw: f64, f: &mut Foglio) {
    let Ok(sheet) = roxmltree::Document::parse(sheet_xml) else { return };
    let Some(drawing) = sheet.descendants().find(|n| n.tag_name().name() == "drawing") else { return };
    let Some(rid) = drawing.attributes().find(|a| a.name() == "id").map(|a| a.value().to_string()) else { return };
    let Ok(rels_doc) = roxmltree::Document::parse(rels) else { return };
    let dir = sheet_path.rsplit_once('/').map_or("xl", |x| x.0);
    let resolve = |base_dir: &str, target: &str| -> String {
        let mut parts: Vec<&str> = base_dir.split('/').collect();
        for p in target.split('/') {
            match p {
                ".." => {
                    parts.pop();
                }
                "." | "" => {}
                x => parts.push(x),
            }
        }
        parts.join("/")
    };
    let Some(target) = rels_doc.descendants().find(|n| attr(*n, "Id") == Some(rid.as_str())).and_then(|n| attr(n, "Target")) else { return };
    let drawing_path = if let Some(abs) = target.strip_prefix('/') { abs.to_string() } else { resolve(dir, target) };
    let Some(dxml) = voce(zip, &drawing_path) else { return };
    let (ddir, dfile) = drawing_path.rsplit_once('/').unwrap_or(("", &drawing_path));
    let drels = voce(zip, &format!("{ddir}/_rels/{dfile}.rels")).unwrap_or_default();
    let Ok(ddoc) = roxmltree::Document::parse(&dxml) else { return };
    let Ok(drels_doc) = roxmltree::Document::parse(&drels) else { return };
    let emu = |v: Option<&str>| v.and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0) as f32 / 12_700.0;
    for anchor in ddoc.root_element().children().filter(|n| matches!(n.tag_name().name(), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor")) {
        let Some(blip) = anchor.descendants().find(|n| n.tag_name().name() == "blip") else { continue };
        let Some(embed) = blip.attributes().find(|a| a.name() == "embed").map(|a| a.value()) else { continue };
        let Some(img_target) = drels_doc.descendants().find(|n| attr(*n, "Id") == Some(embed)).and_then(|n| attr(n, "Target")) else { continue };
        let img_path = if let Some(abs) = img_target.strip_prefix('/') { abs.to_string() } else { resolve(ddir, img_target) };
        let Some(bytes) = voce_bytes(zip, &img_path) else { continue };
        let pos = |n: Option<roxmltree::Node>| -> (u32, u32, f32, f32) {
            let Some(n) = n else { return (0, 0, 0.0, 0.0) };
            let num = |name: &str| child(n, name).and_then(|x| x.text()).and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
            (num("row") as u32, num("col") as u32, num("colOff") as f32 / 12_700.0, num("rowOff") as f32 / 12_700.0)
        };
        let (r, c, dx, dy) = pos(child(anchor, "from"));
        // La misura: dall'«ext» della forma (vale per tutti gli ancoraggi).
        let ext = anchor.descendants().find(|n| n.tag_name().name() == "ext" && n.parent().is_some_and(|p| p.tag_name().name() == "xfrm"));
        let (mut w, mut h) = (emu(ext.and_then(|e| attr(e, "cx"))), emu(ext.and_then(|e| attr(e, "cy"))));
        if anchor.tag_name().name() == "twoCellAnchor" && (w <= 0.0 || h <= 0.0) {
            let (r2, c2, dx2, dy2) = pos(child(anchor, "to"));
            w = (c..c2).map(|x| f.col_width(x)).sum::<f32>() - dx + dx2;
            h = (r..r2).map(|y| f.righe.get(&y).and_then(|x| x.0).unwrap_or(f.riga_default)).sum::<f32>() - dy + dy2;
        }
        let _ = mdw;
        if w > 0.5 && h > 0.5 {
            if let Some(img) = crate::pdfdoc::DocImage::from_bytes(&bytes, Some((w, h)), 0) {
                f.immagini.push((r, c, dx, dy, w, h, img));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_areas_and_colors() {
        assert_eq!(cell_ref("B3"), Some((2, 1)));
        assert_eq!(cell_ref("$AA$10"), Some((9, 26)));
        assert_eq!(parse_area("'Foglio 1'!$A$1:$D$20"), Some((0, 0, 19, 3)));
        assert_eq!(parse_area("Foglio1!$1:$2"), Some((0, 0, 1, MAX_COLS)));
        assert_eq!(parse_area("$A:$B"), Some((0, 0, MAX_ROWS, 1)));
        assert_eq!(hex_color("FF4472C4"), Some((0x44, 0x72, 0xC4)));
        // La sfumatura «Più chiaro 80%» dell'azzurro del tema.
        assert_eq!(tint((0x44, 0x72, 0xC4), 0.7999816888943144), (0xDA, 0xE3, 0xF3));
        assert_eq!(tint((0, 0, 0), -0.5), (0, 0, 0));
        // Calibri 11: la colonna normale di 8,43 caratteri (9,140625 nel file) è larga 64 pixel, 48 punti.
        assert_eq!(col_points(9.140625, 7.0), 48.0);
        assert_eq!(base_points(8.0, 7.0), 48.0);
        assert_eq!(indexed_color(10), Some((255, 0, 0)));
    }
}
