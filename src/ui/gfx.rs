//! Il disegno dell'interfaccia: un pittore in Rust puro (tiny-skia per le forme, skrifa per i
//! caratteri) che riempie un'immagine RGBA. Windows e Linux la copiano sullo schermo così com'è:
//! le finestre sono uguali sui due sistemi e si possono fotografare anche senza schermo.
//! Le misure sono in unità logiche (1 = un pixel a 100%); `scale` le porta ai pixel veri.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::ui::theme::ThemeId;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use tiny_skia::{
    FillRule, FilterQuality, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, StrokeDash, Transform,
};

pub type Rgb = (u8, u8, u8);

/// Il peso di un carattere.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Weight {
    Regular,
    Semibold,
    Bold,
    /// Il carattere con le grazie, solo per il contenuto dei documenti (Georgia su Windows).
    Serif,
    SerifBold,
    /// A larghezza fissa, per l'anteprima dei file di testo (Consolas su Windows).
    Mono,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Un file di carattere già letto, con l'indice dentro le raccolte .ttc.
#[derive(Clone)]
pub struct Face {
    data: Arc<dyn AsRef<[u8]> + Send + Sync>,
    index: u32,
}

impl Face {
    pub fn new(data: Vec<u8>, index: u32) -> Option<Face> {
        Face::from_bytes(Arc::new(data), index)
    }

    /// Un carattere incorporato nel programma: si legge da lì, senza copiarlo in memoria.
    pub fn from_static(data: &'static [u8], index: u32) -> Option<Face> {
        Face::from_bytes(Arc::new(data), index)
    }

    fn from_bytes(data: Arc<dyn AsRef<[u8]> + Send + Sync>, index: u32) -> Option<Face> {
        FontRef::from_index((*data).as_ref(), index).ok()?;
        Some(Face { data, index })
    }

    fn font(&self) -> FontRef<'_> {
        FontRef::from_index((*self.data).as_ref(), self.index).expect("controllato in new")
    }
}

/// Un file di carattere mappato in memoria (Windows): le pagine restano quelle del file, condivise con
/// gli altri programmi e restituibili al sistema, invece di una copia privata di qualche MB.
#[cfg(windows)]
struct Mapped {
    view: windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS,
    len: usize,
}

#[cfg(windows)]
unsafe impl Send for Mapped {}
#[cfg(windows)]
unsafe impl Sync for Mapped {}

#[cfg(windows)]
impl AsRef<[u8]> for Mapped {
    fn as_ref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.view.Value as *const u8, self.len) }
    }
}

#[cfg(windows)]
impl Drop for Mapped {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::Memory::UnmapViewOfFile(self.view);
        }
    }
}

#[cfg(windows)]
fn map_file(path: &std::path::Path) -> Option<Mapped> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, FILE_MAP_READ, PAGE_READONLY};
    let file = std::fs::File::open(path).ok()?;
    let len = usize::try_from(file.metadata().ok()?.len()).ok().filter(|&l| l > 0)?;
    unsafe {
        let mapping = CreateFileMappingW(HANDLE(file.as_raw_handle()), None, PAGE_READONLY, 0, 0, None).ok()?;
        let view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0);
        // La vista tiene viva la mappatura anche dopo aver chiuso le maniglie.
        let _ = CloseHandle(mapping);
        if view.Value.is_null() {
            return None;
        }
        Some(Mapped { view, len })
    }
}

/// I caratteri dell'interfaccia: Segoe UI su Windows, quello di sistema su Linux; il Titillium
/// incorporato copre i segni che mancano e fa da riserva se non si trova niente.
pub struct Fonts {
    faces: HashMap<Weight, Face>,
    fallback: Face,
}

const TITILLIUM: &[u8] = include_bytes!("../../assets/fonts/TitilliumWeb-Regular.ttf");
const TITILLIUM_BOLD: &[u8] = include_bytes!("../../assets/fonts/TitilliumWeb-Bold.ttf");

fn read_face(path: &std::path::Path, index: u32) -> Option<Face> {
    #[cfg(windows)]
    if let Some(mapped) = map_file(path) {
        return Face::from_bytes(Arc::new(mapped), index);
    }
    Face::new(std::fs::read(path).ok()?, index)
}

#[cfg(target_os = "linux")]
fn fc_match(pattern: &str) -> Option<Face> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{file}\n%{index}", pattern]).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let mut lines = text.lines();
    let file = lines.next()?;
    let index = lines.next().and_then(|i| i.trim().parse().ok()).unwrap_or(0);
    read_face(std::path::Path::new(file), index)
}

impl Fonts {
    /// Cerca i caratteri del sistema. `CEDRO_UI_FONTS` (cartella con regular.ttf, semibold.ttf,
    /// bold.ttf, serif.ttf) li impone, per le prove e le fotografie.
    pub fn system() -> Fonts {
        let mut faces = HashMap::new();
        if let Some(dir) = std::env::var_os("CEDRO_UI_FONTS") {
            let dir = std::path::PathBuf::from(dir);
            for (w, name) in [(Weight::Regular, "regular.ttf"), (Weight::Semibold, "semibold.ttf"), (Weight::Bold, "bold.ttf"), (Weight::Serif, "serif.ttf"), (Weight::SerifBold, "serif-bold.ttf"), (Weight::Mono, "mono.ttf")] {
                if let Some(f) = read_face(&dir.join(name), 0) {
                    faces.insert(w, f);
                }
            }
        }
        #[cfg(windows)]
        {
            let fonts = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts");
            for (w, name) in [(Weight::Regular, "segoeui.ttf"), (Weight::Semibold, "seguisb.ttf"), (Weight::Bold, "segoeuib.ttf"), (Weight::Serif, "georgia.ttf"), (Weight::SerifBold, "georgiab.ttf"), (Weight::Mono, "consola.ttf")] {
                if !faces.contains_key(&w) {
                    if let Some(f) = read_face(&fonts.join(name), 0) {
                        faces.insert(w, f);
                    }
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            for (w, pattern) in [
                (Weight::Regular, "sans-serif:weight=80"),
                (Weight::Semibold, "sans-serif:weight=180"),
                (Weight::Bold, "sans-serif:weight=200"),
                (Weight::Serif, "serif:weight=80"),
                (Weight::SerifBold, "serif:weight=200"),
                (Weight::Mono, "monospace:weight=80"),
            ] {
                if let std::collections::hash_map::Entry::Vacant(e) = faces.entry(w) {
                    if let Some(f) = fc_match(pattern) {
                        e.insert(f);
                    }
                }
            }
        }
        #[cfg(target_os = "android")]
        {
            // Roboto, il carattere del telefono (Medium per i semigrassetti).
            let dir = std::path::Path::new("/system/fonts");
            for (w, name) in [(Weight::Regular, "Roboto-Regular.ttf"), (Weight::Semibold, "Roboto-Medium.ttf"), (Weight::Bold, "Roboto-Bold.ttf"), (Weight::Serif, "NotoSerif-Regular.ttf"), (Weight::SerifBold, "NotoSerif-Bold.ttf"), (Weight::Mono, "DroidSansMono.ttf")] {
                if !faces.contains_key(&w) {
                    if let Some(f) = read_face(&dir.join(name), 0) {
                        faces.insert(w, f);
                    }
                }
            }
        }
        Fonts::with(faces)
    }

    pub fn with(mut faces: HashMap<Weight, Face>) -> Fonts {
        let regular = Face::from_static(TITILLIUM, 0).expect("Titillium incorporato");
        let bold = Face::from_static(TITILLIUM_BOLD, 0).expect("Titillium incorporato");
        faces.entry(Weight::Regular).or_insert_with(|| regular.clone());
        let base = faces[&Weight::Regular].clone();
        faces.entry(Weight::Bold).or_insert_with(|| if Arc::ptr_eq(&base.data, &regular.data) { bold.clone() } else { base.clone() });
        let strong = faces[&Weight::Bold].clone();
        faces.entry(Weight::Semibold).or_insert_with(|| strong.clone());
        faces.entry(Weight::Serif).or_insert_with(|| base.clone());
        let serif = faces[&Weight::Serif].clone();
        faces.entry(Weight::SerifBold).or_insert_with(|| serif.clone());
        faces.entry(Weight::Mono).or_insert_with(|| base.clone());
        Fonts { faces, fallback: regular }
    }

    /// I caratteri di tutto il processo, letti la prima volta che servono.
    pub fn shared() -> &'static Fonts {
        static FONTS: OnceLock<Fonts> = OnceLock::new();
        FONTS.get_or_init(Fonts::system)
    }

    /// I caratteri del tema Bondeno: Titillium Web (come il sito del Comune), con serif di sistema per le pagine.
    pub fn bondeno() -> &'static Fonts {
        static FONTS: OnceLock<Fonts> = OnceLock::new();
        FONTS.get_or_init(|| Fonts::embedded(TITILLIUM, TITILLIUM_BOLD, TITILLIUM_BOLD))
    }

    /// Una famiglia incorporata (regular, semibold, bold); serif e monospazio restano quelli di sistema.
    fn embedded(regular: &'static [u8], semibold: &'static [u8], bold: &'static [u8]) -> Fonts {
        let system = Fonts::shared();
        let face = |b: &'static [u8]| Face::from_static(b, 0).expect("carattere incorporato");
        let mut faces = HashMap::new();
        faces.insert(Weight::Regular, face(regular));
        faces.insert(Weight::Semibold, face(semibold));
        faces.insert(Weight::Bold, face(bold));
        for w in [Weight::Serif, Weight::SerifBold, Weight::Mono] {
            faces.insert(w, system.faces[&w].clone());
        }
        Fonts::with(faces)
    }

    /// Il carattere di ogni tema: Segoe UI (quello di sistema) per Windows essenziale, uno incorporato per gli altri.
    pub fn for_theme(theme: ThemeId) -> &'static Fonts {
        macro_rules! famiglia {
            ($nome:literal) => {{
                static FONTS: OnceLock<Fonts> = OnceLock::new();
                FONTS.get_or_init(|| {
                    Fonts::embedded(
                        include_bytes!(concat!("../../assets/fonts/tema/", $nome, "-regular.ttf")),
                        include_bytes!(concat!("../../assets/fonts/tema/", $nome, "-semibold.ttf")),
                        include_bytes!(concat!("../../assets/fonts/tema/", $nome, "-bold.ttf")),
                    )
                })
            }};
        }
        match theme {
            ThemeId::WindowsEssenziale => Fonts::shared(),
            ThemeId::PercorsoGuidato => famiglia!("atkinson"),
            ThemeId::CedroCompatto => famiglia!("figtree"),
            ThemeId::BancoDiLavoro => famiglia!("plex"),
            ThemeId::DocumentoAlCentro => famiglia!("source"),
            ThemeId::Bondeno => Fonts::bondeno(),
        }
    }

    /// I caratteri del tema in uso adesso (`edizione::tema`).
    pub fn current() -> &'static Fonts {
        Fonts::for_theme(crate::edizione::tema())
    }

    /// Il carattere che disegna `ch` con quel peso, e il suo glifo.
    fn glyph(&self, weight: Weight, ch: char) -> (&Face, GlyphId) {
        let face = &self.faces[&weight];
        if let Some(g) = face.font().charmap().map(ch) {
            return (face, g);
        }
        let g = self.fallback.font().charmap().map(ch).unwrap_or(GlyphId::new(0));
        (&self.fallback, g)
    }

    fn advance(&self, face: &Face, gid: GlyphId, px: f32) -> f32 {
        face.font().glyph_metrics(Size::new(px), LocationRef::default()).advance_width(gid).unwrap_or(px * 0.5)
    }

    /// Larghezza del testo, in pixel, alla misura `px`.
    pub fn measure(&self, s: &str, px: f32, weight: Weight) -> f32 {
        s.chars().map(|c| {
            let (face, g) = self.glyph(weight, c);
            self.advance(face, g, px)
        }).sum()
    }

    /// Ascendente e discendente (positivi) del carattere principale.
    pub fn extents(&self, px: f32, weight: Weight) -> (f32, f32) {
        let m = self.faces[&weight].font().metrics(Size::new(px), LocationRef::default());
        (m.ascent, -m.descent)
    }
}

/// La misura vera dei testi: i disegni dicono la misura di progetto, qui diventa più grande per leggere
/// meglio (+2 per i testi, +1 dai titoli di 22 in su, che sono già grandi).
pub fn text_size(size: f32) -> f32 {
    if size < 22.0 { size + 2.0 } else { size + 1.0 }
}

struct PathPen(PathBuilder);

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, -y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, -y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, -cy0, x, -y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.cubic_to(cx0, -cy0, cx1, -cy1, x, -y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// Un glifo già disegnato: copertura 0..255, e dove sta rispetto all'origine della riga.
struct Glyph {
    mask: Vec<u8>,
    w: i32,
    h: i32,
    left: i32,
    top: i32,
}

type GlyphKey = (usize, u32, u32, u8);

fn raster_glyph(face: &Face, gid: GlyphId, px: f32, sub: f32) -> Option<Glyph> {
    let font = face.font();
    let outline = font.outline_glyphs().get(gid)?;
    let mut pen = PathPen(PathBuilder::new());
    outline.draw(DrawSettings::unhinted(Size::new(px), LocationRef::default()), &mut pen).ok()?;
    let path = pen.0.finish()?;
    let b = path.bounds();
    let left = (b.left() + sub).floor() as i32 - 1;
    let top = b.top().floor() as i32 - 1;
    let w = ((b.right() + sub).ceil() as i32 + 1 - left).max(1);
    let h = (b.bottom().ceil() as i32 + 1 - top).max(1);
    let mut pm = Pixmap::new(w as u32, h as u32)?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::from_translate(sub - left as f32, -top as f32), None);
    let mask = pm.data().chunks_exact(4).map(|p| p[3]).collect();
    Some(Glyph { mask, w, h, left, top })
}

/// Un'immagine decodificata per le miniature e le anteprime (RGBA non premoltiplicato).
#[derive(Clone)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pixmap: Arc<Pixmap>,
}

impl Image {
    pub fn from_rgba(w: u32, h: u32, rgba: &[u8]) -> Option<Image> {
        let mut pm = Pixmap::new(w, h)?;
        for (d, s) in pm.data_mut().chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            let a = s[3] as u32;
            d[0] = (s[0] as u32 * a / 255) as u8;
            d[1] = (s[1] as u32 * a / 255) as u8;
            d[2] = (s[2] as u32 * a / 255) as u8;
            d[3] = s[3];
        }
        Some(Image { w, h, pixmap: Arc::new(pm) })
    }

    /// L'immagine girata di `quarters` quarti di giro in senso orario (le pagine ruotate dei PDF).
    pub fn turned(&self, quarters: i32) -> Image {
        let q = quarters.rem_euclid(4);
        if q == 0 {
            return self.clone();
        }
        let (w, h) = (self.w as usize, self.h as usize);
        let (nw, nh) = if q % 2 == 1 { (h, w) } else { (w, h) };
        let Some(mut pm) = Pixmap::new(nw as u32, nh as u32) else { return self.clone() };
        let src = self.pixmap.data();
        let dst = pm.data_mut();
        for y in 0..h {
            for x in 0..w {
                let (nx, ny) = match q {
                    1 => (h - 1 - y, x),
                    2 => (w - 1 - x, h - 1 - y),
                    _ => (y, w - 1 - x),
                };
                let (s, d) = ((y * w + x) * 4, (ny * nw + nx) * 4);
                dst[d..d + 4].copy_from_slice(&src[s..s + 4]);
            }
        }
        Image { w: nw as u32, h: nh as u32, pixmap: Arc::new(pm) }
    }

    /// Quello che ha disegnato un pittore, così com'è.
    pub fn from_painter(p: Painter) -> Image {
        Image { w: p.pixmap.width(), h: p.pixmap.height(), pixmap: Arc::new(p.pixmap) }
    }
}

/// Uno stemma del Comune di Bondeno pronto da disegnare (decodificato una volta sola), qualunque sia il tema:
/// chi disegna decide se mostrarlo.
pub fn stemma(quale: crate::edizione::Stemma) -> Option<&'static Image> {
    use crate::edizione::{Stemma, CERCHIO, SCRITTA, SCRITTA_RITAGLIO, STEMMA};
    static SOLO: OnceLock<Option<Image>> = OnceLock::new();
    static SCRITTA_: OnceLock<Option<Image>> = OnceLock::new();
    static CERCHIO_: OnceLock<Option<Image>> = OnceLock::new();
    let (cell, png, ritaglio) = match quale {
        Stemma::Solo => (&SOLO, STEMMA, None),
        Stemma::Scritta => (&SCRITTA_, SCRITTA, Some(SCRITTA_RITAGLIO)),
        Stemma::Cerchio => (&CERCHIO_, CERCHIO, None),
    };
    cell.get_or_init(|| {
        let mut rgba = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
        if let Some((x, y, lato)) = ritaglio {
            rgba = image::imageops::crop_imm(&rgba, x, y, lato, lato).to_image();
        }
        let (w, h) = rgba.dimensions();
        Image::from_rgba(w, h, rgba.as_raw())
    })
    .as_ref()
}

/// Come un'immagine riempie il suo riquadro.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Copre tutto il riquadro, tagliando quel che avanza.
    Cover,
    /// Sta tutta dentro, con lo spazio vuoto ai lati.
    Contain,
}

/// Il pittore: disegna in unità logiche su un'immagine in pixel.
pub struct Painter<'a> {
    pub pixmap: Pixmap,
    pub scale: f32,
    fonts: &'a Fonts,
    cache: &'a mut GlyphCache,
    clips: Vec<(Rect, std::rc::Rc<Mask>)>,
    /// Spostamento corrente delle coordinate logiche (per le zone che scorrono).
    pub dx: f32,
    pub dy: f32,
    /// Testi più grandi della misura di progetto (`text_size`): sì sul computer; su Android è la scelta «Caratteri».
    pub larger: bool,
}

/// La maschera del ritaglio solo se serve: se il tracciato (allargato di `margine` pixel) sta tutto dentro
/// il rettangolo del ritaglio, niente maschera. Con la maschera tiny-skia passa a una pipeline molto più lenta.
fn maschera_per<'m>(clips: &'m [(Rect, std::rc::Rc<Mask>)], b: Rect, tf: Transform, margine: f32) -> Option<&'m Mask> {
    let (r, m) = clips.last()?;
    match b.transform(tf) {
        Some(d) if d.left() - margine >= r.left() && d.top() - margine >= r.top() && d.right() + margine <= r.right() && d.bottom() + margine <= r.bottom() => None,
        _ => Some(&**m),
    }
}

/// Un'immagine preparata: quale (indirizzo dei pixel), misura in pixel, raggio in pixel, adattamento.
type ChiaveImg = (usize, u32, u32, u32, bool);

/// Il peso massimo delle immagini preparate tenute fra un fotogramma e l'altro.
const IMMAGINI_MAX_BYTE: usize = 96 << 20;

/// I glifi già disegnati: si tiene fra un fotogramma e l'altro.
#[derive(Default)]
pub struct GlyphCache {
    glyphs: HashMap<GlyphKey, Option<Glyph>>,
    /// Le immagini già ridotte e arrotondate alla misura in cui si disegnano, col fotogramma dell'ultimo uso:
    /// ridimensionarle (bicubico) e prepararne la maschera a ogni fotogramma costava più di tutto il resto.
    /// Con l'originale tenuto vivo: finché la voce c'è, il suo indirizzo non può toccare a un'altra immagine.
    immagini: HashMap<ChiaveImg, (Arc<Pixmap>, Arc<Pixmap>, u64)>,
    /// Le maschere dei ritagli rettangolari (una per rettangolo e misura della finestra), riusate fra i fotogrammi.
    maschere: HashMap<(i32, i32, i32, i32, u32, u32), (std::rc::Rc<Mask>, u64)>,
    fotogramma: u64,
    /// La scala dei glifi tenuti.
    scale: f32,
    /// Il foglio del fotogramma prima, da riusare (`Painter::new_riusando`): un foglio nuovo di 10 MB costa
    /// soprattutto la prima scrittura di ogni pagina di memoria, qualche millisecondo a fotogramma sul telefono.
    tela: Option<Pixmap>,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Da chiamare prima di disegnare a `scale`: a ogni nuova scala (finestra stretta che si ridimensiona,
    /// DPI) nasce una serie nuova di glifi e quelle vecchie non servono più, quindi si ricomincia da capo.
    /// Un tetto largo copre il resto (cambi di tema). Si riempie di nuovo in un fotogramma.
    pub fn trim(&mut self, scale: f32) {
        if self.scale != scale || self.glyphs.len() > 20_000 {
            self.glyphs.clear();
            self.scale = scale;
        }
    }

    /// Restituisce il foglio di un fotogramma finito, per il prossimo `Painter::new_riusando`.
    pub fn ricicla(&mut self, pm: Pixmap) {
        self.tela = Some(pm);
    }

    /// A ogni fotogramma: si dimenticano le immagini e le maschere non usate da un po'.
    fn nuovo_fotogramma(&mut self) {
        self.fotogramma += 1;
        let f = self.fotogramma;
        // Anche a peso: ogni misura nuova (finestra che si ridimensiona, zoom, pagine sfogliate) è una voce
        // fino a 16 MB, e sotto le 48 voci restavano tutte, centinaia di MB mai restituiti.
        let peso = |m: &HashMap<ChiaveImg, (Arc<Pixmap>, Arc<Pixmap>, u64)>| m.values().map(|(_, pm, _)| pm.data().len()).sum::<usize>();
        if self.immagini.len() > 48 || peso(&self.immagini) > IMMAGINI_MAX_BYTE {
            self.immagini.retain(|_, (_, _, u)| f - *u < 30);
            if peso(&self.immagini) > IMMAGINI_MAX_BYTE {
                self.immagini.retain(|_, (_, _, u)| f - *u <= 1);
            }
        }
        // Le maschere sono grandi quanto la finestra: ridimensionandola ne nasce una serie a ogni misura.
        let peso_m = |m: &HashMap<(i32, i32, i32, i32, u32, u32), (std::rc::Rc<Mask>, u64)>| m.values().map(|(mk, _)| mk.data().len()).sum::<usize>();
        if self.maschere.len() > 16 {
            self.maschere.retain(|_, (_, u)| f - *u < 30);
            if peso_m(&self.maschere) > IMMAGINI_MAX_BYTE {
                self.maschere.retain(|_, (_, u)| f - *u <= 1);
            }
        }
    }
}

fn color(c: Rgb, a: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.0, c.1, c.2, (a.clamp(0.0, 1.0) * 255.0).round() as u8);
    p.anti_alias = true;
    p
}

/// Un riquadro con gli angoli arrotondati (raggio limitato a metà del lato corto).
pub fn rrect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r == 0.0 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?));
    }
    let k = 0.552_284_8 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

impl<'a> Painter<'a> {
    /// Come `new`, ma riusa il foglio restituito con `GlyphCache::ricicla` se ha la stessa misura: il
    /// contenuto è quello del fotogramma prima, quindi va pulito (`clear`) prima di disegnare.
    pub fn new_riusando(w: u32, h: u32, scale: f32, fonts: &'a Fonts, cache: &'a mut GlyphCache) -> Painter<'a> {
        match cache.tela.take() {
            Some(pm) if pm.width() == w.max(1) && pm.height() == h.max(1) => {
                cache.nuovo_fotogramma();
                Painter { pixmap: pm, scale, fonts, cache, clips: Vec::new(), dx: 0.0, dy: 0.0, larger: true }
            }
            _ => Painter::new(w, h, scale, fonts, cache),
        }
    }

    /// Il foglio disegnato (per restituirlo con `GlyphCache::ricicla`).
    pub fn in_foglio(self) -> Pixmap {
        self.pixmap
    }

    /// Un'immagine nuova di `w`×`h` pixel, con `scale` pixel per unità logica.
    pub fn new(w: u32, h: u32, scale: f32, fonts: &'a Fonts, cache: &'a mut GlyphCache) -> Painter<'a> {
        cache.nuovo_fotogramma();
        Painter { pixmap: Pixmap::new(w.max(1), h.max(1)).expect("dimensioni valide"), scale, fonts, cache, clips: Vec::new(), dx: 0.0, dy: 0.0, larger: true }
    }

    pub fn fonts(&self) -> &Fonts {
        self.fonts
    }

    /// Cambia i caratteri (per mostrare un testo in quelli di un altro tema) e dà quelli di prima.
    pub fn swap_fonts(&mut self, fonts: &'a Fonts) -> &'a Fonts {
        std::mem::replace(&mut self.fonts, fonts)
    }

    fn tf(&self) -> Transform {
        Transform::from_row(self.scale, 0.0, 0.0, self.scale, self.dx * self.scale, self.dy * self.scale)
    }

    /// Da qui in poi si disegna solo dentro il riquadro (intersecato con quello di prima).
    pub fn push_clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let s = self.scale;
        let (x0, y0) = ((x + self.dx) * s, (y + self.dy) * s);
        let mut r = Rect::from_ltrb(x0.floor(), y0.floor(), (x0 + w * s).ceil(), (y0 + h * s).ceil()).unwrap_or(Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap());
        if let Some((prev, _)) = self.clips.last() {
            r = r.intersect(prev).unwrap_or(Rect::from_xywh(0.0, 0.0, 0.0001, 0.0001).unwrap_or(*prev));
        }
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let chiave = (r.left() as i32, r.top() as i32, r.right() as i32, r.bottom() as i32, pw, ph);
        let f = self.cache.fotogramma;
        let m = match self.cache.maschere.get_mut(&chiave) {
            Some((m, u)) => {
                *u = f;
                m.clone()
            }
            None => {
                let mut m = Mask::new(pw, ph).expect("maschera");
                m.fill_path(&PathBuilder::from_rect(r), FillRule::Winding, false, Transform::identity());
                let m = std::rc::Rc::new(m);
                self.cache.maschere.insert(chiave, (m.clone(), f));
                m
            }
        };
        self.clips.push((r, m));
    }

    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn clip_rect(&self) -> (i32, i32, i32, i32) {
        match self.clips.last() {
            Some((r, _)) => (r.left() as i32, r.top() as i32, r.right() as i32, r.bottom() as i32),
            None => (0, 0, self.pixmap.width() as i32, self.pixmap.height() as i32),
        }
    }

    /// Vero se la striscia logica `y`..`y + h` (più un margine per bordi e linee) tocca la zona
    /// ritagliata: le righe di una lista fuori vista si possono saltare.
    pub fn shows_y(&self, y: f32, h: f32) -> bool {
        let (_, cy0, _, cy1) = self.clip_rect();
        let top = (y - 4.0 + self.dy) * self.scale;
        let bottom = (y + h + 4.0 + self.dy) * self.scale;
        bottom >= cy0 as f32 && top <= cy1 as f32
    }

    /// Disegna l'immagine già ridotta e arrotondata (preparata la prima volta e tenuta nella cache):
    /// a ogni fotogramma resta solo una copia con la trasparenza. Falso se non si può (troppo grande).
    #[allow(clippy::too_many_arguments)]
    fn image_pronta(&mut self, img: &Image, x: f32, y: f32, w: f32, h: f32, r: f32, fit: Fit) -> bool {
        let s = self.scale;
        let (dw, dh) = ((w * s).round(), (h * s).round());
        if dw < 1.0 || dh < 1.0 || dw * dh > 4_000_000.0 {
            return false;
        }
        let (ix, iy) = (((x + self.dx) * s).round() as i32, ((y + self.dy) * s).round() as i32);
        let (cx0, cy0, cx1, cy1) = self.clip_rect();
        if ix + dw as i32 <= cx0 || iy + dh as i32 <= cy0 || ix >= cx1 || iy >= cy1 {
            return true;
        }
        let chiave = (Arc::as_ptr(&img.pixmap) as usize, dw as u32, dh as u32, (r * s).round() as u32, matches!(fit, Fit::Cover));
        let f = self.cache.fotogramma;
        let pronta = match self.cache.immagini.get_mut(&chiave) {
            Some((_, pm, u)) => {
                *u = f;
                pm.clone()
            }
            None => {
                let Some(mut pm) = Pixmap::new(dw as u32, dh as u32) else { return false };
                let (iw, ih) = (img.w as f32, img.h as f32);
                let k = match fit {
                    Fit::Cover => (dw / iw).max(dh / ih),
                    Fit::Contain => (dw / iw).min(dh / ih),
                };
                let (ow, oh) = (iw * k, ih * k);
                let tf = Transform::from_translate((dw - ow) / 2.0, (dh - oh) / 2.0).pre_scale(k, k);
                let mut mask = Mask::new(dw as u32, dh as u32).expect("maschera");
                if let Some(path) = rrect_path(0.0, 0.0, dw, dh, r * s) {
                    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
                }
                let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() };
                pm.draw_pixmap(0, 0, img.pixmap.as_ref().as_ref(), &paint, tf, Some(&mask));
                let pm = Arc::new(pm);
                self.cache.immagini.insert(chiave, (img.pixmap.clone(), pm.clone(), f));
                pm
            }
        };
        // Solo la parte che si vede, copiata a mano (sorgente sopra, premoltiplicata): il ritaglio è un rettangolo.
        // Il ritaglio può uscire dal foglio (zone più grandi della finestra): si taglia anche al foglio.
        let (vx0, vy0) = (ix.max(cx0).max(0), iy.max(cy0).max(0));
        let (vx1, vy1) = ((ix + dw as i32).min(cx1).min(self.pixmap.width() as i32), (iy + dh as i32).min(cy1).min(self.pixmap.height() as i32));
        if vx1 > vx0 && vy1 > vy0 {
            let pw = self.pixmap.width() as usize;
            let sw = pronta.width() as usize;
            let src = pronta.data();
            let dst = self.pixmap.data_mut();
            for y in vy0..vy1 {
                let sy = (y - iy) as usize;
                let so = sy * sw * 4 + (vx0 - ix) as usize * 4;
                let n = (vx1 - vx0) as usize * 4;
                let srow = &src[so..so + n];
                let drow = &mut dst[y as usize * pw * 4 + vx0 as usize * 4..y as usize * pw * 4 + vx0 as usize * 4 + n];
                for (d, s4) in drow.chunks_exact_mut(4).zip(srow.chunks_exact(4)) {
                    let sa = s4[3] as u32;
                    if sa == 255 {
                        d.copy_from_slice(s4);
                    } else if sa > 0 {
                        let inv = 255 - sa;
                        for i in 0..4 {
                            d[i] = (s4[i] as u32 + (d[i] as u32 * inv + 127) / 255).min(255) as u8;
                        }
                    }
                }
            }
        }
        true
    }

    /// `rett_pixel` senza la parte dentro `buco`: fino a quattro rettangoli intorno.
    fn rett_buco(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, buco: Option<(f32, f32, f32, f32)>, c: Rgb, a: f32) {
        let Some((bx0, by0, bx1, by1)) = buco else { return self.rett_pixel(x0, y0, x1, y1, c, a) };
        if bx1 <= x0 || bx0 >= x1 || by1 <= y0 || by0 >= y1 {
            return self.rett_pixel(x0, y0, x1, y1, c, a);
        }
        let (bx0, by0, bx1, by1) = (bx0.max(x0), by0.max(y0), bx1.min(x1), by1.min(y1));
        self.rett_pixel(x0, y0, x1, by0, c, a);
        self.rett_pixel(x0, by1, x1, y1, c, a);
        self.rett_pixel(x0, by0, bx0, by1, c, a);
        self.rett_pixel(bx1, by0, x1, by1, c, a);
    }

    /// Un rettangolo già in pixel, tagliato al ritaglio corrente: niente maschera (il ritaglio è un rettangolo).
    /// I bordi frazionari restano sfumati; quelli interi sono netti, così i pezzi accostati non lasciano righe.
    fn rett_pixel(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Rgb, a: f32) {
        let (cx0, cy0, cx1, cy1) = self.clip_rect();
        let (x0, y0) = (x0.max(cx0 as f32), y0.max(cy0 as f32));
        let (x1, y1) = (x1.min(cx1 as f32), y1.min(cy1 as f32));
        if x1 <= x0 || y1 <= y0 || a <= 0.0 {
            return;
        }
        // A mano: l'interno a pixel interi si riempie a blocchi (pieno: parole da 32 bit; trasparente: conti
        // interi), solo i bordi frazionari si mescolano con la loro copertura. tiny-skia qui passava dalla
        // pipeline generale con la maschera, molto più lenta.
        let (pw, ph) = (self.pixmap.width() as usize, self.pixmap.height() as usize);
        let a = a.clamp(0.0, 1.0);
        let (ox0, oy0) = (x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize);
        let (ox1, oy1) = ((x1.ceil().max(0.0) as usize).min(pw), (y1.ceil().max(0.0) as usize).min(ph));
        let (jx0, jy0) = ((x0.ceil().max(0.0) as usize).min(ox1), (y0.ceil().max(0.0) as usize).min(oy1));
        let (jx1, jy1) = ((x1.floor().max(0.0) as usize).max(jx0).min(pw), (y1.floor().max(0.0) as usize).max(jy0).min(ph));
        let data = self.pixmap.data_mut();
        let mescola = |d: &mut [u8], k: f32| {
            if k <= 0.002 {
                return;
            }
            let ka = ((k * 255.0) + 0.5).min(255.0) as u32;
            let inv = 255 - ka;
            d[0] = ((c.0 as u32 * ka + d[0] as u32 * inv + 127) / 255) as u8;
            d[1] = ((c.1 as u32 * ka + d[1] as u32 * inv + 127) / 255) as u8;
            d[2] = ((c.2 as u32 * ka + d[2] as u32 * inv + 127) / 255) as u8;
            d[3] = ((255 * ka + d[3] as u32 * inv + 127) / 255) as u8;
        };
        let copertura_x = |px: usize| (x1.min(px as f32 + 1.0) - x0.max(px as f32)).clamp(0.0, 1.0);
        let parola = u32::from_ne_bytes([c.0, c.1, c.2, 255]);
        let ka = ((a * 255.0) + 0.5).min(255.0) as u32;
        let inv = 255 - ka;
        let pre = [c.0 as u32 * ka, c.1 as u32 * ka, c.2 as u32 * ka, 255 * ka];
        for py in oy0..oy1 {
            let cy = (y1.min(py as f32 + 1.0) - y0.max(py as f32)).clamp(0.0, 1.0);
            let riga = &mut data[py * pw * 4..(py + 1) * pw * 4];
            let interna = py >= jy0 && py < jy1;
            // I bordi a sinistra e a destra (e le righe di bordo, tutte).
            for px in (ox0..ox1).filter(|&px| !interna || px < jx0 || px >= jx1) {
                mescola(&mut riga[px * 4..px * 4 + 4], a * cy * copertura_x(px));
            }
            if !interna || jx1 <= jx0 {
                continue;
            }
            let pezzo = &mut riga[jx0 * 4..jx1 * 4];
            if ka >= 255 {
                // SAFETY: il pezzo è fatto di gruppi interi di 4 byte; `align_to_mut` lascia ai lati quello che non è allineato.
                let (testa, corpo, coda) = unsafe { pezzo.align_to_mut::<u32>() };
                if testa.is_empty() && coda.is_empty() {
                    corpo.fill(parola);
                } else {
                    for d in pezzo.chunks_exact_mut(4) {
                        d.copy_from_slice(&[c.0, c.1, c.2, 255]);
                    }
                }
            } else if ka > 0 {
                // SAFETY: come sopra.
                let (testa, corpo, coda) = unsafe { pezzo.align_to_mut::<u32>() };
                if testa.is_empty() && coda.is_empty() {
                    // Due canali per volta in una parola (rosso e blu, verde e alfa): la velatura a tutto
                    // schermo dei fogli costa la metà. Il /255 diventa /256 con arrotondamento (errore ≤ 1).
                    let inv2 = inv + 1;
                    let sorg = u32::from_ne_bytes([((pre[0] + 127) / 255) as u8, ((pre[1] + 127) / 255) as u8, ((pre[2] + 127) / 255) as u8, ((pre[3] + 127) / 255) as u8]);
                    for d in corpo.iter_mut() {
                        let v = *d;
                        let rb = (((v & 0x00ff_00ff) * inv2) >> 8) & 0x00ff_00ff;
                        let ga = (((v >> 8) & 0x00ff_00ff) * inv2) & 0xff00_ff00;
                        *d = sorg.wrapping_add(rb | ga);
                    }
                } else {
                    for d in pezzo.chunks_exact_mut(4) {
                        d[0] = ((pre[0] + d[0] as u32 * inv + 127) / 255) as u8;
                        d[1] = ((pre[1] + d[1] as u32 * inv + 127) / 255) as u8;
                        d[2] = ((pre[2] + d[2] as u32 * inv + 127) / 255) as u8;
                        d[3] = ((pre[3] + d[3] as u32 * inv + 127) / 255) as u8;
                    }
                }
            }
        }
    }

    /// I riquadri grandi senza passare dal tracciato: il centro e i lati sono rettangoli (veloci), solo i
    /// quattro angoli sono tracciati. Su un telefono un fondo a tutto schermo così costa un decimo.
    /// Falso se conviene il tracciato (riquadri piccoli, trasformazioni non semplici).
    #[allow(clippy::too_many_arguments)]
    fn rrect_veloce(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, a: f32) -> bool {
        self.rrect_veloce_buco(x, y, w, h, r, c, a, None)
    }

    /// Come `rrect_veloce`, lasciando vuoto il rettangolo `buco` (in pixel): per le ombre, che sotto
    /// l'oggetto non si vedono e lì costavano sei riempimenti grandi quanto l'oggetto.
    #[allow(clippy::too_many_arguments)]
    fn rrect_veloce_buco(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, a: f32, buco: Option<(f32, f32, f32, f32)>) -> bool {
        let s = self.scale;
        if w <= 0.0 || h <= 0.0 || w * h * s * s < 40_000.0 {
            return false;
        }
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let (x0, y0) = ((x + self.dx) * s, (y + self.dy) * s);
        let (x1, y1) = (x0 + w * s, y0 + h * s);
        let rd = r * s;
        let paint = color(c, a);
        if rd < 0.5 {
            self.rett_buco(x0, y0, x1, y1, buco, c, a);
            return true;
        }
        // I tagli interni sui pixel interi.
        let (xa, xb) = ((x0 + rd).ceil(), (x1 - rd).floor());
        let (ya, yb) = ((y0 + rd).ceil(), (y1 - rd).floor());
        if xb <= xa || yb <= ya {
            return false;
        }
        self.rett_buco(xa, y0, xb, y1, buco, c, a);
        self.rett_buco(x0, ya, xa, yb, buco, c, a);
        self.rett_buco(xb, ya, x1, yb, buco, c, a);
        // Gli angoli: il quarto di riquadro arrotondato dentro ciascun quadratino d'angolo.
        let k = 0.552_284_8 * rd;
        let clips = self.clips.clone();
        let angoli = [
            // (spigolo interno, punto sul lato orizzontale, controllo, punto sul lato verticale)
            ((xa, ya), (xa, y0), (x0 + rd, y0), (x0 + rd - k, y0), (x0, y0 + rd - k), (x0, y0 + rd), (x0, ya)),
            ((xb, ya), (xb, y0), (x1 - rd, y0), (x1 - rd + k, y0), (x1, y0 + rd - k), (x1, y0 + rd), (x1, ya)),
            ((xa, yb), (xa, y1), (x0 + rd, y1), (x0 + rd - k, y1), (x0, y1 - rd + k), (x0, y1 - rd), (x0, yb)),
            ((xb, yb), (xb, y1), (x1 - rd, y1), (x1 - rd + k, y1), (x1, y1 - rd + k), (x1, y1 - rd), (x1, yb)),
        ];
        for (dentro, lato_o, arco0, c1, c2, arco1, lato_v) in angoli {
            let mut pb = PathBuilder::new();
            pb.move_to(dentro.0, dentro.1);
            pb.line_to(lato_o.0, lato_o.1);
            pb.line_to(arco0.0, arco0.1);
            pb.cubic_to(c1.0, c1.1, c2.0, c2.1, arco1.0, arco1.1);
            pb.line_to(lato_v.0, lato_v.1);
            pb.close();
            if let Some(path) = pb.finish() {
                let m = maschera_per(&clips, path.bounds(), Transform::identity(), 1.0);
                self.pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), m);
            }
        }
        true
    }

    pub fn clear(&mut self, c: Rgb) {
        // Una riga, poi copie in blocco che raddoppiano: molto più veloce del riempimento pixel per pixel
        // (su un telefono 1080×2340 da 4,4 ms a meno di 1).
        let w = self.pixmap.width() as usize * 4;
        let data = self.pixmap.data_mut();
        if data.len() < w || w == 0 {
            return;
        }
        for px in data[..w].chunks_exact_mut(4) {
            px.copy_from_slice(&[c.0, c.1, c.2, 255]);
        }
        let mut fatto = w;
        while fatto < data.len() {
            let n = fatto.min(data.len() - fatto);
            data.copy_within(0..n, fatto);
            fatto += n;
        }
    }

    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb) {
        self.fill_a(x, y, w, h, c, 1.0);
    }

    pub fn fill_a(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb, a: f32) {
        // Le righe sottili (un pixel o meno) vanno sulla griglia dei pixel: nitide, e tiny-skia con
        // un rettangolo così stretto a metà pixel si ferma su un controllo interno (hairline_aa).
        let s = self.scale;
        let (mut x, mut y, mut w, mut h) = (x, y, w, h);
        if w * s < 1.5 {
            let dx = ((x + self.dx) * s).round();
            x = dx / s - self.dx;
            w = (w * s).round().max(1.0) / s;
        }
        if h * s < 1.5 {
            let dy = ((y + self.dy) * s).round();
            y = dy / s - self.dy;
            h = (h * s).round().max(1.0) / s;
        }
        // In pixel, tagliato al ritaglio rettangolare: niente maschera.
        let (x0, y0) = ((x + self.dx) * s, (y + self.dy) * s);
        if w > 0.0 && h > 0.0 {
            self.rett_pixel(x0, y0, x0 + w * s, y0 + h * s, c, a);
        }
    }

    /// Riquadro arrotondato pieno e/o bordato (il bordo sta dentro il riquadro).
    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, fill: Option<Rgb>, line: Option<(Rgb, f32)>) {
        if let Some(c) = fill {
            self.rrect_a(x, y, w, h, r, c, 1.0);
        }
        if let Some((c, lw)) = line {
            self.rrect_line(x, y, w, h, r, c, lw, 1.0, None);
        }
    }

    pub fn rrect_a(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, a: f32) {
        if self.rrect_veloce(x, y, w, h, r, c, a) {
            return;
        }
        if let Some(p) = rrect_path(x, y, w, h, r) {
            let tf = self.tf();
            let m = maschera_per(&self.clips, p.bounds(), tf, 1.0);
            self.pixmap.fill_path(&p, &color(c, a), FillRule::Winding, tf, m);
        }
    }

    /// Riquadro arrotondato con una sfumatura verticale (da `top` in alto a `bottom` in basso).
    pub fn rrect_vgrad(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, top: Rgb, bottom: Rgb) {
        let stops = vec![
            tiny_skia::GradientStop::new(0.0, tiny_skia::Color::from_rgba8(top.0, top.1, top.2, 255)),
            tiny_skia::GradientStop::new(1.0, tiny_skia::Color::from_rgba8(bottom.0, bottom.1, bottom.2, 255)),
        ];
        let Some(shader) = tiny_skia::LinearGradient::new(tiny_skia::Point::from_xy(0.0, y), tiny_skia::Point::from_xy(0.0, y + h), stops, tiny_skia::SpreadMode::Pad, Transform::identity()) else {
            return self.rrect_a(x, y, w, h, r, bottom, 1.0);
        };
        if let Some(p) = rrect_path(x, y, w, h, r) {
            let paint = Paint { shader, anti_alias: true, ..Default::default() };
            let tf = self.tf();
            let m = maschera_per(&self.clips, p.bounds(), tf, 1.0);
            self.pixmap.fill_path(&p, &paint, FillRule::Winding, tf, m);
        }
    }

    /// Solo il bordo; con `dash` tratteggiato (lunghezza tratto, spazio).
    pub fn rrect_line(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, lw: f32, a: f32, dash: Option<(f32, f32)>) {
        let half = lw / 2.0;
        if let Some(p) = rrect_path(x + half, y + half, w - lw, h - lw, (r - half).max(0.0)) {
            let mut st = Stroke { width: lw, line_join: LineJoin::Round, ..Default::default() };
            if let Some((on, off)) = dash {
                st.dash = StrokeDash::new(vec![on, off], 0.0);
            }
            let tf = self.tf();
            let m = maschera_per(&self.clips, p.bounds(), tf, 8.0);
            self.pixmap.stroke_path(&p, &color(c, a), &st, tf, m);
        }
    }

    /// Ombra morbida sotto un riquadro: strati sempre più larghi e più chiari.
    pub fn shadow(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, spread: f32, a: f32, dy: f32) {
        let steps = 6;
        // Sotto l'oggetto (che si disegna dopo, pieno) l'ombra non si vede: lì non si riempie.
        let s = self.scale;
        let rr = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let buco = ((x + self.dx + rr) * s, (y + self.dy + rr) * s, (x + self.dx + w - rr) * s, (y + self.dy + h - rr) * s);
        let buco = (buco.0.ceil(), buco.1.ceil(), buco.2.floor(), buco.3.floor());
        let buco = (buco.2 > buco.0 && buco.3 > buco.1).then_some(buco);
        for i in (1..=steps).rev() {
            let k = i as f32 / steps as f32;
            let g = spread * k;
            let (lx, ly, lw, lh, lr) = (x - g, y - g + dy, w + 2.0 * g, h + 2.0 * g, r + g);
            let al = a / steps as f32 * (1.0 - k * 0.6);
            if !self.rrect_veloce_buco(lx, ly, lw, lh, lr, (0, 0, 0), al, buco) {
                self.rrect_a(lx, ly, lw, lh, lr, (0, 0, 0), al);
            }
        }
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: Rgb, lw: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(x2, y2);
        if let Some(p) = pb.finish() {
            let st = Stroke { width: lw, line_cap: LineCap::Butt, ..Default::default() };
            let tf = self.tf();
            let m = maschera_per(&self.clips, p.bounds(), tf, 8.0);
            self.pixmap.stroke_path(&p, &color(c, 1.0), &st, tf, m);
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, fill: Option<Rgb>, line: Option<(Rgb, f32)>) {
        if r <= 0.0 {
            return;
        }
        let tf = self.tf();
        let m = self.clips.last().map(|(_, m)| &**m);
        if let (Some(c), Some(p)) = (fill, PathBuilder::from_circle(cx, cy, r)) {
            self.pixmap.fill_path(&p, &color(c, 1.0), FillRule::Winding, tf, m);
        }
        if let Some((c, lw)) = line {
            if let Some(p) = PathBuilder::from_circle(cx, cy, (r - lw / 2.0).max(0.1)) {
                let st = Stroke { width: lw, ..Default::default() };
                self.pixmap.stroke_path(&p, &color(c, 1.0), &st, tf, m);
            }
        }
    }

    /// Un cerchio pieno con trasparenza.
    pub fn disc_a(&mut self, cx: f32, cy: f32, r: f32, c: Rgb, a: f32) {
        if let Some(p) = PathBuilder::from_circle(cx, cy, r.max(0.1)) {
            self.fill_path(&p, c, a, false);
        }
    }

    /// Un poligono con gli angoli arrotondati di raggio `r` (il poligono va già rimpicciolito di `r`).
    pub fn round_poly(&mut self, pts: &[(f32, f32)], r: f32, c: Rgb, a: f32) {
        let mut pb = PathBuilder::new();
        for (i, p) in pts.iter().enumerate() {
            if i == 0 {
                pb.move_to(p.0, p.1);
            } else {
                pb.line_to(p.0, p.1);
            }
        }
        pb.close();
        let Some(path) = pb.finish() else { return };
        let tf = self.tf();
        let m = maschera_per(&self.clips, path.bounds(), tf, 1.0);
        self.pixmap.fill_path(&path, &color(c, a), FillRule::Winding, tf, m);
        if r > 0.0 {
            let st = Stroke { width: 2.0 * r, line_join: LineJoin::Round, line_cap: LineCap::Round, ..Default::default() };
            self.pixmap.stroke_path(&path, &color(c, a), &st, tf, m);
        }
    }

    /// Un poligono pieno (punti in unità logiche).
    pub fn poly(&mut self, pts: &[(f32, f32)], c: Rgb, a: f32) {
        let mut pb = PathBuilder::new();
        for (i, p) in pts.iter().enumerate() {
            if i == 0 {
                pb.move_to(p.0, p.1);
            } else {
                pb.line_to(p.0, p.1);
            }
        }
        pb.close();
        if let Some(p) = pb.finish() {
            let tf = self.tf();
            let m = maschera_per(&self.clips, p.bounds(), tf, 1.0);
            self.pixmap.fill_path(&p, &color(c, a), FillRule::Winding, tf, m);
        }
    }

    /// Un tracciato qualunque già costruito, in unità logiche.
    pub fn fill_path(&mut self, p: &tiny_skia::Path, c: Rgb, a: f32, evenodd: bool) {
        let tf = self.tf();
        let m = maschera_per(&self.clips, p.bounds(), tf, 1.0);
        self.pixmap.fill_path(p, &color(c, a), if evenodd { FillRule::EvenOdd } else { FillRule::Winding }, tf, m);
    }

    pub fn stroke_path(&mut self, p: &tiny_skia::Path, c: Rgb, lw: f32) {
        let st = Stroke { width: lw, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        let tf = self.tf();
        let m = maschera_per(&self.clips, p.bounds(), tf, 8.0);
        self.pixmap.stroke_path(p, &color(c, 1.0), &st, tf, m);
    }

    /// Un'icona del kit (24×24 a tratto) larga `size`, con l'angolo in alto a sinistra in (x, y).
    pub fn icon(&mut self, d: &str, x: f32, y: f32, size: f32, c: Rgb) {
        let k = size / 24.0;
        let Some(path) = super::svgpath::parse(d) else { return };
        let st = Stroke { width: 1.8, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        let tf = self.tf().pre_translate(x, y).pre_scale(k, k);
        let m = maschera_per(&self.clips, path.bounds(), tf, 8.0);
        self.pixmap.stroke_path(&path, &color(c, 1.0), &st, tf, m);
    }

    /// Come `icon`, girata di `deg` gradi attorno al suo centro (le icone che si muovono sotto il mouse).
    pub fn icon_turned(&mut self, d: &str, x: f32, y: f32, size: f32, c: Rgb, deg: f32) {
        let k = size / 24.0;
        let Some(path) = super::svgpath::parse(d) else { return };
        let st = Stroke { width: 1.8, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        let h = size / 2.0;
        let tf = self.tf().pre_translate(x + h, y + h).pre_concat(Transform::from_rotate(deg)).pre_translate(-h, -h).pre_scale(k, k);
        let m = maschera_per(&self.clips, path.bounds(), tf, 8.0);
        self.pixmap.stroke_path(&path, &color(c, 1.0), &st, tf, m);
    }

    /// Come `icon`, ma la forma è piena (le parti piene delle icone dei formati).
    pub fn icon_fill(&mut self, d: &str, x: f32, y: f32, size: f32, c: Rgb) {
        let k = size / 24.0;
        let Some(path) = super::svgpath::parse(d) else { return };
        let tf = self.tf().pre_translate(x, y).pre_scale(k, k);
        let m = maschera_per(&self.clips, path.bounds(), tf, 1.0);
        self.pixmap.fill_path(&path, &color(c, 0.9), FillRule::Winding, tf, m);
    }

    /// Il simbolo Spicchio, quadrato di lato `size`.
    pub fn symbol(&mut self, x: f32, y: f32, size: f32) {
        self.brand(x, y, size / 512.0, 2, None);
    }

    /// Lo stemma del Comune, quadrato di lato `size`.
    pub fn stemma(&mut self, quale: crate::edizione::Stemma, x: f32, y: f32, size: f32) {
        if let Some(img) = stemma(quale) {
            self.image(img, x, y, size, size, 0.0, Fit::Contain);
        }
    }

    /// Il simbolo tutto di un colore (per i fondi scuri o le icone piccole).
    pub fn symbol_mono(&mut self, x: f32, y: f32, size: f32, c: Rgb) {
        self.brand(x, y, size / 512.0, 2, Some(c));
    }

    /// Simbolo e scritta «Cedro», alti `h`; con `word` si cambia il colore della scritta (fondi scuri).
    pub fn lockup(&mut self, x: f32, y: f32, h: f32, word: Option<Rgb>) {
        let k = h / 512.0;
        self.brand(x, y, k, 2, None);
        if let Some(&(c, contours)) = super::assets::LOCKUP.get(2) {
            let col = word.unwrap_or(((c >> 16) as u8, (c >> 8) as u8, c as u8));
            self.contours(x, y, k, contours, col);
        }
    }

    /// Larghezza della scritta con il simbolo, alta `h`.
    pub fn lockup_width(h: f32) -> f32 {
        h * 1130.0 / 512.0
    }

    fn brand(&mut self, x: f32, y: f32, k: f32, parts: usize, mono: Option<Rgb>) {
        for &(c, contours) in super::assets::LOCKUP.iter().take(parts) {
            let col = mono.unwrap_or(((c >> 16) as u8, (c >> 8) as u8, c as u8));
            self.contours(x, y, k, contours, col);
        }
    }

    fn contours(&mut self, x: f32, y: f32, k: f32, contours: &[&[f32]], c: Rgb) {
        let mut pb = PathBuilder::new();
        for pts in contours {
            for (i, p) in pts.chunks_exact(2).enumerate() {
                let (px, py) = (x + p[0] * k, y + p[1] * k);
                if i == 0 {
                    pb.move_to(px, py);
                } else {
                    pb.line_to(px, py);
                }
            }
            pb.close();
        }
        if let Some(p) = pb.finish() {
            self.fill_path(&p, c, 1.0, true);
        }
    }

    /// Un'immagine nel riquadro, con gli angoli arrotondati se `r` > 0.
    pub fn image(&mut self, img: &Image, x: f32, y: f32, w: f32, h: f32, r: f32, fit: Fit) {
        if self.image_pronta(img, x, y, w, h, r, fit) {
            return;
        }
        let (iw, ih) = (img.w as f32, img.h as f32);
        let k = match fit {
            Fit::Cover => (w / iw).max(h / ih),
            Fit::Contain => (w / iw).min(h / ih),
        };
        let (dw, dh) = (iw * k, ih * k);
        let (ox, oy) = (x + (w - dw) / 2.0, y + (h - dh) / 2.0);
        // La maschera: il riquadro (arrotondato) dentro la zona già ritagliata. Fuori dal riquadro
        // resta a zero, quindi il conto con il ritaglio si fa solo lì: il costo segue l'immagine, non la finestra.
        let Some(p) = rrect_path(x, y, w, h, r) else { return };
        let (pw, ph) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        let Some(b) = p.bounds().transform(self.tf()) else { return };
        let bx0 = (b.left().floor() as i32 - 1).max(0);
        let by0 = (b.top().floor() as i32 - 1).max(0);
        let bx1 = (b.right().ceil() as i32 + 1).min(pw);
        let by1 = (b.bottom().ceil() as i32 + 1).min(ph);
        // Tutto fuori dalla zona ritagliata: non si vedrebbe niente.
        let (cx0, cy0, cx1, cy1) = self.clip_rect();
        if bx1.min(cx1) <= bx0.max(cx0) || by1.min(cy1) <= by0.max(cy0) {
            return;
        }
        let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height()).expect("maschera");
        mask.fill_path(&p, FillRule::Winding, true, self.tf());
        if let Some((_, clip)) = self.clips.last() {
            let (m, c) = (mask.data_mut(), clip.data());
            for row in by0..by1 {
                let (a, z) = ((row * pw + bx0) as usize, (row * pw + bx1) as usize);
                for (m, c) in m[a..z].iter_mut().zip(&c[a..z]) {
                    *m = ((*m as u16 * *c as u16) / 255) as u8;
                }
            }
        }
        let tf = self.tf().pre_translate(ox, oy).pre_scale(k, k);
        let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() };
        self.pixmap.draw_pixmap(0, 0, img.pixmap.as_ref().as_ref(), &paint, tf, Some(&mask));
    }

    // ---- Testo ----

    pub fn measure(&self, s: &str, size: f32, weight: Weight) -> f32 {
        self.measure_raw(&crate::i18n::tr(s), size, weight)
    }

    /// La misura vera di un testo di progetto `size`.
    fn ts(&self, size: f32) -> f32 {
        if self.larger { text_size(size) } else { size }
    }

    fn measure_raw(&self, s: &str, size: f32, weight: Weight) -> f32 {
        let size = self.ts(size);
        self.fonts.measure(s, size * self.scale, weight) / self.scale
    }

    /// Il testo accorciato con «…» perché stia in `w`.
    pub fn ellipsize(&self, s: &str, size: f32, weight: Weight, w: f32) -> String {
        self.ellipsize_raw(&crate::i18n::tr(s), size, weight, w)
    }

    fn ellipsize_raw(&self, s: &str, size: f32, weight: Weight, w: f32) -> String {
        if self.measure_raw(s, size, weight) <= w {
            return s.to_string();
        }
        let chars: Vec<char> = s.chars().collect();
        let (mut lo, mut hi) = (0, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let t: String = chars[..mid].iter().collect::<String>() + "…";
            if self.measure_raw(&t, size, weight) <= w {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"
    }

    /// Una riga di testo centrata in verticale nel riquadro; troppo lunga, finisce con «…».
    pub fn text(&mut self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, weight: Weight, c: Rgb, align: Align) {
        let s = self.ellipsize_raw(&crate::i18n::tr(s), size, weight, w.max(1.0));
        let tw = self.measure_raw(&s, size, weight);
        let (asc, desc) = self.fonts.extents(self.ts(size), weight);
        let base = y + (h - (asc + desc)) / 2.0 + asc;
        let lx = match align {
            Align::Left => x,
            Align::Center => x + (w - tw) / 2.0,
            Align::Right => x + w - tw,
        };
        self.text_at(&s, lx, base, size, weight, c);
    }

    /// Testo che va a capo nella larghezza `w`, dall'alto; restituisce l'altezza usata.
    /// Con `max_lines` > 0 l'ultima riga finisce con «…».
    pub fn text_wrap(&mut self, s: &str, x: f32, y: f32, w: f32, size: f32, weight: Weight, c: Rgb, line_h: f32, max_lines: usize) -> f32 {
        let lines = self.wrap_lines(s, size, weight, w, max_lines);
        let (asc, _) = self.fonts.extents(self.ts(size), weight);
        let line_h = line_h * self.ts(size) / size;
        for (i, line) in lines.iter().enumerate() {
            self.text_at(line, x, y + asc + (line_h - self.ts(size) * 1.2) / 2.0 + i as f32 * line_h, size, weight, c);
        }
        lines.len() as f32 * line_h
    }

    /// L'altezza che avrà `text_wrap` con gli stessi valori (anche con i caratteri grandi), per disegnare
    /// il riquadro prima del testo.
    pub fn wrap_height(&self, s: &str, size: f32, weight: Weight, w: f32, line_h: f32, max_lines: usize) -> f32 {
        self.wrap_lines(s, size, weight, w, max_lines).len() as f32 * line_h * self.ts(size) / size
    }

    /// Le righe in cui va a capo un testo (`\n` forza l'a capo).
    pub fn wrap_lines(&self, s: &str, size: f32, weight: Weight, w: f32, max_lines: usize) -> Vec<String> {
        let s = &*crate::i18n::tr(s);
        let mut lines: Vec<String> = Vec::new();
        for para in s.split('\n') {
            let mut cur = String::new();
            for word in para.split(' ') {
                let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if !cur.is_empty() && self.measure_raw(&next, size, weight) > w {
                    lines.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = next;
                }
            }
            lines.push(cur);
        }
        if max_lines > 0 && lines.len() > max_lines {
            let rest = lines[max_lines - 1..].join(" ");
            lines.truncate(max_lines - 1);
            lines.push(self.ellipsize_raw(&rest, size, weight, w));
        }
        lines
    }

    /// Testo con la linea di base in `base`, a partire da `x` (unità logiche).
    pub fn text_at(&mut self, s: &str, x: f32, base: f32, size: f32, weight: Weight, c: Rgb) {
        let size = self.ts(size);
        let s_px = size * self.scale;
        let mut pen = (x + self.dx) * self.scale;
        let base_px = ((base + self.dy) * self.scale).round();
        let (cx0, cy0, cx1, cy1) = self.clip_rect();
        let (pw, ph) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        let size_key = (s_px * 4.0).round() as u32;
        for ch in s.chars() {
            let (face, gid) = self.fonts.glyph(weight, ch);
            let adv = self.fonts.advance(face, gid, s_px);
            let ix = pen.floor();
            let sub = ((pen - ix) * 4.0).floor() as u8;
            let key: GlyphKey = (Arc::as_ptr(&face.data) as *const () as usize ^ face.index as usize, gid.to_u32(), size_key, sub);
            let glyph = self.cache.glyphs.entry(key).or_insert_with(|| raster_glyph(face, gid, s_px, sub as f32 / 4.0));
            if let Some(g) = glyph {
                let ox = ix as i32 + g.left;
                let oy = base_px as i32 + g.top;
                let data = self.pixmap.data_mut();
                for gy in 0..g.h {
                    let py = oy + gy;
                    if py < cy0.max(0) || py >= cy1.min(ph) {
                        continue;
                    }
                    for gx in 0..g.w {
                        let px = ox + gx;
                        if px < cx0.max(0) || px >= cx1.min(pw) {
                            continue;
                        }
                        let cov = g.mask[(gy * g.w + gx) as usize] as u32;
                        if cov == 0 {
                            continue;
                        }
                        let i = ((py * pw + px) * 4) as usize;
                        let inv = 255 - cov;
                        data[i] = ((c.0 as u32 * cov + data[i] as u32 * inv) / 255) as u8;
                        data[i + 1] = ((c.1 as u32 * cov + data[i + 1] as u32 * inv) / 255) as u8;
                        data[i + 2] = ((c.2 as u32 * cov + data[i + 2] as u32 * inv) / 255) as u8;
                        data[i + 3] = (cov + data[i + 3] as u32 * inv / 255) as u8;
                    }
                }
            }
            pen += adv;
        }
    }

    /// Salva l'immagine in PNG (per le fotografie delle prove).
    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        let mut rgba = self.pixmap.data().to_vec();
        for px in rgba.chunks_exact_mut(4) {
            let a = px[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut px[..3] {
                    *c = (*c as u32 * 255 / a).min(255) as u8;
                }
            }
        }
        image::save_buffer(path, &rgba, self.pixmap.width(), self.pixmap.height(), image::ColorType::Rgba8).map_err(|e| e.to_string())
    }

    /// I pixel finiti, BGRA premoltiplicati come li vuole Windows. Consuma il pittore: niente copia del fotogramma.
    pub fn bgra(self) -> Vec<u8> {
        let mut out = self.pixmap.take();
        for p in out.chunks_exact_mut(4) {
            p.swap(0, 2);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::THEMES;

    #[test]
    fn text_is_two_points_bigger_and_titles_one() {
        assert_eq!(text_size(13.0), 15.0);
        assert_eq!(text_size(21.0), 23.0);
        assert_eq!(text_size(34.0), 35.0);
    }

    #[test]
    fn every_theme_has_a_font_that_covers_italian() {
        for theme in THEMES {
            let fonts = Fonts::for_theme(theme);
            for weight in [Weight::Regular, Weight::Semibold, Weight::Bold] {
                let accented = fonts.measure("àèéìòù « » € · … → × –", 18.0, weight);
                assert!(accented > 100.0, "{theme:?} {weight:?}");
                // Se mancasse un segno si userebbe il carattere di riserva: le larghezze tonde lo tradirebbero.
                let (face, g) = fonts.glyph(weight, 'è');
                assert!(face.font().charmap().map('è') == Some(g), "{theme:?} senza «è»");
            }
        }
    }

    #[test]
    fn draws_text_shapes_icons_and_logo() {
        let fonts = Fonts::system();
        let mut cache = GlyphCache::new();
        let mut p = Painter::new(400, 160, 1.0, &fonts, &mut cache);
        p.clear((255, 255, 255));
        p.rrect(10.0, 10.0, 180.0, 44.0, 8.0, Some((0, 94, 142)), None);
        p.text("Converti 3 file", 10.0, 10.0, 180.0, 44.0, 18.0, Weight::Semibold, (255, 255, 255), Align::Center);
        p.icon(crate::ui::assets::FOLDER, 210.0, 20.0, 24.0, (23, 43, 58));
        p.lockup(250.0, 10.0, 40.0, None);
        p.text("Un testo molto lungo che va accorciato", 10.0, 70.0, 150.0, 24.0, 16.0, Weight::Regular, (0, 0, 0), Align::Left);
        let dark = p.pixmap.data().chunks_exact(4).filter(|px| px[0] < 128 && px[1] < 128).count();
        assert!(dark > 500, "si è disegnato poco: {dark}");
        if let Some(dir) = std::env::var_os("CEDRO_UI_SHOTS") {
            p.save_png(&std::path::Path::new(&dir).join("gfx.png")).unwrap();
        }
    }

    /// Il disegno di prima: maschera grande quanto tutto il fotogramma, moltiplicata per il ritaglio.
    fn image_full_mask(p: &mut Painter, img: &Image, x: f32, y: f32, w: f32, h: f32, r: f32, fit: Fit) {
        let (iw, ih) = (img.w as f32, img.h as f32);
        let k = match fit {
            Fit::Cover => (w / iw).max(h / ih),
            Fit::Contain => (w / iw).min(h / ih),
        };
        let (ox, oy) = (x + (w - iw * k) / 2.0, y + (h - ih * k) / 2.0);
        let mut mask = Mask::new(p.pixmap.width(), p.pixmap.height()).unwrap();
        if let Some(path) = rrect_path(x, y, w, h, r) {
            mask.fill_path(&path, FillRule::Winding, true, p.tf());
        }
        if let Some((_, clip)) = p.clips.last() {
            for (m, c) in mask.data_mut().iter_mut().zip(clip.data()) {
                *m = ((*m as u16 * *c as u16) / 255) as u8;
            }
        }
        let tf = p.tf().pre_translate(ox, oy).pre_scale(k, k);
        let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() };
        p.pixmap.draw_pixmap(0, 0, img.pixmap.as_ref().as_ref(), &paint, tf, Some(&mask));
    }

    #[test]
    fn image_matches_full_frame_mask() {
        let fonts = Fonts::system();
        let rgba: Vec<u8> = (0..64 * 40).flat_map(|i| [(i % 64 * 4) as u8, (i / 64 * 6) as u8, 120, if i % 7 == 0 { 200 } else { 255 }]).collect();
        let img = Image::from_rgba(64, 40, &rgba).unwrap();
        let cases = [(10.0, 15.0, 80.0, 60.0, 6.0, Fit::Cover), (-20.0, 40.0, 90.0, 50.0, 4.0, Fit::Contain), (30.0, -30.0, 70.0, 70.0, 0.0, Fit::Cover), (300.0, 10.0, 40.0, 40.0, 5.0, Fit::Cover)];
        for clip in [None, Some((20.0, 25.0, 100.0, 40.0))] {
            for (x, y, w, h, r, fit) in cases {
                let (mut c1, mut c2) = (GlyphCache::new(), GlyphCache::new());
                let mut a = Painter::new(200, 130, 1.5, &fonts, &mut c1);
                let mut b = Painter::new(200, 130, 1.5, &fonts, &mut c2);
                for p in [&mut a, &mut b] {
                    p.clear((240, 240, 240));
                    if let Some((cx, cy, cw, ch)) = clip {
                        p.push_clip(cx, cy, cw, ch);
                    }
                    p.dy = -7.3;
                }
                a.image(&img, x, y, w, h, r, fit);
                image_full_mask(&mut b, &img, x, y, w, h, r, fit);
                // L'immagine ora si prepara una volta sola, già ridotta e arrotondata, e si posa sui pixel
                // interi: rispetto al disegno con la maschera a tutto schermo cambiano solo i bordi di
                // mezzo pixel. Stessa immagine, stesso posto, stesso ritaglio: differenza media minima.
                let diff: u64 = a.pixmap.data().iter().zip(b.pixmap.data()).map(|(p, q)| (*p as i32 - *q as i32).unsigned_abs() as u64).sum();
                let media = diff as f64 / a.pixmap.data().len() as f64;
                assert!(media < 3.0, "immagine diversa: {clip:?} {x} {y} (differenza media {media:.2})");
            }
        }
    }

    #[test]
    fn immagine_con_ritaglio_oltre_il_foglio() {
        // Un ritaglio più grande della finestra (lista più alta della finestra piccola) e un'immagine che
        // esce in basso e a destra: si disegna la parte dentro il foglio, senza uscire dai suoi pixel.
        let fonts = Fonts::system();
        let mut cache = GlyphCache::new();
        let rgba = vec![200u8; 32 * 32 * 4];
        let img = Image::from_rgba(32, 32, &rgba).unwrap();
        let mut p = Painter::new(100, 100, 1.0, &fonts, &mut cache);
        p.clear((255, 255, 255));
        p.push_clip(0.0, 0.0, 300.0, 300.0);
        p.image(&img, 80.0, 80.0, 40.0, 40.0, 0.0, Fit::Cover);
        p.image(&img, 70.0, 10.0, 60.0, 30.0, 4.0, Fit::Contain);
        assert_ne!(&p.pixmap.data()[(99 * 100 + 99) * 4..][..3], &[255, 255, 255]);
    }

    #[test]
    fn le_immagini_preparate_non_crescono_senza_limite() {
        // Venti misure diverse di una pagina grande (finestra che si ridimensiona): 4 MB l'una.
        let mut cache = GlyphCache::new();
        let orig = Arc::new(Pixmap::new(1, 1).unwrap());
        for i in 0..40u32 {
            cache.nuovo_fotogramma();
            let f = cache.fotogramma;
            cache.immagini.insert((0, 1000 + i, 1000, 0, false), (orig.clone(), Arc::new(Pixmap::new(1000 + i, 1000).unwrap()), f));
        }
        cache.nuovo_fotogramma();
        let peso: usize = cache.immagini.values().map(|(_, pm, _)| pm.data().len()).sum();
        assert!(peso <= IMMAGINI_MAX_BYTE, "{} MB tenuti", peso >> 20);
        assert!(!cache.immagini.is_empty(), "quella dell'ultimo fotogramma resta");
    }

    #[test]
    fn riquadri_veloci_come_il_tracciato() {
        // Il riquadro grande fatto a rettangoli e angoli deve venire come il tracciato intero, anche
        // trasparente, con un ritaglio e con l'ombra (che sotto l'oggetto non si disegna).
        let fonts = Fonts::system();
        for (clip, alfa) in [(None, 1.0), (Some((15.0, 12.0, 160.0, 90.0)), 0.45)] {
            let (mut c1, mut c2) = (GlyphCache::new(), GlyphCache::new());
            let mut a = Painter::new(400, 260, 2.0, &fonts, &mut c1);
            let mut b = Painter::new(400, 260, 2.0, &fonts, &mut c2);
            for p in [&mut a, &mut b] {
                p.clear((240, 242, 245));
                if let Some((x, y, w, h)) = clip {
                    p.push_clip(x, y, w, h);
                }
                p.dy = -0.3;
            }
            a.rrect_a(10.3, 8.6, 170.2, 110.7, 18.0, (20, 90, 140), alfa);
            if let Some(path) = rrect_path(10.3, 8.6, 170.2, 110.7, 18.0) {
                let tf = b.tf();
                let m = b.clips.last().map(|(_, m)| m.clone());
                b.pixmap.fill_path(&path, &color((20, 90, 140), alfa), FillRule::Winding, tf, m.as_deref());
            }
            let peggiore = a.pixmap.data().iter().zip(b.pixmap.data()).map(|(p, q)| (*p as i32 - *q as i32).unsigned_abs()).max().unwrap_or(0);
            assert!(peggiore <= 40, "riquadro diverso: {clip:?} (scarto {peggiore})");
            let diff: u64 = a.pixmap.data().iter().zip(b.pixmap.data()).map(|(p, q)| (*p as i32 - *q as i32).unsigned_abs() as u64).sum();
            assert!((diff as f64 / a.pixmap.data().len() as f64) < 0.5, "riquadro diverso: {clip:?}");
        }
    }

    #[test]
    fn shows_y_follows_clip_and_scroll() {
        let fonts = Fonts::system();
        let mut cache = GlyphCache::new();
        let mut p = Painter::new(200, 200, 2.0, &fonts, &mut cache);
        p.push_clip(0.0, 20.0, 100.0, 40.0);
        assert!(p.shows_y(30.0, 10.0));
        assert!(p.shows_y(0.0, 20.0));
        assert!(!p.shows_y(80.0, 10.0));
        assert!(!p.shows_y(-20.0, 10.0));
        p.dy -= 70.0;
        assert!(p.shows_y(100.0, 10.0));
        assert!(!p.shows_y(30.0, 10.0));
    }
}

