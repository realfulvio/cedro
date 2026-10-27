//! La ruota della versione standard, come nella tavola «Ruota e dettagli»: un disco di fette con
//! l'anteprima del file al centro, il riquadro dei dettagli accanto (Standard) o la scritta guida
//! (Semplificata) e, sotto, i tasti che cambiano modo. Si disegna con il pittore di `cedro::ui`,
//! quindi è identica su Windows e su Linux; la finestra di sistema copia l'immagine e passa il mouse.
//!
//! Le misure sono quelle della tavola (1536×1024) e si portano a schermo con `ZOOM` e la densità.
//! Il disegno e la ricerca della fetta sotto il mouse usano la stessa geometria.

use std::path::Path;

use super::assets;
use super::gfx::{Align, Fit, Fonts, Image, Painter, Rgb, Weight};
use crate::engine::{self, Kind, Mode, Slice};

type R = (f32, f32, f32, f32);

/// Raggi del disco, dalla tavola: fuori 230, dentro 104.
const RO: f32 = 230.0;
const RI: f32 = 104.0;
/// Raggio della fotografia al centro.
const HOLE: f32 = 100.0;
/// Margine attorno al disco per l'ombra e per la fetta che sporge.
const PAD: f32 = 34.0;
/// Quanto sporge la fetta sotto il mouse.
const PUSH: f32 = 14.0;
/// Metà dello spazio tra due fette.
const HALF_GAP: f32 = 5.0;
/// Arrotondamento degli angoli delle fette.
const CORNER: f32 = 14.0;
const PANEL_W: f32 = 327.0;

const ACC: Rgb = (8, 116, 242);
const ACC_DARK: Rgb = (2, 98, 226);
const INK: Rgb = (23, 43, 58);
const MUTED: Rgb = (88, 107, 128);
const LINE: Rgb = (208, 223, 236);
const SLICE: Rgb = (246, 249, 252);
const CARD: Rgb = (252, 253, 254);

/// L'ordine dei formati d'immagine nella tavola: ogni fetta porta il suo identificativo, mai l'indice.
const IMAGE_ORDER: [&str; 7] = ["jpeg", "webp", "bmp", "gif", "tiff", "pdf", "png"];

/// Le fette da mostrare per un tipo di file e un modo, nell'ordine della tavola.
pub fn list(kind: Kind, mode: Mode) -> Vec<Slice> {
    let base = engine::slices(kind, mode);
    if kind == Kind::Image && mode == Mode::Convert {
        let mut out: Vec<Slice> = IMAGE_ORDER.iter().filter_map(|id| base.iter().find(|s| s.id == *id).copied()).collect();
        out.extend(base.iter().filter(|s| !IMAGE_ORDER.contains(&s.id)).copied());
        return out;
    }
    base.to_vec()
}

/// Dove sta il mouse, o cosa si è toccato.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    /// Fuori da tutto: la parte trasparente della finestra.
    None,
    /// Il buco al centro, con la fotografia.
    Hole,
    Slice(usize),
    Quality,
    Check,
    Go,
    OpenFull,
    /// Uno dei tre modi: 0 conversione, 1 strumenti, 2 azioni.
    Mode(u8),
    /// Semplificata (true) o Standard (false).
    Pill(bool),
    Esc,
}

/// Cosa deve fare chi ospita la ruota dopo un clic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    None,
    Redraw,
    /// Esegue la fetta (indice nell'elenco di `list`).
    Commit(usize),
    OpenFull,
    Mode(u8),
    Simple(bool),
    Close,
}

/// Dove va la copia: la tavola ha «Salva accanto all'originale».
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Beside {
    /// Le Impostazioni dicono già «nella stessa cartella»: la scelta non si può cambiare.
    Locked,
    On,
    Off,
}

pub struct WheelUi {
    pub list: Vec<Slice>,
    pub mode: u8,
    /// Quali dei tre modi si possono usare (le Impostazioni possono spegnere Shift, Ctrl, Alt).
    pub modes_on: [bool; 3],
    pub simple: bool,
    pub name: String,
    pub kind: Kind,
    pub has_file: bool,
    pub thumb: Option<Image>,
    pub quality: u8,
    pub beside: Beside,
    /// Dove va la copia, a parole, per la scritta della Semplificata.
    pub dest_note: String,
    /// La fetta sotto il mouse, -1 se nessuna.
    pub hover: i32,
    pub selected: Option<usize>,
    zone: Zone,
    drag: bool,
}

/// Le zone rettangolari e le misure di una disposizione.
struct Layout {
    w: f32,
    h: f32,
    cx: f32,
    cy: f32,
    zones: Vec<(R, Zone)>,
    panel: Option<R>,
}

impl WheelUi {
    pub fn new(kind: Kind, mode: u8, simple: bool) -> WheelUi {
        WheelUi {
            list: list(kind, mode_of(mode)),
            mode,
            modes_on: [true; 3],
            simple,
            name: String::new(),
            kind,
            has_file: false,
            thumb: None,
            quality: 82,
            beside: Beside::Locked,
            dest_note: "Copia accanto all'originale".into(),
            hover: -1,
            selected: if simple { None } else { Some(0) },
            zone: Zone::None,
            drag: false,
        }
    }

    /// Cambia tipo di file o modo: l'elenco delle fette si rifà.
    pub fn set_kind_mode(&mut self, kind: Kind, mode: u8) {
        if kind != self.kind || mode != self.mode || self.list.is_empty() {
            self.kind = kind;
            self.mode = mode;
            self.list = list(kind, mode_of(mode));
            self.hover = -1;
            self.selected = if self.simple || self.list.is_empty() { None } else { Some(0) };
        }
    }

    pub fn set_simple(&mut self, simple: bool) {
        self.simple = simple;
        if simple {
            self.selected = None;
        } else if self.selected.is_none() && !self.list.is_empty() {
            self.selected = Some(0);
        }
    }

    /// La fetta in evidenza: quella sotto il mouse, altrimenti la scelta.
    pub fn current(&self) -> Option<usize> {
        if self.hover >= 0 {
            Some(self.hover as usize).filter(|i| *i < self.list.len())
        } else {
            self.selected.filter(|i| *i < self.list.len())
        }
    }

    /// Vero se per la fetta in evidenza il motore usa la qualità.
    pub fn quality_applies(&self) -> bool {
        self.mode == 0 && self.current().is_some_and(|i| super::views::uses_quality(self.kind, self.list[i].id))
    }

    fn layout(&self) -> Layout {
        let fonts = Fonts::current();
        let cx = if self.simple { PAD + RO + 45.0 } else { PAD + RO };
        let cy = PAD + RO + if self.simple { 62.0 } else { 0.0 };
        let disc_bottom = cy + RO;
        let mut zones: Vec<(R, Zone)> = Vec::new();
        let mut panel = None;
        let (w, h);
        if self.simple {
            w = 2.0 * (PAD + RO) + 90.0;
            // Il selettore Semplificata / Standard sopra il disco.
            let seg = [("Semplificata", true, 188.0), ("Standard", false, 128.0)];
            let total: f32 = seg.iter().map(|s| s.2).sum();
            let mut x = (w - total) / 2.0;
            for (_, simple, sw) in seg {
                zones.push(((x, 20.0, sw, 44.0), Zone::Pill(simple)));
                x += sw;
            }
            // Sotto il disco: titolo, frase e la riga con la destinazione e Esc.
            let line_y = disc_bottom + 142.0;
            let a = fonts.measure(&crate::i18n::tr(&self.dest_note), 21.0, Weight::Regular) + 40.0;
            let esc = fonts.measure("Esc", 20.0, Weight::Regular) + 32.0 + 14.0 + fonts.measure(&crate::i18n::tr("per chiudere"), 21.0, Weight::Regular);
            let full = a + 38.0 + esc;
            let x0 = (w - full) / 2.0;
            zones.push(((x0 + a + 38.0, line_y, esc, 44.0), Zone::Esc));
            h = line_y + 44.0 + 22.0;
        } else {
            let px = cx + RO + 12.0;
            let pw = PANEL_W;
            let top = cy - 203.0;
            let mut y = top + 26.0 + 44.0 + 14.0;
            if self.quality_applies() {
                zones.push(((px + 26.0, y + 40.0, pw - 52.0 - 70.0, 36.0), Zone::Quality));
                y += 83.0 + 14.0;
            }
            let locked_note = self.beside != Beside::Locked;
            if self.has_file && self.mode != 2 {
                if locked_note {
                    zones.push(((px + 22.0, y, pw - 44.0, 60.0), Zone::Check));
                }
                y += 60.0 + 20.0;
            }
            zones.push(((px + 26.0, y, pw - 52.0, 56.0), Zone::Go));
            y += 56.0 + 13.0;
            zones.push(((px + 26.0, y, pw - 52.0, 55.0), Zone::OpenFull));
            y += 55.0 + 24.0;
            panel = Some((px, top, pw, y - top));
            w = px + pw + PAD;
            // I tasti sotto, centrati.
            let items = self.hint_items();
            let widths: Vec<f32> = items.iter().map(|(k, t, _)| hint_width(fonts, k, t)).collect();
            let total = widths.iter().sum::<f32>() + 34.0 * (items.len() as f32 - 1.0).max(0.0);
            let mut x = (w - total) / 2.0;
            let hy = disc_bottom + 36.0;
            for ((_, _, zone), iw) in items.iter().zip(&widths) {
                zones.push(((x, hy, *iw, 44.0), *zone));
                x += iw + 34.0;
            }
            // L'altezza è sempre quella del riquadro più alto (con la qualità), così la finestra non cambia misura.
            let tallest = top + 26.0 + 44.0 + 14.0 + (83.0 + 14.0) + (60.0 + 20.0) + 56.0 + 13.0 + 55.0 + 24.0;
            h = (hy + 44.0 + 22.0).max(tallest + 22.0);
        }
        Layout { w, h, cx, cy, zones, panel }
    }

    fn hint_items(&self) -> Vec<(&'static str, &'static str, Zone)> {
        let mut v = Vec::new();
        for (m, key, label) in [(0u8, "Shift", "Conversione"), (1, "Ctrl", "Strumenti"), (2, "Alt", "Azioni")] {
            if self.modes_on[m as usize] {
                v.push((key, label, Zone::Mode(m)));
            }
        }
        v.push(("Esc", "per chiudere", Zone::Esc));
        v
    }

    /// Misura della finestra, in unità della tavola.
    pub fn size(&self) -> (f32, f32) {
        let l = self.layout();
        (l.w, l.h)
    }

    /// Il centro del disco, in unità della tavola, dall'angolo della finestra.
    pub fn center(&self) -> (f32, f32) {
        let l = self.layout();
        (l.cx, l.cy)
    }

    fn slice_at(&self, l: &Layout, x: f32, y: f32) -> Zone {
        let n = self.list.len();
        let (dx, dy) = (x - l.cx, y - l.cy);
        let r = (dx * dx + dy * dy).sqrt();
        if r < RI - 4.0 {
            return Zone::Hole;
        }
        if r > RO + PUSH || n == 0 {
            return Zone::None;
        }
        let tau = std::f32::consts::TAU;
        let angle = (dy.atan2(dx) + std::f32::consts::FRAC_PI_2).rem_euclid(tau);
        let i = ((angle / (tau / n as f32)) as usize).min(n - 1);
        Zone::Slice(i)
    }

    /// Cosa c'è nel punto (x, y), in unità della tavola dall'angolo della finestra.
    pub fn zone_at(&self, x: f32, y: f32) -> Zone {
        let l = self.layout();
        self.zone_in(&l, x, y)
    }

    fn zone_in(&self, l: &Layout, x: f32, y: f32) -> Zone {
        for (r, z) in &l.zones {
            if x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3 {
                return *z;
            }
        }
        // I riquadri vengono prima del disco, ma il disco ha la precedenza dove le due cose si toccano.
        self.slice_at(l, x, y)
    }

    /// Il mouse si muove (anche con il tasto premuto). Vero se serve ridisegnare.
    pub fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        let l = self.layout();
        if self.drag {
            if let Some((r, _)) = l.zones.iter().find(|(_, z)| *z == Zone::Quality) {
                let q = quality_at(*r, x);
                let changed = q != self.quality;
                self.quality = q;
                return changed;
            }
        }
        let zone = self.zone_in(&l, x, y);
        let hover = match zone {
            Zone::Slice(i) => i as i32,
            _ => -1,
        };
        let changed = zone != self.zone || hover != self.hover;
        self.zone = zone;
        self.hover = hover;
        changed
    }

    pub fn pointer_leave(&mut self) -> bool {
        let changed = self.zone != Zone::None || self.hover != -1;
        self.zone = Zone::None;
        self.hover = -1;
        changed
    }

    /// Un clic con il tasto sinistro.
    pub fn press(&mut self, x: f32, y: f32) -> Effect {
        let l = self.layout();
        match self.zone_in(&l, x, y) {
            Zone::Slice(i) => {
                if self.simple {
                    Effect::Commit(i)
                } else if self.selected == Some(i) {
                    Effect::Redraw
                } else {
                    self.selected = Some(i);
                    Effect::Redraw
                }
            }
            Zone::Quality => {
                if let Some((r, _)) = l.zones.iter().find(|(_, z)| *z == Zone::Quality) {
                    self.quality = quality_at(*r, x);
                }
                self.drag = true;
                Effect::Redraw
            }
            Zone::Check => {
                self.beside = match self.beside {
                    Beside::On => Beside::Off,
                    Beside::Off => Beside::On,
                    Beside::Locked => Beside::Locked,
                };
                Effect::Redraw
            }
            Zone::Go => match self.current() {
                Some(i) => Effect::Commit(i),
                None => Effect::None,
            },
            Zone::OpenFull => Effect::OpenFull,
            Zone::Mode(m) => Effect::Mode(m),
            Zone::Pill(s) => Effect::Simple(s),
            Zone::Esc | Zone::None | Zone::Hole => Effect::Close,
        }
    }

    /// Il tasto sinistro è stato rilasciato.
    pub fn release(&mut self) {
        self.drag = false;
    }

    // ---- Disegno ----

    /// Disegna la ruota nell'immagine trasparente del pittore (scala già impostata dal chiamante).
    pub fn draw(&self, p: &mut Painter) {
        let l = self.layout();
        let active = self.current();
        self.draw_disc(p, &l, active);
        self.draw_center(p, &l, active);
        if self.simple {
            self.draw_simple(p, &l, active);
        } else {
            self.draw_panel(p, &l, active);
            self.draw_hints(p, &l);
        }
    }

    fn draw_disc(&self, p: &mut Painter, l: &Layout, active: Option<usize>) {
        let (cx, cy) = (l.cx, l.cy);
        if self.list.is_empty() {
            return;
        }
        disc_shapes(p, cx, cy, RI, RO, 1.0, self.list.len(), active);
        let n = self.list.len();
        let sweep = std::f32::consts::TAU / n as f32;
        for (i, slice) in self.list.iter().enumerate() {
            let on = active == Some(i);
            let mid = -std::f32::consts::FRAC_PI_2 + (i as f32 + 0.5) * sweep;
            let (ox, oy) = if on { (PUSH * mid.cos(), PUSH * mid.sin()) } else { (0.0, 0.0) };
            // Icona e nome, sul raggio di mezzo.
            let rm = (RI + RO) / 2.0 + 4.0;
            let (tx, ty) = (cx + ox + rm * mid.cos(), cy + oy + rm * mid.sin());
            let fg = if on { (255, 255, 255) } else { INK };
            p.icon(icon_of(slice.id), tx - 17.0, ty - 40.0, 34.0, fg);
            p.text(slice.label, tx - 60.0, ty + 6.0, 120.0, 30.0, 21.0, Weight::Semibold, fg, Align::Center);
        }
    }

    fn draw_center(&self, p: &mut Painter, l: &Layout, active: Option<usize>) {
        let (cx, cy) = (l.cx, l.cy);
        // Anello bianco con ombra attorno alla fotografia.
        p.disc_a(cx, cy + 6.0, HOLE + 10.0, (10, 40, 90), 0.10);
        p.disc_a(cx, cy, HOLE + 6.0, (255, 255, 255), 1.0);
        let d = HOLE * 2.0 - 8.0;
        let (ix, iy) = (cx - d / 2.0, cy - d / 2.0);
        match &self.thumb {
            Some(img) => p.image(img, ix, iy, d, d, d / 2.0, Fit::Cover),
            None => {
                p.disc_a(cx, cy, d / 2.0, (232, 241, 252), 1.0);
                p.icon(kind_icon(self.kind), cx - 28.0, cy - 54.0, 56.0, ACC);
            }
        }
        // La fascia bianca in basso, con il nome e cosa succederà.
        let seg_top = cy + 34.0;
        let r = d / 2.0;
        let mut pts = Vec::new();
        let a_top = ((seg_top - cy) / r).asin();
        let steps = 40;
        for k in 0..=steps {
            let a = std::f32::consts::PI - a_top - (std::f32::consts::PI - 2.0 * a_top) * k as f32 / steps as f32;
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
        p.poly(&pts, (255, 255, 255), 0.96);
        let name = if self.has_file { clip(p, &self.name, 18.0, Weight::Bold, 168.0) } else { "In attesa…".to_string() };
        p.text(&name, cx - 90.0, cy + 40.0, 180.0, 26.0, 18.0, Weight::Bold, INK, Align::Center);
        let sub = match (active, self.simple, self.mode) {
            (Some(i), true, _) => self.list[i].label.to_string(),
            (Some(i), false, 0) => format!("Converti in {}", self.list[i].label),
            (Some(i), false, 1) => format!("Applica: {}", self.list[i].label),
            (Some(i), false, _) => self.list[i].label.to_string(),
            (None, _, 0) => "Scegli un formato".to_string(),
            (None, _, 1) => "Scegli uno strumento".to_string(),
            (None, _, _) => "Scegli un'azione".to_string(),
        };
        p.text(&sub, cx - 90.0, cy + 66.0, 180.0, 22.0, 14.5, Weight::Regular, MUTED, Align::Center);
    }

    fn draw_panel(&self, p: &mut Painter, l: &Layout, active: Option<usize>) {
        let Some((px, py, pw, ph)) = l.panel else { return };
        p.shadow(px, py, pw, ph, 18.0, 26.0, 0.16, 8.0);
        p.rrect(px, py, pw, ph, 18.0, Some(CARD), Some((LINE, 1.0)));
        let title = match active {
            Some(i) => self.list[i].label.to_string(),
            None => "Scegli".to_string(),
        };
        p.text(&title, px + 26.0, py + 22.0, pw - 52.0, 44.0, 34.0, Weight::Semibold, INK, Align::Left);
        let mut y = py + 26.0 + 44.0 + 14.0;
        if self.quality_applies() {
            p.text("Qualità", px + 26.0, y, 120.0, 28.0, 21.0, Weight::Regular, INK, Align::Left);
            let slider = l.zones.iter().find(|(_, z)| *z == Zone::Quality).map(|(r, _)| *r).unwrap_or((px + 26.0, y + 40.0, 200.0, 36.0));
            let frac = (self.quality as f32 - 40.0) / 55.0;
            let ty = slider.1 + slider.3 / 2.0;
            p.rrect(slider.0, ty - 3.0, slider.2, 6.0, 3.0, Some((212, 224, 238)), None);
            p.rrect(slider.0, ty - 3.0, slider.2 * frac, 6.0, 3.0, Some(ACC), None);
            let kx = slider.0 + slider.2 * frac;
            p.disc_a(kx, ty + 2.0, 15.0, (0, 40, 120), 0.18);
            p.circle(kx, ty, 14.0, Some((255, 255, 255)), Some((ACC, 3.0)));
            p.rrect(px + pw - 26.0 - 56.0, y + 36.0, 56.0, 44.0, 8.0, Some((255, 255, 255)), Some((LINE, 1.0)));
            p.text(&self.quality.to_string(), px + pw - 26.0 - 56.0, y + 36.0, 56.0, 44.0, 20.0, Weight::Regular, INK, Align::Center);
            y += 83.0 + 14.0;
            p.fill(px + 26.0, y - 8.0, pw - 52.0, 1.0, LINE);
        }
        if self.has_file && self.mode != 2 {
            let on = self.beside != Beside::Off;
            let hot = self.zone == Zone::Check && self.beside != Beside::Locked;
            if hot {
                p.rrect(px + 16.0, y - 4.0, pw - 32.0, 62.0, 10.0, Some((238, 244, 251)), None);
            }
            let (bx, by) = (px + 28.0, y + 2.0);
            if on {
                p.rrect(bx, by, 26.0, 26.0, 6.0, Some(if self.beside == Beside::Locked { (120, 160, 220) } else { ACC }), None);
                p.icon(assets::CHECK, bx + 3.0, by + 3.0, 20.0, (255, 255, 255));
            } else {
                p.rrect(bx, by, 26.0, 26.0, 6.0, Some((255, 255, 255)), Some((MUTED, 2.0)));
            }
            p.text("Salva accanto all'originale", px + 66.0, y, pw - 76.0, 30.0, 19.0, Weight::Regular, INK, Align::Left);
            let sub = if on { "L'originale resta al sicuro." } else { "Va dove dicono le Impostazioni." };
            p.text(sub, px + 68.0, y + 28.0, pw - 90.0, 26.0, 17.0, Weight::Regular, MUTED, Align::Left);
            y += 60.0 + 20.0;
        }
        // Il pulsante principale.
        let go_label = match self.mode {
            0 => "Converti",
            1 => "Applica",
            _ => "Esegui",
        };
        let go = l.zones.iter().find(|(_, z)| *z == Zone::Go).map(|(r, _)| *r).unwrap_or((px + 26.0, y, pw - 52.0, 56.0));
        let ready = active.is_some();
        let hot = self.zone == Zone::Go && ready;
        p.rrect(go.0, go.1, go.2, go.3, 10.0, Some(if !ready { (150, 190, 240) } else if hot { ACC_DARK } else { ACC }), None);
        let tw = p.measure(go_label, 22.0, Weight::Semibold);
        let ix = go.0 + (go.2 - tw - 38.0) / 2.0;
        p.icon(assets::CONVERT, ix, go.1 + 15.0, 26.0, (255, 255, 255));
        p.text(go_label, ix + 38.0, go.1, tw + 6.0, go.3, 22.0, Weight::Semibold, (255, 255, 255), Align::Left);
        // «Apri Conversioni».
        let full = l.zones.iter().find(|(_, z)| *z == Zone::OpenFull).map(|(r, _)| *r).unwrap_or((px + 26.0, y + 69.0, pw - 52.0, 55.0));
        let hot = self.zone == Zone::OpenFull;
        p.rrect(full.0, full.1, full.2, full.3, 10.0, Some(if hot { (238, 244, 251) } else { (255, 255, 255) }), Some((LINE, 1.5)));
        let label = "Apri Conversioni";
        let tw = p.measure(label, 21.0, Weight::Regular);
        let ix = full.0 + (full.2 - tw - 38.0) / 2.0;
        p.icon(assets::FOLDER, ix, full.1 + 14.0, 26.0, INK);
        p.text(label, ix + 38.0, full.1, tw + 6.0, full.3, 21.0, Weight::Regular, INK, Align::Left);
    }

    fn draw_hints(&self, p: &mut Painter, l: &Layout) {
        let sep = LINE;
        let items = self.hint_items();
        let mut last_end = None;
        for (_, _, zone) in &items {
            let Some((r, _)) = l.zones.iter().find(|(_, z)| z == zone) else { continue };
            if let Some(prev) = last_end {
                p.fill(prev + 17.0, r.1 + 6.0, 1.0, 32.0, sep);
            }
            last_end = Some(r.0 + r.2);
        }
        for (key, label, zone) in items {
            let Some((r, _)) = l.zones.iter().find(|(_, z)| *z == zone) else { continue };
            let on = matches!(zone, Zone::Mode(m) if m == self.mode);
            let hot = self.zone == zone;
            self.keycap(p, r.0, r.1, key, on, hot);
            let kw = p.measure(key, 20.0, Weight::Regular) + 32.0;
            p.text(label, r.0 + kw + 14.0, r.1, r.2 - kw - 14.0, r.3, 21.0, if on { Weight::Semibold } else { Weight::Regular }, if on { ACC } else { MUTED }, Align::Left);
        }
    }

    /// Un tasto disegnato, come nella tavola.
    fn keycap(&self, p: &mut Painter, x: f32, y: f32, key: &str, on: bool, hot: bool) {
        let w = p.measure(key, 20.0, Weight::Regular) + 32.0;
        p.rrect(x, y, w, 44.0, 10.0, Some(if hot { (238, 244, 251) } else { (252, 253, 254) }), Some((if on { ACC } else { (190, 206, 222) }, if on { 2.0 } else { 1.5 })));
        p.text(key, x, y, w, 44.0, 20.0, Weight::Regular, if on { ACC } else { INK }, Align::Center);
    }

    fn draw_simple(&self, p: &mut Painter, l: &Layout, active: Option<usize>) {
        let w = l.w;
        // Selettore Semplificata / Standard.
        let segs: Vec<(R, bool)> = l.zones.iter().filter_map(|(r, z)| if let Zone::Pill(s) = z { Some((*r, *s)) } else { None }).collect();
        if let (Some(first), Some(last)) = (segs.first(), segs.last()) {
            let (x0, y0, total) = (first.0 .0, first.0 .1, last.0 .0 + last.0 .2 - first.0 .0);
            p.shadow(x0, y0, total, 44.0, 22.0, 10.0, 0.10, 3.0);
            p.rrect(x0, y0, total, 44.0, 22.0, Some((255, 255, 255)), Some((LINE, 1.0)));
            for (r, simple) in &segs {
                let on = *simple == self.simple;
                if on {
                    p.rrect(r.0, r.1, r.2, r.3, 22.0, Some(ACC), None);
                }
                let label = if *simple { "Semplificata" } else { "Standard" };
                let fg = if on { (255, 255, 255) } else { INK };
                if *simple {
                    p.icon(assets::CONVERT, r.0 + 22.0, r.1 + 11.0, 22.0, fg);
                    p.text(label, r.0 + 52.0, r.1, r.2 - 52.0, r.3, 20.0, Weight::Semibold, fg, Align::Left);
                } else {
                    p.text(label, r.0, r.1, r.2, r.3, 20.0, if on { Weight::Semibold } else { Weight::Regular }, fg, Align::Center);
                }
            }
        }
        // Titolo e frase.
        let (head, note) = match self.mode {
            0 => ("Clicca un formato per convertire.", "La conversione usa le impostazioni consigliate."),
            1 => ("Clicca uno strumento da applicare.", "Lo strumento usa le impostazioni consigliate."),
            _ => ("Clicca un'azione.", "L'originale resta sempre dov'è."),
        };
        let top = l.cy + RO + 38.0;
        p.text(head, 0.0, top, w, 44.0, 32.0, Weight::Semibold, INK, Align::Center);
        p.text(note, 0.0, top + 50.0, w, 30.0, 22.0, Weight::Regular, MUTED, Align::Center);
        // Riga con la destinazione e Esc.
        let line_y = l.cy + RO + 142.0;
        let a = p.measure(&self.dest_note, 21.0, Weight::Regular) + 40.0;
        let esc_zone = l.zones.iter().find(|(_, z)| *z == Zone::Esc).map(|(r, _)| *r).unwrap_or((0.0, line_y, 0.0, 44.0));
        let x0 = esc_zone.0 - 38.0 - a;
        p.icon(assets::FOLDER, x0, line_y + 8.0, 28.0, MUTED);
        p.text(&self.dest_note, x0 + 40.0, line_y, a, 44.0, 21.0, Weight::Regular, MUTED, Align::Left);
        p.fill(x0 + a + 18.0, line_y + 6.0, 1.0, 32.0, LINE);
        let hot = self.zone == Zone::Esc;
        self.keycap(p, esc_zone.0, esc_zone.1, "Esc", false, hot);
        let kw = p.measure("Esc", 20.0, Weight::Regular) + 32.0;
        p.text("per chiudere", esc_zone.0 + kw + 14.0, esc_zone.1, esc_zone.2 - kw - 14.0, 44.0, 21.0, Weight::Regular, MUTED, Align::Left);
        let _ = active;
    }
}

/// Le forme del disco (ombra e fette, senza scritte) tra i raggi `ri` e `ro`; `k` porta le misure fisse
/// (spazio, angoli, sporgenza) alla grandezza del disco. Le usa anche la Guida, che ci scrive sopra a modo suo.
pub fn disc_shapes(p: &mut Painter, cx: f32, cy: f32, ri: f32, ro: f32, k: f32, n: usize, active: Option<usize>) {
    for i in 0..14 {
        let t = i as f32 / 14.0;
        p.disc_a(cx, cy + 8.0 * k, ro + 22.0 * k * (1.0 - t), (10, 40, 90), 0.012 + 0.010 * t);
    }
    let n = n.max(1);
    let (corner, gap, push) = (CORNER * k, HALF_GAP * k, PUSH * k);
    let sweep = std::f32::consts::TAU / n as f32;
    // Prima le fette ferme, poi quella in evidenza (sporge e sta sopra le altre).
    for pass in 0..2 {
        for i in 0..n {
            let on = active == Some(i);
            if on != (pass == 1) {
                continue;
            }
            let a0 = -std::f32::consts::FRAC_PI_2 + i as f32 * sweep;
            let a1 = a0 + sweep;
            let mid = (a0 + a1) / 2.0;
            let (ox, oy) = if on { (push * mid.cos(), push * mid.sin()) } else { (0.0, 0.0) };
            let r1 = ro + if on { 6.0 * k } else { 0.0 };
            if on {
                // Ombra della fetta sporgente.
                for j in 0..5 {
                    let grow = (10.0 - j as f32 * 2.0) * k;
                    let pts = sector(cx + ox, cy + oy + 5.0 * k, ri + corner - grow, r1 - corner + grow, a0, a1, gap + corner - grow);
                    p.round_poly(&pts, corner, (0, 40, 120), 0.035);
                }
            }
            let pts = sector(cx + ox, cy + oy, ri + corner, r1 - corner, a0, a1, gap + corner);
            p.round_poly(&pts, corner, if on { ACC } else { SLICE }, 1.0);
        }
    }
}

fn mode_of(m: u8) -> Mode {
    match m {
        1 => Mode::Tools,
        2 => Mode::Actions,
        _ => Mode::Convert,
    }
}

fn quality_at(slider: R, x: f32) -> u8 {
    let frac = ((x - slider.0) / slider.2).clamp(0.0, 1.0);
    (40.0 + frac * 55.0).round() as u8
}

fn hint_width(fonts: &Fonts, key: &str, label: &str) -> f32 {
    let kw = fonts.measure(key, 20.0, Weight::Regular) + 32.0;
    kw + 14.0 + fonts.measure(&crate::i18n::tr(label), 21.0, Weight::Semibold)
}

fn clip(p: &Painter, text: &str, size: f32, weight: Weight, w: f32) -> String {
    p.ellipsize(text, size, weight, w)
}

/// Il contorno della fetta tra due raggi e due angoli, con uno spazio costante `gap` da ogni lato:
/// i lati restano paralleli al raggio. Con gli angoli arrotondati si passa il contorno già rimpicciolito.
fn sector(cx: f32, cy: f32, r0: f32, r1: f32, a0: f32, a1: f32, gap: f32) -> Vec<(f32, f32)> {
    let mut pts = Vec::new();
    let steps = 24;
    let (s0, s1) = ((gap / r1.max(gap + 1.0)).asin(), (gap / r0.max(gap + 1.0)).asin());
    for k in 0..=steps {
        let a = a0 + s0 + (a1 - a0 - 2.0 * s0) * k as f32 / steps as f32;
        pts.push((cx + r1 * a.cos(), cy + r1 * a.sin()));
    }
    for k in (0..=steps).rev() {
        let a = a0 + s1 + (a1 - a0 - 2.0 * s1) * k as f32 / steps as f32;
        pts.push((cx + r0 * a.cos(), cy + r0 * a.sin()));
    }
    pts
}

/// L'icona di una fetta, dal suo identificativo.
fn icon_of(id: &str) -> &'static str {
    match id {
        "webp" | "bmp" | "gray" => assets::IMAGE,
        "gif" | "mp4" | "webm" | "mov" | "mkv" | "avi" => assets::VIDEO,
        "wav" | "mp3" | "ogg" | "m4a" | "aac" | "flac" => assets::AUDIO,
        "compress" | "half" | "strip" => assets::DOWNLOAD,
        "rot90" => assets::CONVERT,
        "ocr" => assets::SEARCH,
        "dup" => assets::COPY,
        "path" => assets::LINK,
        "zip" => assets::FOLDER,
        "upper" | "lower" => assets::TRANSCRIBE,
        _ => assets::FILE,
    }
}

fn kind_icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => assets::IMAGE,
        Kind::Audio => assets::AUDIO,
        Kind::Video => assets::VIDEO,
        _ => assets::FILE,
    }
}

/// Un nome di file corto per la scritta al centro.
pub fn file_name_of(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_order_follows_the_board_and_ids_stay_attached() {
        let l = list(Kind::Image, Mode::Convert);
        let ids: Vec<&str> = l.iter().map(|s| s.id).collect();
        assert_eq!(ids, IMAGE_ORDER);
        assert_eq!(l[0].label, "JPEG");
    }

    #[test]
    fn hit_test_matches_drawing_order() {
        let ui = WheelUi::new(Kind::Image, 0, false);
        let (cx, cy) = ui.center();
        // Un punto sul raggio di mezzo, a un quarto di giro in senso orario da mezzogiorno,
        // sta nella terza fetta (BMP) delle sette: 90° / (360/7) = 1.75 → indice 1... verifichiamo per angoli noti.
        let rm = (RI + RO) / 2.0;
        let n = ui.list.len() as f32;
        for i in 0..ui.list.len() {
            let mid = -std::f32::consts::FRAC_PI_2 + (i as f32 + 0.5) * std::f32::consts::TAU / n;
            let z = ui.zone_at(cx + rm * mid.cos(), cy + rm * mid.sin());
            assert_eq!(z, Zone::Slice(i));
        }
        assert_eq!(ui.zone_at(cx, cy), Zone::Hole);
    }

    #[test]
    fn simple_click_commits_standard_selects() {
        let mut ui = WheelUi::new(Kind::Image, 0, false);
        let (cx, cy) = ui.center();
        let rm = (RI + RO) / 2.0;
        let n = ui.list.len() as f32;
        let mid = -std::f32::consts::FRAC_PI_2 + 2.5 * std::f32::consts::TAU / n;
        let (x, y) = (cx + rm * mid.cos(), cy + rm * mid.sin());
        assert_eq!(ui.press(x, y), Effect::Redraw);
        assert_eq!(ui.selected, Some(2));
        ui.set_simple(true);
        let (cx, cy) = ui.center();
        let (x, y) = (cx + rm * mid.cos(), cy + rm * mid.sin());
        assert_eq!(ui.press(x, y), Effect::Commit(2));
    }
}
