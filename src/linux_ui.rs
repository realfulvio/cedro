//! Ruota su GNOME e Plasma. Wayland non lascia prendere il clic destro di un altro programma,
//! quindi la ruota si apre dal menu del file. Shift, Ctrl e Alt, mentre è aperta, cambiano fetta.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cedro::ui::wheel::{Effect, WheelUi};
use cedro::{append_log, kind_of, perform_into, read_quality, read_settings, same_kind_paths, update, Mode, Settings};
use gtk4::cairo::{FontSlant, FontWeight};
use crate::linux_settings;
use gtk4::gdk::Key;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, CssProvider, DrawingArea, EventControllerKey, EventControllerMotion,
    GestureClick, STYLE_PROVIDER_PRIORITY_APPLICATION,
};

const MARK: &[u8] = cedro::edizione::ICONA;

static UPDATE: Mutex<Option<UpdateNews>> = Mutex::new(None);
static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);
/// Cosa ha chiesto la tray: la legge il ciclo principale, perché le finestre vanno aperte da lì.
static ASKED: Mutex<Option<Asked>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq)]
enum Asked {
    Settings,
    Guide,
    Conversions,
    Compact,
}

fn cfg() -> Settings {
    let mut slot = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    slot.get_or_insert_with(read_settings).clone()
}

fn remember(settings: &Settings) {
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Some(settings.clone());
}

#[derive(Clone, Copy, PartialEq)]
enum Open {
    Nothing,
    Settings,
    Guide,
    Conversions,
}

enum UpdateNews {
    Found { release: update::Release, manual: bool },
    Note(String),
}

struct Launch {
    mode: Mode,
    paths: Vec<PathBuf>,
    stay: bool,
    open: Open,
    background: bool,
}

struct Anchor {
    x: f64,
    y: f64,
    monitor: usize,
}

struct Screen {
    width: f64,
    height: f64,
}

struct Wheel {
    launch: Launch,
    hover: i32,
    anchor: Option<Anchor>,
    // Il centro dello schermo vale finché non arriva il primo vero movimento del puntatore.
    provisional: bool,
    screens: Vec<Screen>,
    areas: Vec<DrawingArea>,
    windows: Vec<ApplicationWindow>,
    busy: bool,
    finished: Arc<Mutex<Option<String>>>,
    // La guardia esiste solo per il suo Drop: senza un riferimento vivo il vassoio si chiude.
    #[allow(dead_code)]
    resident: Option<gtk4::gio::ApplicationHoldGuard>,
    job: Option<gtk4::gio::ApplicationHoldGuard>,
    /// La ruota della versione standard (vuota in quella del Comune di Bondeno).
    look: Option<WheelUi>,
    /// La miniatura del file, preparata da un altro thread.
    thumb: Arc<Mutex<Option<cedro::ui::gfx::Image>>>,
}

pub fn start() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch = parse_launch(&args);
    install_integrations();
    let quit = Arc::new(AtomicBool::new(false));
    if launch.stay {
        spawn_tray(quit.clone());
        daily_updates();
    }
    let state = Rc::new(RefCell::new(Wheel {
        launch,
        hover: -1,
        anchor: None,
        provisional: false,
        screens: Vec::new(),
        areas: Vec::new(),
        windows: Vec::new(),
        busy: false,
        finished: Arc::new(Mutex::new(None)),
        resident: None,
        job: None,
        look: None,
        thumb: Arc::new(Mutex::new(None)),
    }));
    prepare_look(&state);
    // Ogni apertura dal menu del file è un processo a sé: se condividesse l'id con quello
    // residente nella tray, D-Bus lo girerebbe a lui, che non ha file e non mostra nulla.
    let mut flags = gtk4::gio::ApplicationFlags::empty();
    if !state.borrow().launch.stay || state.borrow().launch.open != Open::Nothing {
        flags |= gtk4::gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = Application::builder().application_id("dev.cedro.Cedro").flags(flags).build();
    let opened = state.clone();
    let flag = quit.clone();
    let started = std::cell::Cell::new(false);
    let started_font = std::cell::Cell::new(false);
    app.connect_activate(move |app| {
        if !started_font.replace(true) {
            crate::linux_font::light_theme();
            if crate::linux_font::load() {
                crate::linux_font::apply();
            }
        }
        let launch_open = opened.borrow().launch.open;
        let background = opened.borrow().launch.background;
        if started.replace(true) {
            // Un secondo `cedro` senza file, mentre quello residente è già attivo: mostra le Impostazioni.
            if opened.borrow().launch.paths.is_empty() {
                open_settings_window(app);
            }
            return;
        }
        if opened.borrow().launch.stay {
            let guard = app.hold();
            opened.borrow_mut().resident = Some(guard);
        }
        present_wheel(app, opened.clone());
        match launch_open {
            Open::Settings => open_settings_window(app),
            Open::Guide => linux_settings::open_guide(app),
            Open::Conversions => open_panel(app),
            // Avviato a mano, senza file: si vede subito che c'è, anche senza vassoio.
            Open::Nothing if opened.borrow().launch.stay && !background => {
                if cedro::read_settings().view_asked {
                    open_settings_window(app)
                } else {
                    linux_settings::open_view_choice(app)
                }
            }
            Open::Nothing => {}
        }
        let app = app.clone();
        let flag = flag.clone();
        let opened = opened.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            if flag.load(Ordering::Relaxed) {
                app.quit();
                return glib::ControlFlow::Break;
            }
            if let Some(news) = UPDATE.lock().ok().and_then(|mut slot| slot.take()) {
                on_update(&app, news);
            }
            match ASKED.lock().ok().and_then(|mut slot| slot.take()) {
                Some(Asked::Settings) => open_settings_window(&app),
                Some(Asked::Guide) => linux_settings::open_guide(&app),
                Some(Asked::Conversions) => open_panel(&app),
                Some(Asked::Compact) => crate::linux_ui2::open_compact(&app),
                None => {}
            }
            let thumb_ready = opened.borrow().thumb.lock().map(|g| g.is_some()).unwrap_or(false);
            if thumb_ready {
                redraw_all(&opened);
            }
            let slot = opened.borrow().finished.clone();
            if let Some(note) = slot.lock().ok().and_then(|mut done| done.take()) {
                if cfg().notify {
                    notify(&app, &note);
                }
                opened.borrow_mut().busy = false;
                opened.borrow_mut().job.take();
                if !opened.borrow().launch.stay && app.windows().is_empty() {
                    app.quit();
                    return glib::ControlFlow::Break;
                }
            }
            if app.windows().is_empty() && !opened.borrow().launch.stay && !opened.borrow().busy {
                app.quit();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    });
    app.run_with_args(&["cedro"]).get() as i32
}

/// Il pannello delle conversioni.
fn open_panel(app: &Application) {
    crate::linux_ui2::open(app);
}

pub(crate) fn open_settings_window(app: &Application) {
    linux_settings::open_settings(app, |settings| remember(settings));
}

fn parse_launch(args: &[String]) -> Launch {
    let mut mode = Mode::Convert;
    let mut paths = Vec::new();
    let mut open = Open::Nothing;
    let mut background = false;
    for arg in args.iter().filter(|arg| arg.as_str() != "show") {
        match arg.as_str() {
            "settings" | "impostazioni" => open = Open::Settings,
            "guida" => open = Open::Guide,
            "conversioni" => open = Open::Conversions,
            "--background" => background = true,
            "--conversione" => mode = Mode::Convert,
            "--strumenti" => mode = Mode::Tools,
            "--azioni" => mode = Mode::Actions,
            other if !other.starts_with('-') => {
                let path = PathBuf::from(other);
                if path.is_file() {
                    paths.push(path);
                }
            }
            _ => {}
        }
    }
    Launch { stay: paths.is_empty() && open == Open::Nothing, mode, paths, open, background }
}

fn present_wheel(app: &Application, state: Rc<RefCell<Wheel>>) {
    let paths_empty = state.borrow().launch.paths.is_empty();
    if paths_empty {
        return;
    }
    let Some(display) = gtk4::gdk::Display::default() else {
        eprintln!("Cedro non vede uno schermo.");
        return;
    };
    let provider = CssProvider::new();
    provider.load_from_string("window.cedro, window.cedro drawingarea { background-color: transparent; }");
    gtk4::style_context_add_provider_for_display(&display, &provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    let monitors = display.monitors();
    for index in 0..monitors.n_items() {
        let Some(item) = monitors.item(index) else { continue };
        let Ok(monitor) = item.downcast::<gtk4::gdk::Monitor>() else { continue };
        let geo = monitor.geometry();
        let window = ApplicationWindow::builder()
            .application(app)
            .decorated(false)
            .title("Cedro")
            .build();
        window.add_css_class("cedro");
        let area = DrawingArea::new();
        area.add_css_class("cedro");
        area.set_hexpand(true);
        area.set_vexpand(true);
        window.set_child(Some(&area));
        let draw_state = state.clone();
        let monitor_index = state.borrow().screens.len();
        area.set_draw_func(move |_, cr, width, height| draw_wheel(cr, width, height, monitor_index, &draw_state));
        let motion = EventControllerMotion::new();
        let move_state = state.clone();
        let move_area = area.clone();
        motion.connect_motion(move |_, x, y| {
            point_at(&move_state, monitor_index, x, y);
            move_area.queue_draw();
        });
        let enter_state = state.clone();
        let enter_area = area.clone();
        motion.connect_enter(move |_, x, y| {
            point_at(&enter_state, monitor_index, x, y);
            enter_area.queue_draw();
        });
        area.add_controller(motion);
        let click = GestureClick::new();
        click.set_button(1);
        let click_state = state.clone();
        let click_app = app.clone();
        click.connect_pressed(move |_, _, x, y| press_look(&click_app, &click_state, monitor_index, x, y));
        let up_state = state.clone();
        click.connect_released(move |_, _, _, _| {
            if let Some(ui) = up_state.borrow_mut().look.as_mut() {
                ui.release();
            }
        });
        area.add_controller(click);
        let key = EventControllerKey::new();
        let key_state = state.clone();
        let key_app = app.clone();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == Key::Escape {
                close_wheel(&key_state);
                return glib::Propagation::Stop;
            }
            if let Some(mode) = mode_of(key) {
                set_mode(&key_state, mode);
            }
            if matches!(key, Key::Return | Key::KP_Enter) && key_state.borrow().look.as_ref().is_some_and(|u| u.current().is_some()) {
                // Invio esegue la fetta in evidenza, come il pulsante «Converti».
                let at = key_state.borrow().look.as_ref().and_then(|u| u.current());
                if let Some(i) = at {
                    key_state.borrow_mut().hover = i as i32;
                    choose_hover(&key_app, &key_state);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(key);
        window.fullscreen_on_monitor(&monitor);
        window.present();
        state.borrow_mut().screens.push(Screen { width: geo.width() as f64, height: geo.height() as f64 });
        state.borrow_mut().areas.push(area);
        state.borrow_mut().windows.push(window);
    }
    // Solo nelle build di prova: `CEDRO_AUTOCHOOSE=<numero>` sceglie da solo quella fetta, come un clic dell'utente.
    #[cfg(debug_assertions)]
    if let Some(index) = std::env::var("CEDRO_AUTOCHOOSE").ok().and_then(|v| v.parse::<usize>().ok()) {
        let state = state.clone();
        let app = app.clone();
        glib::timeout_add_local_once(Duration::from_millis(900), move || {
            let (width, height, n) = {
                let wheel = state.borrow();
                let Some(screen) = wheel.screens.first() else { return };
                (screen.width, screen.height, current_slices(&wheel).0.len())
            };
            point_at(&state, 0, width / 2.0, height / 2.0);
            if state.borrow().look.is_some() {
                state.borrow_mut().hover = index as i32;
                choose_hover(&app, &state);
                return;
            }
            let (ox, oy) = origin(width / 2.0, height / 2.0, width, height);
            let (a0, a1) = cedro::wheelgeom_bounds(index, n);
            let mid = (a0 + a1) / 2.0;
            let radius = (cedro::wheelgeom::R0 + cedro::wheelgeom::R1) / 2.0;
            let x = ox + 220.0 + (radius * mid.cos()) as f64;
            let y = oy + 220.0 + (radius * mid.sin()) as f64;
            choose(&app, &state, 0, x, y);
        });
    }
    let later = state.clone();
    glib::timeout_add_local_once(Duration::from_millis(50), move || {
        let mut wheel = later.borrow_mut();
        if wheel.anchor.is_none() {
            if let Some(screen) = wheel.screens.first() {
                wheel.anchor = Some(Anchor { x: screen.width / 2.0, y: screen.height / 2.0, monitor: 0 });
                wheel.provisional = true;
            }
        }
        let areas = wheel.areas.clone();
        drop(wheel);
        for area in areas {
            area.queue_draw();
        }
    });
}

/// Le fette del file e del modo, nell'ordine in cui si vedono.
fn wheel_list(kind: cedro::Kind, mode: Mode) -> Vec<cedro::Slice> {
    cedro::ui::wheel::list(kind, mode)
}

fn mode_code(mode: Mode) -> u8 {
    match mode {
        Mode::Convert => 0,
        Mode::Tools => 1,
        Mode::Actions => 2,
    }
}

/// Costruisce la ruota per i file lanciati e ne prepara la miniatura.
fn prepare_look(state: &Rc<RefCell<Wheel>>) {
    if state.borrow().launch.paths.is_empty() {
        return;
    }
    let s = read_settings();
    let (paths, mode) = {
        let w = state.borrow();
        (w.launch.paths.clone(), w.launch.mode)
    };
    let kind = kind_of(&paths[0]);
    let mut ui = WheelUi::new(kind, mode_code(mode), s.simple);
    ui.modes_on = [s.key_shift, s.key_ctrl, s.key_alt];
    ui.quality = s.quality;
    ui.beside = if s.dest == cedro::Dest::Same { cedro::ui::wheel::Beside::Locked } else { cedro::ui::wheel::Beside::Off };
    ui.dest_note = match s.dest {
        cedro::Dest::Same => "Copia accanto all'originale".to_string(),
        cedro::Dest::Sub => "Copia nella cartella «Cedro»".to_string(),
        cedro::Dest::Fixed => {
            let name = std::path::Path::new(s.dest_dir.trim()).file_name().and_then(|n| n.to_str()).unwrap_or("la cartella scelta").to_string();
            format!("Copia in {name}")
        }
    };
    ui.has_file = true;
    let first = cedro::ui::wheel::file_name_of(&paths[0]);
    ui.name = if paths.len() > 1 { format!("{first} (+{})", paths.len() - 1) } else { first };
    let slot = state.borrow().thumb.clone();
    let first_path = paths[0].clone();
    std::thread::spawn(move || {
        if let Some(img) = cedro::ui::media::thumb_of(&first_path, 420) {
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(img);
        }
    });
    state.borrow_mut().look = Some(ui);
}

/// Dove sta la finestra della ruota nuova nello schermo (in pixel logici) e quanto è grande.
fn look_frame(ui: &WheelUi, anchor_x: f64, anchor_y: f64, width: f64, height: f64) -> (f64, f64, f64, f64) {
    let z = cedro::ui::ZOOM as f64;
    let (w, h) = ui.size();
    let (cx, cy) = ui.center();
    let (w, h, cx, cy) = (w as f64 * z, h as f64 * z, cx as f64 * z, cy as f64 * z);
    let ox = (anchor_x - cx).clamp(0.0, (width - w).max(0.0));
    let oy = (anchor_y - cy).clamp(0.0, (height - h).max(0.0));
    (ox, oy, w, h)
}

fn point_at(state: &Rc<RefCell<Wheel>>, monitor: usize, x: f64, y: f64) {
    let mut wheel = state.borrow_mut();
    // L'ancora si fissa al primo ingresso del puntatore: se seguisse il mouse, il centro
    // starebbe sempre sotto il cursore e nessuna fetta potrebbe essere scelta.
    if wheel.anchor.is_none() || wheel.provisional {
        wheel.anchor = Some(Anchor { x, y, monitor });
        wheel.provisional = false;
    }
    wheel.hover = hover_of(&wheel, monitor, x, y);
    if let (Some(anchor), Some(screen)) = (wheel.anchor.as_ref().map(|a| (a.x, a.y, a.monitor)), wheel.screens.get(monitor).map(|s| (s.width, s.height))) {
        if anchor.2 == monitor {
            let z = cedro::ui::ZOOM as f64;
            if let Some(ui) = wheel.look.as_mut() {
                let (ox, oy, _, _) = look_frame(ui, anchor.0, anchor.1, screen.0, screen.1);
                ui.pointer_move(((x - ox) / z) as f32, ((y - oy) / z) as f32);
            }
        }
    }
}

fn hover_of(wheel: &Wheel, monitor: usize, x: f64, y: f64) -> i32 {
    let Some(anchor) = wheel.anchor.as_ref() else {
        return -1;
    };
    if anchor.monitor != monitor {
        return -2;
    }
    let Some(screen) = wheel.screens.get(monitor) else {
        return -2;
    };
    if let Some(ui) = wheel.look.as_ref() {
        let (ox, oy, _, _) = look_frame(ui, anchor.x, anchor.y, screen.width, screen.height);
        let z = cedro::ui::ZOOM as f64;
        return match ui.zone_at(((x - ox) / z) as f32, ((y - oy) / z) as f32) {
            cedro::ui::wheel::Zone::Slice(i) => i as i32,
            _ => -1,
        };
    }
    let (ox, oy) = origin(anchor.x, anchor.y, screen.width, screen.height);
    let n = current_slices(wheel).0.len();
    cedro::wheelgeom_hover(ox as f32 + 220.0, oy as f32 + 220.0, x as f32, y as f32, n)
}

fn choose(app: &Application, state: &Rc<RefCell<Wheel>>, monitor: usize, x: f64, y: f64) {
    point_at(state, monitor, x, y);
    choose_hover(app, state);
}

/// Un clic sulla ruota nuova: lei dice cosa fare (scegliere una fetta, spostare il cursore, un pulsante).
fn press_look(app: &Application, state: &Rc<RefCell<Wheel>>, monitor: usize, x: f64, y: f64) {
    point_at(state, monitor, x, y);
    let effect = {
        let mut wheel = state.borrow_mut();
        let Some(anchor) = wheel.anchor.as_ref().map(|a| (a.x, a.y)) else { return };
        let Some(screen) = wheel.screens.get(monitor).map(|s| (s.width, s.height)) else { return };
        let z = cedro::ui::ZOOM as f64;
        let Some(ui) = wheel.look.as_mut() else { return };
        let (ox, oy, _, _) = look_frame(ui, anchor.0, anchor.1, screen.0, screen.1);
        ui.press(((x - ox) / z) as f32, ((y - oy) / z) as f32)
    };
    match effect {
        Effect::Commit(i) => {
            state.borrow_mut().hover = i as i32;
            choose_hover(app, state);
        }
        Effect::Close => close_wheel(state),
        Effect::Redraw | Effect::None => redraw_all(state),
        Effect::OpenFull => {
            let (paths, action) = {
                let w = state.borrow();
                let kind = w.launch.paths.first().map(|p| kind_of(p)).unwrap_or(cedro::Kind::Other);
                let action = w.look.as_ref().and_then(|u| u.current()).and_then(|i| wheel_list(kind, w.launch.mode).get(i).map(|s| s.id));
                (w.launch.paths.clone(), action.filter(|_| w.launch.mode == Mode::Convert))
            };
            close_wheel(state);
            crate::linux_ui2::open_with(app, paths, action);
        }
        Effect::Mode(m) => {
            set_mode(state, match m {
                1 => Mode::Tools,
                2 => Mode::Actions,
                _ => Mode::Convert,
            });
        }
        Effect::Simple(on) => {
            let mut saved = read_settings();
            saved.simple = on;
            let _ = cedro::write_settings(&saved);
            remember(&saved);
            if let Some(ui) = state.borrow_mut().look.as_mut() {
                ui.set_simple(on);
            }
            redraw_all(state);
        }
    }
}

fn redraw_all(state: &Rc<RefCell<Wheel>>) {
    for area in state.borrow().areas.iter() {
        area.queue_draw();
    }
}

/// Cambia modo (Shift, Ctrl, Alt): la ruota rifà le sue fette.
fn set_mode(state: &Rc<RefCell<Wheel>>, mode: Mode) {
    {
        let mut w = state.borrow_mut();
        w.launch.mode = mode;
        w.hover = -1;
        let kind = w.launch.paths.first().map(|p| kind_of(p)).unwrap_or(cedro::Kind::Other);
        if let Some(ui) = w.look.as_mut() {
            ui.set_kind_mode(kind, mode_code(mode));
        }
    }
    redraw_all(state);
}

fn choose_hover(app: &Application, state: &Rc<RefCell<Wheel>>) {
    let job = {
        let wheel = state.borrow();
        let hover = wheel.hover;
        let mode = wheel.launch.mode;
        let paths = wheel.launch.paths.clone();
        let stay = wheel.launch.stay;
        (hover, mode, paths, stay)
    };
    let (quality, beside) = {
        let w = state.borrow();
        let quality = w.look.as_ref().map_or_else(read_quality, |u| u.quality);
        (quality, w.look.as_ref().is_some_and(|u| u.beside == cedro::ui::wheel::Beside::On))
    };
    let (hover, mode, paths, stay) = job;
    if paths.is_empty() || hover < 0 {
        close_wheel(state);
        return;
    }
    let kind = kind_of(&paths[0]);
    let list = wheel_list(kind, mode);
    let Some(slice) = list.get(hover as usize) else {
        close_wheel(state);
        return;
    };
    if slice.id == "none" {
        close_wheel(state);
        notify(app, "Niente da fare su questo file");
        if !stay {
            app.quit();
        }
        return;
    }
    if slice.id == "path" {
        let text = paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join("\n");
        copy_text(&text);
        close_wheel(state);
        notify(app, &text);
        if !stay {
            app.quit();
        }
        return;
    }
    let id = slice.id.to_string();
    let label = slice.label.to_string();
    close_wheel(state);
    state.borrow_mut().busy = true;
    state.borrow_mut().job = Some(app.hold());
    let slot = state.borrow().finished.clone();
    std::thread::spawn(move || {
        let note = run_slices(&paths, &id, &label, quality, beside);
        if let Ok(mut done) = slot.lock() {
            *done = Some(note);
        }
    });
}

fn run_slices(all: &[PathBuf], id: &str, label: &str, quality: u8, beside: bool) -> String {
    let paths = if matches!(id, "dup" | "zip") { all.to_vec() } else { same_kind_paths(all) };
    let skipped = all.len().saturating_sub(paths.len());
    let mut notes = Vec::new();
    let mut failed = None;
    let mut last_out = None;
    let name = |p: &std::path::Path| p.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    let name = move |p: &std::path::Path| cedro::i18n::lit(&name(p));
    let step_label = |i: usize, p: &std::path::Path| {
        if paths.len() == 1 { format!("{} → {label}", name(p)) } else { format!("{} di {} · {}", i + 1, paths.len(), name(p)) }
    };
    if let Some(first) = paths.first() {
        crate::linux_progress::start(paths.len(), &step_label(0, first));
    }
    for (i, path) in paths.iter().enumerate() {
        crate::linux_progress::step(i, &step_label(i, path));
        // «Salva accanto all'originale» spuntato nella ruota vale per questo lavoro e basta.
        let out_dir = if beside { path.parent() } else { None };
        match perform_into(path, id, quality, out_dir) {
            Ok(out) => {
                notes.push(format!("{} · {}", name(&out.path), out.message));
                last_out = Some(out.path);
            }
            Err(err) => {
                failed.get_or_insert_with(|| err.0.clone());
                notes.push(err.0);
            }
        }
    }
    match &failed {
        Some(e) => crate::linux_progress::finish(false, &format!("Errore · {e}")),
        None => crate::linux_progress::finish(true, &format!("Fatto · {} file in {label}", paths.len())),
    }
    if skipped > 0 {
        notes.push(format!("{skipped} file di altro tipo lasciati stare"));
    }
    let summary = if notes.is_empty() {
        label.to_string()
    } else if notes.len() == 1 {
        notes.remove(0)
    } else {
        format!("{} file\n{}", notes.len(), notes.join("\n"))
    };
    append_log(&summary);
    if let (true, Some(out)) = (read_settings().reveal, last_out) {
        reveal_in_file_manager(&out);
    }
    summary
}

/// Apre il file manager sulla cartella del file nuovo, con il file selezionato (Dolphin, Nautilus e gli altri
/// che parlano `org.freedesktop.FileManager1`); senza, apre solo la cartella.
fn reveal_in_file_manager(path: &std::path::Path) {
    let uri = gtk4::gio::File::for_path(path).uri().to_string();
    let shown = gtk4::gio::bus_get_sync(gtk4::gio::BusType::Session, None::<&gtk4::gio::Cancellable>)
        .ok()
        .and_then(|bus| {
            use gtk4::glib::prelude::ToVariant;
            let args = gtk4::glib::Variant::tuple_from_iter([vec![uri.clone()].to_variant(), "cedro".to_variant()]);
            bus.call_sync(
                Some("org.freedesktop.FileManager1"),
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1",
                "ShowItems",
                Some(&args),
                None,
                gtk4::gio::DBusCallFlags::NONE,
                3000,
                None::<&gtk4::gio::Cancellable>,
            )
            .ok()
        })
        .is_some();
    if !shown {
        if let Some(dir) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }
}

fn close_wheel(state: &Rc<RefCell<Wheel>>) {
    let mut wheel = state.borrow_mut();
    for window in wheel.windows.drain(..) {
        window.close();
    }
    wheel.areas.clear();
    wheel.anchor = None;
    wheel.provisional = false;
    wheel.hover = -1;
}

fn current_slices(wheel: &Wheel) -> (Vec<cedro::Slice>, String) {
    let kind = wheel.launch.paths.first().map(|path| kind_of(path)).unwrap_or(cedro::Kind::Other);
    let name = wheel
        .launch
        .paths
        .first()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string();
    (wheel_list(kind, wheel.launch.mode), clip_name(&name))
}

thread_local! {
    static GLYPHS: RefCell<cedro::ui::gfx::GlyphCache> = RefCell::new(cedro::ui::gfx::GlyphCache::new());
}

/// La ruota nuova: la disegna il pittore di `cedro::ui` (come su Windows) e qui si appoggia sul fotogramma.
fn draw_look(cr: &gtk4::cairo::Context, width: i32, height: i32, monitor: usize, state: &Rc<RefCell<Wheel>>) {
    {
        let mut w = state.borrow_mut();
        let ready = w.thumb.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let (Some(img), Some(ui)) = (ready, w.look.as_mut()) {
            ui.thumb = Some(img);
        }
    }
    let wheel = state.borrow();
    let (Some(anchor), Some(ui)) = (wheel.anchor.as_ref(), wheel.look.as_ref()) else { return };
    if anchor.monitor != monitor {
        return;
    }
    let sf = cr.target().device_scale().0.round().max(1.0);
    let (ox, oy, w, h) = look_frame(ui, anchor.x, anchor.y, width as f64, height as f64);
    let (pw, ph) = ((w * sf).ceil() as i32, (h * sf).ceil() as i32);
    let bgra = GLYPHS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let mut p = cedro::ui::gfx::Painter::new(pw as u32, ph as u32, cedro::ui::ZOOM * sf as f32, cedro::ui::gfx::Fonts::current(), &mut cache);
        ui.draw(&mut p);
        p.bgra()
    });
    if let Ok(surface) = gtk4::cairo::ImageSurface::create_for_data(bgra, gtk4::cairo::Format::ARgb32, pw, ph, pw * 4) {
        surface.set_device_scale(sf, sf);
        let _ = cr.set_source_surface(&surface, ox, oy);
        let _ = cr.paint();
    }
}

fn draw_wheel(cr: &gtk4::cairo::Context, width: i32, height: i32, monitor: usize, state: &Rc<RefCell<Wheel>>) {
    let _ = cr.set_operator(gtk4::cairo::Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    let _ = cr.paint();
    let _ = cr.set_operator(gtk4::cairo::Operator::Over);
    if state.borrow().look.is_some() {
        draw_look(cr, width, height, monitor, state);
        return;
    }
    let wheel = state.borrow();
    let Some(anchor) = wheel.anchor.as_ref() else {
        return;
    };
    if anchor.monitor != monitor {
        return;
    }
    let (list, name) = current_slices(&wheel);
    let (ox, oy) = origin(anchor.x, anchor.y, width as f64, height as f64);
    let cx = ox + 220.0;
    let cy = oy + 220.0;
    let n = list.len().max(1);
    if let Some(surface) = wheel_surface(cr, n, wheel.hover) {
        let _ = cr.set_source_surface(&surface, ox, oy);
        let _ = cr.paint();
    }
    let rgb = |c: (u8, u8, u8)| cr.set_source_rgb(c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0);
    let tema = cedro::tema::attivo();
    let _ = cr.select_font_face(tema.font, FontSlant::Normal, FontWeight::Bold);
    for (index, slice) in list.iter().enumerate() {
        let (a0, a1) = cedro::wheelgeom_bounds(index, n);
        let mid = (a0 + a1) / 2.0;
        let radius = (cedro::wheelgeom::R0 + cedro::wheelgeom::R1) / 2.0;
        let tx = cx as f32 + radius * mid.cos();
        let ty = cy as f32 + radius * mid.sin();
        if index as i32 == wheel.hover {
            rgb(tema.hover_fg);
        } else {
            rgb(tema.slice_fg);
        }
        cr.set_font_size(if n > 6 { 15.0 } else { 18.0 });
        show_centered(cr, tx as f64, ty as f64, slice.label);
    }
    let title = tema.titolo(match wheel.launch.mode {
        Mode::Convert => "Conversione",
        Mode::Tools => "Strumenti",
        Mode::Actions => "Azioni",
    });
    let center = if wheel.hover >= 0 {
        list.get(wheel.hover as usize).map(|slice| slice.label).unwrap_or("Annulla")
    } else {
        "Annulla"
    };
    rgb(tema.hole_title);
    cr.set_font_size(12.0);
    show_centered(cr, cx, cy - 20.0, &title);
    rgb(tema.hole_main);
    cr.set_font_size(17.0);
    show_centered(cr, cx, cy + 2.0, center);
    rgb(tema.hole_sub);
    let _ = cr.select_font_face(tema.font, FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(12.0);
    show_centered(cr, cx, cy + 24.0, &name);
}

thread_local! {
    static SHAPES: RefCell<Option<((usize, i32, u32), gtk4::cairo::ImageSurface)>> = const { RefCell::new(None) };
}

/// Le forme della ruota cambiano solo con il numero di fette, la fetta sotto il mouse e la scala dello schermo.
fn wheel_surface(cr: &gtk4::cairo::Context, n: usize, hover: i32) -> Option<gtk4::cairo::ImageSurface> {
    let scale = cr.target().device_scale().0.round().max(1.0) as u32;
    let key = (n, hover, scale);
    SHAPES.with(|cache| {
        if let Some((k, surface)) = cache.borrow().as_ref() {
            if *k == key {
                return Some(surface.clone());
            }
        }
        let side = 440 * scale as i32;
        let surface = gtk4::cairo::ImageSurface::create_for_data(
            cedro::wheelgfx::render_scaled(n, hover, scale),
            gtk4::cairo::Format::ARgb32,
            side,
            side,
            side * 4,
        )
        .ok()?;
        surface.set_device_scale(scale as f64, scale as f64);
        *cache.borrow_mut() = Some((key, surface.clone()));
        Some(surface)
    })
}

fn show_centered(cr: &gtk4::cairo::Context, x: f64, y: f64, text: &str) {
    let text = &*cedro::i18n::tr(text);
    let Ok(ext) = cr.text_extents(text) else {
        return;
    };
    cr.move_to(x - ext.width() / 2.0 - ext.x_bearing(), y - ext.height() / 2.0 - ext.y_bearing());
    let _ = cr.show_text(text);
}

fn origin(x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
    let ox = (x - 220.0).clamp(0.0, (width - 440.0).max(0.0));
    let oy = (y - 220.0).clamp(0.0, (height - 440.0).max(0.0));
    (ox, oy)
}

fn mode_of(key: Key) -> Option<Mode> {
    let on = cfg();
    match key {
        Key::Shift_L | Key::Shift_R if on.key_shift => Some(Mode::Convert),
        Key::Control_L | Key::Control_R if on.key_ctrl => Some(Mode::Tools),
        Key::Alt_L | Key::Alt_R if on.key_alt => Some(Mode::Actions),
        _ => None,
    }
}

fn clip_name(name: &str) -> String {
    if name.chars().count() > 18 {
        format!("{}…", name.chars().take(16).collect::<String>())
    } else {
        name.to_string()
    }
}

fn notify(app: &Application, text: &str) {
    let note = gtk4::gio::Notification::new("Cedro");
    note.set_body(Some(&cedro::i18n::tr(text)));
    app.send_notification(Some("cedro"), &note);
}

fn copy_text(text: &str) {
    if let Some(display) = gtk4::gdk::Display::default() {
        display.clipboard().set_text(text);
    }
}

fn ask(what: Asked) {
    if let Ok(mut slot) = ASKED.lock() {
        *slot = Some(what);
    }
}

fn spawn_tray(quit: Arc<AtomicBool>) {
    let service = ksni::TrayService::new(CedroTray { quit, pixmap: tray_pixels() });
    service.spawn();
}

struct CedroTray {
    quit: Arc<AtomicBool>,
    pixmap: Vec<u8>,
}

impl ksni::Tray for CedroTray {
    fn id(&self) -> String {
        "dev.cedro.Cedro".into()
    }
    fn title(&self) -> String {
        "Cedro".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![ksni::Icon { width: 32, height: 32, data: self.pixmap.clone() }]
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        // Con «Cedro compatto» un clic sull'icona apre la finestrella.
        if cedro::ui::theme::ThemeId::from_key(&read_settings().ui_theme) == cedro::ui::theme::ThemeId::CedroCompatto {
            ask(Asked::Compact);
        } else {
            ask(Asked::Conversions);
        }
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};
        // Stesso ordine del menu di Windows; le icone sono del tema dell'utente (nomi freedesktop).
        vec![
            StandardItem {
                // Con il tema Bondeno: «Cedro x.y.z · Comune di Bondeno» con lo stemma.
                label: cedro::edizione::titolo(update::CURRENT),
                icon_name: if cedro::edizione::bondeno() { String::new() } else { "dev.cedro.Cedro".into() },
                icon_data: if cedro::edizione::bondeno() { cedro::edizione::STEMMA.to_vec() } else { Vec::new() },
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: cedro::i18n::tr("Conversioni…").into_owned(),
                icon_name: "document-edit".into(),
                activate: Box::new(|_| ask(Asked::Conversions)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: cedro::i18n::tr("Impostazioni…").into_owned(),
                icon_name: "preferences-system".into(),
                activate: Box::new(|_| ask(Asked::Settings)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: cedro::i18n::tr("Guida…").into_owned(),
                icon_name: "help-contents".into(),
                activate: Box::new(|_| ask(Asked::Guide)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: cedro::i18n::tr("Controlla aggiornamenti").into_owned(),
                icon_name: "view-refresh".into(),
                activate: Box::new(|_| check_updates(true)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: cedro::i18n::tr("Avvio automatico").into_owned(),
                checked: autostart_file().is_some_and(|path| path.exists()),
                activate: Box::new(|_| {
                    let on = !autostart_file().is_some_and(|path| path.exists());
                    if let Err(e) = set_autostart(on) {
                        append_log(&format!("avvio automatico: {e}"));
                    }
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: cedro::i18n::tr("Esci").into_owned(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| tray.quit.store(true, Ordering::Relaxed)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Dopo qualche secondo e poi ogni giorno, finché Cedro resta aperto. Rilegge la scelta a ogni giro.
fn daily_updates() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(8));
        loop {
            if cfg().update_check {
                check_updates_now(false);
            }
            std::thread::sleep(Duration::from_secs(24 * 60 * 60));
        }
    });
}

/// Dal menu risponde sempre, anche quando non c'è niente di nuovo.
pub(crate) fn check_updates(manual: bool) {
    std::thread::spawn(move || check_updates_now(manual));
}

fn check_updates_now(manual: bool) {
    let news = match update::check() {
        Ok(Some(release)) if manual || !update::skipped(&release.version) => UpdateNews::Found { release, manual },
        Ok(_) if manual => UpdateNews::Note(format!("Hai già l'ultima versione ({}).", update::CURRENT)),
        Err(e) if manual => UpdateNews::Note(e),
        Err(e) => {
            append_log(&format!("aggiornamenti: {e}"));
            return;
        }
        Ok(_) => return,
    };
    if let Ok(mut slot) = UPDATE.lock() {
        *slot = Some(news);
    }
}

fn on_update(app: &Application, news: UpdateNews) {
    let (release, manual) = match news {
        UpdateNews::Note(text) => return notify(app, &text),
        UpdateNews::Found { release, manual } => (release, manual),
    };
    let dialog = gtk4::AlertDialog::builder()
        .message(cedro::i18n::tr_string(&format!("È disponibile Cedro {}", release.version)))
        .detail(cedro::i18n::tr_string(&format!("Hai la {}. Vuoi aggiornare? Cedro scarica il pacchetto, controlla lo SHA-256, lo installa e si riapre.", update::CURRENT)))
        .buttons([cedro::i18n::tr_string("Non ora"), cedro::i18n::tr_string("Aggiorna")])
        .cancel_button(0)
        .default_button(0)
        .build();
    let app = app.clone();
    dialog.choose(None::<&gtk4::Window>, None::<&gtk4::gio::Cancellable>, move |answer| {
        if answer == Ok(1) {
            // Scaricare e installare porta via secondi: si fa fuori dal thread delle finestre, che intanto
            // resta libero, e l'esito torna qui da un canale.
            notify(&app, &format!("Scarico Cedro {}…", release.version));
            let (tx, rx) = std::sync::mpsc::channel();
            let release = release.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::cli::install(&release));
            });
            let app = app.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(300), move || match rx.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(_) => {
                    notify(&app, "Aggiornamento non riuscito: il lavoro si è interrotto.");
                    glib::ControlFlow::Break
                }
                Ok(Err(e)) => {
                    notify(&app, &format!("Aggiornamento non riuscito: {e}"));
                    glib::ControlFlow::Break
                }
                Ok(Ok(done)) => {
                    // Il programma nuovo è già al suo posto: questo si chiude e il nuovo parte da lì.
                    notify(&app, &format!("{done} Cedro si riapre."));
                    if let Ok(exe) = update::installed_binary() {
                        let _ = std::process::Command::new("sh").arg("-c").arg("sleep 2; exec \"$0\" --background").arg(exe).spawn();
                    }
                    glib::timeout_add_local_once(std::time::Duration::from_millis(800), || std::process::exit(0));
                    glib::ControlFlow::Break
                }
            });
        } else if !manual {
            update::skip(&release.version);
        }
    });
}

fn autostart_file() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("autostart/dev.cedro.Cedro.desktop"))
}

pub fn autostart_enabled() -> bool {
    autostart_file().is_some_and(|path| path.exists())
}

pub fn set_autostart(on: bool) -> std::io::Result<()> {
    let path = autostart_file().ok_or_else(|| std::io::Error::other("HOME non impostata"))?;
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe()?;
    let exe = exe.to_str().ok_or_else(|| std::io::Error::other("percorso non UTF-8"))?;
    write_text(&path, &desktop_file(exe, " --background"), false)
}

fn tray_pixels() -> Vec<u8> {
    let image = image::load_from_memory(MARK)
        .map(|img| img.resize(32, 32, image::imageops::FilterType::Triangle).to_rgba8())
        .ok();
    let mut data = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32 {
        for x in 0..32 {
            let (r, g, b, a) = if let Some(image) = &image {
                let p = image.get_pixel(x, y);
                (p[0], p[1], p[2], p[3])
            } else {
                (35, 40, 28, 255)
            };
            data.extend_from_slice(&[a, r, g, b]);
        }
    }
    data
}

fn install_integrations() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(exe) = exe.to_str() else {
        return;
    };
    let Some(root) = data_home() else {
        return;
    };
    let _ = write_text(&root.join("applications/dev.cedro.Cedro.desktop"), &desktop_file(exe, ""), false);
    refresh_icon(&root, MARK);
    // Chi ha l'avvio automatico dalla 0.11 lo riceve senza finestra all'accesso.
    if autostart_enabled() {
        let _ = set_autostart(true);
    }
    let _ = write_text(&root.join("kio/servicemenus/cedro.desktop"), &dolphin_menu(exe), true);
    let _ = write_text(
        &root.join("nautilus-python/extensions/cedro.py"),
        &nautilus_python(exe),
        false,
    );
    for (name, flag) in [("Conversione", "conversione"), ("Strumenti", "strumenti"), ("Azioni", "azioni")] {
        let _ = write_text(&root.join("nautilus/scripts/Cedro").join(name), &nautilus_script(exe, flag), true);
    }
}

/// Scrive l'icona nel tema dell'utente; se è cambiata (aggiornamento) avvisa KDE, che altrimenti tiene
/// nella barra delle applicazioni quella vecchia finché non si rilegge la cache delle icone.
fn refresh_icon(root: &std::path::Path, bytes: &[u8]) {
    let path = root.join("icons/hicolor/256x256/apps/dev.cedro.Cedro.png");
    if std::fs::read(&path).is_ok_and(|old| old == bytes) {
        return;
    }
    if write_bytes(&path, bytes).is_err() {
        return;
    }
    if let Ok(dir) = std::fs::File::open(root.join("icons/hicolor")) {
        let _ = dir.set_modified(std::time::SystemTime::now());
    }
    for tool in ["kbuildsycoca6", "kbuildsycoca5"] {
        if std::process::Command::new(tool)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
        {
            break;
        }
    }
}

fn data_home() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
}

fn write_text(path: &std::path::Path, body: &str, executable: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)?;
    if executable {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn write_bytes(path: &std::path::Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)
}

fn quote_exec(exe: &str) -> String {
    if exe.contains([' ', '\t', '"']) {
        format!("\"{}\"", exe.replace('"', "\\\""))
    } else {
        exe.to_string()
    }
}

fn desktop_file(exe: &str, args: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Cedro\nComment=Conversione locale\nComment[en]=Local file conversion\nExec={}{args}\nIcon=dev.cedro.Cedro\nTerminal=false\nCategories=Utility;\nStartupNotify=false\nActions=settings;guida;\n\n[Desktop Action settings]\nName=Impostazioni\nName[en]=Settings\nExec={} settings\n\n[Desktop Action guida]\nName=Guida\nName[en]=Help\nExec={} guida\n",
        quote_exec(exe),
        quote_exec(exe),
        quote_exec(exe)
    )
}

fn dolphin_menu(exe: &str) -> String {
    let exe = quote_exec(exe);
    format!(
        "[Desktop Entry]\nType=Service\nMimeType=all/allfiles;\nActions=conversione;strumenti;azioni;\nX-KDE-Submenu=Cedro\nX-KDE-Priority=TopLevel\n\n[Desktop Action conversione]\nName=Conversione\nName[en]=Conversion\nExec={exe} show --conversione %F\n\n[Desktop Action strumenti]\nName=Strumenti\nName[en]=Tools\nExec={exe} show --strumenti %F\n\n[Desktop Action azioni]\nName=Azioni\nName[en]=Actions\nExec={exe} show --azioni %F\n"
    )
}

fn nautilus_python(exe: &str) -> String {
    let exe = exe.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"# Scritto da Cedro a ogni avvio.
import os
import subprocess

EXE = "{exe}"

try:
    from gi.repository import GObject, Nautilus
except Exception:
    GObject = None
    Nautilus = None

if Nautilus is not None:
    class Cedro(GObject.Object, Nautilus.MenuProvider):
        def get_file_items(self, *args):
            files = args[-1] if args else []
            paths = []
            for item in files:
                location = item.get_location()
                path = location.get_path() if location is not None else None
                if path and os.path.isfile(path):
                    paths.append(path)
            if not paths:
                return []
            root = Nautilus.MenuItem(name="Cedro::root", label="Cedro")
            menu = Nautilus.Menu()
            root.set_submenu(menu)
            for mode, label in (("conversione", "Conversione"), ("strumenti", "Strumenti"), ("azioni", "Azioni")):
                entry = Nautilus.MenuItem(name="Cedro::" + mode, label=label)
                entry.connect("activate", self.open_mode, mode, list(paths))
                menu.append_item(entry)
            return [root]

        def open_mode(self, _item, mode, paths):
            subprocess.Popen([EXE, "show", "--" + mode, *paths])
"#
    )
}

fn nautilus_script(exe: &str, mode: &str) -> String {
    format!("#!/bin/bash\nmapfile -t paths <<< \"$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS\"\nexec {exe} show --{mode} \"${{paths[@]}}\"\n", exe = shell_quote(exe))
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
