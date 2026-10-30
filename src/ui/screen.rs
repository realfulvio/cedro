//! Una finestra della nuova interfaccia, senza sistema operativo: tiene lo stato, disegna il
//! fotogramma e risponde a mouse, tastiera e file trascinati. Windows e Linux la ospitano e le
//! danno i servizi che solo loro hanno (scegliere file, aprire cartelle, appunti, la finestra).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::core::{inside, Act, Field, Fold, Frame, HitKind, LinkFlag, LinkMenu, Menu, Scroll, UiState, R};
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
}

/// Che finestra è.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// La finestra principale del tema.
    Main,
    /// La finestrella che si apre dall'icona vicino all'orologio (Cedro compatto).
    Compact,
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
    focusables: Vec<Act>,
}

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
        };
        views::draw(&mut f, &self.app, self.surface);
        views::overlays(&mut f, &self.app);
        f.menu_overlay();
        self.animating = f.animating || self.app.busy();
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
        if self.theme != ThemeId::DocumentoAlCentro || self.surface != Surface::Main || self.app.activity != Activity::Convert {
            self.app.player = None;
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
        self.ui.menu = Some(Menu { anchor: (x, y - 4.0, 0.0, 0.0), items, selected: None });
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
        self.add_files(paths);
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        let simple = self.simple;
        self.app.add_files(paths, simple);
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
        // In Semplificata la scelta deve stare tra quelle proposte; le opzioni scelte restano.
        if simple && !was_simple && !self.app.simple_choices().iter().any(|f| f.id == self.app.action) {
            let keep = conversioni::formats(self.app.kind()).iter().chain(conversioni::tools(self.app.kind())).any(|s| s.id == self.app.action);
            if !keep {
                self.app.action = self.app.default_action(true);
            }
        }
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
        self.ui.menu = Some(Menu { anchor, items, selected });
    }

    /// Le voci del menu «Formato»: i formati, poi gli strumenti della categoria.
    pub fn action_items(&self) -> Vec<(String, Act)> {
        let kind = self.app.kind();
        let mut items: Vec<(String, Act)> = Vec::new();
        if self.simple {
            for f in self.app.simple_choices() {
                items.push((format!("{} · {}", f.title, f.note), Act::Action(f.id)));
            }
        } else {
            for s in conversioni::formats(kind) {
                items.push((s.label.to_string(), Act::Action(s.id)));
            }
            for s in conversioni::tools(kind) {
                items.push((format!("Strumento · {}", s.label), Act::Action(s.id)));
            }
        }
        items
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
                let items = self.action_items();
                let sel = items.iter().position(|(_, a)| matches!(a, Act::Action(id) if *id == self.app.action));
                self.open_menu(items, sel, &Act::FormatMenu);
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
            Act::MediaPlay => app.media_toggle(),
            Act::Edit(field) => {
                // Una correzione si annulla in un colpo solo: l'undo si prende quando si entra nella frase.
                if let (Field::Review(i), Some(r)) = (field, app.review.as_mut()) {
                    if self.ui.edit != Some(field) {
                        r.push_undo();
                    }
                    r.sel = Some(i);
                }
                self.ui.edit = Some(field);
                self.ui.sel_all = false;
                self.ui.caret = self.field_text(field).chars().count();
            }
            Act::FieldCopy | Act::FieldCut | Act::FieldClear | Act::FieldPaste | Act::FieldSelectAll => {
                if let Some(field) = self.ui.edit {
                    let text = self.field_text(field);
                    match act {
                        Act::FieldCopy => plat.copy_text(&text),
                        Act::FieldCut => {
                            plat.copy_text(&text);
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
                    _ => app.transcribe.log.iter().map(|l| if l.key.is_empty() { l.text.clone() } else { format!("{} {}", l.key, l.text) }).collect::<Vec<_>>().join("\n"),
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
        }
    }

    fn field_text(&self, field: Field) -> String {
        match field {
            Field::Opt(opt) => self.app.option_defs().into_iter().find(|d| d.opt == opt).map(|d| self.app.values.text(d).to_string()).unwrap_or_default(),
            Field::Link(web) => self.app.links(web).typed.clone(),
            Field::FilePages(i) => self.app.files.get(i).map(|f| f.pages.clone()).unwrap_or_default(),
            Field::Review(i) => self.app.review.as_ref().and_then(|r| r.rows.get(i)).map(|r| r.text.clone()).unwrap_or_default(),
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
                    plat.copy_text(&chars.iter().collect::<String>());
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
    fn a_wrong_link_shows_an_error() {
        let mut s = Screen::new(Surface::Main, Arc::new(|| {}));
        s.app.activity = Activity::YouTube;
        s.app.youtube.typed = "ciao".into();
        s.app.add_typed_links(false);
        assert!(s.app.youtube.items.is_empty());
        assert!(s.app.notice.as_ref().is_some_and(|n| n.error));
    }
}
