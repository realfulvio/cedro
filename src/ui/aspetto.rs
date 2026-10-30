//! La pagina «Aspetto» delle Impostazioni: la modalità (Standard o Semplificata) e i cinque temi,
//! ciascuno con la sua anteprima. Le anteprime sono fotografie vere del disegno di Cedro, fatte
//! in un thread con file di prova, quindi mostrano esattamente quello che si otterrà.
//! Le due finestre delle Impostazioni (Windows e Linux) la ospitano così com'è: qui non c'è
//! nulla di legato al sistema.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::tema::Tema;

use super::gfx::{Align, Fit, Image, Painter, Weight};
use super::model::Activity;
use super::screen::{Key, Screen, Surface};
use super::theme::{ThemeId, THEMES};

type R = (f32, f32, f32, f32);

/// Larghezza in pixel delle anteprime (il doppio di quanto servirebbe: restano nitide a 200%).
const THUMB_W: f32 = 520.0;
const THUMB_H: f32 = 340.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hit {
    Theme(ThemeId),
    Mode(bool),
}

type Thumbs = Arc<Mutex<HashMap<(ThemeId, bool), Image>>>;

pub struct Aspetto {
    pub theme: ThemeId,
    pub simple: bool,
    hover: Option<Hit>,
    scroll: f32,
    content_h: f32,
    view_h: f32,
    zones: Vec<(R, Hit)>,
    thumbs: Thumbs,
    tema: &'static Tema,
}

impl Aspetto {
    /// `wake` chiede alla finestra di ridisegnarsi quando un'anteprima è pronta.
    pub fn new(theme: ThemeId, simple: bool, wake: Arc<dyn Fn() + Send + Sync>) -> Aspetto {
        let thumbs: Thumbs = Arc::new(Mutex::new(HashMap::new()));
        let t = thumbs.clone();
        std::thread::spawn(move || {
            // Prima la modalità scelta, poi l'altra; il tema scelto per primo.
            let mut order: Vec<(ThemeId, bool)> = Vec::new();
            for mode in [simple, !simple] {
                order.push((theme, mode));
                order.extend(THEMES.into_iter().filter(|t| *t != theme).map(|t| (t, mode)));
            }
            let dir = std::env::temp_dir().join("cedro-aspetto");
            let files = demo_files(&dir);
            for key in order {
                if let Some(img) = render_thumb(key.0, key.1, &files) {
                    t.lock().unwrap().insert(key, img);
                    wake();
                }
            }
        });
        Aspetto { theme, simple, hover: None, scroll: 0.0, content_h: 0.0, view_h: 0.0, zones: Vec::new(), thumbs, tema: crate::tema::attivo() }
    }

    /// I colori della pagina (di norma quelli dell'edizione; le Impostazioni di Windows usano i propri).
    pub fn set_tema(&mut self, tema: &'static Tema) {
        self.tema = tema;
    }

    /// Disegna la pagina in un'area di `w`×`h` unità.
    pub fn draw(&mut self, p: &mut Painter, full_w: f32, h: f32) {
        // A destra resta il posto della barra di scorrimento.
        let w = full_w - 14.0;
        let t = self.tema;
        p.clear(t.bg);
        self.zones.clear();
        self.view_h = h;
        let off = -self.scroll;
        let mut y = off + 4.0;

        // Modalità
        p.text("Modalità", 0.0, y, w, 20.0, 14.0, Weight::Bold, t.fg, Align::Left);
        y += 26.0;
        let gap = 12.0;
        let bw = (w - gap) / 2.0;
        for (i, (simple, name, text)) in [(false, "Standard", "Tutte le opzioni a portata di mano."), (true, "Semplificata", "Solo l'essenziale; il resto in «Altre opzioni».")].into_iter().enumerate() {
            let r = (i as f32 * (bw + gap), y, bw, 76.0);
            let on = self.simple == simple;
            let hot = self.hover == Some(Hit::Mode(simple));
            self.choice(p, r, on, hot);
            p.text(name, r.0 + 40.0, r.1 + 11.0, r.2 - 52.0, 20.0, 14.0, Weight::Bold, t.fg, Align::Left);
            p.text_wrap(text, r.0 + 40.0, r.1 + 34.0, r.2 - 52.0, 12.0, Weight::Regular, t.mute, 16.0, 2);
            radio(p, r.0 + 20.0, r.1 + 22.0, on, t);
            self.zones.push((r, Hit::Mode(simple)));
        }
        y += 76.0 + 22.0;

        // Temi
        p.text("Tema", 0.0, y, w, 20.0, 14.0, Weight::Bold, t.fg, Align::Left);
        y += 26.0;
        let cols = if w >= 540.0 { 3 } else { 2 };
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let th = cw * (THUMB_H / THUMB_W);
        let desc_lines = if cols == 3 { 3 } else { 2 };
        // I nomi lunghi vanno a capo: lo spazio è quello del più lungo.
        let name_lines = THEMES.into_iter().map(|th| p.wrap_lines(th.name(), 14.0, Weight::Bold, cw - 24.0, 2).len()).max().unwrap_or(1);
        let ch = 10.0 + th + 10.0 + name_lines as f32 * 20.0 + 2.0 + desc_lines as f32 * 19.0 + 14.0;
        for (i, theme) in THEMES.into_iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let r = (col as f32 * (cw + gap), y + row as f32 * (ch + gap), cw, ch);
            let on = self.theme == theme;
            let hot = self.hover == Some(Hit::Theme(theme));
            self.choice(p, r, on, hot);
            let tr = (r.0 + 10.0, r.1 + 10.0, cw - 20.0, th);
            self.thumb(p, tr, theme);
            if on {
                // La spunta sull'anteprima scelta.
                let (cx, cy) = (tr.0 + tr.2 - 16.0, tr.1 + 16.0);
                p.circle(cx, cy, 11.0, Some(t.acc), Some((t.bg, 2.0)));
                p.line(cx - 4.5, cy + 0.5, cx - 1.0, cy + 4.0, t.acc_fg, 2.2);
                p.line(cx - 1.0, cy + 4.0, cx + 5.0, cy - 3.5, t.acc_fg, 2.2);
            }
            p.text_wrap(theme.name(), r.0 + 12.0, tr.1 + th + 10.0, cw - 24.0, 14.0, Weight::Bold, t.fg, 17.0, 2);
            p.text_wrap(theme.description(), r.0 + 12.0, tr.1 + th + 12.0 + name_lines as f32 * 20.0, cw - 24.0, 12.0, Weight::Regular, t.mute, 16.0, desc_lines);
            self.zones.push((r, Hit::Theme(theme)));
        }
        let rows = THEMES.len().div_ceil(cols) as f32;
        y += rows * (ch + gap);

        let note = "Il nuovo aspetto compare nella finestra principale appena ci torni. Se Cedro sta lavorando, al termine del lavoro. I tuoi file e le opzioni scelte restano come sono.";
        let used = p.text_wrap(note, 0.0, y + 4.0, w, 12.0, Weight::Regular, t.mute, 17.0, 5);
        y += 4.0 + used + 8.0;

        self.content_h = y - off;
        let max = (self.content_h - h).max(0.0);
        if self.scroll > max {
            self.scroll = max;
        }
        if max > 0.0 {
            // Barra di scorrimento sottile a destra.
            let track = h - 8.0;
            let bar = (track * h / self.content_h).max(28.0);
            let top = 4.0 + (track - bar) * (self.scroll / max);
            p.rrect(full_w - 6.0, top, 4.0, bar, 2.0, Some(t.line), None);
        }
    }

    fn choice(&self, p: &mut Painter, r: R, on: bool, hot: bool) {
        let t = self.tema;
        let rad = (t.radius as f32).max(4.0);
        let fill = if hot && !on { t.surf } else { t.bg };
        p.rrect(r.0, r.1, r.2, r.3, rad, Some(fill), Some(if on { (t.acc, 2.0) } else { (t.line, 1.0) }));
    }

    fn thumb(&self, p: &mut Painter, r: R, theme: ThemeId) {
        let t = self.tema;
        let img = self.thumbs.lock().unwrap().get(&(theme, self.simple)).cloned();
        match img {
            Some(img) => {
                let pal = theme.palette();
                p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(pal.bg), Some((t.line, 1.0)));
                p.image(&img, r.0, r.1, r.2, r.3, 6.0, Fit::Contain);
            }
            None => {
                p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(t.surf), Some((t.line, 1.0)));
                p.text("Preparo l'anteprima…", r.0, r.1, r.2, r.3, 12.0, Weight::Regular, t.mute, Align::Center);
            }
        }
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.zones.iter().rev().find(|(r, _)| x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3).map(|(_, h)| *h)
    }

    /// Il puntatore si è mosso; vero se serve ridisegnare.
    pub fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        let h = self.hit_at(x, y);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    pub fn pointer_leave(&mut self) -> bool {
        self.hover.take().is_some()
    }

    /// Vero se sotto il puntatore c'è qualcosa da scegliere (cursore a mano).
    pub fn is_hand(&self) -> bool {
        self.hover.is_some()
    }

    /// Un clic: sceglie. Vero se la scelta è cambiata.
    pub fn pointer_down(&mut self, x: f32, y: f32) -> bool {
        match self.hit_at(x, y) {
            Some(Hit::Theme(t)) if t != self.theme => {
                self.theme = t;
                true
            }
            Some(Hit::Mode(s)) if s != self.simple => {
                self.simple = s;
                true
            }
            _ => false,
        }
    }

    /// Rotellina: `dy` positivo scende. Vero se serve ridisegnare.
    pub fn wheel(&mut self, dy: f32) -> bool {
        let max = (self.content_h - self.view_h).max(0.0);
        let to = (self.scroll + dy).clamp(0.0, max);
        let changed = to != self.scroll;
        self.scroll = to;
        changed
    }

    /// Frecce: scelgono il tema vicino. Sinistra/destra e su/giù fanno lo stesso giro.
    pub fn key(&mut self, key: Key) -> bool {
        let at = THEMES.iter().position(|t| *t == self.theme).unwrap_or(0);
        let to = match key {
            Key::Left | Key::Up => (at + THEMES.len() - 1) % THEMES.len(),
            Key::Right | Key::Down => (at + 1) % THEMES.len(),
            _ => return false,
        };
        self.theme = THEMES[to];
        true
    }
}

fn radio(p: &mut Painter, cx: f32, cy: f32, on: bool, t: &Tema) {
    p.circle(cx, cy, 9.0, Some(t.bg), Some((if on { t.acc } else { t.mute }, 1.6)));
    if on {
        p.circle(cx, cy, 4.5, Some(t.acc), None);
    }
}

/// Tre fotografie finte per riempire le anteprime (si fanno una volta sola).
fn demo_files(dir: &Path) -> Vec<PathBuf> {
    let _ = std::fs::create_dir_all(dir);
    let mut out = Vec::new();
    for (i, name) in ["Riunione-giunta-2026.jpg", "Sopralluogo-via-Roma.jpg", "Festa-del-paese.jpg"].iter().enumerate() {
        let p = dir.join(name);
        if !p.exists() {
            let (w, h) = (640 + i as u32 * 80, 480);
            let img = image::RgbImage::from_fn(w, h, |x, y| {
                let t = x as f32 / w as f32;
                let s = y as f32 / h as f32;
                match i {
                    0 => image::Rgb([(60.0 + 120.0 * t) as u8, (110.0 + 80.0 * s) as u8, 170]),
                    1 => image::Rgb([(200.0 * s) as u8 + 40, 150, (90.0 + 100.0 * t) as u8]),
                    _ => image::Rgb([230, (120.0 + 100.0 * t) as u8, (60.0 + 120.0 * s) as u8]),
                }
            });
            if img.save(&p).is_err() {
                continue;
            }
        }
        out.push(p);
    }
    out
}

/// Fotografa un tema con tre file in coda, come lo vedrebbe chi lo sceglie.
fn render_thumb(theme: ThemeId, simple: bool, files: &[PathBuf]) -> Option<Image> {
    let surface = if theme == ThemeId::CedroCompatto { Surface::Compact } else { Surface::Main };
    let mut s = Screen::new(surface, Arc::new(|| {}));
    s.request_appearance(theme, simple);
    s.app.activity = Activity::Convert;
    s.drop_files(files.to_vec());
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(1500) {
        s.pump();
        let ready = s.app.files.iter().all(|f| s.app.media_of(&f.path).is_some_and(|m| m.thumb.is_some()));
        if ready && t.elapsed() > Duration::from_millis(200) {
            break;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    let (w, h) = s.preferred_size();
    // Il popup è alto e stretto: sta tutto nell'altezza dell'anteprima.
    let scale = (THUMB_W / w).min(THUMB_H / h);
    let (pw, ph) = ((w * scale).round() as u32, (h * scale).round() as u32);
    let p = s.render(pw, ph, scale);
    let rgba: Vec<u8> = p.pixmap.data().to_vec();
    Image::from_rgba(pw, ph, &rgba)
}
