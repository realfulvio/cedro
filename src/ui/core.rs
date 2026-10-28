//! I pezzi comuni dei temi: le azioni che un clic può fare, le zone cliccabili raccolte mentre si
//! disegna, e i controlli (pulsanti, scelte, levette, cursori, campi) con i colori del tema.
//! Ogni fotogramma si ridisegna tutto e si raccolgono di nuovo le zone: niente stato nascosto.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::assets as ic;
use super::gfx::{Align, Fit, Painter, Rgb, Weight};
use super::model::{Activity, App, FileState};
use super::theme::{mix, Palette, ThemeId};
use crate::conversioni::Opt;
use crate::engine::{ExistsChoice, Kind};

pub type R = (f32, f32, f32, f32);

pub fn inside(r: R, x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

/// Le scelte dei download che si aprono in un elenco.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum LinkMenu {
    Quality,
    Format,
    Subtitles,
}

/// Le spunte dei download.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum LinkFlag {
    Thumbnail,
    Chapters,
    Split,
    Sponsors,
    Normalize,
    Playlist,
    SkipExisting,
}

/// I campi in cui si scrive.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Field {
    Opt(Opt),
    Link(bool),
}

/// Le parti che si aprono e si chiudono («Altre opzioni»…).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Fold {
    MoreOptions,
    LinkMore,
    Log,
}

/// Le zone che scorrono.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Scroll {
    Queue,
    Inspector,
    Links,
    Log,
    Preview,
    Page,
}

/// Cosa fa un clic.
#[derive(Clone, PartialEq, Debug)]
pub enum Act {
    Activity(Activity),
    Category(usize),
    Action(&'static str),
    FormatMenu,
    AddFiles,
    AddFolder,
    AddMenu,
    Remove(usize),
    RemoveChecked,
    Check(usize),
    CheckAll,
    Select(usize),
    OptSwitch(Opt),
    OptMenu(Opt),
    OptPick(Opt, u32),
    OptSlider(Opt),
    Edit(Field),
    /// Menu del tasto destro in un campo di testo.
    FieldCut,
    FieldCopy,
    FieldPaste,
    FieldClear,
    FieldSelectAll,
    Quality,
    DestMenu,
    DestSame,
    DestPick,
    ExistsMenu,
    ExistsPick(usize),
    Start,
    Cancel,
    Fold(Fold),
    SetSimple(bool),
    ResetRecommended,
    OpenGuide,
    OpenSettings,
    OpenPath(PathBuf),
    Minimize,
    Maximize,
    Close,
    Step(u8),
    Next,
    Back,
    LinkAdd(bool),
    LinkRemove(bool, usize),
    LinkAudio(bool, bool),
    LinkMenu(bool, LinkMenu),
    LinkPick(bool, LinkMenu, usize),
    LinkFlag(bool, LinkFlag),
    LinkDest(bool),
    CopyLog,
    TrFormat(bool),
    TrModelMenu,
    TrModel(usize),
    PagePrev,
    PageNext,
    /// Play o pausa dell'audio o del video in anteprima.
    MediaPlay,
    /// La barra della posizione dell'audio o del video (si trascina).
    MediaSeek,
    CompactTab(u8),
    OpenFull,
    Theme(ThemeId),
    Answer(ExistsChoice),
    DismissNotice,
    MenuPick(usize),
    ActivityMenu,
    /// Solo per bloccare i clic sotto un riquadro (per esempio dentro un menu aperto).
    Nothing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Button,
    Text,
    Slider,
}

pub struct Hit {
    pub r: R,
    pub act: Act,
    pub kind: HitKind,
}

/// Un elenco aperto sotto una scelta.
pub struct Menu {
    pub anchor: R,
    pub items: Vec<(String, Act)>,
    pub selected: Option<usize>,
}

/// Lo stato della finestra che non è del lavoro: dove sta il mouse, cosa è aperto, quanto si è scorso.
#[derive(Default)]
pub struct UiState {
    pub hover: Option<Act>,
    pub pressed: Option<Act>,
    pub menu: Option<Menu>,
    pub edit: Option<Field>,
    /// Posizione del cursore di scrittura, in caratteri.
    pub caret: usize,
    /// Tutto il testo del campo è selezionato.
    pub sel_all: bool,
    pub scroll: HashMap<Scroll, f32>,
    pub open: HashSet<Fold>,
    /// Il cursore che si sta trascinando, con il suo riquadro.
    pub drag: Option<(Act, R)>,
    /// Dei file passano sopra la finestra.
    pub dropping: bool,
    /// Il passo del Percorso guidato (0 file, 1 operazione, 2 salvataggio).
    pub step: u8,
    /// La scheda della finestrella compatta (0 file, 1 link, 2 trascrivi).
    pub compact_tab: u8,
    /// La finestra è ingrandita (per l'icona del pulsante).
    pub maximized: bool,
    /// Il controllo con il fuoco della tastiera (indice tra quelli raggiungibili col Tab).
    pub focus: Option<usize>,
    pub focus_act: Option<Act>,
}

impl UiState {
    pub fn scroll(&self, s: Scroll) -> f32 {
        self.scroll.get(&s).copied().unwrap_or(0.0)
    }

    pub fn is_open(&self, f: Fold) -> bool {
        self.open.contains(&f)
    }
}

/// Un fotogramma: si disegna e intanto si annotano le zone cliccabili.
pub struct Frame<'a> {
    pub p: Painter<'a>,
    pub hits: Vec<Hit>,
    /// Zone da cui si trascina la finestra (la barra del titolo disegnata).
    pub captions: Vec<R>,
    /// Zone che scorrono: riquadro, quale, quanto si può scorrere al massimo.
    pub scrolls: Vec<(R, Scroll, f32)>,
    pub ui: &'a UiState,
    pub pal: &'static Palette,
    pub theme: ThemeId,
    pub simple: bool,
    pub w: f32,
    pub h: f32,
    /// Millisecondi da quando la finestra è aperta (per le animazioni).
    pub ms: u64,
    pub animating: bool,
}

pub const SANS: Weight = Weight::Regular;
pub const SEMI: Weight = Weight::Semibold;
pub const BOLD: Weight = Weight::Bold;

impl<'a> Frame<'a> {
    pub fn hit(&mut self, r: R, act: Act) {
        self.hits.push(Hit { r, act, kind: HitKind::Button });
    }

    pub fn hit_kind(&mut self, r: R, act: Act, kind: HitKind) {
        self.hits.push(Hit { r, act, kind });
    }

    pub fn caption(&mut self, r: R) {
        self.captions.push(r);
    }

    pub fn hovered(&self, act: &Act) -> bool {
        self.ui.hover.as_ref() == Some(act)
    }

    fn focused(&self, act: &Act) -> bool {
        self.ui.focus_act.as_ref() == Some(act)
    }

    /// L'anello del fuoco della tastiera attorno a un controllo.
    pub fn focus_ring(&mut self, r: R, radius: f32, act: &Act) {
        if self.focused(act) {
            self.p.rrect_line(r.0 - 3.0, r.1 - 3.0, r.2 + 6.0, r.3 + 6.0, radius + 3.0, self.pal.primary, 2.0, 1.0, None);
        }
    }

    pub fn text(&mut self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, weight: Weight, c: Rgb) {
        self.p.text(s, x, y, w, h, size, weight, c, Align::Left);
    }

    pub fn text_c(&mut self, s: &str, r: R, size: f32, weight: Weight, c: Rgb) {
        self.p.text(s, r.0, r.1, r.2, r.3, size, weight, c, Align::Center);
    }

    pub fn text_r(&mut self, s: &str, r: R, size: f32, weight: Weight, c: Rgb) {
        self.p.text(s, r.0, r.1, r.2, r.3, size, weight, c, Align::Right);
    }

    /// Testo a capo; restituisce l'altezza.
    pub fn para(&mut self, s: &str, x: f32, y: f32, w: f32, size: f32, weight: Weight, c: Rgb, lh: f32, max_lines: usize) -> f32 {
        self.p.text_wrap(s, x, y, w, size, weight, c, size * lh, max_lines)
    }

    pub fn measure(&self, s: &str, size: f32, weight: Weight) -> f32 {
        self.p.measure(s, size, weight)
    }

    pub fn icon(&mut self, d: &str, x: f32, y: f32, size: f32, c: Rgb) {
        self.p.icon(d, x, y, size, c);
    }

    pub fn card(&mut self, r: R, radius: f32) {
        let pal = self.pal;
        self.p.rrect(r.0, r.1, r.2, r.3, radius, Some(pal.surface), Some((pal.border, 1.0)));
    }

    pub fn hline(&mut self, x: f32, y: f32, w: f32) {
        self.p.fill(x, y, w, 1.0, self.pal.border);
    }

    pub fn vline(&mut self, x: f32, y: f32, h: f32) {
        self.p.fill(x, y, 1.0, h, self.pal.border);
    }

    /// Il pulsante principale: pieno del colore del tema, con un'icona a sinistra del testo.
    pub fn primary(&mut self, r: R, label: &str, icon: Option<&str>, size: f32, act: Act, enabled: bool) {
        let pal = self.pal;
        let hover = enabled && self.hovered(&act);
        let bg = if !enabled { mix(pal.primary, pal.surface, 0.55) } else if hover { mix(pal.primary, (0, 0, 0), 0.12) } else { pal.primary };
        self.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(bg), None);
        let isz = (size * 1.35).round();
        let tw = self.measure(label, size, SEMI);
        let total = tw + if icon.is_some() { isz + 14.0 } else { 0.0 };
        let mut x = r.0 + (r.2 - total) / 2.0;
        if let Some(d) = icon {
            self.icon(d, x, r.1 + (r.3 - isz) / 2.0, isz, pal.on_primary);
            x += isz + 14.0;
        }
        self.p.text(label, x, r.1, tw + 2.0, r.3, size, SEMI, pal.on_primary, Align::Left);
        self.focus_ring(r, 8.0, &act);
        if enabled {
            self.hit(r, act);
        }
    }

    /// Pulsante secondario: fondo della superficie, bordo, testo normale.
    pub fn button(&mut self, r: R, label: &str, icon: Option<&str>, size: f32, act: Act) {
        let pal = self.pal;
        let hover = self.hovered(&act);
        self.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(if hover { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
        let isz = (size * 1.3).round();
        let tw = self.measure(label, size, SANS);
        let total = tw + if icon.is_some() { isz + 10.0 } else { 0.0 };
        let mut x = r.0 + (r.2 - total) / 2.0;
        if let Some(d) = icon {
            self.icon(d, x, r.1 + (r.3 - isz) / 2.0, isz, pal.text);
            x += isz + 10.0;
        }
        self.p.text(label, x, r.1, tw + 2.0, r.3, size, SANS, pal.text, Align::Left);
        self.focus_ring(r, 8.0, &act);
        self.hit(r, act);
    }

    /// Un comando senza riquadro: icona e testo (barra degli strumenti).
    pub fn tool(&mut self, x: f32, y: f32, h: f32, label: &str, icon: &str, size: f32, act: Act, enabled: bool) -> f32 {
        let pal = self.pal;
        let tw = self.measure(label, size, SANS);
        let w = 24.0 + 12.0 + tw + 12.0;
        let r = (x - 6.0, y, w, h);
        if enabled && self.hovered(&act) {
            self.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(pal.alt), None);
        }
        let c = if enabled { pal.text } else { pal.muted };
        self.icon(icon, x, y + (h - 24.0) / 2.0, 24.0, c);
        self.p.text(label, x + 36.0, y, tw + 2.0, h, size, SANS, c, Align::Left);
        if enabled {
            self.hit(r, act);
        }
        w
    }

    /// Una scelta chiusa (si apre in un elenco al clic).
    pub fn select(&mut self, r: R, label: &str, icon: Option<&str>, size: f32, act: Act) {
        let pal = self.pal;
        let open = self.ui.menu.as_ref().is_some_and(|m| m.anchor == r);
        let hover = self.hovered(&act);
        self.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(pal.surface), Some((if open || hover { pal.primary } else { pal.border }, 1.0)));
        let pad = if r.3 > 56.0 { 26.0 } else { 16.0 };
        let mut x = r.0 + pad;
        if let Some(d) = icon {
            let isz = (size * 1.2).round();
            self.icon(d, x, r.1 + (r.3 - isz) / 2.0, isz, pal.text);
            x += isz + 22.0;
        }
        let chev = (size * 1.05).round();
        self.p.text(label, x, r.1, r.0 + r.2 - pad - chev - 8.0 - x, r.3, size, SANS, pal.text, Align::Left);
        self.icon(ic::CHEVRON_DOWN, r.0 + r.2 - pad - chev, r.1 + (r.3 - chev) / 2.0, chev, pal.text);
        self.focus_ring(r, 6.0, &act);
        self.hit(r, act);
    }

    /// La levetta acceso/spento (52×28 come nelle tavole, oppure della misura data).
    pub fn switch(&mut self, x: f32, y: f32, w: f32, h: f32, on: bool, act: Act, on_color: Option<Rgb>) {
        let pal = self.pal;
        let r = (x, y, w, h);
        let knob = h - 8.0;
        if on {
            self.p.rrect(x, y, w, h, h / 2.0, Some(on_color.unwrap_or(pal.primary)), None);
            let kc = if on_color.is_some() { (255, 255, 255) } else { pal.on_primary };
            self.p.circle(x + w - 4.0 - knob / 2.0, y + h / 2.0, knob / 2.0, Some(kc), None);
        } else {
            self.p.rrect(x, y, w, h, h / 2.0, None, Some((pal.muted, 2.0)));
            self.p.circle(x + 4.0 + knob / 2.0, y + h / 2.0, knob / 2.0 - 1.0, Some(pal.muted), None);
        }
        self.focus_ring(r, h / 2.0, &act);
        self.hit(r, act);
    }

    /// La casella da spuntare.
    pub fn checkbox(&mut self, x: f32, y: f32, s: f32, on: bool, act: Option<Act>) {
        let pal = self.pal;
        if on {
            self.p.rrect(x, y, s, s, 4.0, Some(pal.primary), None);
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(x + s * 0.24, y + s * 0.52);
            pb.line_to(x + s * 0.43, y + s * 0.70);
            pb.line_to(x + s * 0.78, y + s * 0.32);
            if let Some(path) = pb.finish() {
                self.p.stroke_path(&path, pal.on_primary, (s * 0.12).max(2.0));
            }
        } else {
            self.p.rrect(x, y, s, s, 4.0, None, Some((pal.muted, 2.0)));
        }
        if let Some(a) = act {
            self.hit((x - 8.0, y - 8.0, s + 16.0, s + 16.0), a);
        }
    }

    /// Il pallino di una scelta esclusiva.
    pub fn radio(&mut self, cx: f32, cy: f32, r: f32, on: bool, grey: Rgb) {
        let pal = self.pal;
        self.p.circle(cx, cy, r, None, Some((if on { pal.primary } else { grey }, 2.0)));
        if on {
            self.p.circle(cx, cy, r * 0.5, Some(pal.primary), None);
        }
    }

    /// Il cursore orizzontale: `frac` 0..1. Si trascina (vedi `HitKind::Slider`).
    pub fn slider(&mut self, r: R, frac: f32, act: Act) {
        let pal = self.pal;
        let cy = r.1 + r.3 / 2.0;
        let frac = frac.clamp(0.0, 1.0);
        self.p.rrect(r.0, cy - 2.5, r.2, 5.0, 2.5, Some(pal.border), None);
        self.p.rrect(r.0, cy - 2.5, r.2 * frac, 5.0, 2.5, Some(pal.primary), None);
        let kx = r.0 + r.2 * frac;
        self.p.circle(kx, cy, 14.0, Some(pal.surface), None);
        self.p.circle(kx, cy, 10.0, Some(pal.primary), None);
        self.focus_ring(r, 4.0, &act);
        self.hit_kind((r.0 - 10.0, r.1 - 6.0, r.2 + 20.0, r.3 + 12.0), act, HitKind::Slider);
    }

    /// Il riquadro con un numero (accanto al cursore della qualità).
    pub fn numbox(&mut self, r: R, text: &str, size: f32) {
        let pal = self.pal;
        self.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(pal.surface), Some((pal.border, 1.0)));
        self.text_c(text, r, size, SANS, pal.text);
    }

    /// Un campo in cui si scrive. `masked` per le password.
    pub fn field(&mut self, r: R, field: Field, text: &str, placeholder: &str, size: f32, masked: bool) {
        let pal = self.pal;
        let editing = self.ui.edit == Some(field);
        self.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(pal.surface), Some((if editing { pal.primary } else { pal.border }, if editing { 2.0 } else { 1.0 })));
        let shown: String = if masked { "•".repeat(text.chars().count()) } else { text.to_string() };
        let pad = 14.0;
        self.p.push_clip(r.0 + 2.0, r.1, r.2 - 4.0, r.3);
        if editing && self.ui.sel_all && !shown.is_empty() {
            let tw = self.measure(&shown, size, SANS).min(r.2 - 2.0 * pad);
            self.p.rrect_a(r.0 + pad - 2.0, r.1 + r.3 * 0.2, tw + 4.0, r.3 * 0.6, 3.0, pal.primary, 0.35);
        }
        // Se il testo è più lungo del campo, si vede la parte dove si scrive.
        let caret_text: String = shown.chars().take(self.ui.caret.min(shown.chars().count())).collect();
        let caret_x = self.measure(&caret_text, size, SANS);
        let room = r.2 - 2.0 * pad;
        let shift = if editing && caret_x > room { caret_x - room } else { 0.0 };
        if shown.is_empty() && !editing {
            self.p.text(placeholder, r.0 + pad, r.1, room, r.3, size, SANS, pal.muted, Align::Left);
        } else {
            let (asc, desc) = self.p.fonts().extents(size * self.p.scale, SANS);
            let base = r.1 + (r.3 - (asc + desc) / self.p.scale) / 2.0 + asc / self.p.scale;
            self.p.text_at(&shown, r.0 + pad - shift, base, size, SANS, pal.text);
            if editing && (self.ms / 530).is_multiple_of(2) {
                self.p.fill(r.0 + pad - shift + caret_x, r.1 + r.3 * 0.22, 1.5, r.3 * 0.56, pal.text);
            }
        }
        self.p.pop_clip();
        if editing {
            self.animating = true;
        }
        self.hit_kind(r, Act::Edit(field), HitKind::Text);
    }

    /// La rotellina di «in corso».
    pub fn spinner(&mut self, cx: f32, cy: f32, r: f32, c: Rgb) {
        self.p.circle(cx, cy, r, None, Some((mix(c, self.pal.surface, 0.72), r * 0.3)));
        let a0 = (self.ms % 1000) as f32 / 1000.0 * std::f32::consts::TAU;
        let mut pb = tiny_skia::PathBuilder::new();
        let rr = r - r * 0.15;
        let steps = 12;
        for i in 0..=steps {
            let a = a0 + i as f32 / steps as f32 * std::f32::consts::FRAC_PI_2 * 1.3;
            let (px, py) = (cx + rr * a.cos(), cy + rr * a.sin());
            if i == 0 {
                pb.move_to(px, py);
            } else {
                pb.line_to(px, py);
            }
        }
        if let Some(p) = pb.finish() {
            self.p.stroke_path(&p, c, r * 0.3);
        }
        self.animating = true;
    }

    /// Il pallino pieno con la spunta (completato).
    pub fn ok_dot(&mut self, cx: f32, cy: f32, r: f32, c: Rgb, tick: Rgb) {
        self.p.circle(cx, cy, r, Some(c), None);
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(cx - r * 0.47, cy + r * 0.03);
        pb.line_to(cx - r * 0.15, cy + r * 0.35);
        pb.line_to(cx + r * 0.47, cy - r * 0.3);
        if let Some(p) = pb.finish() {
            self.p.stroke_path(&p, tick, r * 0.2);
        }
    }

    /// L'icona dello stato di un file: spunta, rotellina, orologio, errore.
    pub fn state_icon(&mut self, state: FileState, cx: f32, cy: f32, r: f32) {
        let pal = self.pal;
        match state {
            FileState::Done => self.ok_dot(cx, cy, r, pal.ok, if self.theme.dark() { (6, 37, 20) } else { (255, 255, 255) }),
            FileState::Running => self.spinner(cx, cy, r * 0.8, pal.primary),
            FileState::Waiting => self.icon(ic::CLOCK, cx - r, cy - r, r * 2.0, pal.muted),
            FileState::Skipped => self.icon(ic::COPY, cx - r, cy - r, r * 2.0, pal.muted),
            FileState::Failed => self.icon(ic::WARNING, cx - r, cy - r, r * 2.0, pal.bad),
        }
    }

    /// La parola dello stato.
    pub fn state_word(state: FileState) -> &'static str {
        match state {
            FileState::Waiting => "In attesa",
            FileState::Running => "In corso",
            FileState::Done => "Completato",
            FileState::Skipped => "Saltato",
            FileState::Failed => "Errore",
        }
    }

    pub fn state_color(&self, state: FileState) -> Rgb {
        match state {
            FileState::Done => self.pal.ok,
            FileState::Running => self.pal.primary,
            FileState::Failed => self.pal.bad,
            _ => self.pal.muted,
        }
    }

    /// La miniatura di un file: l'immagine vera se è pronta, altrimenti l'icona del tipo.
    pub fn thumb(&mut self, app: &App, index: usize, r: R, radius: f32) {
        let pal = self.pal;
        let Some(f) = app.files.get(index) else { return };
        match app.media_of(&f.path).and_then(|m| m.thumb.as_ref()) {
            Some(img) => {
                let img = img.clone();
                self.p.image(&img, r.0, r.1, r.2, r.3, radius, if f.kind == Kind::Pdf { Fit::Contain } else { Fit::Cover });
                if f.kind == Kind::Pdf {
                    self.p.rrect_line(r.0, r.1, r.2, r.3, radius, pal.border, 1.0, 1.0, None);
                }
            }
            None => {
                self.p.rrect(r.0, r.1, r.2, r.3, radius, Some(pal.alt), None);
                let s = r.3.min(r.2) * 0.6;
                self.kind_icon(f.kind, r.0 + (r.2 - s) / 2.0, r.1 + (r.3 - s) / 2.0, s);
            }
        }
    }

    /// L'icona disegnata di un tipo di file (PDF rosso, Word blu, immagine verde, testo).
    pub fn kind_icon(&mut self, kind: Kind, x: f32, y: f32, s: f32) {
        match kind {
            Kind::Pdf => self.pdf_icon(x + s * 0.08, y, s),
            Kind::Doc => self.word_icon(x, y, s),
            Kind::Image => self.image_icon(x, y, s),
            Kind::Text => self.text_icon(x, y, s),
            Kind::Audio => self.icon(ic::AUDIO, x, y, s, self.pal.primary),
            Kind::Video => self.icon(ic::VIDEO, x, y, s, self.pal.primary),
            Kind::Other | Kind::Archive => self.icon(ic::FILE, x, y, s, self.pal.muted),
        }
    }

    /// Il foglio del PDF con il bordo rosso (40×48 alla misura 48).
    pub fn pdf_icon(&mut self, x: f32, y: f32, h: f32) {
        let k = h / 48.0;
        let red = (0xD9, 0x3A, 0x2E);
        let d = "M4 2h22l10 10v32a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z";
        if let Some(path) = super::svgpath::parse(d) {
            let tf = tiny_skia::Transform::from_row(k, 0.0, 0.0, k, x, y);
            if let Some(p) = path.clone().transform(tf) {
                self.p.fill_path(&p, (255, 255, 255), 1.0, false);
                self.p.stroke_path(&p, red, 2.4 * k);
            }
        }
        for d in ["M26 2v10h10", "M11 36c4-3 7-9 8-15 0-3-2-3-2 0 0 4 4 10 10 12 2 1 3-1 1-2-5-1-12 1-16 4-2 2-1 3 1 1"] {
            if let Some(path) = super::svgpath::parse(d).and_then(|p| p.transform(tiny_skia::Transform::from_row(k, 0.0, 0.0, k, x, y))) {
                self.p.stroke_path(&path, red, 2.2 * k);
            }
        }
    }

    pub fn word_icon(&mut self, x: f32, y: f32, s: f32) {
        let k = s / 80.0;
        let r = |p: &mut Painter, rx: f32, ry: f32, w: f32, h: f32, rad: f32, c: Rgb| p.rrect(x + rx * k, y + ry * k, w * k, h * k, rad * k, Some(c), None);
        r(&mut self.p, 22.0, 8.0, 52.0, 64.0, 6.0, (0x2B, 0x7C, 0xD3));
        r(&mut self.p, 22.0, 8.0, 52.0, 16.0, 6.0, (0x41, 0xA5, 0xEE));
        self.p.fill(x + 22.0 * k, y + 18.0 * k, 52.0 * k, 6.0 * k, (0x41, 0xA5, 0xEE));
        self.p.fill(x + 22.0 * k, y + 40.0 * k, 52.0 * k, 16.0 * k, (0x18, 0x5A, 0xBD));
        r(&mut self.p, 22.0, 56.0, 52.0, 16.0, 6.0, (0x10, 0x3F, 0x91));
        self.p.fill(x + 22.0 * k, y + 56.0 * k, 52.0 * k, 6.0 * k, (0x10, 0x3F, 0x91));
        r(&mut self.p, 6.0, 22.0, 40.0, 40.0, 5.0, (0x18, 0x5A, 0xBD));
        self.p.text("W", x + 6.0 * k, y + 22.0 * k, 40.0 * k, 40.0 * k, 28.0 * k, BOLD, (255, 255, 255), Align::Center);
    }

    pub fn image_icon(&mut self, x: f32, y: f32, s: f32) {
        let k = s / 80.0;
        self.p.rrect(x + 4.0 * k, y + 6.0 * k, 72.0 * k, 68.0 * k, 6.0 * k, Some((0xD6, 0xF0, 0xE8)), None);
        self.p.circle(x + 56.0 * k, y + 26.0 * k, 7.0 * k, Some((0xF5, 0xA6, 0x23)), None);
        self.p.poly(&[(x + 8.0 * k, y + 66.0 * k), (x + 28.0 * k, y + 38.0 * k), (x + 42.0 * k, y + 56.0 * k), (x + 52.0 * k, y + 44.0 * k), (x + 72.0 * k, y + 66.0 * k)], (0x2E, 0x8B, 0x6A), 1.0);
    }

    pub fn text_icon(&mut self, x: f32, y: f32, s: f32) {
        let k = s / 80.0;
        if let Some(p) = super::svgpath::parse("M14 6h40l14 14v52a2 2 0 0 1-2 2H14a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z").and_then(|p| p.transform(tiny_skia::Transform::from_row(k, 0.0, 0.0, k, x, y))) {
            self.p.fill_path(&p, (0xE8, 0xEE, 0xF6), 1.0, false);
        }
        for (i, (len, c)) in [(36.0, (0x2B, 0x7C, 0xD3)), (36.0, (0x41, 0xA5, 0xEE)), (36.0, (0x2B, 0x7C, 0xD3)), (22.0, (0x41, 0xA5, 0xEE))].iter().enumerate() {
            let yy = y + (30.0 + i as f32 * 10.0) * k;
            self.p.rrect(x + 20.0 * k, yy - 2.5 * k, (*len + 4.0) * k, 5.0 * k, 2.5 * k, Some(*c), None);
        }
    }

    /// I tre pulsanti della finestra (riduci, ingrandisci, chiudi) disegnati a destra della barra.
    pub fn window_controls(&mut self, right: f32, top: f32, h: f32, c: Rgb, hover_bg: Rgb) {
        let w = 46.0;
        let items = [(Act::Minimize, 0), (Act::Maximize, 1), (Act::Close, 2)];
        for (i, (act, kind)) in items.into_iter().enumerate() {
            let x = right - (3 - i) as f32 * w;
            let r = (x, top, w, h);
            if self.hovered(&act) {
                if kind == 2 {
                    self.p.fill(r.0, r.1, r.2, r.3, (0xC4, 0x2B, 0x1C));
                } else {
                    self.p.fill_a(r.0, r.1, r.2, r.3, hover_bg, 1.0);
                }
            }
            let col = if kind == 2 && self.hovered(&act) { (255, 255, 255) } else { c };
            let (cx, cy) = (x + w / 2.0, top + h / 2.0);
            match kind {
                0 => self.p.fill(cx - 7.0, cy, 14.0, 1.2, col),
                1 => {
                    if self.ui.maximized {
                        self.p.rrect_line(cx - 7.0, cy - 4.0, 11.0, 11.0, 1.5, col, 1.2, 1.0, None);
                        self.p.fill(cx - 4.0, cy - 7.0, 11.0, 1.2, col);
                        self.p.fill(cx + 6.0, cy - 7.0, 1.2, 11.0, col);
                    } else {
                        self.p.rrect_line(cx - 6.5, cy - 6.5, 13.0, 13.0, 1.5, col, 1.2, 1.0, None);
                    }
                }
                _ => {
                    self.p.line(cx - 6.5, cy - 6.5, cx + 6.5, cy + 6.5, col, 1.3);
                    self.p.line(cx + 6.5, cy - 6.5, cx - 6.5, cy + 6.5, col, 1.3);
                }
            }
            self.hit(r, act);
        }
    }

    /// Il simbolo Spicchio con la scritta «Cedro» accanto (la scritta prende il colore del tema).
    pub fn brand(&mut self, x: f32, y: f32, sym: f32, word_size: f32, gap: f32, word: Option<Rgb>) {
        let mut x = x;
        if self.theme == ThemeId::Bondeno {
            // Il tema del Comune: davanti al logo, lo stemma.
            let side = (sym * 1.5).round();
            self.p.stemma(crate::edizione::Stemma::Solo, x, y + sym / 2.0 - side / 2.0, side);
            x += side + gap;
        }
        self.p.symbol(x, y, sym);
        if word_size > 0.0 {
            let c = word.unwrap_or(self.pal.text);
            self.p.text("Cedro", x + sym + gap, y, 200.0, sym, word_size, SEMI, c, Align::Left);
        }
    }

    /// Le barre di scorrimento sottili, solo quando c'è da scorrere.
    pub fn scrollbar(&mut self, r: R, which: Scroll, content: f32) {
        let max = (content - r.3).max(0.0);
        self.scrolls.push((r, which, max));
        if max <= 0.0 {
            return;
        }
        let off = self.ui.scroll(which).clamp(0.0, max);
        let bar_h = (r.3 * r.3 / content).max(30.0);
        let y = r.1 + (r.3 - bar_h) * off / max;
        self.p.rrect_a(r.0 + r.2 - 6.0, y, 4.0, bar_h, 2.0, self.pal.muted, 0.5);
    }

    /// Il menu aperto, sopra a tutto.
    pub fn menu_overlay(&mut self) {
        let Some(menu) = self.ui.menu.as_ref() else { return };
        let pal = self.pal;
        let row = 40.0f32.max(menu.anchor.3.min(48.0));
        let size = if menu.anchor.3 > 56.0 { 20.0 } else { 16.0 };
        let mut w = menu.anchor.2;
        for (label, _) in &menu.items {
            w = w.max(self.measure(label, size, SANS) + 48.0);
        }
        let h = row * menu.items.len() as f32 + 8.0;
        let below = menu.anchor.1 + menu.anchor.3 + 4.0;
        let y = if below + h > self.h - 8.0 && menu.anchor.1 - h - 4.0 > 8.0 { menu.anchor.1 - h - 4.0 } else { below.min(self.h - h - 8.0).max(8.0) };
        let x = menu.anchor.0.min(self.w - w - 8.0).max(8.0);
        self.p.shadow(x, y, w, h, 8.0, 10.0, 0.22, 4.0);
        self.p.rrect(x, y, w, h, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
        let items: Vec<(String, Act)> = menu.items.iter().map(|(l, a)| (l.clone(), a.clone())).collect();
        let selected = menu.selected;
        // Il menu copre quello che c'è sotto: i clic lì dentro non vanno ai controlli dietro.
        self.hits.push(Hit { r: (x, y, w, h), act: Act::Nothing, kind: HitKind::Button });
        for (i, (label, act)) in items.into_iter().enumerate() {
            let r = (x + 4.0, y + 4.0 + i as f32 * row, w - 8.0, row);
            let hot = self.ui.hover == Some(Act::MenuPick(i));
            if hot || selected == Some(i) {
                self.p.rrect(r.0, r.1, r.2, r.3, 6.0, Some(if hot { pal.alt } else { mix(pal.alt, pal.surface, 0.4) }), None);
            }
            if selected == Some(i) {
                self.p.rrect(r.0 + 2.0, r.1 + 10.0, 3.0, r.3 - 20.0, 1.5, Some(pal.primary), None);
            }
            self.p.text(&label, r.0 + 16.0, r.1, r.2 - 24.0, r.3, size, SANS, pal.text, Align::Left);
            let _ = act;
            self.hits.push(Hit { r, act: Act::MenuPick(i), kind: HitKind::Button });
        }
    }
}
