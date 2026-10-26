//! La Guida di Cedro: sei passi animati (ruota, Conversioni, YouTube, trascrizione, Sistema, piattaforme).
//! Qui c'è tutto quello che si vede e come si muove; Windows (GDI) e Linux (cairo) prestano solo le
//! primitive di disegno tramite `Canvas`, così le due finestre sono identiche.
//! Le coordinate sono in pixel logici: la finestra è `W` x `H`.

use crate::logo;
use crate::wheelgeom::{slice_bounds, R0, R1, WHEEL};

pub type Rgb = (u8, u8, u8);

pub const W: f32 = 780.0;
pub const H: f32 = 440.0;

const BLUE: Rgb = (0x00, 0x5E, 0x8E);
const BLUE2: Rgb = (0x00, 0x66, 0xCC);
const INK: Rgb = (0x19, 0x19, 0x19);
const GREY: Rgb = (0x45, 0x5A, 0x64);
const LINE: Rgb = (0xD3, 0xDD, 0xE6);
const STAGE: Rgb = (0x00, 0x22, 0x4A);
const LEMON: Rgb = (0xF2, 0xC2, 0x30);
const TINT: Rgb = (0xCF, 0xE3, 0xF2);
const WHITE: Rgb = (0xFF, 0xFF, 0xFF);
const DARK: Rgb = (0x0F, 0x17, 0x1D);
const REAL_ON_DARK: Rgb = (0x93, 0xA4, 0xB0);

const STAGE_X: f32 = 400.0;
const STAGE_Y: f32 = 56.0;
const STAGE_W: f32 = 364.0;
const STAGE_H: f32 = 316.0;
const FOOT_Y: f32 = 384.0;
const MODE_MS: u32 = 5400;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Le primitive che ogni finestra deve saper disegnare. Tutto in pixel logici.
pub trait Canvas {
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb);
    fn round(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, fill: Option<Rgb>, line: Option<Rgb>);
    /// Testo in un riquadro: centrato in verticale, oppure a capo dall'alto se `wrap`.
    fn text(&mut self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, bold: bool, c: Rgb, align: Align, wrap: bool);
    fn text_width(&mut self, s: &str, size: f32, bold: bool) -> f32;
    fn poly(&mut self, pts: &[(f32, f32)], fill: Rgb, line: Rgb);
    fn clip(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn unclip(&mut self);
    /// La ruota con `n` fette (fetta `hover` accesa, -1 nessuna), centrata e larga `size`, con opacità `alpha`.
    fn wheel(&mut self, n: usize, hover: i32, cx: f32, cy: f32, size: f32, alpha: f32);
    fn logo(&mut self, left: f32, baseline: f32, scale: f32, ink: Rgb, accent: Rgb, real: Rgb);
}

/// Cosa c'è sotto un clic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Page(usize),
    Row(usize),
    Prev,
    Next,
}

struct Page {
    title: [&'static str; 2],
    intro: &'static str,
    rows: [(&'static str, &'static str, &'static str); 3],
    ms: u32,
}

const PAGES: [Page; 6] = [
    Page {
        title: ["Tieni premuto un tasto,", "poi clic destro sul file"],
        intro: "Si apre la ruota. Clic sulla fetta: la copia nasce accanto all'originale, che resta dov'è.",
        rows: [("Shift", "Converti", "Cambia formato: JPEG, PDF, MP4…"), ("Ctrl", "Modifica", "Comprimi, ruota, OCR, metadati…"), ("Alt", "Azioni sul file", "Duplica, copia il percorso, ZIP")],
        ms: MODE_MS * 3,
    },
    Page {
        title: ["Tutte le conversioni", "in un solo pannello"],
        intro: "Dal menu dell'icona apri Conversioni. Trascini file anche di tipi diversi e scegli formato, strumenti e qualità.",
        rows: [("Foto", "Immagini e documenti", "Formato, dimensioni, raddrizza, PDF/A"), ("PDF", "Da PDF a Word", "Tabelle, colonne, OCR in italiano"), ("Audio", "Audio e video", "Bitrate, taglio, risoluzione, fps")],
        ms: 7000,
    },
    Page {
        title: ["YouTube e video", "dal web, senza altro"],
        intro: "Incolla uno o più link, scegli Video o Solo audio. Gli strumenti si scaricano da soli la prima volta.",
        rows: [("Link", "YouTube", "Anche elenchi di link in un colpo"), ("Web", "Video dal web", "Da molti altri siti"), ("Audio", "Solo audio", "MP3 o M4A, senza immagini")],
        ms: 7000,
    },
    Page {
        title: ["Dalla voce al testo,", "anche con la scheda video"],
        intro: "Trascrivi vocali, registrazioni e l'audio dei video. Se c'è una scheda video compatibile, Cedro la usa.",
        rows: [("Testo", "Trascrizione audio", "Con i tempi, copiabile con un clic"), ("GPU", "Più veloce", "Vulkan, oppure il processore giusto"), ("Locale", "Resta sul PC", "Il motore si scarica al primo uso")],
        ms: 7000,
    },
    Page {
        title: ["Si tiene aggiornato", "da solo"],
        intro: "Impostazioni ha una scheda Sistema con le caratteristiche del PC e una Informazioni con crediti e licenze.",
        rows: [("Auto", "Aggiornamenti", "Controlla e chiede prima di installare"), ("PC", "Sistema", "Processore, scheda video, memoria"), ("Info", "Informazioni", "Versione e licenze dei componenti")],
        ms: 7000,
    },
    Page {
        title: ["Su Windows,", "Windows on ARM e Linux"],
        intro: "Stesse funzioni dappertutto. L'installatore giusto lo sceglie Cedro quando si aggiorna.",
        rows: [("x64", "Windows 10 e 11", "Il solito installatore"), ("ARM64", "Windows on ARM", "Versione nativa, non emulata"), ("Linux", "Wayland", "KDE Plasma e GNOME")],
        ms: 7000,
    },
];

pub const PAGE_COUNT: usize = PAGES.len();

/// Dove siamo: il passo e il momento in cui è cominciato (in millisecondi di un orologio qualsiasi).
#[derive(Clone, Copy, Default)]
pub struct Tour {
    pub page: usize,
    start: u64,
}

impl Tour {
    pub const fn new(now: u64) -> Self {
        Tour { page: 0, start: now }
    }

    pub fn goto(&mut self, page: usize, now: u64) {
        self.page = page % PAGE_COUNT;
        self.start = now;
    }

    /// Fa andare avanti da solo il passo scaduto; dopo l'ultimo si ricomincia.
    pub fn tick(&mut self, now: u64) {
        if now.saturating_sub(self.start) >= PAGES[self.page].ms as u64 {
            self.goto(self.page + 1, now);
        }
    }

    pub fn elapsed(&self, now: u64) -> u32 {
        now.saturating_sub(self.start).min(u32::MAX as u64) as u32
    }

    pub fn is_last(&self) -> bool {
        self.page + 1 == PAGE_COUNT
    }

    /// Un clic su una riga del primo passo sceglie la ruota da vedere.
    pub fn show_row(&mut self, row: usize, now: u64) {
        if self.page == 0 {
            self.start = now.saturating_sub((row as u64) * MODE_MS as u64);
        }
    }
}

// ---------- geometria dei controlli ----------

const ROW_Y: f32 = 202.0;
const ROW_STEP: f32 = 58.0;
const BTN_W: f32 = 88.0;
const BTN_H: f32 = 32.0;
const NEXT_X: f32 = W - 20.0 - BTN_W;
const PREV_X: f32 = NEXT_X - 8.0 - BTN_W;
const BTN_Y: f32 = FOOT_Y + 12.0;
const DOT_X: f32 = 20.0;

fn dot_rect(i: usize, page: usize) -> (f32, f32) {
    let mut x = DOT_X;
    for k in 0..i {
        x += if k == page { 26.0 } else { 10.0 } + 8.0;
    }
    (x, if i == page { 26.0 } else { 10.0 })
}

pub fn hit(x: f32, y: f32, page: usize) -> Option<Hit> {
    let inside = |rx: f32, ry: f32, rw: f32, rh: f32| x >= rx && x < rx + rw && y >= ry && y < ry + rh;
    if inside(PREV_X, BTN_Y, BTN_W, BTN_H) && page > 0 {
        return Some(Hit::Prev);
    }
    if inside(NEXT_X, BTN_Y, BTN_W, BTN_H) {
        return Some(Hit::Next);
    }
    for i in 0..PAGE_COUNT {
        let (dx, dw) = dot_rect(i, page);
        if inside(dx - 3.0, FOOT_Y + 16.0, dw + 6.0, 24.0) {
            return Some(Hit::Page(i));
        }
    }
    (0..3).find(|&i| inside(24.0, ROW_Y + i as f32 * ROW_STEP, 360.0, 50.0)).map(Hit::Row)
}

// ---------- utilità di animazione ----------

fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// 0 prima di `a`, 1 dopo `b`, in mezzo una curva morbida.
fn seg(t: f32, a: f32, b: f32) -> f32 {
    ease((t - a) / (b - a))
}

fn lerp(a: (f32, f32), b: (f32, f32), k: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k)
}

// ---------- disegno ----------

pub fn draw(c: &mut dyn Canvas, tour: &Tour, now: u64) {
    let page = &PAGES[tour.page];
    let t = tour.elapsed(now);
    c.fill(0.0, 0.0, W, H, WHITE);
    c.fill(0.0, 0.0, W, 44.0, BLUE);
    c.text("Guida di Cedro", 20.0, 0.0, 400.0, 44.0, 19.0, true, WHITE, Align::Left, false);
    c.text(&format!("{} di {}", tour.page + 1, PAGE_COUNT), W - 120.0, 0.0, 100.0, 44.0, 13.0, false, mix(BLUE, WHITE, 0.85), Align::Right, false);

    c.text(page.title[0], 24.0, 60.0, 370.0, 30.0, 26.0, true, INK, Align::Left, false);
    c.text(page.title[1], 24.0, 90.0, 370.0, 30.0, 26.0, true, INK, Align::Left, false);
    c.text(page.intro, 24.0, 128.0, 360.0, 64.0, 15.0, false, GREY, Align::Left, true);

    let lit = lit_row(tour.page, t);
    for (i, (key, name, detail)) in page.rows.iter().enumerate() {
        let y = ROW_Y + i as f32 * ROW_STEP;
        let on = i == lit;
        c.round(24.0, y, 360.0, 50.0, 10.0, Some(WHITE), Some(if on { BLUE } else { LINE }));
        let cap_y = y + 10.0 + if on { 2.0 } else { 0.0 };
        c.round(36.0, cap_y, 74.0, 30.0, 8.0, Some(if on { BLUE } else { LINE }), None);
        c.text(key, 36.0, cap_y, 74.0, 30.0, 15.0, true, if on { WHITE } else { INK }, Align::Center, false);
        c.text(name, 122.0, y + 6.0, 254.0, 20.0, 15.0, true, INK, Align::Left, false);
        c.text(detail, 122.0, y + 27.0, 254.0, 18.0, 13.0, false, GREY, Align::Left, false);
    }

    c.round(STAGE_X, STAGE_Y, STAGE_W, STAGE_H, 12.0, Some(STAGE), None);
    c.clip(STAGE_X, STAGE_Y, STAGE_W, STAGE_H);
    let mut s = Stage { c, x: STAGE_X, y: STAGE_Y };
    match tour.page {
        0 => scene_wheel(&mut s, t),
        1 => scene_panel(&mut s, t),
        2 => scene_web(&mut s, t),
        3 => scene_voice(&mut s, t),
        4 => scene_settings(&mut s, t),
        _ => scene_platforms(&mut s, t),
    }
    c.unclip();

    footer(c, tour);
}

/// Quale delle tre righe è in evidenza al millisecondo `t` del passo.
fn lit_row(page: usize, t: u32) -> usize {
    let t = t as f32;
    match page {
        0 => (t / MODE_MS as f32) as usize % 3,
        1 => if t < 2200.0 { 0 } else if t < 4200.0 { 1 } else { 2 },
        2 => if t < 2600.0 { 0 } else if t < 4400.0 { 1 } else { 2 },
        3 => if t < 2800.0 { 0 } else if t < 4800.0 { 1 } else { 2 },
        4 => if t < 1800.0 { 1 } else if t < 4400.0 { 0 } else { 2 },
        _ => (((t - 400.0) / 900.0).max(0.0) as usize).min(2),
    }
}

fn footer(c: &mut dyn Canvas, tour: &Tour) {
    c.fill(0.0, FOOT_Y, W, 1.0, LINE);
    for i in 0..PAGE_COUNT {
        let (x, w) = dot_rect(i, tour.page);
        c.round(x, FOOT_Y + 23.0, w, 10.0, 5.0, Some(if i == tour.page { BLUE } else { LINE }), None);
    }
    // Firma: «Cedro x.y.z · Powered by Luca real>hcok_ A.», centrata nello spazio tra i pallini e i pulsanti.
    let head = format!("Cedro {}  ·  Powered by Luca ", crate::update::CURRENT);
    let size = 12.0;
    let scale = size * 0.82 / logo::HEIGHT;
    let hw = c.text_width(&head, size, false);
    let aw = c.text_width(" A.", size, false);
    let lw = logo::width(true) * scale;
    let total = hw + lw + aw;
    let left = 150.0 + (PREV_X - 150.0 - 12.0 - total) / 2.0;
    let mid = FOOT_Y + 28.0;
    c.text(&head, left, mid - 10.0, hw + 4.0, 20.0, size, false, GREY, Align::Left, false);
    c.logo(left + hw, mid + size * 0.36, scale, BLUE, LEMON, GREY);
    c.text(" A.", left + hw + lw, mid - 10.0, aw + 4.0, 20.0, size, false, GREY, Align::Left, false);

    if tour.page > 0 {
        c.round(PREV_X, BTN_Y, BTN_W, BTN_H, 0.0, Some(WHITE), Some(LINE));
        c.text("Indietro", PREV_X, BTN_Y, BTN_W, BTN_H, 15.0, true, INK, Align::Center, false);
    }
    c.round(NEXT_X, BTN_Y, BTN_W, BTN_H, 0.0, Some(BLUE), Some(BLUE));
    c.text(if tour.is_last() { "Chiudi" } else { "Avanti" }, NEXT_X, BTN_Y, BTN_W, BTN_H, 15.0, true, WHITE, Align::Center, false);
}

/// Il palco: le coordinate delle scene sono relative al suo angolo alto a sinistra.
struct Stage<'a> {
    c: &'a mut dyn Canvas,
    x: f32,
    y: f32,
}

impl Stage<'_> {
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, col: Rgb) {
        self.c.fill(self.x + x, self.y + y, w, h, col);
    }
    fn round(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, fill: Option<Rgb>, line: Option<Rgb>) {
        self.c.round(self.x + x, self.y + y, w, h, r, fill, line);
    }
    fn text(&mut self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, bold: bool, col: Rgb, align: Align) {
        self.c.text(s, self.x + x, self.y + y, w, h, size, bold, col, align, false);
    }
    fn width(&mut self, s: &str, size: f32, bold: bool) -> f32 {
        self.c.text_width(s, size, bold)
    }
    fn clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.c.unclip();
        self.c.clip(self.x + x, self.y + y, w, h);
    }
    fn reclip(&mut self) {
        self.c.unclip();
        self.c.clip(self.x, self.y, STAGE_W, STAGE_H);
    }
}

// ---------- 1. la ruota ----------

const MODES: [(&str, &str, usize, &[&str]); 3] = [
    ("Shift", "CONVERSIONE", 2, &["PDF", "PNG", "JPEG", "WebP", "BMP", "GIF", "TIFF"]),
    ("Ctrl", "STRUMENTI", 5, &["Comprimi", "Scala", "B/N", "Ruota", "Metadati", "OCR", "MAIUSC."]),
    ("Alt", "AZIONI", 1, &["Duplica", "Percorso", "ZIP"]),
];

fn scene_wheel(s: &mut Stage, t: u32) {
    let mode = (t / MODE_MS) as usize % 3;
    let lt = (t % MODE_MS) as f32;
    let (key, caption, pick, labels) = MODES[mode];
    let n = labels.len();
    let (cx, cy, size) = (182.0, 158.0, 316.0);
    // il file
    s.round(cx - 46.0, cy - 46.0, 92.0, 92.0, 10.0, Some(WHITE), None);
    s.round(cx - 34.0, cy - 36.0, 68.0, 42.0, 6.0, Some(TINT), None);
    s.text("foto.png", cx - 46.0, cy + 8.0, 92.0, 30.0, 12.0, true, INK, Align::Center);

    let alpha = seg(lt, 1700.0, 2000.0) * (1.0 - seg(lt, 4600.0, 4900.0));
    let lit = (3000.0..4600.0).contains(&lt);
    let hover = if lit { pick as i32 } else { -1 };
    if alpha > 0.0 {
        s.c.wheel(n, hover, s.x + cx, s.y + cy, size, alpha);
        if alpha >= 0.6 {
            let k = size / WHEEL;
            let r = (R0 + R1) / 2.0 * k;
            for (i, label) in labels.iter().enumerate() {
                let (a0, a1) = slice_bounds(i, n);
                let mid = (a0 + a1) / 2.0;
                let (x, y) = (cx + r * mid.cos(), cy + r * mid.sin());
                let col = if i as i32 == hover { WHITE } else { INK };
                s.text(label, x - 50.0, y - 12.0, 100.0, 24.0, if n > 6 { 13.0 } else { 15.0 }, true, col, Align::Center);
            }
            s.text(caption, cx - 40.0, cy - 22.0, 80.0, 16.0, 9.0, true, GREY, Align::Center);
            let name = if hover >= 0 { labels[hover as usize] } else { "foto.png" };
            s.text(name, cx - 40.0, cy - 4.0, 80.0, 22.0, 14.0, true, (0x00, 0x1A, 0x4D), Align::Center);
        }
    }
    // il tasto premuto
    let chip = seg(lt, 300.0, 500.0) * (1.0 - seg(lt, 4600.0, 4900.0));
    if chip > 0.0 {
        let col = mix(STAGE, BLUE2, chip);
        s.round(14.0, STAGE_H - 44.0, 76.0, 30.0, 7.0, Some(col), None);
        s.text(key, 14.0, STAGE_H - 44.0, 76.0, 30.0, 15.0, true, mix(STAGE, WHITE, chip), Align::Center);
    }
    // il cursore: arriva sul file, poi va sulla fetta
    let rest = (310.0, 260.0);
    let on_file = (170.0, 150.0);
    let a = {
        let (a0, a1) = slice_bounds(pick, n);
        (a0 + a1) / 2.0
    };
    let r = (R0 + R1) / 2.0 * size / WHEEL;
    let target = (cx + r * a.cos(), cy + r * a.sin());
    let pos = if lt < 1300.0 {
        lerp(rest, on_file, seg(lt, 300.0, 1300.0))
    } else {
        lerp(on_file, target, seg(lt, 2300.0, 2900.0))
    };
    let pts = [(0.0, 0.0), (0.0, 20.0), (5.0, 15.0), (9.0, 24.0), (13.0, 22.0), (9.0, 14.0), (16.0, 14.0)];
    let moved: Vec<(f32, f32)> = pts.iter().map(|p| (s.x + pos.0 + p.0, s.y + pos.1 + p.1)).collect();
    s.c.poly(&moved, WHITE, (0, 0, 0));
}

// ---------- 2. il pannello Conversioni ----------

fn scene_panel(s: &mut Stage, t: u32) {
    let t = t as f32;
    s.round(14.0, 14.0, 336.0, 288.0, 6.0, Some(WHITE), None);
    for (i, name) in ["Immagini", "Documenti", "PDF", "Testo", "Audio", "Video"].iter().enumerate() {
        let y = 14.0 + i as f32 * 30.0;
        let on = i == 2;
        if on {
            s.fill(14.0, y, 92.0, 30.0, BLUE);
        }
        s.text(name, 24.0, y, 80.0, 30.0, 13.0, on, if on { WHITE } else { INK }, Align::Left);
    }
    s.fill(106.0, 14.0, 1.0, 288.0, LINE);
    let x = 118.0;
    s.text("PDF", x, 22.0, 200.0, 24.0, 17.0, true, BLUE, Align::Left);
    s.round(x, 50.0, 220.0, 28.0, 0.0, Some(mix(TINT, WHITE, 0.4)), Some(BLUE));
    s.text("Trascina qui i file o clicca", x, 50.0, 220.0, 28.0, 12.0, true, BLUE, Align::Center);
    s.round(x, 86.0, 220.0, 62.0, 0.0, Some(WHITE), Some(LINE));
    let files = ["relazione.pdf", "contratto.pdf", "scansione.pdf"];
    for (i, f) in files.iter().enumerate() {
        let p = seg(t, 700.0 + i as f32 * 450.0, 1050.0 + i as f32 * 450.0);
        let y = 88.0 + i as f32 * 19.0 + (1.0 - p) * -8.0;
        let ink = mix(WHITE, INK, p);
        let done = t > 3800.0 + i as f32 * 900.0;
        s.text("PDF", x + 6.0, y, 36.0, 18.0, 11.0, true, ink, Align::Left);
        s.text(f, x + 44.0, y, 110.0, 18.0, 11.5, false, ink, Align::Left);
        s.text(if done { "Fatto" } else { "In attesa" }, x + 120.0, y, 94.0, 18.0, 11.0, false, mix(WHITE, if done { BLUE } else { GREY }, p), Align::Right);
    }
    s.text("CONVERTI IN", x, 154.0, 200.0, 14.0, 10.0, true, GREY, Align::Left);
    chips(s, x, 170.0, &[("Word", true), ("JPG", false), ("Testo", false)]);
    s.text("OPZIONI", x, 200.0, 200.0, 14.0, 10.0, true, GREY, Align::Left);
    chips(s, x, 216.0, &[("OCR", true), ("Tabelle", true), ("Colonne", false)]);
    // il pulsante: si riempie mentre converte
    let frac = if t > 2800.0 { ((t - 2800.0) / 3000.0).min(1.0) } else { 0.0 };
    s.round(x, 252.0, 220.0, 30.0, 0.0, Some(BLUE), None);
    if frac > 0.0 {
        s.fill(x, 252.0, 220.0 * frac, 30.0, mix(BLUE, (0x4C, 0xAF, 0x50), 0.85));
    }
    let label = if t > 6200.0 { "Fatto: 3 file Word accanto agli originali" } else if t > 2800.0 { "Sto convertendo…" } else { "Converti 3 file · Word" };
    s.text(label, x, 252.0, 220.0, 30.0, 12.5, true, WHITE, Align::Center);
}

fn chips(s: &mut Stage, x: f32, y: f32, items: &[(&str, bool)]) {
    let mut cx = x;
    for (label, on) in items {
        let w = s.width(label, 11.5, false) + 16.0;
        s.round(cx, y, w, 22.0, 0.0, Some(if *on { BLUE } else { WHITE }), Some(if *on { BLUE } else { LINE }));
        s.text(label, cx, y, w, 22.0, 11.5, false, if *on { WHITE } else { INK }, Align::Center);
        cx += w + 4.0;
    }
}

// ---------- 3. YouTube e Video dal web ----------

fn scene_web(s: &mut Stage, t: u32) {
    let t = t as f32;
    let url = "https://www.youtube.com/watch?v=Zs4k9";
    let typed = (((t - 300.0) / 45.0).max(0.0) as usize).min(url.chars().count());
    let shown: String = url.chars().take(typed).collect();
    s.round(18.0, 26.0, 328.0, 32.0, 6.0, Some(WHITE), Some(BLUE2));
    s.text(&shown, 28.0, 26.0, 310.0, 32.0, 12.0, false, INK, Align::Left);
    let picked = t > 2200.0;
    for (i, (name, note)) in [("Video", "Immagini e suono"), ("Solo audio", "Per ascoltare")].iter().enumerate() {
        let x = 18.0 + i as f32 * 178.0;
        if picked && i == 0 {
            s.round(x - 3.0, 89.0, 156.0, 68.0, 10.0, Some(LEMON), None);
        }
        s.round(x, 92.0, 150.0, 62.0, 8.0, Some(WHITE), None);
        s.text(name, x + 10.0, 98.0, 130.0, 24.0, 15.0, true, INK, Align::Left);
        s.text(note, x + 10.0, 122.0, 130.0, 20.0, 11.0, false, GREY, Align::Left);
    }
    if picked {
        let frac = ((t - 2200.0) / 3200.0).min(1.0);
        let label = if t > 5600.0 { "Fatto: il video è nella cartella" } else { "Scarico dal web…" };
        s.text(label, 18.0, 180.0, 328.0, 20.0, 12.0, false, WHITE, Align::Left);
        s.round(18.0, 206.0, 328.0, 8.0, 4.0, Some(mix(STAGE, WHITE, 0.2)), None);
        s.round(18.0, 206.0, (328.0 * frac).max(8.0), 8.0, 4.0, Some(LEMON), None);
    }
}

// ---------- 4. trascrizione ----------

fn scene_voice(s: &mut Stage, t: u32) {
    let tf = t as f32;
    let bars = 46;
    let pitch = (328.0 - 4.0) / bars as f32;
    let live = tf < 6900.0;
    for i in 0..bars {
        let f = i as f32;
        let level = if live { 0.12 + (f * 0.5 + tf * 0.0039).sin().abs() * 0.7 * (f * 0.13 + 1.0).sin().abs() } else { 0.12 };
        let h = 60.0 * level;
        s.round(18.0 + f * pitch, 52.0 - h / 2.0, 4.2, h, 2.0, Some((0x7F, 0xB4, 0xDC)), None);
    }
    s.round(18.0, 100.0, 328.0, 142.0, 6.0, Some(WHITE), None);
    let lines = [("00:00", "Buongiorno a tutti e grazie di essere qui."), ("00:07", "Oggi parliamo del bilancio del comune."), ("00:15", "Il primo punto riguarda le manutenzioni…")];
    for (i, (time, text)) in lines.iter().enumerate() {
        let p = seg(tf, 1100.0 + i as f32 * 1400.0, 1600.0 + i as f32 * 1400.0);
        if p <= 0.0 {
            continue;
        }
        let y = 108.0 + i as f32 * 40.0;
        s.text(time, 30.0, y, 44.0, 18.0, 12.0, true, mix(WHITE, BLUE, p), Align::Left);
        s.c.text(text, s.x + 76.0, s.y + y, 258.0, 36.0, 12.5, false, mix(WHITE, INK, p), Align::Left, true);
    }
    s.round(18.0, 270.0, 40.0, 22.0, 5.0, Some(LEMON), None);
    s.text("GPU", 18.0, 270.0, 40.0, 22.0, 12.0, true, DARK, Align::Center);
    s.text("Scheda video trovata: trascrizione più veloce", 66.0, 270.0, 290.0, 22.0, 12.0, false, WHITE, Align::Left);
}

// ---------- 5. Sistema e aggiornamenti ----------

fn scene_settings(s: &mut Stage, t: u32) {
    let tf = t as f32;
    s.round(14.0, 14.0, 336.0, 288.0, 6.0, Some(WHITE), None);
    for (i, name) in ["Generale", "Ruota", "Salvataggio", "Qualità", "OCR", "Aggiornamenti", "Sistema", "Informazioni"].iter().enumerate() {
        let y = 14.0 + i as f32 * 30.0;
        let on = i == 6;
        if on {
            s.fill(14.0, y, 104.0, 30.0, BLUE);
        }
        s.text(name, 24.0, y, 92.0, 30.0, 13.0, on, if on { WHITE } else { INK }, Align::Left);
    }
    s.fill(118.0, 14.0, 1.0, 288.0, LINE);
    s.text("Sistema", 132.0, 22.0, 200.0, 24.0, 17.0, true, BLUE, Align::Left);
    for (i, (k, v)) in [("Windows", "11 · 64 bit"), ("Processore", "AVX2 · VNNI"), ("Scheda video", "Vulkan"), ("Memoria", "16 GB")].iter().enumerate() {
        let y = 54.0 + i as f32 * 26.0;
        s.text(k, 132.0, y, 100.0, 24.0, 12.5, false, GREY, Align::Left);
        s.text(v, 232.0, y, 106.0, 24.0, 12.5, false, INK, Align::Right);
        s.fill(132.0, y + 25.0, 206.0, 1.0, LINE);
    }
    let slide = 1.0 - seg(tf, 1800.0, 2300.0);
    s.clip(14.0, 14.0, 336.0, 288.0);
    s.round(132.0, 254.0 + 80.0 * slide, 206.0, 36.0, 6.0, Some(BLUE), None);
    s.text("Nuova versione pronta · Aggiorna", 132.0, 254.0 + 80.0 * slide, 206.0, 36.0, 12.5, true, WHITE, Align::Center);
    s.reclip();
}

// ---------- 6. Windows, ARM, Linux ----------

fn scene_platforms(s: &mut Stage, t: u32) {
    let tf = t as f32;
    let rows = [("Windows 10 e 11", "x64"), ("Windows on ARM", "Snapdragon X e simili"), ("Linux", "Wayland · KDE Plasma e GNOME")];
    for (i, (name, note)) in rows.iter().enumerate() {
        let p = seg(tf, 400.0 + i as f32 * 900.0, 900.0 + i as f32 * 900.0);
        if p <= 0.0 {
            continue;
        }
        let dx = (1.0 - p) * 16.0;
        let y = 20.0 + i as f32 * 63.0;
        s.round(18.0 + dx, y, 328.0, 54.0, 8.0, Some(mix(STAGE, WHITE, 0.08 * p)), Some(mix(STAGE, WHITE, 0.2 * p)));
        s.text(name, 30.0 + dx, y, 190.0, 54.0, 15.0, true, mix(STAGE, WHITE, p), Align::Left);
        s.text(note, 150.0 + dx, y, 184.0, 54.0, 12.0, false, mix(STAGE, (0xB9, 0xD3, 0xE3), p), Align::Right);
    }
    let scale = 0.5;
    let left = (STAGE_W - logo::width(true) * scale) / 2.0;
    s.c.logo(s.x + left, s.y + STAGE_H - 28.0, scale, WHITE, LEMON, REAL_ON_DARK);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tour_runs_through_all_pages_and_wraps() {
        let mut t = Tour::new(0);
        let mut now = 0;
        for expect in 1..PAGE_COUNT {
            now += PAGES[t.page].ms as u64;
            t.tick(now);
            assert_eq!(t.page, expect);
        }
        now += PAGES[t.page].ms as u64;
        t.tick(now);
        assert_eq!(t.page, 0);
    }

    #[test]
    fn controls_are_hit() {
        assert_eq!(hit(NEXT_X + 5.0, BTN_Y + 5.0, 0), Some(Hit::Next));
        assert_eq!(hit(PREV_X + 5.0, BTN_Y + 5.0, 0), None);
        assert_eq!(hit(PREV_X + 5.0, BTN_Y + 5.0, 2), Some(Hit::Prev));
        let (x, w) = dot_rect(3, 0);
        assert_eq!(hit(x + w / 2.0, FOOT_Y + 28.0, 0), Some(Hit::Page(3)));
        assert_eq!(hit(100.0, ROW_Y + ROW_STEP + 10.0, 0), Some(Hit::Row(1)));
    }
}
