//! Una finestra della nuova interfaccia, senza sistema operativo: tiene lo stato, disegna il
//! fotogramma e risponde a mouse, tastiera e file trascinati. Windows e Linux la ospitano e le
//! danno i servizi che solo loro hanno (scegliere file, aprire cartelle, appunti, la finestra).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::core::{inside, Act, Field, Fold, Frame, HitKind, LinkFlag, LinkMenu, Menu, Scroll, Swatch, UiState, NEW_VOICE, R};
use super::gfx::{Fonts, GlyphCache, Painter};
use super::model::{Activity, App, Notice, AUDIO_FORMATS, SUBTITLES, VIDEO_FORMATS};
use super::theme::ThemeId;
use super::views;
use crate::conversioni::{self, Control, CATEGORIES, EXISTS_CHOICES};
use crate::trascrizione;

/// Le cose che solo il sistema sa fare.
pub trait Platform {
    fn pick_files(&mut self) -> Vec<PathBuf>;
    fn pick_folder(&mut self, title: &str) -> Option<PathBuf>;
    /// Apre un file o una cartella con il programma di sistema.
    fn open_path(&mut self, path: &Path);
    fn copy_text(&mut self, text: &str);
    fn paste_text(&mut self) -> Option<String>;
    fn open_guide(&mut self);
    fn open_settings(&mut self);
    fn minimize(&mut self);
    fn toggle_maximize(&mut self);
    fn close(&mut self);
    /// Dalla finestrella compatta al pannello completo.
    fn open_full(&mut self);
    /// Manda un file alla stampante (dove il sistema non sa farlo, lo apre col programma che lo stampa).
    fn print_path(&mut self, path: &Path) {
        self.open_path(path);
    }
}

/// Che finestra è.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// La finestra principale del tema.
    Main,
    /// La finestrella che si apre dall'icona vicino all'orologio (Cedro compatto).
    Compact,
    /// Il visualizzatore di un file firmato (doppio clic su un .p7m).
    Viewer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cursor {
    Arrow,
    Hand,
    Text,
}

/// I tasti che contano.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Tab,
    ShiftTab,
    Enter,
    Space,
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    Paste,
    Copy,
    SelectAll,
}

pub struct Screen {
    pub app: App,
    pub ui: UiState,
    pub theme: ThemeId,
    pub simple: bool,
    pub surface: Surface,
    /// Tema e modalità scelti mentre un lavoro era in corso: si applicano alla fine.
    pub pending: Option<(ThemeId, bool)>,
    hits: Vec<super::core::Hit>,
    captions: Vec<R>,
    scrolls: Vec<(R, Scroll, f32)>,
    cache: GlyphCache,
    started: Instant,
    animating: bool,
    gliding: bool,
    focusables: Vec<Act>,
}

/// Le voci del menu «Formato», con iconcine e suggerimenti, e i titoli dei gruppi.
pub type FormatItems = (Vec<(String, Act)>, Vec<Option<super::voci::Voce>>, Vec<(usize, &'static str)>);

impl Screen {
    pub fn new(surface: Surface, wake: Arc<dyn Fn() + Send + Sync>) -> Screen {
        let s = crate::read_settings();
        let theme = ThemeId::from_key(&s.ui_theme);
        let mut app = App::new(wake);
        if s.simple {
            app.action = app.default_action(true);
        }
        Screen {
            app,
            ui: UiState::default(),
            theme,
            simple: s.simple,
            surface,
            pending: None,
            hits: Vec::new(),
            captions: Vec::new(),
            scrolls: Vec::new(),
            cache: GlyphCache::new(),
            started: Instant::now(),
            animating: false,
            gliding: false,
            focusables: Vec::new(),
        }
    }

    /// La misura con cui il tema si apre (unità logiche).
    pub fn preferred_size(&self) -> (f32, f32) {
        views::preferred_size(self.theme, self.simple, self.surface)
    }

    pub fn min_size(&self) -> (f32, f32) {
        views::min_size(self.theme, self.surface)
    }

    /// Pixel per unità in una finestra di `w`×`h` pixel: quello voluto, ma abbastanza piccolo perché
    /// ci stia almeno la misura minima della disposizione (su schermi piccoli tutto si rimpicciolisce
    /// invece di sovrapporsi).
    pub fn fit_scale(&self, base: f32, w: f32, h: f32) -> f32 {
        let (mw, mh) = self.min_size();
        base.min(w / mw).min(h / mh).max(0.25)
    }

    /// Disegna il fotogramma in un'immagine di `w`×`h` pixel con `scale` pixel per unità.
    pub fn render(&mut self, w: u32, h: u32, scale: f32) -> Painter<'_> {
        self.app.simple_view = self.simple;
        if self.theme == ThemeId::CedroLabs {
            self.app.lab_sync();
        } else if self.theme.is_cedro() {
            self.app.cedro_sync();
        }
        if self.app.activity == Activity::Firma && self.surface == Surface::Main {
            super::firma::watch_recent(self.app.wake.clone());
        }
        if self.surface == Surface::Viewer {
            self.want_viewer();
        }
        self.want_preview();
        self.want_page_thumbs();
        if let Some(to) = self.app.review_follow() {
            self.ui.scroll.insert(Scroll::Review, to);
        }
        self.clamp_scrolls();
        let fonts = Fonts::for_theme(self.theme);
        self.cache.trim(scale);
        let painter = Painter::new(w, h, scale, fonts, &mut self.cache);
        let ms = self.started.elapsed().as_millis() as u64;
        let mut f = Frame {
            p: painter,
            hits: Vec::new(),
            captions: Vec::new(),
            scrolls: Vec::new(),
            ui: &self.ui,
            pal: self.theme.palette(),
            theme: self.theme,
            simple: self.simple,
            w: w as f32 / scale,
            h: h as f32 / scale,
            ms,
            animating: false,
            gliding: false,
        };
        views::draw(&mut f, &self.app, self.surface);
        views::overlays(&mut f, &self.app);
        // Il menu a comparsa appare in dissolvenza: si tiene il fotogramma senza menu e lo si
        // rimette sopra, sempre più trasparente (solo nei pochi fotogrammi dell'apertura).
        let menu_key = Act::Nothing;
        let open = f.ui.menu.is_some() as u8 as f32;
        let shown = f.tween(&menu_key, super::anim::Slot::Menu, open, super::anim::HOVER_MS, super::anim::ease_out);
        let under = (open > 0.0 && shown < 0.99).then(|| f.p.pixmap.clone());
        f.menu_overlay();
        if let Some(under) = under {
            let paint = tiny_skia::PixmapPaint { opacity: 1.0 - shown, ..Default::default() };
            f.p.pixmap.draw_pixmap(0, 0, under.as_ref(), &paint, tiny_skia::Transform::identity(), None);
        }
        self.ui.anim.borrow_mut().end_frame();
        self.animating = f.animating || self.app.busy();
        self.gliding = f.gliding;
        self.hits = std::mem::take(&mut f.hits);
        self.captions = std::mem::take(&mut f.captions);
        self.scrolls = std::mem::take(&mut f.scrolls);
        self.focusables = self.hits.iter().filter(|h| !matches!(h.act, Act::Nothing | Act::MenuPick(_) | Act::Minimize | Act::Maximize | Act::Close)).map(|h| h.act.clone()).collect();
        self.focusables.dedup();
        // Una zona scorsa oltre la fine (file tolti, pagina più corta) va riportata nei limiti:
        // lo fa il prossimo fotogramma, che si chiede subito.
        if self.scrolls.iter().any(|(_, which, max)| self.ui.scroll(*which) > *max) {
            self.animating = true;
        }
        f.p
    }

    /// Riporta nei limiti le zone scorse oltre la fine nel fotogramma di prima.
    fn clamp_scrolls(&mut self) {
        let mut limits: std::collections::HashMap<Scroll, f32> = Default::default();
        for (_, which, max) in &self.scrolls {
            let e = limits.entry(*which).or_insert(0.0);
            *e = e.max(*max);
        }
        for (which, max) in limits {
            if self.ui.scroll(which) > max {
                self.ui.scroll.insert(which, max);
            }
        }
    }

    /// Il tema «Documento al centro» mostra il file scelto: lo si fa preparare al thread dei file.
    fn want_preview(&mut self) {
        // «Firma documenti», passo 2: la pagina dove va la firma.
        if self.app.activity == Activity::Firma && self.surface == Surface::Main {
            let su = &self.app.firma.sign;
            if su.page == super::firmare::Page::Sign && su.step == 2 {
                if let Some(pdf) = su.first_pdf() {
                    let page = su.box_frac().map(|b| b.0).unwrap_or(su.box_page).min(pdf.pages.len().saturating_sub(1));
                    let path = pdf.path.clone();
                    self.app.want_preview(&path, page);
                }
            }
        }
        // «Rivedi» copre tutta la finestra e ha il suo lettore (l'audio trascritto): non c'è anteprima da
        // preparare e quel lettore non va fermato, altrimenti Play e i tempi non fanno sentire nulla.
        if let Some(r) = self.app.review.as_ref() {
            if self.app.player.as_ref().is_some_and(|p| p.path != r.source) {
                self.app.player = None;
            }
            return;
        }
        if !(matches!(self.theme, ThemeId::DocumentoAlCentro) || self.theme.paged()) || self.surface != Surface::Main || self.app.activity != Activity::Convert {
            if self.surface != Surface::Viewer {
                self.app.player = None;
            }
            return;
        }
        // Cambiato file (o tolto dalla coda): quello che suonava si ferma.
        let shown = self.app.selected.filter(|i| self.app.visible().contains(i)).and_then(|i| self.app.files.get(i)).map(|f| f.path.clone());
        if self.app.player.as_ref().is_some_and(|p| Some(&p.path) != shown.as_ref()) {
            self.app.player = None;
        }
        let vis = self.app.visible();
        let sel = self.app.selected.filter(|i| vis.contains(i)).or(vis.first().copied());
        self.app.selected = sel;
        if let Some(i) = sel {
            let path = self.app.files[i].path.clone();
            let page = self.app.preview_page;
            self.app.want_preview(&path, page);
        }
    }

    /// Unire e dividere i PDF mostrano le pagine: la striscia del file in evidenza (Dividi) e il file unito (Unisci).
    fn want_page_thumbs(&mut self) {
        let app = &mut self.app;
        if app.activity != Activity::Convert || app.kind() != crate::engine::Kind::Pdf {
            return;
        }
        // «Più piccolo» della Semplificata dice anche lui quanto si risparmia.
        if app.pdf_mode() == super::model::PdfMode::Compress {
            app.want_estimates();
            return;
        }
        if self.simple {
            return;
        }
        match app.pdf_mode() {
            super::model::PdfMode::Split => {
                let Some(i) = app.selected.filter(|i| app.visible().contains(i)).or(app.visible().first().copied()) else { return };
                let Some(n) = app.page_count(i) else { return };
                let path = app.files[i].path.clone();
                app.want_page_thumbs(&path, 0..n.min(super::views::STRIP_MAX));
            }
            super::model::PdfMode::Merge => {
                let mut budget = super::views::MERGED_MAX;
                for i in app.targets() {
                    let Some(n) = app.page_count(i) else { continue };
                    let pages = super::views::merged_pages(app, i, n);
                    let take: Vec<usize> = pages.into_iter().take(budget).collect();
                    budget -= take.len();
                    let path = app.files[i].path.clone();
                    app.want_page_thumbs(&path, take);
                    if budget == 0 {
                        break;
                    }
                }
            }
            super::model::PdfMode::Pages => {
                let Some(i) = super::views::split_file(app) else { return };
                let Some(n) = app.page_count(i) else { return };
                let path = app.files[i].path.clone();
                app.want_page_thumbs(&path, 0..n.min(super::views::BOARD_MAX));
                app.want_stamp_preview(i);
            }
            super::model::PdfMode::Compress => app.want_estimates(),
            super::model::PdfMode::Convert => {}
        }
    }

    /// Vero se la finestra va ridisegnata spesso (rotelline, cursore che lampeggia).
    pub fn animating(&self) -> bool {
        self.animating
    }

    /// Vero mentre si muove un'animazione breve (mouse sopra, pressione, schede): va ridisegnata
    /// al ritmo dello schermo, non al passo lento delle rotelline.
    pub fn gliding(&self) -> bool {
        self.gliding
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<&super::core::Hit> {
        self.hits.iter().rev().find(|h| inside(h.r, x, y))
    }

    /// Vero se lì si trascina la finestra (la barra del titolo disegnata, fuori dai pulsanti).
    pub fn caption_at(&self, x: f32, y: f32) -> bool {
        self.hit_at(x, y).is_none() && self.captions.iter().any(|r| inside(*r, x, y))
    }

    /// Il pulsante di sistema sotto il punto (per gli effetti di Windows 11 sul pulsante ingrandisci).
    pub fn window_button_at(&self, x: f32, y: f32) -> Option<Act> {
        match self.hit_at(x, y).map(|h| &h.act) {
            Some(a @ (Act::Minimize | Act::Maximize | Act::Close)) => Some(a.clone()),
            _ => None,
        }
    }

    pub fn cursor_at(&self, x: f32, y: f32) -> Cursor {
        match self.hit_at(x, y) {
            Some(h) if h.kind == HitKind::Text => Cursor::Text,
            Some(h) if h.act == Act::Nothing => Cursor::Arrow,
            Some(_) => Cursor::Hand,
            None => Cursor::Arrow,
        }
    }

    /// Il mouse si muove: vero se cambia qualcosa da ridisegnare.
    pub fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        if let Some((act, r)) = self.ui.drag.clone() {
            if matches!(act, Act::RowDrag(..) | Act::PageDrag(..)) {
                self.ui.drag_y.1 = y;
                self.ui.drag_x.1 = x;
                return true;
            }
            if act == Act::FirmaBox {
                self.box_drag(r, x, y);
                return true;
            }
            self.slide(&act, r, x);
            return true;
        }
        let hover = self.hit_at(x, y).map(|h| h.act.clone()).filter(|a| *a != Act::Nothing);
        if hover != self.ui.hover {
            self.ui.hover = hover;
            return true;
        }
        false
    }

    pub fn pointer_leave(&mut self) -> bool {
        let changed = self.ui.hover.is_some();
        self.ui.hover = None;
        changed
    }

    pub fn pointer_down(&mut self, x: f32, y: f32) {
        let hit = self.hit_at(x, y).map(|h| (h.act.clone(), h.kind, h.r));
        self.ui.pressed = hit.as_ref().map(|h| h.0.clone());
        if let Some((act, HitKind::Slider, r)) = hit {
            self.ui.drag = Some((act.clone(), r));
            if matches!(act, Act::RowDrag(..) | Act::PageDrag(..)) {
                self.ui.drag_y = (y, y);
                self.ui.drag_x = (x, x);
            } else if act == Act::FirmaBox {
                self.box_drag(r, x, y);
            } else {
                self.slide(&act, r, x);
            }
        }
    }

    pub fn pointer_up(&mut self, x: f32, y: f32, plat: &mut dyn Platform) {
        if let Some((act, _)) = self.ui.drag.take() {
            if let Act::RowDrag(i, row_h) = act {
                // Lasciata la riga: va al posto più vicino a dove è arrivata (un clic fermo non sposta niente).
                let rows = ((self.ui.drag_y.1 - self.ui.drag_y.0) / row_h.max(1) as f32).round() as i32;
                if rows != 0 {
                    self.app.move_file(i, rows);
                }
                self.ui.pressed = None;
                return;
            }
            if let Act::PageDrag(i, pos, uw, uh, per_row) = act {
                let (dx, dy) = (self.ui.drag_x.1 - self.ui.drag_x.0, self.ui.drag_y.1 - self.ui.drag_y.0);
                self.ui.pressed = None;
                // Un clic fermo sceglie la pagina; trascinata, va al posto più vicino a dove è arrivata.
                if dx.abs() < 6.0 && dy.abs() < 6.0 {
                    self.app.page_click(i, pos);
                } else {
                    let to = super::views::page_drop(pos, dx, dy, uw as f32, uh as f32, per_row as usize);
                    self.app.pages_move(i, pos, to);
                }
                return;
            }
            if act == Act::MediaSeek {
                if let Some(frac) = self.app.media_scrub.take() {
                    self.app.media_seek(frac);
                }
            }
            if act == Act::ReviewSeek {
                if let Some(frac) = self.app.review_scrub.take() {
                    self.app.review_seek(frac);
                }
            }
            self.ui.pressed = None;
            return;
        }
        let pressed = self.ui.pressed.take();
        let hit = self.hit_at(x, y).map(|h| (h.act.clone(), h.kind));
        // Un clic fuori dal menu lo chiude e basta.
        if self.ui.menu.is_some() {
            match &hit {
                Some((Act::MenuPick(i), _)) if pressed == Some(Act::MenuPick(*i)) => {
                    let act = self.ui.menu.take().and_then(|m| m.items.get(*i).map(|it| it.1.clone()));
                    if let Some(a) = act {
                        self.act(a, plat);
                    }
                }
                Some((Act::Nothing, _)) => {}
                _ => self.ui.menu = None,
            }
            return;
        }
        match hit {
            Some((act, kind)) if pressed.as_ref() == Some(&act) => {
                if kind != HitKind::Text {
                    self.ui.edit = None;
                }
                self.act(act, plat);
            }
            None => self.ui.edit = None,
            _ => {}
        }
    }

    /// Tasto destro: sopra un campo di testo si apre il menu classico.
    pub fn context_click(&mut self, x: f32, y: f32) -> bool {
        if self.ui.menu.take().is_some() {
            return true;
        }
        let Some(Act::Edit(field)) = self.hit_at(x, y).filter(|h| h.kind == HitKind::Text).map(|h| h.act.clone()) else { return false };
        if self.ui.edit != Some(field) {
            self.ui.edit = Some(field);
            self.ui.sel_all = false;
            self.ui.caret = self.field_text(field).chars().count();
        }
        let items = vec![
            ("Taglia".to_string(), Act::FieldCut),
            ("Copia".to_string(), Act::FieldCopy),
            ("Incolla".to_string(), Act::FieldPaste),
            ("Elimina".to_string(), Act::FieldClear),
            ("Seleziona tutto".to_string(), Act::FieldSelectAll),
        ];
        self.ui.menu = Some(Menu { anchor: (x, y - 4.0, 0.0, 0.0), items, selected: None, title: None, swatches: Vec::new(), voci: Vec::new(), groups: Vec::new(), plain: false });
        true
    }

    pub fn wheel(&mut self, x: f32, y: f32, dy: f32) -> bool {
        if self.ui.menu.is_some() {
            return false;
        }
        for (r, which, max) in self.scrolls.iter().rev() {
            if inside(*r, x, y) && *max > 0.0 {
                // Il registro conta dal fondo (0 = le righe più nuove): la rotella va al contrario.
                let d = if *which == Scroll::Log { -dy } else { dy };
                let v = (self.ui.scroll(*which) + d).clamp(0.0, *max);
                self.ui.scroll.insert(*which, v);
                return true;
            }
        }
        false
    }

    pub fn drag_over(&mut self, on: bool) -> bool {
        let changed = self.ui.dropping != on;
        self.ui.dropping = on;
        changed
    }

    pub fn drop_files(&mut self, paths: Vec<PathBuf>) {
        self.ui.dropping = false;
        if self.surface == Surface::Viewer {
            // Sul visualizzatore: il file lasciato si apre in un'altra finestra.
            for p in paths.iter().filter(|p| p.is_file()) {
                if let Err(e) = super::firma::launch_viewer(p) {
                    self.notify(&e, true);
                }
            }
            return;
        }
        if self.app.activity == Activity::Firma && self.app.review.is_none() {
            self.firma_files(paths);
            return;
        }
        self.add_files(paths);
    }

    /// Apre questo file nel visualizzatore (la finestra `Surface::Viewer`).
    pub fn open_viewer(&mut self, path: PathBuf) {
        let wake = self.app.wake.clone();
        self.app.firma.open_viewer(path, wake);
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        let simple = self.simple;
        let added = self.app.add_files(paths, simple);
        // Il Cedro di partenza: un file lasciato su «Inizio» apre la pagina che gli serve, con il file dentro.
        if added > 0 && self.theme.is_cedro() && self.app.lab == super::model::Lab::Home {
            self.app.lab = super::model::Lab::Converti;
            self.app.cedro_sync();
        }
    }

    /// Applica gli eventi dei lavori; vero se c'è da ridisegnare.
    pub fn pump(&mut self) -> bool {
        let was_busy = self.app.busy();
        let changed = self.app.pump();
        if was_busy && !self.app.busy() {
            if let Some((theme, simple)) = self.pending.take() {
                self.set_appearance(theme, simple);
            }
        }
        changed
    }

    /// Tema e modalità nuovi (dalle Impostazioni). Durante un lavoro si aspetta che finisca.
    pub fn request_appearance(&mut self, theme: ThemeId, simple: bool) {
        if self.app.busy() {
            self.pending = Some((theme, simple));
        } else {
            self.set_appearance(theme, simple);
        }
    }

    /// Rilegge tema e modalità dal file delle impostazioni (le ha cambiate la pagina Aspetto).
    pub fn reload_appearance(&mut self) {
        let s = crate::read_settings();
        let theme = ThemeId::from_key(&s.ui_theme);
        if theme != self.theme || s.simple != self.simple {
            self.request_appearance(theme, s.simple);
        }
    }

    fn set_appearance(&mut self, theme: ThemeId, simple: bool) {
        let was_simple = self.simple;
        self.theme = theme;
        self.simple = simple;
        self.ui.menu = None;
        // Il tema può non avere l'attività in corso (Cedro 4 Labs non ha YouTube né i video dal web).
        if !theme.activities().contains(&self.app.activity) {
            self.app.activity = Activity::Convert;
            self.app.selected = self.app.visible().first().copied();
        }
        // In Semplificata la scelta deve stare tra quelle proposte; le opzioni scelte restano.
        if simple && !was_simple && !self.app.simple_choices().iter().any(|f| f.id == self.app.action) {
            let keep = conversioni::formats(self.app.kind()).iter().chain(conversioni::tools(self.app.kind())).any(|s| s.id == self.app.action);
            if !keep {
                self.app.action = self.app.default_action(true);
            }
        }
    }

    /// Il riquadro della firma trascinato sulla pagina: il centro dove sta il puntatore, senza uscire dal foglio.
    fn box_drag(&mut self, r: R, x: f32, y: f32) {
        let su = &mut self.app.firma.sign;
        let (hw, hh) = match su.box_frac() {
            Some((_, b)) => ((b[2] - b[0]) / 2.0, (b[3] - b[1]) / 2.0),
            None => (0.17, 0.04),
        };
        let cx = ((x - r.0) / r.2.max(1.0)).clamp(hw.min(0.5), (1.0 - hw).max(0.5));
        let cy = ((y - r.1) / r.3.max(1.0)).clamp(hh.min(0.5), (1.0 - hh).max(0.5));
        su.box_center = (cx, cy);
    }

    fn slide(&mut self, act: &Act, r: R, x: f32) {
        let frac = ((x - r.0 - 10.0) / (r.2 - 20.0)).clamp(0.0, 1.0);
        match act {
            Act::Quality => self.app.quality = (40.0 + frac * 55.0).round() as u8,
            Act::MediaSeek => self.app.media_scrub = Some(frac),
            Act::ReviewSeek => self.app.review_scrub = Some(frac),
            Act::OptSlider(opt) => {
                if let Some(def) = self.app.option_defs().into_iter().find(|d| d.opt == *opt) {
                    if let Control::Range { min, max, .. } = def.control {
                        let v = min as f32 + frac * (max - min) as f32;
                        self.app.values.set(def, v.round() as u32);
                    }
                }
            }
            _ => {}
        }
    }

    fn open_menu(&mut self, items: Vec<(String, Act)>, selected: Option<usize>, anchor_act: &Act) {
        let anchor = self.hits.iter().rev().find(|h| &h.act == anchor_act).map(|h| h.r).unwrap_or((0.0, 0.0, 200.0, 40.0));
        self.ui.menu = Some(Menu { anchor, items, selected, title: None, swatches: Vec::new(), voci: Vec::new(), groups: Vec::new(), plain: false });
    }

    /// Un menu con un titolo e un pallino colorato per voce (chi dice una frase, a chi unire una voce).
    fn open_swatch_menu(&mut self, title: &str, items: Vec<(String, Act, Swatch)>, selected: Option<usize>, anchor_act: &Act) {
        let anchor = self.hits.iter().rev().find(|h| &h.act == anchor_act).map(|h| h.r).unwrap_or((0.0, 0.0, 200.0, 40.0));
        let swatches = items.iter().map(|i| i.2).collect();
        let items = items.into_iter().map(|(l, a, _)| (l, a)).collect();
        self.ui.menu = Some(Menu { anchor, items, selected, title: Some(title.to_string()), swatches, voci: Vec::new(), groups: Vec::new(), plain: false });
    }

    /// Le voci del menu «Formato»: i formati, poi gli strumenti della categoria, ognuna con la sua
    /// iconcina colorata e il suggerimento; e i titoli dei due gruppi quando ci sono gli strumenti.
    pub fn action_items(&self) -> FormatItems {
        use super::voci;
        let kind = self.app.kind();
        let mut items: Vec<(String, Act)> = Vec::new();
        let mut rich = Vec::new();
        let mut groups = Vec::new();
        if self.simple {
            for f in self.app.simple_choices() {
                let mut v = self.app.voce(f.id);
                v.label = f.title;
                v.hint = f.note;
                items.push((f.title.to_string(), Act::Action(f.id)));
                rich.push(Some(v));
            }
        } else {
            let tools = self.app.tools();
            if !tools.is_empty() {
                groups.push((0, "Converti in"));
            }
            for s in conversioni::formats(kind) {
                let v = self.app.voce(s.id);
                items.push((v.label.to_string(), Act::Action(s.id)));
                rich.push(Some(v));
            }
            if !tools.is_empty() {
                groups.push((items.len(), voci::tools_heading(kind)));
            }
            for s in &tools {
                let v = self.app.voce(s.id);
                items.push((v.label.to_string(), Act::Action(s.id)));
                rich.push(Some(v));
            }
        }
        (items, rich, groups)
    }

    /// Esegue l'azione di un clic.
    pub fn act(&mut self, act: Act, plat: &mut dyn Platform) {
        let simple = self.simple;
        let app = &mut self.app;
        match act {
            Act::Nothing | Act::MenuPick(_) => {}
            Act::Activity(a) => {
                app.activity = a;
                if a == Activity::Transcribe || a == Activity::Convert {
                    app.selected = app.visible().first().copied();
                }
                if a == Activity::Firma && !app.firma.sign.scanned_once {
                    let wake = app.wake.clone();
                    app.firma.sign.scan(wake);
                }
            }
            Act::Lab(lab) => {
                // Il Cedro di partenza: l'avviso («Fatto: 3 file.») resta sulla pagina che ha fatto il lavoro.
                if self.theme.is_cedro() && lab != app.lab {
                    app.notice = None;
                }
                app.set_lab(lab, simple);
                // Il Cedro di partenza ha i PDF in una pagina loro: «Converti» parte dagli altri file.
                if self.theme.is_cedro() && lab == super::model::Lab::Converti && app.kind() == crate::engine::Kind::Pdf {
                    app.cedro_leave_pdf(simple);
                }
                if self.theme.is_cedro() && lab == super::model::Lab::Pdf {
                    app.cedro_back_to_pdf();
                }
            }
            Act::LabGo(super::model::Lab::Video, _) => {
                if self.theme.is_cedro() {
                    app.notice = None;
                }
                // «Inizio»: il link incollato va nella pagina dei video, pronto da scaricare.
                app.set_lab(super::model::Lab::Video, simple);
                app.add_typed_links(false);
            }
            Act::LabGo(super::model::Lab::Home, _) => {
                if self.theme.is_cedro() {
                    app.notice = None;
                }
                // Le tessere di Inizio: si sceglie il file, poi la scheda dice che cosa ci si può fare.
                let files = plat.pick_files();
                self.add_files(files);
            }
            Act::LabGo(lab, id) => {
                app.set_lab(lab, simple);
                if lab == super::model::Lab::Pdf && conversioni::is_page_action(app.kind(), id) {
                    let mode = match id {
                        "merge" => super::model::PdfMode::Merge,
                        "split" => super::model::PdfMode::Split,
                        "compress" => super::model::PdfMode::Compress,
                        _ => super::model::PdfMode::Pages,
                    };
                    app.set_pdf_mode(mode, simple);
                } else if app.activity == Activity::Convert {
                    app.set_action(id);
                }
            }
            Act::ActivityMenu => {
                let items = Activity::ALL.iter().map(|a| (a.name().to_string(), Act::Activity(*a))).collect();
                let sel = Activity::ALL.iter().position(|a| *a == app.activity);
                self.open_menu(items, sel, &Act::ActivityMenu);
            }
            Act::Category(i) => {
                app.activity = Activity::Convert;
                app.set_category(i, simple);
            }
            Act::Action(id) => app.set_action(id),
            Act::PdfMode(mode) => app.set_pdf_mode(mode, simple),
            Act::MoveFile(i, d) => app.move_file(i, d),
            Act::RowDrag(..) | Act::PageDrag(..) => {}
            Act::PagesRotate(i, dir) => self.app.pages_rotate(i, dir),
            Act::PagesRemove(i) => self.app.pages_remove(i),
            Act::PagesExtract(i) => self.app.pages_extract(i),
            Act::PagesAll(i) => self.app.page_select_all(i),
            Act::PagesReset(i) => self.app.pages_reset(i),
            Act::PagesFileMenu => {
                let vis = self.app.visible();
                let items = vis.iter().map(|&i| (self.app.files[i].name(), Act::Select(i))).collect();
                let sel = super::views::split_file(&self.app).and_then(|s| vis.iter().position(|&v| v == s));
                self.open_menu(items, sel, &Act::PagesFileMenu);
            }
            Act::StampPick(i, pos) => {
                if let Some(b) = self.app.board(i) {
                    self.app.stamp_page = b.pages[..pos.min(b.pages.len())].iter().filter(|p| !p.2).count();
                    if let Some(b) = self.app.boards.get_mut(&self.app.files[i].path) {
                        b.sel = vec![pos];
                    } else {
                        let mut nb = b.clone();
                        nb.sel = vec![pos];
                        let path = self.app.files[i].path.clone();
                        self.app.boards.insert(path, nb);
                    }
                }
            }
            Act::Cut(i, after) => app.toggle_cut(i, after),
            Act::FormatMenu => {
                let (items, voci, groups) = self.action_items();
                let sel = items.iter().position(|(_, a)| matches!(a, Act::Action(id) if *id == self.app.action));
                self.open_menu(items, sel, &Act::FormatMenu);
                if let Some(m) = self.ui.menu.as_mut() {
                    m.voci = voci;
                    m.groups = groups;
                }
            }
            Act::AddFiles => {
                let files = plat.pick_files();
                self.add_files(files);
            }
            Act::AddFolder => {
                if let Some(dir) = plat.pick_folder("Cartella con i file da convertire") {
                    self.add_files(vec![dir]);
                }
            }
            Act::AddMenu => {
                let items = vec![("Aggiungi file…".to_string(), Act::AddFiles), ("Aggiungi una cartella…".to_string(), Act::AddFolder)];
                self.open_menu(items, None, &Act::AddMenu);
            }
            Act::Remove(i) => app.remove(i),
            Act::RemoveChecked => app.remove_checked(),
            Act::Check(i) => app.toggle_checked(i),
            Act::CheckAll => app.toggle_all(),
            Act::Select(i) => {
                if app.selected != Some(i) {
                    app.selected = Some(i);
                    app.preview_page = 0;
                    app.stamp_page = 0;
                }
            }
            Act::OptSwitch(opt) => {
                if let Some(def) = app.option_defs().into_iter().find(|d| d.opt == opt) {
                    let v = app.values.get(def);
                    app.values.set(def, if v == 0 { 1 } else { 0 });
                }
            }
            Act::OptMenu(opt) => {
                if let Some(def) = app.option_defs().into_iter().find(|d| d.opt == opt) {
                    if let Control::Choice(choices) = def.control {
                        let cur = app.values.get(def);
                        let items = choices.iter().map(|c| (c.label.to_string(), Act::OptPick(opt, c.value))).collect();
                        let sel = choices.iter().position(|c| c.value == cur);
                        self.open_menu(items, sel, &Act::OptMenu(opt));
                        // L'iconcina dell'opzione resta sul pulsante; nel menu, due parole per valore.
                        if let (Some(m), Some(icon)) = (self.ui.menu.as_mut(), super::voci::opzione(opt)) {
                            m.voci = choices.iter().map(|c| Some(super::voci::Voce { label: c.label, hint: super::voci::suggerimento(opt, c.value), ..icon })).collect();
                            m.plain = true;
                        }
                    }
                }
            }
            Act::OptPick(opt, v) => {
                if let Some(def) = app.option_defs().into_iter().find(|d| d.opt == opt) {
                    app.values.set(def, v);
                }
                // Un altro modo di dividere: i tagli della striscia valgono solo per gli intervalli.
                if opt == crate::conversioni::Opt::SplitHow && v != 2 {
                    app.cuts.clear();
                }
            }
            Act::OptSlider(_) | Act::Quality | Act::MediaSeek | Act::ReviewSeek => {}
            Act::ReviewOpen(i) => app.review_open(i),
            Act::ReviewClose => app.review_close(),
            Act::ReviewPlay => app.review_toggle(),
            Act::ReviewSkip(d) => app.review_skip(d),
            Act::ReviewRow(i) => app.review_row_play(i),
            Act::ReviewPick(i) => {
                if let Some(r) = app.review.as_mut() {
                    r.sel = Some(i);
                }
            }
            Act::ReviewFlag(i) => app.review_go_flag(i),
            Act::ReviewFix(i) => {
                if let Some(r) = app.review.as_mut() {
                    r.push_undo();
                    r.fix(i);
                    let at = r.flags.get(i).and_then(|f| f.segs.first().copied());
                    r.sel = at.or(r.sel);
                    r.step_flag(1);
                }
            }
            Act::ReviewDismiss(i) => {
                if let Some(r) = app.review.as_mut() {
                    r.dismissed.insert(i);
                    r.step_flag(1);
                }
            }
            Act::ReviewFixAll => {
                if let Some(r) = app.review.as_mut() {
                    r.push_undo();
                    r.fix_repeats();
                    r.step_flag(1);
                }
            }
            Act::ReviewStep(d) => app.review_step(d),
            Act::ReviewUndo => {
                if let Some(r) = app.review.as_mut() {
                    r.undo();
                }
            }
            Act::ReviewDelete(i) => {
                if let Some(r) = app.review.as_mut() {
                    r.push_undo();
                    if let Some(row) = r.rows.get_mut(i) {
                        row.removed = !row.removed;
                    }
                }
            }
            Act::ReviewFollow => {
                if let Some(r) = app.review.as_mut() {
                    r.follow = !r.follow;
                    r.followed = None;
                }
            }
            Act::ReviewOnly => {
                if let Some(r) = app.review.as_mut() {
                    r.only_flagged = !r.only_flagged;
                }
                self.ui.scroll.insert(Scroll::Review, 0.0);
            }
            Act::ReviewFormat(i) => {
                if let Some(r) = app.review.as_mut() {
                    r.format = i;
                }
            }
            Act::ReviewSave => app.review_save(),
            Act::ReviewTab(t) => {
                if let Some(r) = app.review.as_mut() {
                    r.tab = t.min(2);
                }
            }
            Act::SummaryOpen(i) => app.summary_open(i),
            Act::SummaryStart => app.summary_start(),
            Act::SummaryCancel => app.summary_cancel(),
            Act::SummaryCopy => {
                if let Some(s) = app.review.as_ref().and_then(|r| r.summary_sections()) {
                    plat.copy_text(&s.plain());
                    if let Some(r) = app.review.as_mut() {
                        r.summary_copied = true;
                    }
                }
            }
            Act::SummaryTodo(k) => app.summary_toggle_todo(k),
            Act::SummaryOnTop => {
                if let Some(r) = app.review.as_mut().filter(|r| r.summary.is_some()) {
                    r.summary_on_top = !r.summary_on_top;
                }
            }
            Act::ReviewWriteVoices => {
                if let Some(r) = app.review.as_mut() {
                    r.write_voices = !r.write_voices;
                }
            }
            Act::ReviewSpeakerMenu(row) => {
                let Some(r) = app.review.as_ref() else { return };
                let cur = r.rows.get(row).and_then(|x| x.speaker);
                let mut items: Vec<(String, Act, Swatch)> =
                    (0..r.names.len() as u8).map(|v| (crate::i18n::lit(&r.name(v)), Act::ReviewSpeaker(row, v), Swatch::Color(super::theme::voice(self.theme, v as usize)))).collect();
                items.push(("Voce nuova".to_string(), Act::ReviewSpeaker(row, NEW_VOICE), Swatch::New));
                let sel = cur.map(|v| v as usize);
                self.open_swatch_menu("Chi dice questa frase?", items, sel, &Act::ReviewSpeakerMenu(row));
            }
            Act::ReviewSpeaker(row, v) => {
                if let Some(r) = app.review.as_mut() {
                    r.push_undo();
                    r.set_speaker(row, v);
                    r.sel = Some(row);
                }
            }
            Act::ReviewVoicePlay(v) => {
                if let Some(row) = app.review.as_ref().and_then(|r| r.voice_sample(v)) {
                    app.review_row_play(row);
                }
            }
            Act::ReviewMergeMenu(from) => {
                let Some(r) = app.review.as_ref() else { return };
                let items: Vec<(String, Act, Swatch)> = r
                    .live_voices()
                    .into_iter()
                    .filter(|v| *v != from)
                    .map(|v| (crate::i18n::lit(&r.name(v)), Act::ReviewMerge(from, v), Swatch::Color(super::theme::voice(self.theme, v as usize))))
                    .collect();
                if !items.is_empty() {
                    let title = format!("Unisci «{}» a…", crate::i18n::lit(&r.name(from)));
                    self.open_swatch_menu(&title, items, None, &Act::ReviewMergeMenu(from));
                }
            }
            Act::ReviewMerge(from, into) => {
                if let Some(r) = app.review.as_mut() {
                    r.push_undo();
                    r.merge(from, into);
                }
            }
            Act::VoicesPick(n) => app.transcribe.speakers = n.min(trascrizione::MAX_VOICES as u8),
            Act::VoicesGlossary => {
                app.transcribe.glossario_vet = !app.transcribe.glossario_vet;
                let mut st = crate::read_settings();
                st.glossario_vet = app.transcribe.glossario_vet;
                let _ = crate::settings::write_settings(&st);
            }
            Act::CasoApri => app.caso_apri(),
            Act::CasoOk => {
                app.caso_conferma();
                self.ui.edit = None;
            }
            Act::CasoTogli => {
                app.caso_togli();
                self.ui.edit = None;
            }
            Act::CasoChiudi => {
                app.caso_aperto = false;
                self.ui.edit = None;
            }
            Act::LavoriApri => {
                app.lavori_aperti = !app.lavori_aperti;
                app.caso_aperto = false;
                app.quaderno_copiato = None;
            }
            Act::Quaderno(i) => {
                if let Some(l) = crate::caso::di_oggi(&app.lavori).get(i) {
                    plat.copy_text(&l.per_quaderno());
                    app.quaderno_copiato = Some(i);
                }
            }
            Act::VoicesGo => app.voices_go(),
            Act::VoicesCancel => app.transcribe.asking = false,
            Act::MediaPlay => app.media_toggle(),
            Act::Edit(field) => {
                // Una correzione si annulla in un colpo solo: l'undo si prende quando si entra nella frase.
                if let (Field::Review(i), Some(r)) = (field, app.review.as_mut()) {
                    if self.ui.edit != Some(field) {
                        r.push_undo();
                    }
                    r.sel = Some(i);
                }
                if let (Field::VoiceName(_), Some(r)) = (field, app.review.as_mut()) {
                    if self.ui.edit != Some(field) {
                        r.push_undo();
                    }
                }
                self.ui.edit = Some(field);
                self.ui.sel_all = false;
                self.ui.caret = self.field_text(field).chars().count();
            }
            Act::FieldCopy | Act::FieldCut | Act::FieldClear | Act::FieldPaste | Act::FieldSelectAll => {
                if let Some(field) = self.ui.edit {
                    let text = self.field_text(field);
                    match act {
                        Act::FieldCopy if !secret(field) => plat.copy_text(&text),
                        Act::FieldCut => {
                            if !secret(field) {
                                plat.copy_text(&text);
                            }
                            self.set_field_text(field, String::new());
                            self.ui.caret = 0;
                        }
                        Act::FieldClear => {
                            self.set_field_text(field, String::new());
                            self.ui.caret = 0;
                        }
                        Act::FieldPaste => {
                            if let Some(t) = plat.paste_text() {
                                self.text_input(&t.replace(['\r', '\n'], " "));
                            }
                        }
                        _ => {
                            self.ui.sel_all = true;
                            self.ui.caret = text.chars().count();
                        }
                    }
                    if !matches!(act, Act::FieldSelectAll) {
                        self.ui.sel_all = false;
                    }
                }
            }
            Act::DestMenu => {
                let mut items = vec![("Accanto all'originale".to_string(), Act::DestSame), ("Scegli una cartella…".to_string(), Act::DestPick)];
                if let Some(d) = &app.dest {
                    items.insert(1, (d.display().to_string(), Act::Nothing));
                }
                let sel = Some(if app.dest.is_some() { 1 } else { 0 });
                self.open_menu(items, sel, &Act::DestMenu);
            }
            Act::DestSame => app.dest = None,
            Act::DestPick => {
                if let Some(dir) = plat.pick_folder("Cartella di destinazione") {
                    app.dest = Some(dir);
                }
            }
            Act::ExistsMenu => {
                let items = EXISTS_CHOICES.iter().enumerate().map(|(i, (l, _))| (l.to_string(), Act::ExistsPick(i))).collect();
                let sel = EXISTS_CHOICES.iter().position(|(_, e)| *e == app.values.exists);
                self.open_menu(items, sel, &Act::ExistsMenu);
                if let Some(m) = self.ui.menu.as_mut() {
                    m.voci = EXISTS_CHOICES.iter().map(|(l, e)| Some(super::voci::Voce { label: l, hint: super::voci::suggerimento_esiste(*e), ..super::voci::esiste() })).collect();
                    m.plain = true;
                }
            }
            Act::ExistsPick(i) => {
                if let Some((_, e)) = EXISTS_CHOICES.get(i) {
                    app.values.exists = *e;
                }
            }
            Act::Start => {
                if self.ui.edit.is_some() {
                    self.ui.edit = None;
                }
                match app.result_path().filter(|_| app.finished()) {
                    Some(p) => plat.open_path(&p),
                    None => app.start(),
                }
            }
            Act::Restart => app.restart(),
            Act::Cancel => app.cancel(),
            Act::Fold(fold) => {
                if !self.ui.open.remove(&fold) {
                    self.ui.open.insert(fold);
                }
            }
            Act::SetSimple(on) => {
                let theme = self.theme;
                self.set_appearance(theme, on);
                let mut s = crate::read_settings();
                s.simple = on;
                let _ = crate::write_settings(&s);
            }
            Act::ResetRecommended => app.reset_recommended(),
            Act::OpenGuide => plat.open_guide(),
            Act::OpenSettings => plat.open_settings(),
            Act::OpenPath(p) => plat.open_path(&p),
            Act::Minimize => plat.minimize(),
            Act::Maximize => plat.toggle_maximize(),
            Act::Close => plat.close(),
            Act::Step(n) => self.ui.step = n.min(2),
            Act::Next => self.ui.step = (self.ui.step + 1).min(2),
            Act::Back => self.ui.step = self.ui.step.saturating_sub(1),
            Act::LinkAdd(web) => app.add_typed_links(web),
            Act::LinkRemove(web, i) => {
                if !app.busy() {
                    let l = app.links_mut(web);
                    if i < l.items.len() {
                        l.items.remove(i);
                    }
                }
            }
            Act::LinkAudio(web, audio) => app.links_mut(web).set_audio(audio),
            Act::LinkMenu(web, which) => {
                let l = app.links(web);
                let (items, sel): (Vec<(String, Act)>, usize) = match which {
                    LinkMenu::Quality => (l.qualities().iter().enumerate().map(|(i, q)| (q.0.to_string(), Act::LinkPick(web, which, i))).collect(), l.quality),
                    LinkMenu::Format => (l.formats().iter().enumerate().map(|(i, f)| (f.to_uppercase(), Act::LinkPick(web, which, i))).collect(), l.format),
                    LinkMenu::Subtitles => (SUBTITLES.iter().enumerate().map(|(i, s)| (s.0.to_string(), Act::LinkPick(web, which, i))).collect(), l.subtitles),
                };
                self.open_menu(items, Some(sel), &Act::LinkMenu(web, which));
            }
            Act::LinkPick(web, which, i) => {
                let l = app.links_mut(web);
                match which {
                    LinkMenu::Quality => l.quality = i.min(l.qualities().len() - 1),
                    LinkMenu::Format => l.format = i.min(if l.audio { AUDIO_FORMATS.len() } else { VIDEO_FORMATS.len() } - 1),
                    LinkMenu::Subtitles => l.subtitles = i.min(SUBTITLES.len() - 1),
                }
            }
            Act::LinkFlag(web, flag) => {
                let l = app.links_mut(web);
                let slot = match flag {
                    LinkFlag::Thumbnail => &mut l.thumbnail,
                    LinkFlag::Chapters => &mut l.chapters,
                    LinkFlag::Split => &mut l.split_chapters,
                    LinkFlag::Sponsors => &mut l.skip_sponsors,
                    LinkFlag::Normalize => &mut l.normalize,
                    LinkFlag::Playlist => &mut l.playlist,
                    LinkFlag::SkipExisting => &mut l.skip_existing,
                };
                *slot = !*slot;
            }
            Act::LinkDest(web) => {
                if let Some(dir) = plat.pick_folder("Dove salvare i download") {
                    app.links_mut(web).dest = dir;
                }
            }
            Act::CopyLog => {
                let text = match app.activity {
                    Activity::YouTube => app.youtube.log.clone(),
                    Activity::Web => app.web.log.clone(),
                    _ => app
                        .transcribe
                        .log
                        .iter()
                        .map(|l| {
                            let text = match l.speaker {
                                Some(v) => format!("{}: {}", trascrizione::speaker_name(v as usize), l.text),
                                None => l.text.clone(),
                            };
                            if l.key.is_empty() { text } else { format!("{} {text}", l.key) }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                plat.copy_text(&text);
            }
            Act::TrFormat(word) => app.transcribe.format = if word { trascrizione::Format::Word } else { trascrizione::Format::Text },
            Act::TrModelMenu => {
                let items = trascrizione::MODELS
                    .iter()
                    .enumerate()
                    .map(|(i, m)| (format!("{} · {} · {}", m.name, m.note, trascrizione::size_text(m.megabytes)), Act::TrModel(i)))
                    .collect();
                let sel = Some(app.transcribe.model);
                self.open_menu(items, sel, &Act::TrModelMenu);
            }
            Act::TrModel(i) => app.transcribe.model = i.min(trascrizione::MODELS.len() - 1),
            Act::PagePrev => app.preview_page = app.preview_page.saturating_sub(1),
            Act::PageNext => {
                let max = app.selected.and_then(|i| app.page_count(i)).unwrap_or(1);
                app.preview_page = (app.preview_page + 1).min(max.saturating_sub(1));
            }
            Act::CompactTab(t) => {
                self.ui.compact_tab = t;
                app.activity = match t {
                    1 => Activity::Web,
                    2 => Activity::Transcribe,
                    _ => Activity::Convert,
                };
            }
            Act::OpenFull => plat.open_full(),
            Act::Theme(t) => {
                let simple = self.simple;
                self.request_appearance(t, simple);
            }
            Act::Answer(choice) => app.answer(choice),
            Act::DismissNotice => app.notice = None,
            a @ (Act::FirmaAdd
            | Act::FirmaAddFolder
            | Act::FirmaOpen(_)
            | Act::FirmaRemove(_)
            | Act::FirmaClear
            | Act::FirmaRecent(_)
            | Act::FirmaReport
            | Act::FirmaExtract
            | Act::FirmaSetting(_)
            | Act::FirmaTrust) => self.firma_act(a, plat),
            a @ (Act::FirmaPage(_)
            | Act::FirmaStep(_)
            | Act::FirmaSignAdd
            | Act::FirmaSignAddFolder
            | Act::FirmaSignRemove(_)
            | Act::FirmaScan
            | Act::FirmaDevice(_)
            | Act::FirmaFormat(_)
            | Act::FirmaPlace(_)
            | Act::FirmaBox
            | Act::FirmaBoxPage(_)
            | Act::FirmaMore
            | Act::FirmaDest(_)
            | Act::FirmaExisting(_)
            | Act::FirmaGo
            | Act::FirmaDoneFolder
            | Act::FirmaDoneView
            | Act::FirmaInfoClose
            | Act::FirmaDlg(_)
            | Act::FirmaDlgGo
            | Act::FirmaDlgClose
            | Act::FirmaCertView(_)
            | Act::FirmaCertRevoca(_)
            | Act::FirmaCertSave(_)
            | Act::FirmaOpt(_)
            | Act::FirmaFormatMenu
            | Act::FirmaImageMenu
            | Act::FirmaCornerMenu
            | Act::FirmaDriver
            | Act::FirmaMenuPick(..)) => self.sign_act(a, plat),
            a @ (Act::VwSave
            | Act::VwOpenWith
            | Act::VwPrint
            | Act::VwZoom(_)
            | Act::VwReport
            | Act::VwPanel
            | Act::VwSigner(_)
            | Act::VwTech(_)
            | Act::VwAt(_)
            | Act::VwAtApply
            | Act::VwPage(_)
            | Act::VwRevision(_)
            | Act::VwRevisionOff
            | Act::VwOriginal) => self.viewer_act(a, plat),
        }
    }

    /// File scelti o lasciati sulla «Firma digitale»: uno solo si apre nel visualizzatore, di più si
    /// verificano insieme.
    fn firma_files(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let wake = self.app.wake.clone();
        // Dentro «Firma documenti» i file lasciati si aggiungono a quelli da firmare.
        let su = &mut self.app.firma.sign;
        if su.page == super::firmare::Page::Sign && su.step < 4 && self.app.firma.items.is_empty() {
            if !su.running {
                su.step = 1;
                su.add(paths);
            }
            return;
        }
        // Sull'inizio: quelli già firmati (e le cartelle) si verificano, gli altri si preparano per la firma.
        if su.page == super::firmare::Page::Home && self.app.firma.items.is_empty() {
            let (signed, plain): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| p.is_dir() || super::firmare::is_signed(p));
            if !plain.is_empty() {
                su.start(plain, wake);
            }
            if signed.is_empty() {
                return;
            }
            return self.verify_files(signed);
        }
        self.verify_files(paths)
    }

    fn verify_files(&mut self, paths: Vec<PathBuf>) {
        if paths.len() == 1 && paths[0].is_file() && self.app.firma.items.is_empty() {
            if let Err(e) = super::firma::launch_viewer(&paths[0]) {
                self.notify(&e, true);
            }
            return;
        }
        let wake = self.app.wake.clone();
        self.app.firma.add(paths, wake);
    }

    fn firma_act(&mut self, act: Act, plat: &mut dyn Platform) {
        let wake = self.app.wake.clone();
        match act {
            Act::FirmaAdd => {
                let files = plat.pick_files();
                self.firma_files(files);
            }
            Act::FirmaAddFolder => {
                if let Some(d) = plat.pick_folder("Scegli la cartella con i file firmati") {
                    let before = self.app.firma.items.len();
                    self.app.firma.add(vec![d], wake);
                    if self.app.firma.items.len() == before {
                        self.notify("In quella cartella non ci sono file firmati.", false);
                    }
                }
            }
            Act::FirmaOpen(i) => {
                if let Some(it) = self.app.firma.items.get(i) {
                    if let Err(e) = super::firma::launch_viewer(&it.path.clone()) {
                        self.notify(&e, true);
                    }
                }
            }
            Act::FirmaRemove(i) => {
                if i < self.app.firma.items.len() {
                    self.app.firma.items.remove(i);
                }
            }
            Act::FirmaClear => self.app.firma.items.clear(),
            Act::FirmaRecent(k) => {
                if let Some(r) = self.app.firma.recent.get(k) {
                    let p = r.path.clone();
                    if !p.exists() {
                        self.notify("Il file non c'è più: è stato spostato o cancellato.", true);
                    } else if let Err(e) = super::firma::launch_viewer(&p) {
                        self.notify(&e, true);
                    }
                }
            }
            Act::FirmaReport => {
                let reports: Vec<std::sync::Arc<crate::firma::Report>> = self.app.firma.items.iter().filter_map(|i| i.report.clone()).collect();
                let Some(first) = self.app.firma.items.first().map(|i| i.path.clone()) else { return };
                let refs: Vec<&crate::firma::Report> = reports.iter().map(|r| r.as_ref()).collect();
                match crate::firma::rapporto::pdf(&refs).and_then(|data| super::firma::save_beside(&first, "Rapporto di verifica delle firme.pdf", &data)) {
                    Ok(p) => {
                        self.notify(&format!("Rapporto salvato: {}", p.display()), false);
                        plat.open_path(&p);
                    }
                    Err(e) => self.notify(&e, true),
                }
            }
            Act::FirmaExtract => {
                let mut saved = 0;
                let mut errors = Vec::new();
                for it in &self.app.firma.items {
                    let Some(r) = &it.report else { continue };
                    if r.doc.is_empty() || it.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                        continue;
                    }
                    match super::firma::save_beside(&it.path, &r.doc_name, &r.doc) {
                        Ok(_) => saved += 1,
                        Err(e) => errors.push(e),
                    }
                }
                if let Some(e) = errors.first() {
                    self.notify(e, true);
                } else if saved == 0 {
                    self.notify("Non c'è nessun documento da estrarre: i PDF firmati sono già il documento.", false);
                } else {
                    let t = if saved == 1 { "Estratto 1 documento, accanto al suo file firmato.".to_string() } else { format!("Estratti {saved} documenti, ognuno accanto al suo file firmato.") };
                    self.notify(&t, false);
                }
            }
            Act::FirmaSetting(k) => {
                let mut s = crate::read_settings();
                match k {
                    0 => s.firma_assoc = !s.firma_assoc,
                    1 => s.firma_pdf = !s.firma_pdf,
                    _ => s.firma_online = !s.firma_online,
                }
                let _ = crate::write_settings(&s);
                self.app.firma.opts = (s.firma_assoc, s.firma_pdf, s.firma_online);
                if k < 2 {
                    if let Err(e) = crate::associa::apply(s.firma_assoc, s.firma_pdf) {
                        self.notify(&e, true);
                    }
                }
            }
            Act::FirmaTrust => self.app.firma.update_trust(wake),
            _ => {}
        }
    }

    /// «Firma documenti», «Chiavetta e certificati» e «Opzioni della firma».
    fn sign_act(&mut self, act: Act, plat: &mut dyn Platform) {
        use super::firmare::Page;
        let wake = self.app.wake.clone();
        match act {
            Act::FirmaPage(k) => {
                let su = &mut self.app.firma.sign;
                match k {
                    0 => su.home(),
                    1 => su.start(Vec::new(), wake),
                    2 => {
                        su.page = Page::Devices;
                        su.info = None;
                        su.dialog = None;
                        if !su.scanned_once {
                            su.scan(wake);
                        }
                    }
                    _ => {
                        su.page = Page::Options;
                        su.s = crate::read_settings();
                    }
                }
                self.ui.edit = None;
                self.ui.scroll.insert(Scroll::FirmaSign, 0.0);
            }
            Act::FirmaStep(n) => {
                let su = &mut self.app.firma.sign;
                if su.running {
                    return;
                }
                let ok = match n {
                    1 => true,
                    2 => !su.files.is_empty(),
                    _ => !su.files.is_empty() && su.can_sign(),
                };
                if ok {
                    if su.step == 4 {
                        su.files.clear();
                    }
                    su.step = n;
                    su.error = None;
                    su.plan();
                    self.ui.edit = if n == 3 { Some(Field::FirmaPin) } else { None };
                    self.ui.caret = su.pin.chars().count();
                    self.ui.scroll.insert(Scroll::FirmaSign, 0.0);
                }
            }
            Act::FirmaSignAdd => {
                let files = plat.pick_files();
                if !files.is_empty() {
                    let su = &mut self.app.firma.sign;
                    if su.page == Page::Sign && su.step < 4 {
                        su.add(files);
                    } else {
                        su.start(files, wake);
                    }
                }
            }
            Act::FirmaSignAddFolder => {
                if let Some(d) = plat.pick_folder("Cartella con i file da firmare") {
                    let su = &mut self.app.firma.sign;
                    let before = su.files.len();
                    su.add(vec![d]);
                    if su.files.len() == before {
                        self.notify("In quella cartella non ci sono file da aggiungere.", false);
                    }
                }
            }
            Act::FirmaSignRemove(i) => self.app.firma.sign.remove(i),
            Act::FirmaScan => self.app.firma.sign.scan(wake),
            Act::FirmaDevice(i) => {
                let su = &mut self.app.firma.sign;
                if i < su.devices.len() && !su.running {
                    su.device = i;
                    su.pin.clear();
                    su.error = None;
                }
            }
            Act::FirmaFormat(k) => {
                let su = &mut self.app.firma.sign;
                su.s.firma_format = k.min(2);
                su.save_settings();
                su.plan();
            }
            Act::FirmaPlace(k) => {
                let su = &mut self.app.firma.sign;
                if k < 2 && !su.s.firma_visible {
                    su.s.firma_visible = true;
                    su.save_settings();
                }
                if k == 1 && su.place != 1 {
                    // Si parte da dove sarebbe andata da sola (sopra le firme che ci sono già), non da
                    // dove era stata trascinata un'altra volta.
                    su.place = 0;
                    if let Some((page, b)) = su.box_frac() {
                        su.box_page = page;
                        su.box_center = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
                    }
                }
                su.place = k.min(2);
            }
            Act::FirmaBox => {}
            Act::FirmaBoxPage(d) => {
                let su = &mut self.app.firma.sign;
                let n = su.first_pdf().map(|p| p.pages.len()).unwrap_or(1).max(1);
                let cur = su.box_page.min(n - 1) as i32;
                su.box_page = (cur + d).clamp(0, n as i32 - 1) as usize;
            }
            Act::FirmaMore => {
                let su = &mut self.app.firma.sign;
                su.more = !su.more;
            }
            Act::FirmaDest(k) => {
                if k == 0 {
                    self.app.firma.sign.s.firma_dir.clear();
                } else if let Some(d) = plat.pick_folder("Dove salvo i file firmati") {
                    self.app.firma.sign.s.firma_dir = d.to_string_lossy().into_owned();
                }
                let su = &mut self.app.firma.sign;
                su.save_settings();
                su.plan();
            }
            Act::FirmaExisting(k) => {
                let su = &mut self.app.firma.sign;
                su.s.firma_existing = k.min(2);
                su.save_settings();
                su.plan();
            }
            Act::FirmaGo => {
                let su = &mut self.app.firma.sign;
                if su.page == Page::Sign && su.step == 3 && !su.running {
                    self.ui.edit = None;
                    su.sign(wake);
                }
            }
            Act::FirmaDoneFolder => {
                if let Some(d) = self.app.firma.sign.files.iter().find(|f| matches!(f.state, super::firmare::State::Done { .. })).and_then(|f| f.out.parent().map(Path::to_path_buf)) {
                    plat.open_path(&d);
                }
            }
            Act::FirmaDoneView => {
                if let Some(p) = self.app.firma.sign.files.iter().find(|f| matches!(f.state, super::firmare::State::Done { .. })).map(|f| f.out.clone()) {
                    if let Err(e) = super::firma::launch_viewer(&p) {
                        self.notify(&e, true);
                    }
                }
            }
            Act::FirmaInfoClose => self.app.firma.sign.info = None,
            Act::FirmaDlg(k) => {
                let su = &mut self.app.firma.sign;
                su.info = None;
                su.dialog = Some(super::firmare::PinDialog { kind: k.min(2), old: String::new(), new: String::new(), again: String::new(), error: None, busy: false });
                self.ui.edit = Some(Field::FirmaDlg(0));
                self.ui.caret = 0;
            }
            Act::FirmaDlgGo => {
                self.ui.edit = None;
                self.app.firma.sign.run_dialog(wake);
            }
            Act::FirmaDlgClose => {
                self.ui.edit = None;
                let su = &mut self.app.firma.sign;
                if !su.dialog.as_ref().is_some_and(|d| d.busy) {
                    su.dialog = None;
                }
            }
            Act::FirmaCertView(k) | Act::FirmaCertSave(k) => {
                let Some(c) = self.app.firma.sign.device().and_then(|d| d.certs.get(k)).cloned() else { return };
                let name = format!("{}.cer", crate::pdfpages::safe_name(&c.cert.subject.person()));
                if matches!(act, Act::FirmaCertSave(_)) {
                    if let Some(dir) = plat.pick_folder("Dove salvo il certificato") {
                        let p = dir.join(&name);
                        match std::fs::write(&p, &c.der) {
                            Ok(()) => self.notify(&format!("Certificato salvato: {}", p.display()), false),
                            Err(e) => self.notify(&format!("Non riesco a salvare il certificato: {e}"), true),
                        }
                    }
                } else {
                    match crate::engine::private_temp_dir("cedro-cert").and_then(|d| {
                        let p = d.join(&name);
                        std::fs::write(&p, &c.der).map(|_| p).map_err(|e| e.to_string())
                    }) {
                        Ok(p) => plat.open_path(&p),
                        Err(e) => self.notify(&format!("Non riesco a mostrare il certificato: {e}"), true),
                    }
                }
            }
            Act::FirmaCertRevoca(k) => self.app.firma.sign.check_revocation(k, wake),
            Act::FirmaOpt(k) => {
                let su = &mut self.app.firma.sign;
                match k {
                    0 => su.s.firma_keep_pin = !su.s.firma_keep_pin,
                    1 => su.s.firma_visible = !su.s.firma_visible,
                    _ => su.s.firma_remind = !su.s.firma_remind,
                }
                su.save_settings();
                su.plan();
            }
            Act::FirmaFormatMenu => {
                let items = ["Automatico: PDF in PDF firmato, gli altri in .p7m", "Tutto in .p7m", "Tutto in PDF firmato"].iter().enumerate().map(|(k, t)| (t.to_string(), Act::FirmaMenuPick(0, k as u8))).collect();
                let sel = Some(self.app.firma.sign.s.firma_format.min(2) as usize);
                self.open_menu(items, sel, &Act::FirmaFormatMenu);
            }
            Act::FirmaImageMenu => {
                let has = !self.app.firma.sign.s.firma_image.trim().is_empty();
                let items = vec![("Niente".to_string(), Act::FirmaMenuPick(1, 0)), ("Scegli un'immagine (PNG o JPG)…".to_string(), Act::FirmaMenuPick(1, 1))];
                self.open_menu(items, Some(has as usize), &Act::FirmaImageMenu);
            }
            Act::FirmaCornerMenu => {
                let items = ["Ultima pagina, in basso a destra", "Ultima pagina, in basso a sinistra", "Prima pagina, in basso a destra"].iter().enumerate().map(|(k, t)| (t.to_string(), Act::FirmaMenuPick(2, k as u8))).collect();
                let sel = Some(self.app.firma.sign.s.firma_corner.min(2) as usize);
                self.open_menu(items, sel, &Act::FirmaCornerMenu);
            }
            Act::FirmaDriver => {
                if self.app.firma.sign.s.firma_driver.trim().is_empty() {
                    let Some(p) = plat.pick_files().into_iter().next() else { return };
                    self.app.firma.sign.s.firma_driver = p.to_string_lossy().into_owned();
                } else {
                    self.app.firma.sign.s.firma_driver.clear();
                }
                let su = &mut self.app.firma.sign;
                su.save_settings();
                su.scan(wake);
            }
            Act::FirmaMenuPick(menu, k) => {
                match (menu, k) {
                    (0, k) => self.app.firma.sign.s.firma_format = k.min(2),
                    (1, 0) => self.app.firma.sign.s.firma_image.clear(),
                    (1, _) => {
                        let Some(p) = plat.pick_files().into_iter().next() else { return };
                        let ok = image::open(&p).is_ok();
                        if !ok {
                            self.notify("Questa non è un'immagine che so leggere: scegli un PNG o un JPG.", true);
                            return;
                        }
                        self.app.firma.sign.s.firma_image = p.to_string_lossy().into_owned();
                    }
                    (_, k) => self.app.firma.sign.s.firma_corner = k.min(2),
                }
                let su = &mut self.app.firma.sign;
                su.save_settings();
                su.plan();
            }
            _ => {}
        }
    }

    fn firma_at_apply(&mut self) {
        let wake = self.app.wake.clone();
        let Some(v) = self.app.firma.viewer.as_mut() else { return };
        match super::firma::parse_day(&v.at_text) {
            Some(t) => {
                v.at_error = false;
                self.ui.edit = None;
                self.app.firma.reverify(crate::firma::At::Custom(t), None, wake);
            }
            None => v.at_error = true,
        }
    }

    fn viewer_act(&mut self, act: Act, plat: &mut dyn Platform) {
        let wake = self.app.wake.clone();
        let Some(v) = self.app.firma.viewer.as_mut() else { return };
        let shown = v.revision.as_ref().map(|r| r.1.clone()).or(v.doc_path.clone());
        match act {
            Act::VwSave => {
                let Some(rep) = v.report.clone() else { return };
                let (name, data): (String, Vec<u8>) = match &v.revision {
                    Some((i, _)) => {
                        let stem = std::path::Path::new(&rep.doc_name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        (format!("{stem} (versione firmata).pdf"), rep.signed_revision(*i).unwrap_or_default().to_vec())
                    }
                    None => (rep.doc_name.clone(), rep.doc.clone()),
                };
                let signed = v.path.clone();
                match super::firma::save_beside(&signed, &name, &data) {
                    Ok(p) => {
                        let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        self.notify(&format!("Salvato «{n}» nella cartella del file firmato."), false);
                    }
                    Err(e) => self.notify(&e, true),
                }
            }
            Act::VwOpenWith => {
                if let Some(p) = shown {
                    plat.open_path(&p);
                }
            }
            Act::VwPrint => {
                if let Some(p) = shown {
                    plat.print_path(&p);
                }
            }
            Act::VwZoom(d) => {
                const STEPS: [f32; 8] = [0.5, 0.67, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0];
                let k = STEPS.iter().position(|z| (z - v.zoom).abs() < 0.01).unwrap_or(3) as i32;
                v.zoom = STEPS[(k + d).clamp(0, STEPS.len() as i32 - 1) as usize];
            }
            Act::VwReport => {
                let Some(rep) = v.report.clone() else { return };
                let stem = v.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let signed = v.path.clone();
                match crate::firma::rapporto::pdf(&[rep.as_ref()]).and_then(|d| super::firma::save_beside(&signed, &format!("{stem} - rapporto di verifica.pdf"), &d)) {
                    Ok(p) => {
                        // Si dice dove sta: se il lettore PDF non parte, il file si ritrova lo stesso.
                        self.notify(&format!("Rapporto salvato: {}", p.display()), false);
                        plat.open_path(&p);
                    }
                    Err(e) => self.notify(&e, true),
                }
            }
            Act::VwPanel => v.panel = !v.panel,
            Act::VwSigner(i) => {
                if !v.open.remove(&i) {
                    v.open.insert(i);
                    // Si porta la pagina sul riquadro della firma visibile.
                    let place = v.report.as_ref().and_then(|r| r.signers.get(i)).and_then(|s| s.place.clone()).filter(|p| p.visible);
                    if let Some(n) = place.as_ref().and_then(|p| p.page) {
                        let n = n.saturating_sub(1) as usize;
                        let changed = v.page != n;
                        v.page = n;
                        // Le misure dell'ultimo disegno: contenuto = pagina + 2 × 28 di margine.
                        let target = match (place.and_then(|p| p.rect), self.scrolls.iter().find(|x| x.1 == Scroll::FirmaDoc)) {
                            (Some(rc), Some((r, _, max))) if *max > 0.0 => {
                                let ph = max + r.3 - 56.0;
                                (28.0 + rc[1] * ph - r.3 * 0.3).clamp(0.0, *max)
                            }
                            _ => 0.0,
                        };
                        if changed || target > 0.0 {
                            self.ui.scroll.insert(Scroll::FirmaDoc, target);
                        }
                    }
                }
            }
            Act::VwTech(i) => {
                if !v.tech.remove(&i) {
                    v.tech.insert(i);
                }
            }
            Act::VwAt(k) => match k {
                0 => {
                    v.at_error = false;
                    self.app.firma.reverify(crate::firma::At::Now, None, wake);
                }
                1 => {
                    v.at_error = false;
                    self.app.firma.reverify(crate::firma::At::Signing, None, wake);
                }
                _ => {
                    if v.at_text.is_empty() {
                        v.at_text = crate::firma::date(crate::firma::now());
                    }
                    let t = v.at_text.chars().count();
                    self.ui.edit = Some(Field::FirmaDate);
                    self.ui.sel_all = true;
                    self.ui.caret = t;
                }
            },
            Act::VwAtApply => self.firma_at_apply(),
            Act::VwPage(i) => {
                v.page = i;
                self.ui.scroll.insert(Scroll::FirmaDoc, 0.0);
            }
            Act::VwRevision(i) => {
                let Some(rep) = v.report.clone() else { return };
                if let Some(data) = rep.signed_revision(i) {
                    let stem = std::path::Path::new(&rep.doc_name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "documento".into());
                    if let Some(p) = super::firma::write_temp(&format!("{stem} (versione firmata).pdf"), data) {
                        v.revision = Some((i, p));
                        v.page = 0;
                        self.ui.scroll.insert(Scroll::FirmaDoc, 0.0);
                    }
                }
            }
            Act::VwRevisionOff => {
                v.revision = None;
                v.page = 0;
            }
            Act::VwOriginal => {
                let files = plat.pick_files();
                let Some(p) = files.into_iter().next() else { return };
                match std::fs::read(&p) {
                    Ok(data) => {
                        let at = self.app.firma.viewer.as_ref().map(|v| v.at).unwrap_or(crate::firma::At::Now);
                        self.app.firma.reverify(at, Some(data), wake);
                    }
                    Err(e) => self.notify(&format!("Non riesco a leggere il file: {e}"), true),
                }
            }
            _ => {}
        }
    }

    /// Il visualizzatore chiede le informazioni, la pagina e le miniature del documento mostrato.
    fn want_viewer(&mut self) {
        let Some(v) = self.app.firma.viewer.as_ref() else { return };
        let Some(path) = v.revision.as_ref().map(|r| r.1.clone()).or(v.doc_path.clone()) else { return };
        let page = v.page;
        self.app.want_media(&path);
        self.app.want_preview(&path, page);
        if crate::engine::kind_of(&path) == crate::engine::Kind::Pdf {
            if let Some(n) = self.app.media_of(&path).and_then(|m| m.pages) {
                self.app.want_page_thumbs(&path, 0..n.min(400));
            }
        }
    }

    fn field_text(&self, field: Field) -> String {
        match field {
            Field::Opt(opt) => self.app.option_defs().into_iter().find(|d| d.opt == opt).map(|d| self.app.values.text(d).to_string()).unwrap_or_default(),
            Field::Link(web) => self.app.links(web).typed.clone(),
            Field::FilePages(i) => self.app.files.get(i).map(|f| f.pages.clone()).unwrap_or_default(),
            Field::Review(i) => self.app.review.as_ref().and_then(|r| r.rows.get(i)).map(|r| r.text.clone()).unwrap_or_default(),
            Field::VoiceName(v) => self.app.review.as_ref().and_then(|r| r.names.get(v as usize)).cloned().unwrap_or_default(),
            Field::CasoCodice => self.app.caso_bozza.0.clone(),
            Field::CasoNota => self.app.caso_bozza.1.clone(),
            Field::FirmaDate => self.app.firma.viewer.as_ref().map(|v| v.at_text.clone()).unwrap_or_default(),
            Field::FirmaPin => self.app.firma.sign.pin.clone(),
            Field::FirmaReason => self.app.firma.sign.s.firma_reason.clone(),
            Field::FirmaLocation => self.app.firma.sign.s.firma_location.clone(),
            Field::FirmaSuffix => self.app.firma.sign.s.firma_suffix.clone(),
            Field::FirmaDlg(k) => self
                .app
                .firma
                .sign
                .dialog
                .as_ref()
                .map(|d| match k {
                    0 => d.old.clone(),
                    1 => d.new.clone(),
                    _ => d.again.clone(),
                })
                .unwrap_or_default(),
        }
    }

    fn set_field_text(&mut self, field: Field, text: String) {
        match field {
            Field::Opt(opt) => {
                if let Some(def) = self.app.option_defs().into_iter().find(|d| d.opt == opt) {
                    self.app.values.set_text(def, &text);
                }
                if opt == crate::conversioni::Opt::SplitRanges {
                    self.app.ranges_typed();
                }
            }
            Field::Link(web) => self.app.links_mut(web).typed = text,
            Field::FilePages(i) => {
                if let Some(f) = self.app.files.get_mut(i) {
                    f.pages = text;
                }
            }
            Field::Review(i) => {
                if let Some(row) = self.app.review.as_mut().and_then(|r| r.rows.get_mut(i)) {
                    row.text = text;
                }
            }
            Field::VoiceName(v) => {
                if let Some(name) = self.app.review.as_mut().and_then(|r| r.names.get_mut(v as usize)) {
                    *name = text;
                }
            }
            Field::CasoCodice => self.app.caso_bozza.0 = text,
            Field::CasoNota => self.app.caso_bozza.1 = text,
            Field::FirmaDate => {
                if let Some(v) = self.app.firma.viewer.as_mut() {
                    v.at_text = text;
                    v.at_error = false;
                }
            }
            Field::FirmaPin => {
                let su = &mut self.app.firma.sign;
                su.pin = text;
                su.error = None;
            }
            Field::FirmaReason | Field::FirmaLocation | Field::FirmaSuffix => {
                let su = &mut self.app.firma.sign;
                match field {
                    Field::FirmaReason => su.s.firma_reason = text,
                    Field::FirmaLocation => su.s.firma_location = text,
                    _ => su.s.firma_suffix = text.trim().to_string(),
                }
                su.save_settings();
                su.plan();
            }
            Field::FirmaDlg(k) => {
                if let Some(d) = self.app.firma.sign.dialog.as_mut() {
                    match k {
                        0 => d.old = text,
                        1 => d.new = text,
                        _ => d.again = text,
                    }
                    d.error = None;
                }
            }
        }
    }

    /// Un testo scritto (o incollato) nel campo con il cursore.
    pub fn text_input(&mut self, s: &str) {
        let Some(field) = self.ui.edit else { return };
        // Scrivere chiude il menu del tasto destro, come nei campi del sistema.
        self.ui.menu = None;
        if std::mem::take(&mut self.ui.sel_all) {
            self.set_field_text(field, String::new());
            self.ui.caret = 0;
        }
        let mut chars: Vec<char> = self.field_text(field).chars().collect();
        let at = self.ui.caret.min(chars.len());
        let ins: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
        let n = ins.len();
        chars.splice(at..at, ins);
        self.ui.caret = at + n;
        self.set_field_text(field, chars.into_iter().collect());
    }

    /// Un tasto. Vero se è stato usato.
    pub fn key(&mut self, key: Key, plat: &mut dyn Platform) -> bool {
        // Con un menu aperto (anche quello di un campo) le frecce e Esc vanno al menu.
        if let Some(m) = self.ui.menu.as_mut() {
            match key {
                Key::Up | Key::Down => {
                    let n = m.items.len();
                    m.selected = Some(match m.selected {
                        None => 0,
                        Some(cur) if key == Key::Down => (cur + 1).min(n.saturating_sub(1)),
                        Some(cur) => cur.saturating_sub(1),
                    });
                    return true;
                }
                Key::Escape => {
                    self.ui.menu = None;
                    return true;
                }
                _ => {}
            }
        }
        if let Some(field) = self.ui.edit {
            if self.ui.sel_all {
                match key {
                    Key::Left | Key::Home => {
                        self.ui.sel_all = false;
                        self.ui.caret = 0;
                        return true;
                    }
                    Key::Right | Key::End => {
                        self.ui.sel_all = false;
                        return true;
                    }
                    Key::Backspace | Key::Delete => {
                        self.ui.sel_all = false;
                        self.set_field_text(field, String::new());
                        self.ui.caret = 0;
                        return true;
                    }
                    _ => {}
                }
            }
            let mut chars: Vec<char> = self.field_text(field).chars().collect();
            let at = self.ui.caret.min(chars.len());
            match key {
                Key::Backspace if at > 0 => {
                    chars.remove(at - 1);
                    self.ui.caret = at - 1;
                }
                Key::Delete if at < chars.len() => {
                    chars.remove(at);
                }
                Key::Left => self.ui.caret = at.saturating_sub(1),
                Key::Right => self.ui.caret = (at + 1).min(chars.len()),
                Key::Home => self.ui.caret = 0,
                Key::End => self.ui.caret = chars.len(),
                Key::Paste => {
                    if let Some(t) = plat.paste_text() {
                        let one_line = t.replace(['\r', '\n'], " ");
                        self.text_input(&one_line);
                    }
                    return true;
                }
                Key::Copy => {
                    if !secret(field) {
                        plat.copy_text(&chars.iter().collect::<String>());
                    }
                    return true;
                }
                Key::SelectAll => {
                    self.ui.caret = chars.len();
                    self.ui.sel_all = true;
                    return true;
                }
                Key::Enter => {
                    self.ui.edit = None;
                    if let Field::Link(web) = field {
                        self.app.add_typed_links(web);
                    }
                    if field == Field::FirmaDate {
                        self.firma_at_apply();
                    }
                    if field == Field::FirmaPin {
                        self.act(Act::FirmaGo, plat);
                    }
                    if matches!(field, Field::FirmaDlg(_)) {
                        self.act(Act::FirmaDlgGo, plat);
                    }
                    if matches!(field, Field::CasoCodice | Field::CasoNota) {
                        self.act(Act::CasoOk, plat);
                    }
                    return true;
                }
                Key::Escape | Key::Tab | Key::ShiftTab => {
                    self.ui.edit = None;
                    if key == Key::Escape {
                        return true;
                    }
                }
                _ => return true,
            }
            if !matches!(key, Key::Tab | Key::ShiftTab) {
                self.set_field_text(field, chars.into_iter().collect());
                return true;
            }
        }
        match key {
            Key::Escape => {
                if self.ui.menu.take().is_some() {
                    return true;
                }
                if self.app.ask.is_some() {
                    self.app.answer(crate::engine::ExistsChoice::KeepBoth);
                    return true;
                }
                if std::mem::take(&mut self.app.transcribe.asking) {
                    return true;
                }
                if std::mem::take(&mut self.app.caso_aperto) {
                    return true;
                }
                if self.app.notice.take().is_some() {
                    return true;
                }
                false
            }
            Key::Tab | Key::ShiftTab => {
                let n = self.focusables.len();
                if n == 0 {
                    return false;
                }
                let cur = self.ui.focus_act.as_ref().and_then(|a| self.focusables.iter().position(|b| b == a));
                let next = match (cur, key) {
                    (None, Key::Tab) => 0,
                    (None, _) => n - 1,
                    (Some(i), Key::Tab) => (i + 1) % n,
                    (Some(i), _) => (i + n - 1) % n,
                };
                self.ui.focus_act = Some(self.focusables[next].clone());
                true
            }
            Key::Enter if self.app.transcribe.asking && !matches!(self.ui.focus_act, Some(Act::VoicesPick(_) | Act::VoicesCancel)) => {
                // Nella finestrella Invio vuol dire Trascrivi.
                self.app.voices_go();
                true
            }
            Key::Enter | Key::Space => {
                if let Some(a) = self.ui.focus_act.clone() {
                    self.act(a, plat);
                    return true;
                }
                false
            }
            Key::Up | Key::Down => {
                if let Some(m) = self.ui.menu.as_mut() {
                    let n = m.items.len();
                    let cur = m.selected.unwrap_or(0);
                    m.selected = Some(if key == Key::Down { (cur + 1).min(n.saturating_sub(1)) } else { cur.saturating_sub(1) });
                    return true;
                }
                false
            }
            Key::Paste => {
                // Incollare file (dagli appunti come testo con i percorsi) o link nella scheda giusta.
                if let Some(t) = plat.paste_text() {
                    match self.app.activity {
                        Activity::YouTube | Activity::Web => {
                            let web = self.app.activity == Activity::Web;
                            self.app.links_mut(web).add(&t);
                        }
                        _ => {
                            let paths: Vec<PathBuf> = t.lines().map(|l| PathBuf::from(l.trim().trim_matches('"'))).filter(|p| p.exists()).collect();
                            if !paths.is_empty() {
                                self.add_files(paths);
                            }
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Un menu aperto da tastiera: Invio sceglie la voce evidenziata.
    pub fn menu_enter(&mut self, plat: &mut dyn Platform) -> bool {
        if let Some(m) = self.ui.menu.take() {
            if let Some(a) = m.selected.and_then(|i| m.items.get(i)).map(|it| it.1.clone()) {
                self.act(a, plat);
            }
            return true;
        }
        false
    }

    /// Un avviso da far vedere (per esempio un file che non si apre).
    pub fn notify(&mut self, text: &str, error: bool) {
        self.app.notice = Some(Notice { text: text.into(), error });
    }

    /// Le categorie (per gli host che vogliono sapere dove sono i file).
    pub fn categories() -> &'static [conversioni::Category] {
        CATEGORIES
    }

    pub fn is_open(&self, f: Fold) -> bool {
        self.ui.is_open(f)
    }
}

/// I campi del PIN e del PUK: non si copiano negli appunti.
fn secret(field: Field) -> bool {
    matches!(field, Field::FirmaPin | Field::FirmaDlg(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        clip: String,
    }

    impl Platform for Fake {
        fn pick_files(&mut self) -> Vec<PathBuf> {
            Vec::new()
        }
        fn pick_folder(&mut self, _: &str) -> Option<PathBuf> {
            None
        }
        fn open_path(&mut self, _: &Path) {}
        fn copy_text(&mut self, text: &str) {
            self.clip = text.to_string();
        }
        fn paste_text(&mut self) -> Option<String> {
            Some(self.clip.clone())
        }
        fn open_guide(&mut self) {}
        fn open_settings(&mut self) {}
        fn minimize(&mut self) {}
        fn toggle_maximize(&mut self) {}
        fn close(&mut self) {}
        fn open_full(&mut self) {}
    }

    /// «Firma documenti», «Chiavetta e certificati» e «Opzioni»: si disegnano in tutti i temi, i passi
    /// vanno avanti solo quando possono e il riquadro della firma si trascina. Con CEDRO_UI_SHOTS=cartella
    /// salva le schermate (per confrontarle con l'anteprima approvata).
    #[test]
    fn firma_sign_pages_draw() {
        use crate::firma::pkcs11::{DevCert, DevKind, Device, PinState};
        use super::super::firmare::{Page, State};
        let dir = crate::engine::private_temp_dir("cedro-prova-firma-ui").unwrap();
        let pdf = dir.join("Determina_412_2026.pdf");
        std::fs::write(&pdf, crate::firma::pades_firma::tests::blank_pdf(false)).unwrap();
        let doc = dir.join("Lettera_risposta_prot_8841.txt");
        std::fs::write(&doc, "Gentile signora, in risposta alla sua richiesta…").unwrap();
        let key = p256::ecdsa::SigningKey::from_bytes(&[7u8; 32].into()).unwrap();
        let der = crate::firma::firmare::self_signed("MARIO ROSSI", &key);
        let cert = crate::firma::cert::Cert::parse(&der).unwrap();
        let dev = Device {
            driver: PathBuf::from("libbit4xpki.so"),
            slot: 0,
            kind: DevKind::UsbKey,
            label: "ArubaKey".into(),
            manufacturer: "bit4id".into(),
            model: "Aruba Key".into(),
            serial: "1234".into(),
            reader: "bit4id miniLector".into(),
            certs: vec![DevCert { der, cert, id: vec![1], label: "Firma".into(), has_key: true, signing: true }],
            pin: PinState { min_len: 5, max_len: 8, ..Default::default() },
        };
        let shots = std::env::var_os("CEDRO_UI_SHOTS").map(PathBuf::from);
        let mut plat = Fake { clip: String::new() };
        // Un clic sul controllo, se è disegnato nella finestra (se no è sotto: si fa l'azione).
        let click = |s: &mut Screen, act: &Act, plat: &mut Fake| {
            let _ = s.render(1440, 940, 1.0);
            if !s.hits.iter().any(|h| &h.act == act) {
                // Sotto: si scorre in fondo alla pagina.
                s.ui.scroll.insert(Scroll::FirmaSign, 10_000.0);
                let _ = s.render(1440, 940, 1.0);
                let _ = s.render(1440, 940, 1.0);
            }
            match s.hits.iter().rev().find(|h| &h.act == act).map(|h| h.r) {
                Some(r) if r.1 + r.3 / 2.0 < 930.0 => {
                    let (x, y) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
                    s.pointer_down(x, y);
                    s.pointer_up(x, y, plat);
                }
                Some(_) => s.act(act.clone(), plat),
                None => panic!("non disegnato: {act:?}"),
            }
        };
        for theme in super::super::theme::THEMES {
            let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
            s.request_appearance(theme, false);
            s.app.activity = Activity::Firma;
            let su = &mut s.app.firma.sign;
            su.scanned_once = true;
            su.devices = vec![dev.clone()];
            let shot = |s: &mut Screen, name: &str| {
                let (w, h) = (1440u32, 940u32);
                let p = s.render(w, h, 1.0);
                if let Some(d) = &shots {
                    p.save_png(&d.join(format!("{theme:?}-{name}.png"))).unwrap();
                }
            };
            shot(&mut s, "0-inizio");
            click(&mut s, &Act::FirmaPage(1), &mut plat);
            assert_eq!(s.app.firma.sign.page, Page::Sign);
            // Senza file non si va avanti (il pulsante è spento, e anche l'azione non fa niente).
            s.act(Act::FirmaStep(2), &mut plat);
            assert_eq!(s.app.firma.sign.step, 1);
            s.drop_files(vec![pdf.clone(), doc.clone()]);
            assert_eq!(s.app.firma.sign.files.len(), 2);
            shot(&mut s, "1-file");
            click(&mut s, &Act::FirmaStep(2), &mut plat);
            assert_eq!(s.app.firma.sign.step, 2);
            // Prima che arrivi l'anteprima della pagina si aspetta un poco (la prepara un thread).
            for _ in 0..50 {
                let _ = s.render(1440, 940, 1.0);
                s.pump();
                if s.app.previews.get(&(pdf.clone(), 0)).is_some_and(|i| i.is_some()) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            shot(&mut s, "2-come");
            click(&mut s, &Act::FirmaPlace(1), &mut plat);
            let _ = s.render(1440, 940, 1.0);
            let r = s.hits.iter().find(|h| h.act == Act::FirmaBox).map(|h| h.r).expect("la pagina si trascina");
            s.pointer_down(r.0 + r.2 * 0.3, r.1 + r.3 * 0.2);
            s.pointer_move(r.0 + r.2 * 0.4, r.1 + r.3 * 0.25);
            s.pointer_up(r.0 + r.2 * 0.4, r.1 + r.3 * 0.25, &mut plat);
            let (cx, cy) = s.app.firma.sign.box_center;
            assert!((cx - 0.4).abs() < 0.01 && (cy - 0.25).abs() < 0.01, "{cx} {cy}");
            shot(&mut s, "2-come-trascinata");
            click(&mut s, &Act::FirmaMore, &mut plat);
            shot(&mut s, "2-altre-opzioni");
            click(&mut s, &Act::FirmaStep(3), &mut plat);
            assert_eq!(s.app.firma.sign.step, 3);
            assert_eq!(s.ui.edit, Some(Field::FirmaPin));
            s.text_input("12345");
            assert_eq!(s.app.firma.sign.pin, "12345");
            // Il PIN non va negli appunti.
            s.key(Key::Copy, &mut plat);
            assert!(plat.clip.is_empty());
            shot(&mut s, "3-pin");
            {
                let su = &mut s.app.firma.sign;
                su.pin.clear();
                for f in &mut su.files {
                    f.state = State::Done { at: crate::firma::now(), signers: vec!["MARIO ROSSI".into()] };
                }
                su.step = 4;
            }
            shot(&mut s, "4-fatto");
            click(&mut s, &Act::FirmaPage(0), &mut plat);
            assert!(s.app.firma.sign.files.is_empty());
            click(&mut s, &Act::FirmaPage(2), &mut plat);
            shot(&mut s, "5-chiavetta");
            click(&mut s, &Act::FirmaDlg(0), &mut plat);
            shot(&mut s, "5-cambia-pin");
            click(&mut s, &Act::FirmaDlgClose, &mut plat);
            click(&mut s, &Act::FirmaPage(0), &mut plat);
            click(&mut s, &Act::FirmaPage(3), &mut plat);
            shot(&mut s, "6-opzioni");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Il Cedro di partenza: l'avviso resta sulla pagina che ha fatto il lavoro e, tornando su «PDF»
    /// da «Converti», si ritrova la voce lasciata (Unisci), non quella di partenza.
    #[test]
    fn cedro_pages_keep_their_own_notice_and_pdf_choice() {
        use super::super::model::{Lab, Notice};
        let dir = crate::engine::private_temp_dir("cedro-prova-pagine").unwrap();
        let pdf = dir.join("Circolare.pdf");
        std::fs::write(&pdf, crate::firma::pades_firma::tests::blank_pdf(false)).unwrap();
        let jpg = dir.join("Foto.jpg");
        image::RgbImage::from_pixel(64, 48, image::Rgb([200, 120, 60])).save(&jpg).unwrap();
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(ThemeId::Cedro, false);
        s.drop_files(vec![pdf, jpg]);
        let mut plat = Fake { clip: String::new() };
        s.act(Act::Lab(Lab::Pdf), &mut plat);
        assert_eq!(s.app.kind(), crate::engine::Kind::Pdf);
        s.app.set_action("merge");
        s.app.notice = Some(Notice { text: "Fatto: 2 file.".into(), error: false });
        s.act(Act::Lab(Lab::Converti), &mut plat);
        assert!(s.app.notice.is_none(), "l'avviso non deve seguire sulle altre pagine");
        assert_ne!(s.app.kind(), crate::engine::Kind::Pdf);
        s.act(Act::Lab(Lab::Pdf), &mut plat);
        assert_eq!(s.app.action, "merge");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn centre_of(s: &Screen, act: &Act) -> (f32, f32) {
        let r = s.hits.iter().rev().find(|h| &h.act == act).map(|h| h.r).expect("controllo disegnato");
        (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0)
    }

    fn click(s: &mut Screen, act: &Act, plat: &mut Fake) {
        let (w, h) = s.preferred_size();
        let _ = s.render(w as u32, h as u32, 1.0);
        let (x, y) = centre_of(s, act);
        s.pointer_down(x, y);
        s.pointer_up(x, y, plat);
    }

    fn right(s: &mut Screen, x: f32, y: f32) -> bool {
        let (w, h) = s.preferred_size();
        let _ = s.render(w as u32, h as u32, 1.0);
        s.context_click(x, y)
    }

    #[test]
    fn right_click_menu_in_the_link_field() {
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(ThemeId::WindowsEssenziale, false);
        s.app.activity = Activity::YouTube;
        let mut plat = Fake { clip: "https://vimeo.com/76979871".into() };
        let field = Act::Edit(Field::Link(false));
        let (w, h) = s.preferred_size();
        let _ = s.render(w as u32, h as u32, 1.0);
        let (x, y) = centre_of(&s, &field);
        assert!(right(&mut s, x, y));
        let labels: Vec<&str> = s.ui.menu.as_ref().unwrap().items.iter().map(|i| i.0.as_str()).collect();
        assert_eq!(labels, ["Taglia", "Copia", "Incolla", "Elimina", "Seleziona tutto"]);
        // Incolla: il link di un sito che non è YouTube entra nel campo e poi in coda.
        click(&mut s, &Act::MenuPick(2), &mut plat);
        assert!(s.ui.menu.is_none());
        assert_eq!(s.app.youtube.typed, "https://vimeo.com/76979871");
        s.app.add_typed_links(false);
        assert_eq!(s.app.youtube.items.len(), 1);
        assert!(s.app.youtube.typed.is_empty());

        // Seleziona tutto, poi scrivere sostituisce; Taglia svuota e copia.
        s.app.youtube.typed = "abc".into();
        assert!(right(&mut s, x, y));
        click(&mut s, &Act::MenuPick(4), &mut plat);
        assert!(s.ui.sel_all);
        s.text_input("x");
        assert_eq!(s.app.youtube.typed, "x");
        assert!(right(&mut s, x, y));
        click(&mut s, &Act::MenuPick(0), &mut plat);
        assert_eq!(plat.clip, "x");
        assert!(s.app.youtube.typed.is_empty());

        // Da tastiera: la freccia sceglie la prima voce, Esc chiude il menu e il campo resta attivo.
        assert!(right(&mut s, x, y));
        assert!(s.key(Key::Down, &mut plat));
        assert_eq!(s.ui.menu.as_ref().unwrap().selected, Some(0));
        assert!(s.key(Key::Escape, &mut plat));
        assert!(s.ui.menu.is_none());
        assert_eq!(s.ui.edit, Some(Field::Link(false)));
    }

    #[test]
    fn review_play_keeps_playing_across_frames() {
        use crate::trascrizione::Segment;
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.request_appearance(ThemeId::WindowsEssenziale, false);
        s.app.activity = Activity::Transcribe;
        let segments = vec![Segment { t0: 0, t1: 500, text: "Buongiorno.".into(), speaker: None }, Segment { t0: 600, t1: 1100, text: "Arrivederci.".into(), speaker: None }];
        s.app.review = Some(crate::ui::review::Review::new(Path::new("/tmp/cedro-non-c-e.wav"), 12.0, "it".into(), segments, Vec::new(), None));
        let mut plat = Fake { clip: String::new() };
        // Il tempo di una frase la fa sentire da lì: il lettore resta acceso anche dopo altri fotogrammi.
        click(&mut s, &Act::ReviewRow(1), &mut plat);
        let (w, h) = s.preferred_size();
        let _ = s.render(w as u32, h as u32, 1.0);
        let _ = s.render(w as u32, h as u32, 1.0);
        assert!(s.app.review_playing());
        assert!(s.app.review_position() >= 6.0);
        // Play mette in pausa, e la pausa resta.
        click(&mut s, &Act::ReviewPlay, &mut plat);
        let _ = s.render(w as u32, h as u32, 1.0);
        assert!(!s.app.review_playing());
        assert!(s.app.player.is_some());
    }

    #[test]
    fn a_wrong_link_shows_an_error() {
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.app.activity = Activity::YouTube;
        s.app.youtube.typed = "ciao".into();
        s.app.add_typed_links(false);
        assert!(s.app.youtube.items.is_empty());
        assert!(s.app.notice.as_ref().is_some_and(|n| n.error));
    }

    /// Prova a mano: `CEDRO_UI_FILES=a.docx:b.xlsx CEDRO_UI_SHOTS=cartella cargo test --lib foto_ufficio -- --ignored`
    /// apre ogni file nella finestra come se fosse trascinato e salva la schermata con la sua anteprima.
    #[test]
    #[ignore]
    fn foto_ufficio() {
        let (Some(files), Some(dir)) = (std::env::var_os("CEDRO_UI_FILES"), std::env::var_os("CEDRO_UI_SHOTS")) else { return };
        let dir = PathBuf::from(dir);
        for f in std::env::split_paths(&files) {
            let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
            s.drop_files(vec![f.clone()]);
            for _ in 0..100 {
                let _ = s.render(1440, 940, 1.0);
                s.pump();
                if s.app.previews.iter().any(|((p, _), i)| p == &f && i.is_some()) || s.app.previews_failed.iter().any(|(p, _)| p == &f) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let _ = s.render(1440, 940, 1.0);
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            s.render(1440, 940, 1.0).save_png(&dir.join(format!("{name}.png"))).unwrap();
        }
    }
}
