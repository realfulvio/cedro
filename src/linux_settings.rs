//! Finestre Impostazioni e Guida su GNOME e Plasma, con i colori del Comune di Bondeno.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use cedro::wheelgeom::{slice_bounds, R0, R1, WHEEL};
use cedro::wheelgfx;
use cedro::{append_log, read_settings, write_settings, Settings};
use gtk4::cairo::{Context, FontSlant, FontWeight, ImageSurface};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, Box as HBox, Button, CheckButton, CssProvider, DrawingArea, Frame, HeaderBar, Label, Orientation,
    Scale, Window, STYLE_PROVIDER_PRIORITY_APPLICATION,
};

const POWERED_BY: &str = "Powered by Luca \"Realhcok\" A.";

const CSS: &str = "\
headerbar.cedro-head { background: #005E8E; color: #ffffff; box-shadow: none; }\
headerbar.cedro-head .title, headerbar.cedro-head button, headerbar.cedro-head label { color: #ffffff; }\
window.cedro-panel { background: #ffffff; color: #191919; }\
window.cedro-panel label { color: #191919; }\
window.cedro-panel label.cedro-muted { color: #455A64; }\
window.cedro-panel button.suggested-action { background: #005E8E; color: #ffffff; }\
window.cedro-panel button.suggested-action label { color: #ffffff; }\
window.cedro-panel frame > label { color: #005E8E; font-weight: bold; }";

fn style() {
    let Some(display) = gtk4::gdk::Display::default() else {
        return;
    };
    let provider = CssProvider::new();
    provider.load_from_string(CSS);
    gtk4::style_context_add_provider_for_display(&display, &provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
}

pub(crate) fn titled(app: &Application, title: &str, width: i32, height: i32) -> Window {
    style();
    let window = Window::builder()
        .application(app)
        .title(title)
        .default_width(width)
        .default_height(height)
        .resizable(false)
        .build();
    window.add_css_class("cedro-panel");
    let bar = HeaderBar::new();
    bar.add_css_class("cedro-head");
    window.set_titlebar(Some(&bar));
    window
}

fn section(title: &str, rows: &[&gtk4::Widget]) -> Frame {
    let frame = Frame::new(Some(title));
    let column = HBox::new(Orientation::Vertical, 6);
    column.set_margin_top(8);
    column.set_margin_bottom(10);
    column.set_margin_start(12);
    column.set_margin_end(12);
    for row in rows {
        column.append(*row);
    }
    frame.set_child(Some(&column));
    frame
}

/// Apre le Impostazioni, o porta avanti quelle già aperte.
pub fn open_settings(app: &Application, on_saved: impl Fn(&Settings) + 'static) {
    for window in app.windows() {
        if window.title().is_some_and(|t| t == "Impostazioni di Cedro") {
            window.present();
            return;
        }
    }
    let current = read_settings();
    let window = titled(app, "Impostazioni di Cedro", 440, 520);
    let autostart = CheckButton::with_label("Avvia Cedro con il computer");
    autostart.set_active(crate::linux_ui::autostart_enabled());
    let notify = CheckButton::with_label("Avviso a fine conversione");
    notify.set_active(current.notify);
    let progress = CheckButton::with_label("Avanzamento mentre converte");
    progress.set_active(current.progress);
    let updates = CheckButton::with_label("Cerca aggiornamenti (all'avvio e ogni giorno)");
    updates.set_active(current.update_check);
    let shift = CheckButton::with_label("Shift: conversioni");
    shift.set_active(current.key_shift);
    let ctrl = CheckButton::with_label("Ctrl: strumenti");
    ctrl.set_active(current.key_ctrl);
    let alt = CheckButton::with_label("Alt: azioni (duplica, percorso, ZIP)");
    alt.set_active(current.key_alt);
    let hint = Label::new(Some("JPEG, WebP, MP3 e PDF in JPG. Più alta, file più grandi."));
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    hint.add_css_class("cedro-muted");
    let scale = Scale::with_range(Orientation::Horizontal, 40.0, 95.0, 1.0);
    scale.set_value(current.quality as f64);
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk4::PositionType::Right);

    let body = HBox::new(Orientation::Vertical, 12);
    body.set_margin_top(14);
    body.set_margin_bottom(14);
    body.set_margin_start(14);
    body.set_margin_end(14);
    body.append(&section("Generale", &[autostart.upcast_ref(), notify.upcast_ref(), progress.upcast_ref(), updates.upcast_ref()]));
    body.append(&section("Tasti che aprono la ruota", &[shift.upcast_ref(), ctrl.upcast_ref(), alt.upcast_ref()]));
    body.append(&section("Qualità", &[hint.upcast_ref(), scale.upcast_ref()]));

    let credit = Label::new(Some(POWERED_BY));
    credit.set_xalign(0.0);
    credit.set_hexpand(true);
    credit.add_css_class("cedro-muted");
    let guide = Button::with_label("Guida");
    let cancel = Button::with_label("Annulla");
    let save = Button::with_label("Salva");
    save.add_css_class("suggested-action");
    let buttons = HBox::new(Orientation::Horizontal, 8);
    buttons.append(&credit);
    buttons.append(&guide);
    buttons.append(&cancel);
    buttons.append(&save);
    body.append(&buttons);
    window.set_child(Some(&body));
    window.set_default_widget(Some(&save));

    {
        let app = app.clone();
        guide.connect_clicked(move |_| open_guide(&app));
    }
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        save.connect_clicked(move |_| {
            let settings = Settings {
                quality: scale.value().round().clamp(40.0, 95.0) as u8,
                notify: notify.is_active(),
                progress: progress.is_active(),
                update_check: updates.is_active(),
                key_shift: shift.is_active(),
                key_ctrl: ctrl.is_active(),
                key_alt: alt.is_active(),
                // Le opzioni che questa finestra non mostra restano come sono.
                ..cedro::read_settings()
            };
            if let Err(e) = write_settings(&settings) {
                append_log(&format!("impostazioni non salvate: {e}"));
                return;
            }
            if autostart.is_active() != crate::linux_ui::autostart_enabled() {
                if let Err(e) = crate::linux_ui::set_autostart(autostart.is_active()) {
                    append_log(&format!("avvio automatico: {e}"));
                }
            }
            on_saved(&settings);
            window.close();
        });
    }
    window.present();
}

struct Demo {
    key: &'static str,
    title: &'static str,
    detail: &'static str,
    caption: &'static str,
    pick: usize,
    labels: &'static [&'static str],
}

const DEMOS: [Demo; 3] = [
    Demo { key: "Shift", title: "Converti", detail: "Cambia formato: JPEG, PDF, MP4…", caption: "CONVERSIONE", pick: 2, labels: &["PDF", "PNG", "JPEG", "WebP", "BMP", "GIF", "TIFF"] },
    Demo { key: "Ctrl", title: "Modifica", detail: "Comprimi, ruota, OCR, metadati…", caption: "STRUMENTI", pick: 5, labels: &["Comprimi", "Scala", "B/N", "Ruota", "Metadati", "OCR", "MAIUSC."] },
    Demo { key: "Alt", title: "Azioni sul file", detail: "Duplica, copia il percorso, ZIP", caption: "AZIONI", pick: 1, labels: &["Duplica", "Percorso", "ZIP"] },
];

const CYCLE_MS: u128 = 4800;
const SHOWN: f64 = 330.0;

fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    let t = ease(t);
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// Opacità della ruota, fetta accesa e posizione del cursore al millisecondo `t` del ciclo.
fn scene(mode: usize, t: f64, size: (f64, f64)) -> (f64, bool, (f64, f64)) {
    let center = (size.0 / 2.0, size.1 / 2.0);
    let rest = (size.0 - 50.0, size.1 - 40.0);
    let on_file = (center.0 + 28.0, center.1 + 28.0);
    let demo = &DEMOS[mode];
    let k = SHOWN / WHEEL as f64;
    let (a0, a1) = slice_bounds(demo.pick, demo.labels.len());
    let mid = ((a0 + a1) / 2.0) as f64;
    let r = ((R0 + R1) / 2.0) as f64 * k;
    let target = (center.0 + r * mid.cos(), center.1 + r * mid.sin());
    let alpha = if t < 1100.0 {
        0.0
    } else if t < 1450.0 {
        (t - 1100.0) / 350.0
    } else if t < 3800.0 {
        1.0
    } else if t < 4150.0 {
        1.0 - (t - 3800.0) / 350.0
    } else {
        0.0
    };
    let cursor = if t < 1000.0 {
        lerp(rest, on_file, t / 1000.0)
    } else if t < 1900.0 {
        on_file
    } else if t < 2500.0 {
        lerp(on_file, target, (t - 1900.0) / 600.0)
    } else if t < 3800.0 {
        target
    } else {
        lerp(target, rest, (t - 3800.0) / 700.0)
    };
    (alpha, (2500.0..3800.0).contains(&t), cursor)
}

fn rgb(cr: &Context, c: (u8, u8, u8), alpha: f64) {
    cr.set_source_rgba(c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0, alpha);
}

fn rounded(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
    cr.close_path();
}

fn text(cr: &Context, size: f64, bold: bool, x: f64, y: f64, s: &str) {
    let weight = if bold { FontWeight::Bold } else { FontWeight::Normal };
    cr.select_font_face("Titillium Web", FontSlant::Normal, weight);
    cr.set_font_size(size);
    cr.move_to(x, y);
    let _ = cr.show_text(s);
}

fn text_centered(cr: &Context, size: f64, bold: bool, cx: f64, cy: f64, s: &str) {
    let weight = if bold { FontWeight::Bold } else { FontWeight::Normal };
    cr.select_font_face("Titillium Web", FontSlant::Normal, weight);
    cr.set_font_size(size);
    if let Ok(e) = cr.text_extents(s) {
        cr.move_to(cx - e.width() / 2.0 - e.x_bearing(), cy - e.height() / 2.0 - e.y_bearing());
        let _ = cr.show_text(s);
    }
}

fn wheel_image(n: usize, hover: i32, scale: u32) -> Option<ImageSurface> {
    let side = WHEEL as i32 * scale as i32;
    let surface = ImageSurface::create_for_data(
        wheelgfx::render_scaled(n, hover, scale),
        gtk4::cairo::Format::ARgb32,
        side,
        side,
        side * 4,
    )
    .ok()?;
    surface.set_device_scale(scale as f64, scale as f64);
    Some(surface)
}

fn draw_guide(cr: &Context, width: f64, height: f64, elapsed: u128, cache: &mut Vec<((usize, i32), ImageSurface)>) {
    rgb(cr, (0xFF, 0xFF, 0xFF), 1.0);
    let _ = cr.paint();
    let mode = (elapsed / CYCLE_MS) as usize % DEMOS.len();
    let t = (elapsed % CYCLE_MS) as f64;
    rgb(cr, wheelgfx::PRIMARY, 1.0);
    text(cr, 22.0, true, 28.0, 44.0, "Cosa fa la ruota di Cedro");
    rgb(cr, wheelgfx::MUTED, 1.0);
    text(cr, 14.0, false, 28.0, 68.0, "Clic destro sul file, poi tieni premuto un tasto.");
    for (i, demo) in DEMOS.iter().enumerate() {
        let y = 100.0 + i as f64 * 92.0;
        if i == mode {
            rgb(cr, (0xEB, 0xEE, 0xF0), 1.0);
            rounded(cr, 20.0, y, 340.0, 78.0, 10.0);
            let _ = cr.fill();
        }
        rgb(cr, if i == mode { wheelgfx::HOVER } else { wheelgfx::PRIMARY }, 1.0);
        rounded(cr, 32.0, y + 18.0, 74.0, 42.0, 8.0);
        let _ = cr.fill();
        rgb(cr, (0xFF, 0xFF, 0xFF), 1.0);
        text_centered(cr, 16.0, true, 69.0, y + 39.0, demo.key);
        rgb(cr, wheelgfx::TEXT, 1.0);
        text(cr, 18.0, true, 124.0, y + 33.0, demo.title);
        rgb(cr, wheelgfx::MUTED, 1.0);
        text(cr, 13.0, false, 124.0, y + 56.0, demo.detail);
    }
    // Palco blu scuro dietro la ruota.
    let (sx, sy, sw, sh) = (width - 384.0, 24.0, 364.0, height - 48.0);
    rgb(cr, (0x00, 0x22, 0x4A), 1.0);
    rounded(cr, sx, sy, sw, sh, 14.0);
    let _ = cr.fill();
    let (alpha, lit, cursor) = scene(mode, t, (sw, sh));
    let demo = &DEMOS[mode];
    if alpha > 0.0 {
        let hover = if lit { demo.pick as i32 } else { -1 };
        let key = (mode, hover);
        if !cache.iter().any(|(k, _)| *k == key) {
            if let Some(img) = wheel_image(demo.labels.len(), hover, 1) {
                cache.push((key, img));
            }
        }
        let _ = cr.save();
        cr.translate(sx + sw / 2.0, sy + sh / 2.0);
        let k = SHOWN / WHEEL as f64;
        cr.scale(k, k);
        if let Some((_, img)) = cache.iter().find(|(kk, _)| *kk == key) {
            let _ = cr.set_source_surface(img, -(WHEEL as f64) / 2.0, -(WHEEL as f64) / 2.0);
            let _ = cr.paint_with_alpha(alpha);
        }
        let n = demo.labels.len();
        for (i, label) in demo.labels.iter().enumerate() {
            let (a0, a1) = slice_bounds(i, n);
            let mid = ((a0 + a1) / 2.0) as f64;
            let radius = ((R0 + R1) / 2.0) as f64;
            let on = lit && i == demo.pick;
            rgb(cr, if on { wheelgfx::TEXT_ON_HOVER } else { wheelgfx::TEXT }, alpha);
            text_centered(cr, if n > 6 { 15.0 } else { 18.0 }, true, radius * mid.cos(), radius * mid.sin(), label);
        }
        rgb(cr, wheelgfx::MUTED, alpha);
        text_centered(cr, 12.0, true, 0.0, -20.0, demo.caption);
        rgb(cr, wheelgfx::NAVY, alpha);
        text_centered(cr, 17.0, true, 0.0, 2.0, if lit { demo.labels[demo.pick] } else { "Annulla" });
        let _ = cr.restore();
    }
    // Un piccolo cursore.
    let (cx, cy) = (sx + cursor.0, sy + cursor.1);
    rgb(cr, (0xFF, 0xFF, 0xFF), 0.95);
    cr.arc(cx, cy, 7.0, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();
    rgb(cr, wheelgfx::PRIMARY, 1.0);
    cr.arc(cx, cy, 7.0, 0.0, std::f64::consts::TAU);
    cr.set_line_width(2.0);
    let _ = cr.stroke();
}

/// Apre la Guida animata, o porta avanti quella già aperta.
pub fn open_guide(app: &Application) {
    for window in app.windows() {
        if window.title().is_some_and(|t| t == "Guida di Cedro") {
            window.present();
            return;
        }
    }
    let window = titled(app, "Guida di Cedro", 780, 440);
    let area = DrawingArea::new();
    area.set_content_width(780);
    area.set_content_height(440);
    let started = Instant::now();
    let cache: Rc<RefCell<Vec<((usize, i32), ImageSurface)>>> = Rc::new(RefCell::new(Vec::new()));
    area.set_draw_func(move |_, cr, w, h| {
        draw_guide(cr, w as f64, h as f64, started.elapsed().as_millis(), &mut cache.borrow_mut());
    });
    window.set_child(Some(&area));
    let weak = area.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(33), move || match weak.upgrade() {
        Some(area) if area.is_mapped() => {
            area.queue_draw();
            glib::ControlFlow::Continue
        }
        Some(_) => glib::ControlFlow::Continue,
        None => glib::ControlFlow::Break,
    });
    window.present();
}
