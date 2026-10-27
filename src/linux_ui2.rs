//! La finestra GTK della nuova interfaccia (edizione standard, Wayland con Plasma e GNOME). Come
//! quella di Windows fa solo il lavoro del sistema: la barra del titolo è disegnata da `cedro::ui`,
//! qui si copia l'immagine, si passano mouse, tastiera e file trascinati e si offrono a `Screen` i
//! dialoghi e gli appunti. Tutto quello che si vede sta in `cedro::ui`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cedro::ui::screen::{Cursor, Key, Platform, Screen, Surface};
use gtk4::gdk::{self, ModifierType};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, DrawingArea};

const TITLE: &str = "Cedro";
const EDGE: f64 = 7.0;

/// Apre il pannello del tema scelto (o porta davanti quello già aperto).
pub fn open(app: &Application) {
    open_surface(app, Surface::Main);
}

/// File arrivati dalla ruota e il formato da preselezionare: li prende la finestra principale al suo prossimo giro.
static PENDING: std::sync::Mutex<(Vec<PathBuf>, Option<&'static str>)> = std::sync::Mutex::new((Vec::new(), None));

/// Apre il pannello con questi file in coda e, se c'è, il formato già scelto (dalla ruota).
pub fn open_with(app: &Application, paths: Vec<PathBuf>, action: Option<&'static str>) {
    {
        let mut slot = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        slot.0.extend(paths);
        slot.1 = action;
    }
    open_surface(app, Surface::Main);
}

pub fn open_compact(app: &Application) {
    open_surface(app, Surface::Compact);
}

fn title_for(surface: Surface) -> &'static str {
    match surface {
        Surface::Main => TITLE,
        Surface::Compact => "Cedro compatto",
    }
}

struct Plat {
    window: ApplicationWindow,
    app: Application,
    surface: Surface,
}

/// I dialoghi di GTK sono asincroni: si fa girare il ciclo principale finché rispondono.
fn wait<T: 'static>(start: impl FnOnce(Box<dyn Fn(T)>)) -> Option<T> {
    let slot: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let done = Rc::new(Cell::new(false));
    let (s, d) = (slot.clone(), done.clone());
    start(Box::new(move |v| {
        *s.borrow_mut() = Some(v);
        d.set(true);
    }));
    let ctx = glib::MainContext::default();
    while !done.get() {
        ctx.iteration(true);
    }
    slot.take()
}

impl Platform for Plat {
    fn pick_files(&mut self) -> Vec<PathBuf> {
        let dialog = gtk4::FileDialog::builder().title(cedro::i18n::tr_string("Scegli i file")).modal(true).build();
        let win = self.window.clone();
        let res = wait(move |done| {
            dialog.open_multiple(Some(&win), gio::Cancellable::NONE, move |r| {
                let files: Vec<PathBuf> = r
                    .ok()
                    .map(|list| (0..list.n_items()).filter_map(|i| list.item(i).and_downcast::<gio::File>()).filter_map(|f| f.path()).collect())
                    .unwrap_or_default();
                done(files);
            });
        });
        res.unwrap_or_default()
    }

    fn pick_folder(&mut self, title: &str) -> Option<PathBuf> {
        let dialog = gtk4::FileDialog::builder().title(title).modal(true).build();
        let win = self.window.clone();
        wait(move |done| {
            dialog.select_folder(Some(&win), gio::Cancellable::NONE, move |r| done(r.ok().and_then(|f| f.path())));
        })
        .flatten()
    }

    fn open_path(&mut self, path: &Path) {
        // Il file si apre con il programma predefinito; per le cartelle vale lo stesso.
        let uri = gio::File::for_path(path).uri();
        let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
    }

    fn copy_text(&mut self, text: &str) {
        self.window.clipboard().set_text(text);
    }

    fn paste_text(&mut self) -> Option<String> {
        let clip = self.window.clipboard();
        wait(move |done| {
            clip.read_text_async(gio::Cancellable::NONE, move |r| done(r.ok().flatten().map(|s| s.to_string())));
        })
        .flatten()
    }

    fn open_guide(&mut self) {
        crate::linux_settings::open_guide(&self.app);
    }

    fn open_settings(&mut self) {
        crate::linux_ui::open_settings_window(&self.app);
    }

    fn minimize(&mut self) {
        self.window.minimize();
    }

    fn toggle_maximize(&mut self) {
        if self.window.is_maximized() {
            self.window.unmaximize();
        } else {
            self.window.maximize();
        }
    }

    fn close(&mut self) {
        self.window.close();
    }

    fn open_full(&mut self) {
        let app = self.app.clone();
        let win = self.window.clone();
        glib::idle_add_local_once(move || {
            win.close();
            open(&app);
        });
        let _ = self.surface;
    }
}

struct Host {
    screen: RefCell<Screen>,
    /// L'ultimo fotogramma, per ridisegnare mentre un dialogo è aperto.
    frame: RefCell<Option<(gtk4::cairo::ImageSurface, i32, i32)>>,
    pointer: Cell<(f32, f32)>,
}

fn scale_of(host: &Host, area: &DrawingArea) -> f32 {
    let sf = area.scale_factor().max(1) as f32;
    let base = sf * cedro::ui::ZOOM;
    match host.screen.try_borrow() {
        Ok(s) => s.fit_scale(base, area.width() as f32 * sf, area.height() as f32 * sf),
        Err(_) => base,
    }
}

fn open_surface(app: &Application, surface: Surface) {
    let title = title_for(surface);
    for window in app.windows() {
        if window.title().is_some_and(|t| t == title) {
            window.present();
            return;
        }
    }
    let woke = Arc::new(AtomicBool::new(false));
    let w2 = woke.clone();
    let screen = Screen::new(surface, Arc::new(move || w2.store(true, Ordering::Relaxed)));
    let (pw, ph) = screen.preferred_size();
    let (mw, mh) = screen.min_size();
    let host = Rc::new(Host { screen: RefCell::new(screen), frame: RefCell::new(None), pointer: Cell::new((0.0, 0.0)) });

    let window = ApplicationWindow::builder().application(app).title(title).decorated(false).resizable(true).build();
    let z = cedro::ui::ZOOM;
    window.set_default_size((pw * z) as i32, (ph * z) as i32);
    let area = DrawingArea::new();
    area.set_focusable(true);
    area.set_hexpand(true);
    area.set_vexpand(true);
    // Si può stringere molto: sotto la misura minima il disegno si rimpicciolisce.
    area.set_content_width((mw * z * 0.5) as i32);
    area.set_content_height((mh * z * 0.5) as i32);
    window.set_child(Some(&area));
    {
        // Non si chiude mentre lavora: il file in scrittura resterebbe a metà.
        let host = host.clone();
        window.connect_close_request(move |w| {
            let busy = host.screen.try_borrow().map(|s| s.app.busy()).unwrap_or(false);
            if busy {
                gtk4::AlertDialog::builder().message(cedro::i18n::tr_string("Una conversione è ancora in corso: aspetta che finisca.")).modal(true).build().show(Some(w));
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }

    let (h0, a0) = (host.clone(), area.clone());
    let plat = Rc::new(RefCell::new(Plat { window: window.clone(), app: app.clone(), surface }));

    // Disegno.
    {
        let host = host.clone();
        area.set_draw_func(move |a, cr, w, h| {
            let sf = a.scale_factor().max(1);
            let (pw, ph) = (w * sf, h * sf);
            if pw <= 0 || ph <= 0 {
                return;
            }
            let fit = scale_of(&host, a);
            if let Ok(mut screen) = host.screen.try_borrow_mut() {
                let painter = screen.render(pw as u32, ph as u32, fit);
                let data = painter.bgra();
                if let Ok(img) = gtk4::cairo::ImageSurface::create_for_data(data, gtk4::cairo::Format::ARgb32, pw, ph, pw * 4) {
                    *host.frame.borrow_mut() = Some((img, pw, ph));
                }
            }
            if let Some((img, fw, fh)) = host.frame.borrow().as_ref() {
                if *fw == pw && *fh == ph {
                    cr.scale(1.0 / sf as f64, 1.0 / sf as f64);
                    let _ = cr.set_source_surface(img, 0.0, 0.0);
                    let _ = cr.paint();
                }
            }
        });
    }

    // Mouse.
    let motion = gtk4::EventControllerMotion::new();
    {
        let (host, area2) = (h0.clone(), a0.clone());
        motion.connect_motion(move |_, x, y| {
            let s = scale_of(&host, &area2) / area2.scale_factor().max(1) as f32;
            let (lx, ly) = (x as f32 / s, y as f32 / s);
            host.pointer.set((lx, ly));
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                if sc.pointer_move(lx, ly) {
                    area2.queue_draw();
                }
                let name = match sc.cursor_at(lx, ly) {
                    Cursor::Arrow => "default",
                    Cursor::Hand => "pointer",
                    Cursor::Text => "text",
                };
                area2.set_cursor_from_name(Some(name));
            }
        });
        let (host, area2) = (h0.clone(), a0.clone());
        motion.connect_leave(move |_| {
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                if sc.pointer_leave() {
                    area2.queue_draw();
                }
            }
        });
    }
    area.add_controller(motion);

    let click = gtk4::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    {
        let (host, area2, win, p) = (h0.clone(), a0.clone(), window.clone(), plat.clone());
        click.connect_pressed(move |g, n, x, y| {
            area2.grab_focus();
            let s = scale_of(&host, &area2) / area2.scale_factor().max(1) as f32;
            let (lx, ly) = (x as f32 / s, y as f32 / s);
            // Bordi per ridimensionare, barra del titolo per spostare: le fa il sistema.
            if !win.is_maximized() {
                if let Some(edge) = edge_at(x, y, area2.width() as f64, area2.height() as f64) {
                    if let (Some(surface), Some(dev)) = (win.surface(), g.current_event_device()) {
                        if let Ok(top) = surface.downcast::<gdk::Toplevel>() {
                            top.begin_resize(edge, Some(&dev), 1, x, y, g.current_event_time());
                            g.set_state(gtk4::EventSequenceState::Claimed);
                            return;
                        }
                    }
                }
            }
            let on_caption = host.screen.try_borrow().map(|sc| sc.caption_at(lx, ly)).unwrap_or(false);
            if on_caption {
                if n >= 2 {
                    let mut pl = p.borrow_mut();
                    pl.toggle_maximize();
                } else if let (Some(surface), Some(dev)) = (win.surface(), g.current_event_device()) {
                    if let Ok(top) = surface.downcast::<gdk::Toplevel>() {
                        top.begin_move(&dev, 1, x, y, g.current_event_time());
                        g.set_state(gtk4::EventSequenceState::Claimed);
                    }
                }
                return;
            }
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                sc.pointer_down(lx, ly);
            }
            area2.queue_draw();
        });
        let (host, area2, p) = (h0.clone(), a0.clone(), plat.clone());
        click.connect_released(move |_, _, x, y| {
            let s = scale_of(&host, &area2) / area2.scale_factor().max(1) as f32;
            let (lx, ly) = (x as f32 / s, y as f32 / s);
            // Il dialogo dei file gira dentro `pointer_up`: la finestra non deve essere già presa in prestito.
            let mut pl = p.borrow_mut();
            let mut sc = match host.screen.try_borrow_mut() {
                Ok(s) => s,
                Err(_) => return,
            };
            sc.pointer_up(lx, ly, &mut *pl);
            drop(sc);
            area2.queue_draw();
        });
    }
    area.add_controller(click);

    let rclick = gtk4::GestureClick::new();
    rclick.set_button(gdk::BUTTON_SECONDARY);
    {
        let (host, area2) = (h0.clone(), a0.clone());
        rclick.connect_released(move |_, _, x, y| {
            area2.grab_focus();
            let s = scale_of(&host, &area2) / area2.scale_factor().max(1) as f32;
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                sc.context_click(x as f32 / s, y as f32 / s);
            }
            area2.queue_draw();
        });
    }
    area.add_controller(rclick);

    let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    {
        let (host, area2) = (h0.clone(), a0.clone());
        scroll.connect_scroll(move |_, _, dy| {
            let (x, y) = host.pointer.get();
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                if sc.wheel(x, y, dy as f32 * 60.0) {
                    area2.queue_draw();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
    }
    area.add_controller(scroll);

    // Tastiera: i comandi qui, il testo dal metodo di scrittura (accenti, tasti morti).
    let keys = gtk4::EventControllerKey::new();
    let im = gtk4::IMMulticontext::new();
    im.set_client_widget(Some(&area));
    keys.set_im_context(Some(&im));
    {
        let (host, area2) = (h0.clone(), a0.clone());
        im.connect_commit(move |_, s| {
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                sc.text_input(s);
            }
            area2.queue_draw();
        });
        let (host, area2, p) = (h0.clone(), a0.clone(), plat.clone());
        keys.connect_key_pressed(move |_, key, _, state| {
            let ctrl = state.contains(ModifierType::CONTROL_MASK);
            let shift = state.contains(ModifierType::SHIFT_MASK);
            let k = match key {
                gdk::Key::Tab | gdk::Key::ISO_Left_Tab => Some(if shift { Key::ShiftTab } else { Key::Tab }),
                gdk::Key::Return | gdk::Key::KP_Enter => Some(Key::Enter),
                gdk::Key::space if !ctrl => Some(Key::Space),
                gdk::Key::Escape => Some(Key::Escape),
                gdk::Key::BackSpace => Some(Key::Backspace),
                gdk::Key::Delete => Some(Key::Delete),
                gdk::Key::Left => Some(Key::Left),
                gdk::Key::Right => Some(Key::Right),
                gdk::Key::Home => Some(Key::Home),
                gdk::Key::End => Some(Key::End),
                gdk::Key::Up => Some(Key::Up),
                gdk::Key::Down => Some(Key::Down),
                gdk::Key::v | gdk::Key::V if ctrl => Some(Key::Paste),
                gdk::Key::c | gdk::Key::C if ctrl => Some(Key::Copy),
                gdk::Key::a | gdk::Key::A if ctrl => Some(Key::SelectAll),
                _ => None,
            };
            let Some(k) = k else { return glib::Propagation::Proceed };
            let mut pl = p.borrow_mut();
            let Ok(mut sc) = host.screen.try_borrow_mut() else { return glib::Propagation::Proceed };
            let used = if k == Key::Enter && sc.ui.menu.is_some() { sc.menu_enter(&mut *pl) } else { sc.key(k, &mut *pl) };
            drop(sc);
            area2.queue_draw();
            if used { glib::Propagation::Stop } else { glib::Propagation::Proceed }
        });
    }
    area.add_controller(keys);
    let focus = gtk4::EventControllerFocus::new();
    {
        let im1 = im.clone();
        focus.connect_enter(move |_| im1.focus_in());
        let im2 = im.clone();
        focus.connect_leave(move |_| im2.focus_out());
    }
    area.add_controller(focus);

    // File trascinati dal gestore di file.
    let drop = gtk4::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    {
        let (host, area2) = (h0.clone(), a0.clone());
        drop.connect_enter(move |_, _, _| {
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                sc.drag_over(true);
            }
            area2.queue_draw();
            gdk::DragAction::COPY
        });
        let (host, area2) = (h0.clone(), a0.clone());
        drop.connect_leave(move |_| {
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                sc.drag_over(false);
            }
            area2.queue_draw();
        });
        let (host, area2) = (h0.clone(), a0.clone());
        drop.connect_drop(move |_, value, _, _| {
            let paths: Vec<PathBuf> = value.get::<gdk::FileList>().map(|l| l.files().iter().filter_map(|f| f.path()).collect()).unwrap_or_default();
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                sc.drop_files(paths);
            }
            area2.queue_draw();
            true
        });
    }
    area.add_controller(drop);

    // Il ritmo: eventi dei lavori, animazioni, ritorno dalle Impostazioni.
    {
        let (host, area2, woke) = (h0.clone(), a0.clone(), woke.clone());
        let win = window.clone();
        let was_active = Cell::new(true);
        glib::timeout_add_local(Duration::from_millis(40), move || {
            if !win.is_visible() && !win.is_active() && win.surface().is_none() {
                return glib::ControlFlow::Break;
            }
            let active = win.is_active();
            let mut redraw = false;
            if surface == Surface::Main {
                let (paths, action) = {
                    let mut slot = PENDING.lock().unwrap_or_else(|e| e.into_inner());
                    (std::mem::take(&mut slot.0), slot.1.take())
                };
                if !paths.is_empty() {
                    if let Ok(mut sc) = host.screen.try_borrow_mut() {
                        sc.drop_files(paths);
                        if let Some(id) = action {
                            sc.app.set_action(id);
                        }
                        redraw = true;
                    }
                }
            }
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                if woke.swap(false, Ordering::Relaxed) {
                    sc.pump();
                    redraw = true;
                }
                if active && !was_active.get() {
                    sc.reload_appearance();
                    redraw = true;
                }
                if sc.animating() {
                    redraw = true;
                }
            }
            was_active.set(active);
            if redraw {
                area2.queue_draw();
            }
            glib::ControlFlow::Continue
        });
    }
    {
        let host = h0.clone();
        let area2 = a0.clone();
        window.connect_maximized_notify(move |w| {
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                sc.ui.maximized = w.is_maximized();
            }
            area2.queue_draw();
        });
    }
    if surface == Surface::Compact {
        window.set_resizable(true);
    }
    window.present();
}

/// Da che bordo si ridimensiona, se il punto è vicino al margine della finestra.
fn edge_at(x: f64, y: f64, w: f64, h: f64) -> Option<gdk::SurfaceEdge> {
    let (l, r, t, b) = (x < EDGE, x > w - EDGE, y < EDGE, y > h - EDGE);
    Some(match (l, r, t, b) {
        (true, _, true, _) => gdk::SurfaceEdge::NorthWest,
        (_, true, true, _) => gdk::SurfaceEdge::NorthEast,
        (true, _, _, true) => gdk::SurfaceEdge::SouthWest,
        (_, true, _, true) => gdk::SurfaceEdge::SouthEast,
        (true, ..) => gdk::SurfaceEdge::West,
        (_, true, ..) => gdk::SurfaceEdge::East,
        (_, _, true, _) => gdk::SurfaceEdge::North,
        (_, _, _, true) => gdk::SurfaceEdge::South,
        _ => return None,
    })
}
