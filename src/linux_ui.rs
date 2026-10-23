//! Ruota su GNOME e Plasma. Wayland non lascia prendere il clic destro di un altro programma,
//! quindi la ruota si apre dal menu del file. Shift, Ctrl e Alt, mentre è aperta, cambiano fetta.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cedro::{append_log, kind_of, perform, read_quality, read_settings, same_kind_paths, slices, update, Mode, Settings};
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

const MARK: &[u8] = include_bytes!("../docs/mark.png");

static UPDATE: Mutex<Option<UpdateNews>> = Mutex::new(None);
static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);
/// Cosa ha chiesto la tray: la legge il ciclo principale, perché le finestre vanno aperte da lì.
static ASKED: Mutex<Option<Asked>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq)]
enum Asked {
    Settings,
    Guide,
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
    }));
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
    app.connect_activate(move |app| {
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
            // Avviato a mano, senza file: si vede subito che c'è, anche senza vassoio.
            Open::Nothing if opened.borrow().launch.stay && !background => open_settings_window(app),
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
                None => {}
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

fn open_settings_window(app: &Application) {
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
        click.connect_released(move |_, _, x, y| choose(&click_app, &click_state, monitor_index, x, y));
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
                key_state.borrow_mut().launch.mode = mode;
                for area in &key_state.borrow().areas {
                    area.queue_draw();
                }
            }
            let _ = key_app;
            glib::Propagation::Proceed
        });
        window.add_controller(key);
        window.fullscreen_on_monitor(&monitor);
        window.present();
        state.borrow_mut().screens.push(Screen { width: geo.width() as f64, height: geo.height() as f64 });
        state.borrow_mut().areas.push(area);
        state.borrow_mut().windows.push(window);
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

fn point_at(state: &Rc<RefCell<Wheel>>, monitor: usize, x: f64, y: f64) {
    let mut wheel = state.borrow_mut();
    // L'ancora si fissa al primo ingresso del puntatore: se seguisse il mouse, il centro
    // starebbe sempre sotto il cursore e nessuna fetta potrebbe essere scelta.
    if wheel.anchor.is_none() || wheel.provisional {
        wheel.anchor = Some(Anchor { x, y, monitor });
        wheel.provisional = false;
    }
    wheel.hover = hover_of(&wheel, monitor, x, y);
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
    let (ox, oy) = origin(anchor.x, anchor.y, screen.width, screen.height);
    let n = current_slices(wheel).0.len();
    cedro::wheelgeom_hover(ox as f32 + 220.0, oy as f32 + 220.0, x as f32, y as f32, n)
}

fn choose(app: &Application, state: &Rc<RefCell<Wheel>>, monitor: usize, x: f64, y: f64) {
    point_at(state, monitor, x, y);
    let job = {
        let wheel = state.borrow();
        let hover = wheel.hover;
        let mode = wheel.launch.mode;
        let paths = wheel.launch.paths.clone();
        let stay = wheel.launch.stay;
        (hover, mode, paths, stay)
    };
    let (hover, mode, paths, stay) = job;
    if paths.is_empty() || hover < 0 {
        close_wheel(state);
        return;
    }
    let kind = kind_of(&paths[0]);
    let list = slices(kind, mode);
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
        let note = run_slices(&paths, &id, &label);
        if let Ok(mut done) = slot.lock() {
            *done = Some(note);
        }
    });
}

fn run_slices(all: &[PathBuf], id: &str, label: &str) -> String {
    let paths = if matches!(id, "dup" | "zip") { all.to_vec() } else { same_kind_paths(all) };
    let skipped = all.len().saturating_sub(paths.len());
    let quality = read_quality();
    let mut notes = Vec::new();
    for path in &paths {
        match perform(path, id, quality) {
            Ok(out) => {
                let name = out.path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                notes.push(format!("{name} · {}", out.message));
            }
            Err(err) => notes.push(err.0),
        }
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
    summary
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
    (slices(kind, wheel.launch.mode).to_vec(), clip_name(&name))
}

fn draw_wheel(cr: &gtk4::cairo::Context, width: i32, height: i32, monitor: usize, state: &Rc<RefCell<Wheel>>) {
    let _ = cr.set_operator(gtk4::cairo::Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    let _ = cr.paint();
    let _ = cr.set_operator(gtk4::cairo::Operator::Over);
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
    let _ = cr.select_font_face("Titillium Web", FontSlant::Normal, FontWeight::Bold);
    for (index, slice) in list.iter().enumerate() {
        let (a0, a1) = cedro::wheelgeom_bounds(index, n);
        let mid = (a0 + a1) / 2.0;
        let radius = (cedro::wheelgeom::R0 + cedro::wheelgeom::R1) / 2.0;
        let tx = cx as f32 + radius * mid.cos();
        let ty = cy as f32 + radius * mid.sin();
        if index as i32 == wheel.hover {
            rgb(cedro::wheelgfx::TEXT_ON_HOVER);
        } else {
            rgb(cedro::wheelgfx::TEXT);
        }
        cr.set_font_size(if n > 6 { 15.0 } else { 18.0 });
        show_centered(cr, tx as f64, ty as f64, slice.label);
    }
    let title = match wheel.launch.mode {
        Mode::Convert => "CONVERSIONE",
        Mode::Tools => "STRUMENTI",
        Mode::Actions => "AZIONI",
    };
    let center = if wheel.hover >= 0 {
        list.get(wheel.hover as usize).map(|slice| slice.label).unwrap_or("Annulla")
    } else {
        "Annulla"
    };
    rgb(cedro::wheelgfx::MUTED);
    cr.set_font_size(12.0);
    show_centered(cr, cx, cy - 20.0, title);
    rgb(cedro::wheelgfx::NAVY);
    cr.set_font_size(17.0);
    show_centered(cr, cx, cy + 2.0, center);
    rgb(cedro::wheelgfx::MUTED);
    let _ = cr.select_font_face("Titillium Web", FontSlant::Normal, FontWeight::Normal);
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
    note.set_body(Some(text));
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
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};
        vec![
            StandardItem {
                label: format!("Cedro {}", update::CURRENT),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Impostazioni…".into(),
                activate: Box::new(|_| ask(Asked::Settings)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Guida…".into(),
                activate: Box::new(|_| ask(Asked::Guide)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Controlla aggiornamenti".into(),
                activate: Box::new(|_| check_updates(true)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Avvio automatico".into(),
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
            StandardItem {
                label: "Esci".into(),
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
fn check_updates(manual: bool) {
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
        .message(format!("È disponibile Cedro {}", release.version))
        .detail(format!("Hai la {}. Vuoi aggiornare? Si apre la pagina della release.", update::CURRENT))
        .buttons(["Non ora", "Aggiorna"])
        .cancel_button(0)
        .default_button(0)
        .build();
    let app = app.clone();
    dialog.choose(None::<&gtk4::Window>, None::<&gtk4::gio::Cancellable>, move |answer| {
        if answer == Ok(1) {
            if let Err(e) = crate::cli::install(&release) {
                notify(&app, &format!("Aggiornamento non riuscito: {e}"));
            }
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
    let _ = write_bytes(
        &root.join("icons/hicolor/256x256/apps/dev.cedro.Cedro.png"),
        MARK,
    );
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
        "[Desktop Entry]\nType=Application\nName=Cedro\nComment=Conversione locale\nExec={}{args}\nIcon=dev.cedro.Cedro\nTerminal=false\nCategories=Utility;\nStartupNotify=false\nActions=settings;guida;\n\n[Desktop Action settings]\nName=Impostazioni\nExec={} settings\n\n[Desktop Action guida]\nName=Guida\nExec={} guida\n",
        quote_exec(exe),
        quote_exec(exe),
        quote_exec(exe)
    )
}

fn dolphin_menu(exe: &str) -> String {
    let exe = quote_exec(exe);
    format!(
        "[Desktop Entry]\nType=Service\nMimeType=all/allfiles;\nActions=conversione;strumenti;azioni;\nX-KDE-Submenu=Cedro\nX-KDE-Priority=TopLevel\n\n[Desktop Action conversione]\nName=Conversione\nExec={exe} show --conversione %F\n\n[Desktop Action strumenti]\nName=Strumenti\nExec={exe} show --strumenti %F\n\n[Desktop Action azioni]\nName=Azioni\nExec={exe} show --azioni %F\n"
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
