//! Il disegno dell'interfaccia: un pittore in Rust puro (tiny-skia per le forme, skrifa per i
//! caratteri) che riempie un'immagine RGBA. Windows e Linux la copiano sullo schermo così com'è:
//! le finestre sono uguali sui due sistemi e si possono fotografare anche senza schermo.
//! Le misure sono in unità logiche (1 = un pixel a 100%); `scale` le porta ai pixel veri.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

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
    data: Arc<Vec<u8>>,
    index: u32,
}

impl Face {
    pub fn new(data: Vec<u8>, index: u32) -> Option<Face> {
        FontRef::from_index(&data, index).ok()?;
        Some(Face { data: Arc::new(data), index })
    }

    fn font(&self) -> FontRef<'_> {
        FontRef::from_index(&self.data, self.index).expect("controllato in new")
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
            for (w, name) in [(Weight::Regular, "regular.ttf"), (Weight::Semibold, "semibold.ttf"), (Weight::Bold, "bold.ttf"), (Weight::Serif, "serif.ttf"), (Weight::SerifBold, "serif-bold.ttf")] {
                if let Some(f) = read_face(&dir.join(name), 0) {
                    faces.insert(w, f);
                }
            }
        }
        #[cfg(windows)]
        {
            let fonts = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts");
            for (w, name) in [(Weight::Regular, "segoeui.ttf"), (Weight::Semibold, "seguisb.ttf"), (Weight::Bold, "segoeuib.ttf"), (Weight::Serif, "georgia.ttf"), (Weight::SerifBold, "georgiab.ttf")] {
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
            ] {
                if !faces.contains_key(&w) {
                    if let Some(f) = fc_match(pattern) {
                        faces.insert(w, f);
                    }
                }
            }
        }
        Fonts::with(faces)
    }

    pub fn with(mut faces: HashMap<Weight, Face>) -> Fonts {
        let regular = Face::new(TITILLIUM.to_vec(), 0).expect("Titillium incorporato");
        let bold = Face::new(TITILLIUM_BOLD.to_vec(), 0).expect("Titillium incorporato");
        faces.entry(Weight::Regular).or_insert_with(|| regular.clone());
        let base = faces[&Weight::Regular].clone();
        faces.entry(Weight::Bold).or_insert_with(|| if Arc::ptr_eq(&base.data, &regular.data) { bold.clone() } else { base.clone() });
        let strong = faces[&Weight::Bold].clone();
        faces.entry(Weight::Semibold).or_insert_with(|| strong.clone());
        faces.entry(Weight::Serif).or_insert_with(|| base.clone());
        let serif = faces[&Weight::Serif].clone();
        faces.entry(Weight::SerifBold).or_insert_with(|| serif.clone());
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
        FONTS.get_or_init(|| {
            let system = Fonts::shared();
            let regular = Face::new(TITILLIUM.to_vec(), 0).expect("Titillium incorporato");
            let bold = Face::new(TITILLIUM_BOLD.to_vec(), 0).expect("Titillium incorporato");
            let mut faces = HashMap::new();
            faces.insert(Weight::Regular, regular);
            faces.insert(Weight::Semibold, bold.clone());
            faces.insert(Weight::Bold, bold);
            for w in [Weight::Serif, Weight::SerifBold] {
                faces.insert(w, system.faces[&w].clone());
            }
            Fonts::with(faces)
        })
    }

    /// I caratteri da usare con o senza il tema Bondeno.
    pub fn for_theme(bondeno: bool) -> &'static Fonts {
        if bondeno { Fonts::bondeno() } else { Fonts::shared() }
    }

    /// I caratteri del tema in uso adesso (`edizione::bondeno`).
    pub fn current() -> &'static Fonts {
        Fonts::for_theme(crate::edizione::bondeno())
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
    clips: Vec<(Rect, Mask)>,
    /// Spostamento corrente delle coordinate logiche (per le zone che scorrono).
    pub dx: f32,
    pub dy: f32,
}

/// I glifi già disegnati: si tiene fra un fotogramma e l'altro.
#[derive(Default)]
pub struct GlyphCache {
    glyphs: HashMap<GlyphKey, Option<Glyph>>,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self::default()
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
    /// Un'immagine nuova di `w`×`h` pixel, con `scale` pixel per unità logica.
    pub fn new(w: u32, h: u32, scale: f32, fonts: &'a Fonts, cache: &'a mut GlyphCache) -> Painter<'a> {
        Painter { pixmap: Pixmap::new(w.max(1), h.max(1)).expect("dimensioni valide"), scale, fonts, cache, clips: Vec::new(), dx: 0.0, dy: 0.0 }
    }

    pub fn fonts(&self) -> &Fonts {
        self.fonts
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
        let mut m = Mask::new(self.pixmap.width(), self.pixmap.height()).expect("maschera");
        m.fill_path(&PathBuilder::from_rect(r), FillRule::Winding, false, Transform::identity());
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

    pub fn clear(&mut self, c: Rgb) {
        self.pixmap.fill(tiny_skia::Color::from_rgba8(c.0, c.1, c.2, 255));
    }

    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb) {
        self.fill_a(x, y, w, h, c, 1.0);
    }

    pub fn fill_a(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb, a: f32) {
        if let Some(r) = Rect::from_xywh(x, y, w, h) {
            let tf = self.tf();
            let m = self.clips.last().map(|(_, m)| m);
            self.pixmap.fill_rect(r, &color(c, a), tf, m);
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
        if let Some(p) = rrect_path(x, y, w, h, r) {
            let tf = self.tf();
            let m = self.clips.last().map(|(_, m)| m);
            self.pixmap.fill_path(&p, &color(c, a), FillRule::Winding, tf, m);
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
            let m = self.clips.last().map(|(_, m)| m);
            self.pixmap.stroke_path(&p, &color(c, a), &st, tf, m);
        }
    }

    /// Ombra morbida sotto un riquadro: strati sempre più larghi e più chiari.
    pub fn shadow(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, spread: f32, a: f32, dy: f32) {
        let steps = 6;
        for i in (1..=steps).rev() {
            let k = i as f32 / steps as f32;
            let g = spread * k;
            self.rrect_a(x - g, y - g + dy, w + 2.0 * g, h + 2.0 * g, r + g, (0, 0, 0), a / steps as f32 * (1.0 - k * 0.6));
        }
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: Rgb, lw: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(x2, y2);
        if let Some(p) = pb.finish() {
            let st = Stroke { width: lw, line_cap: LineCap::Butt, ..Default::default() };
            let tf = self.tf();
            let m = self.clips.last().map(|(_, m)| m);
            self.pixmap.stroke_path(&p, &color(c, 1.0), &st, tf, m);
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, fill: Option<Rgb>, line: Option<(Rgb, f32)>) {
        if r <= 0.0 {
            return;
        }
        let tf = self.tf();
        let m = self.clips.last().map(|(_, m)| m);
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
        let m = self.clips.last().map(|(_, m)| m);
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
            let m = self.clips.last().map(|(_, m)| m);
            self.pixmap.fill_path(&p, &color(c, a), FillRule::Winding, tf, m);
        }
    }

    /// Un tracciato qualunque già costruito, in unità logiche.
    pub fn fill_path(&mut self, p: &tiny_skia::Path, c: Rgb, a: f32, evenodd: bool) {
        let tf = self.tf();
        let m = self.clips.last().map(|(_, m)| m);
        self.pixmap.fill_path(p, &color(c, a), if evenodd { FillRule::EvenOdd } else { FillRule::Winding }, tf, m);
    }

    pub fn stroke_path(&mut self, p: &tiny_skia::Path, c: Rgb, lw: f32) {
        let st = Stroke { width: lw, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        let tf = self.tf();
        let m = self.clips.last().map(|(_, m)| m);
        self.pixmap.stroke_path(p, &color(c, 1.0), &st, tf, m);
    }

    /// Un'icona del kit (24×24 a tratto) larga `size`, con l'angolo in alto a sinistra in (x, y).
    pub fn icon(&mut self, d: &str, x: f32, y: f32, size: f32, c: Rgb) {
        let k = size / 24.0;
        let Some(path) = super::svgpath::parse(d) else { return };
        let st = Stroke { width: 1.8, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        let tf = self.tf().pre_translate(x, y).pre_scale(k, k);
        let m = self.clips.last().map(|(_, m)| m);
        self.pixmap.stroke_path(&path, &color(c, 1.0), &st, tf, m);
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
        let (iw, ih) = (img.w as f32, img.h as f32);
        let k = match fit {
            Fit::Cover => (w / iw).max(h / ih),
            Fit::Contain => (w / iw).min(h / ih),
        };
        let (dw, dh) = (iw * k, ih * k);
        let (ox, oy) = (x + (w - dw) / 2.0, y + (h - dh) / 2.0);
        // La maschera: il riquadro (arrotondato) dentro la zona già ritagliata.
        let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height()).expect("maschera");
        if let Some(p) = rrect_path(x, y, w, h, r) {
            mask.fill_path(&p, FillRule::Winding, true, self.tf());
        }
        if let Some((_, clip)) = self.clips.last() {
            for (m, c) in mask.data_mut().iter_mut().zip(clip.data()) {
                *m = ((*m as u16 * *c as u16) / 255) as u8;
            }
        }
        let tf = self.tf().pre_translate(ox, oy).pre_scale(k, k);
        let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() };
        self.pixmap.draw_pixmap(0, 0, img.pixmap.as_ref().as_ref(), &paint, tf, Some(&mask));
    }

    // ---- Testo ----

    pub fn measure(&self, s: &str, size: f32, weight: Weight) -> f32 {
        self.fonts.measure(s, size * self.scale, weight) / self.scale
    }

    /// Il testo accorciato con «…» perché stia in `w`.
    pub fn ellipsize(&self, s: &str, size: f32, weight: Weight, w: f32) -> String {
        if self.measure(s, size, weight) <= w {
            return s.to_string();
        }
        let chars: Vec<char> = s.chars().collect();
        let (mut lo, mut hi) = (0, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let t: String = chars[..mid].iter().collect::<String>() + "…";
            if self.measure(&t, size, weight) <= w {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"
    }

    /// Una riga di testo centrata in verticale nel riquadro; troppo lunga, finisce con «…».
    pub fn text(&mut self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, weight: Weight, c: Rgb, align: Align) {
        let s = self.ellipsize(s, size, weight, w.max(1.0));
        let tw = self.measure(&s, size, weight);
        let (asc, desc) = self.fonts.extents(size, weight);
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
        let (asc, _) = self.fonts.extents(size, weight);
        for (i, line) in lines.iter().enumerate() {
            self.text_at(line, x, y + asc + (line_h - size * 1.2) / 2.0 + i as f32 * line_h, size, weight, c);
        }
        lines.len() as f32 * line_h
    }

    /// Le righe in cui va a capo un testo (`\n` forza l'a capo).
    pub fn wrap_lines(&self, s: &str, size: f32, weight: Weight, w: f32, max_lines: usize) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        for para in s.split('\n') {
            let mut cur = String::new();
            for word in para.split(' ') {
                let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if !cur.is_empty() && self.measure(&next, size, weight) > w {
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
            lines.push(self.ellipsize(&rest, size, weight, w));
        }
        lines
    }

    /// Testo con la linea di base in `base`, a partire da `x` (unità logiche).
    pub fn text_at(&mut self, s: &str, x: f32, base: f32, size: f32, weight: Weight, c: Rgb) {
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
            let key: GlyphKey = (Arc::as_ptr(&face.data) as usize ^ face.index as usize, gid.to_u32(), size_key, sub);
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

    /// I pixel finiti, BGRA premoltiplicati come li vuole Windows.
    pub fn bgra(&self) -> Vec<u8> {
        let mut out = self.pixmap.data().to_vec();
        for p in out.chunks_exact_mut(4) {
            p.swap(0, 2);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
