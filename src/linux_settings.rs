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
    Application, Box as HBox, Button, CheckButton, CssProvider, DrawingArea, HeaderBar, Label, Orientation,
    Scale, Window, STYLE_PROVIDER_PRIORITY_APPLICATION,
};

const POWERED_BY: &str = "Powered by Luca \"Realhcok\" A.";

const CSS: &str = "\
headerbar.cedro-head { background: #005E8E; color: #ffffff; box-shadow: none; }\
headerbar.cedro-head .title, headerbar.cedro-head button, headerbar.cedro-head label { color: #ffffff; }\
window.cedro-panel { background: #ffffff; color: #191919; }\
window.cedro-panel label { color: #191919; }\
window.cedro-panel label.cedro-muted { color: #455A64; font-size: 90%; }\
window.cedro-panel label.cedro-h { font-size: 155%; font-weight: bold; }\
window.cedro-panel label.cedro-rt { font-weight: bold; }\
window.cedro-panel button.suggested-action { background: #005E8E; color: #ffffff; }\
window.cedro-panel button.suggested-action label { color: #ffffff; }\
window.cedro-panel frame > label { color: #005E8E; font-weight: bold; }\
window.cedro-panel separator { background: #E5E5E5; min-height: 1px; }\
window.cedro-panel list.cedro-nav { background: #F3F5F7; border-right: 1px solid #E5E5E5; padding: 10px 0; }\
window.cedro-panel list.cedro-nav row { background: transparent; padding: 9px 14px; margin: 1px 8px; border-radius: 5px; }\
window.cedro-panel list.cedro-nav row:selected { background: #ffffff; box-shadow: inset 3px 0 0 #005E8E; }\
window.cedro-panel list.cedro-nav row:selected label { font-weight: bold; }\
window.cedro-panel box.cedro-foot { background: #F3F5F7; border-top: 1px solid #E5E5E5; padding: 12px 18px; }\
window.cedro-panel button.cedro-sec { background: #F3F5F7; background-image: none; border: 1px solid #E5E5E5; border-radius: 5px; padding: 5px 14px; box-shadow: none; }\
window.cedro-panel box.cedro-foot button.cedro-sec { background: #ffffff; }\
window.cedro-panel button.cedro-pri { background: #005E8E; background-image: none; border: 1px solid #005E8E; border-radius: 5px; padding: 5px 20px; box-shadow: none; }\
window.cedro-panel button.cedro-pri:hover { background: #0066CC; border-color: #0066CC; }\
window.cedro-panel button.cedro-pri label { color: #ffffff; font-weight: bold; }\
window.cedro-panel switch.cedro-sw { background: #ffffff; background-image: none; border: 1px solid #455A64; border-radius: 10px; min-width: 40px; min-height: 20px; box-shadow: none; }\
window.cedro-panel switch.cedro-sw:checked { background: #005E8E; border-color: #005E8E; }\
window.cedro-panel switch.cedro-sw slider { background: #455A64; background-image: none; border: none; border-radius: 50%; min-width: 12px; min-height: 12px; margin: 3px; box-shadow: none; }\
window.cedro-panel switch.cedro-sw:checked slider { background: #ffffff; }\
window.cedro-panel switch.cedro-sw label { font-size: 0; min-width: 0; }";

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

const DPI: [u32; 4] = [100, 144, 200, 300];
const MAX_PAGES: [u32; 4] = [50, 100, 300, 0];
const LANGS: [&str; 3] = ["auto", "it", "en"];
const SECTIONS: [&str; 6] = ["Generale", "Ruota", "Salvataggio", "Qualità", "OCR", "Aggiornamenti"];

/// Una riga di impostazione: titolo e descrizione a sinistra, il controllo a destra.
fn setting(title: &str, desc: &str, control: &impl IsA<gtk4::Widget>) -> HBox {
    let text = HBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk4::Align::Center);
    let t = Label::new(Some(title));
    t.set_xalign(0.0);
    t.add_css_class("cedro-rt");
    let d = Label::new(Some(desc));
    d.set_xalign(0.0);
    d.set_wrap(true);
    d.add_css_class("cedro-muted");
    text.append(&t);
    text.append(&d);
    let line = HBox::new(Orientation::Horizontal, 24);
    line.set_margin_top(12);
    line.set_margin_bottom(12);
    line.append(&text);
    control.set_valign(gtk4::Align::Center);
    control.set_halign(gtk4::Align::End);
    line.append(control);
    line
}

/// Una scheda: il titolo grande e le righe separate da una linea sottile.
fn page(title: &str, rows: &[&HBox]) -> HBox {
    let page = HBox::new(Orientation::Vertical, 0);
    page.set_margin_top(18);
    page.set_margin_start(28);
    page.set_margin_end(28);
    let heading = Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.set_margin_bottom(8);
    heading.add_css_class("cedro-h");
    page.append(&heading);
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            page.append(&gtk4::Separator::new(Orientation::Horizontal));
        }
        page.append(*row);
    }
    page
}

fn switch(active: bool) -> gtk4::Switch {
    let sw = gtk4::Switch::new();
    sw.set_active(active);
    sw.add_css_class("cedro-sw");
    sw
}

fn dropdown(items: &[&str], selected: usize) -> gtk4::DropDown {
    let dd = gtk4::DropDown::from_strings(items);
    dd.set_selected(selected as u32);
    dd.set_size_request(190, -1);
    dd
}

fn short_path(path: &str) -> String {
    if path.is_empty() { "Nessuna cartella scelta".into() } else { path.to_string() }
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
    let window = titled(app, "Impostazioni di Cedro", 780, 600);

    // Generale
    let autostart = switch(crate::linux_ui::autostart_enabled());
    let notify = switch(current.notify);
    let reveal = switch(current.reveal);
    let progress = switch(current.progress);
    let log = Button::with_label("Apri cartella");
    log.add_css_class("cedro-sec");
    log.connect_clicked(|_| {
        let dir = cedro::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = gtk4::gio::AppInfo::launch_default_for_uri(&format!("file://{}", dir.display()), None::<&gtk4::gio::AppLaunchContext>);
    });
    let general = page(
        "Generale",
        &[
            &setting("Avvia Cedro con il computer", "La stessa voce del menu dell'icona.", &autostart),
            &setting("Avviso a fine conversione", "Una notifica con il nome del file creato.", &notify),
            &setting("Mostra il file nuovo", "A lavoro finito apre la cartella e seleziona la copia.", &reveal),
            &setting("Avanzamento mentre converte", "Una linea sottile nel pannello e, su KDE, tra le notifiche.", &progress),
            &setting("Registro attività", "Il file log.txt e config.txt stanno nella cartella di Cedro.", &log),
        ],
    );

    // Ruota
    let shift = switch(current.key_shift);
    let ctrl = switch(current.key_ctrl);
    let alt = switch(current.key_alt);
    let dialogs = switch(current.dialogs);
    let wheel = page(
        "Ruota",
        &[
            &setting("Shift apre le conversioni", "Tieni premuto Shift e fai clic destro sul file.", &shift),
            &setting("Ctrl apre gli strumenti", "Comprimi, ruota, OCR e le altre fette di Ctrl.", &ctrl),
            &setting("Alt apre le azioni", "Duplica, percorso e ZIP. Spegnilo se Alt dà fastidio ad altri programmi.", &alt),
            &setting("Apri nelle finestre Apri/Salva", "Usa la ruota anche scegliendo un file in un programma.", &dialogs),
        ],
    );

    // Salvataggio
    let same = CheckButton::with_label("Nella stessa cartella del file");
    let sub = CheckButton::with_label("In una sottocartella «Cedro»");
    let fixed = CheckButton::with_label("Sempre in questa cartella:");
    sub.set_group(Some(&same));
    fixed.set_group(Some(&same));
    match current.dest {
        cedro::Dest::Same => same.set_active(true),
        cedro::Dest::Sub => sub.set_active(true),
        cedro::Dest::Fixed => fixed.set_active(true),
    }
    let dest_dir = Rc::new(RefCell::new(current.dest_dir.clone()));
    let path = Label::new(Some(&short_path(&current.dest_dir)));
    path.set_xalign(0.0);
    path.set_hexpand(true);
    path.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
    path.set_max_width_chars(26);
    path.add_css_class("cedro-muted");
    let browse = Button::with_label("Sfoglia…");
    browse.add_css_class("cedro-sec");
    let place = HBox::new(Orientation::Vertical, 4);
    place.set_size_request(330, -1);
    place.append(&same);
    place.append(&sub);
    place.append(&fixed);
    let chosen = HBox::new(Orientation::Horizontal, 8);
    chosen.set_margin_start(26);
    chosen.append(&path);
    chosen.append(&browse);
    place.append(&chosen);
    let exists = dropdown(&["Aggiungi un numero", "Sostituisci"], current.replace as usize);
    let saving = page(
        "Salvataggio",
        &[
            &setting("Dove mettere la copia", "L'originale resta sempre dov'è.", &place),
            &setting("Se il file esiste già", "Aggiungi un numero: foto-2.jpg. Sostituisci riscrive il file.", &exists),
        ],
    );
    {
        let window = window.clone();
        let dest_dir = dest_dir.clone();
        let path = path.clone();
        let fixed = fixed.clone();
        browse.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder().title("Cartella dove salvare le copie").build();
            let (dest_dir, path, fixed) = (dest_dir.clone(), path.clone(), fixed.clone());
            dialog.select_folder(Some(&window), None::<&gtk4::gio::Cancellable>, move |chosen| {
                if let Some(folder) = chosen.ok().and_then(|f| f.path()) {
                    let text = folder.display().to_string();
                    path.set_text(&short_path(&text));
                    *dest_dir.borrow_mut() = text;
                    fixed.set_active(true);
                }
            });
        });
    }

    // Qualità
    let scale = Scale::with_range(Orientation::Horizontal, 40.0, 95.0, 1.0);
    scale.set_value(current.quality as f64);
    scale.set_size_request(230, -1);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk4::PositionType::Right);
    let dpi = dropdown(&["100 dpi, leggero", "144 dpi, normale", "200 dpi", "300 dpi, stampa"], DPI.iter().position(|d| *d == current.pdf_dpi).unwrap_or(1));
    let compress = dropdown(&["Poco", "Medio", "Molto"], (current.compress as usize).min(2));
    let quality = page(
        "Qualità",
        &[
            &setting("Qualità JPEG e WebP", "Anche per MP3 e PDF in JPG. Più alta, file più grandi.", &scale),
            &setting("PDF in JPG: risoluzione", "Più dpi, immagini più nitide e più grandi.", &dpi),
            &setting("Comprimi immagine", "Quanto riduce la fetta «Comprimi».", &compress),
        ],
    );

    // OCR
    let lang = dropdown(&["Automatica", "Italiano", "Inglese"], LANGS.iter().position(|l| *l == current.ocr_lang).unwrap_or(0));
    let max_pages = dropdown(&["50", "100", "300", "Tutte"], MAX_PAGES.iter().position(|m| *m == current.ocr_max).unwrap_or(1));
    let ocr = page(
        "OCR",
        &[
            &setting("Lingua del testo", "Automatica sceglie da sola tra italiano e inglese.", &lang),
            &setting("Pagine al massimo per PDF", "Oltre il limite l'OCR salta le pagine e lo dice.", &max_pages),
        ],
    );

    // Aggiornamenti
    let check = Button::with_label("Controlla ora");
    check.add_css_class("cedro-sec");
    check.connect_clicked(|_| crate::linux_ui::check_updates(true));
    let updates = switch(current.update_check);
    let unskip = Button::with_label("Proponila di nuovo");
    unskip.add_css_class("cedro-sec");
    let skipped = match cedro::update::skipped_version() {
        Some(v) => format!("Hai detto no alla {v}: all'avvio non la propone più."),
        None => "Nessuna versione rifiutata.".to_string(),
    };
    unskip.set_sensitive(cedro::update::skipped_version().is_some());
    unskip.connect_clicked(|button| {
        cedro::update::unskip();
        button.set_sensitive(false);
    });
    let upd = page(
        "Aggiornamenti",
        &[
            &setting("Versione installata", &format!("Cedro {}", cedro::update::CURRENT), &check),
            &setting("Cerca gli aggiornamenti da solo", "All'avvio e poi ogni 24 ore, in silenzio se non c'è niente.", &updates),
            &setting("Versione saltata", &skipped, &unskip),
        ],
    );

    // Colonna delle schede e contenuto
    let stack = gtk4::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    for (name, content) in [("0", &general), ("1", &wheel), ("2", &saving), ("3", &quality), ("4", &ocr), ("5", &upd)] {
        stack.add_named(content, Some(name));
    }
    let nav = gtk4::ListBox::new();
    nav.add_css_class("cedro-nav");
    nav.set_size_request(190, -1);
    for name in SECTIONS {
        let label = Label::new(Some(name));
        label.set_xalign(0.0);
        nav.append(&label);
    }
    {
        let stack = stack.clone();
        nav.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                stack.set_visible_child_name(&row.index().to_string());
            }
        });
    }
    if let Some(first) = nav.row_at_index(0) {
        nav.select_row(Some(&first));
    }

    let middle = HBox::new(Orientation::Horizontal, 0);
    middle.append(&nav);
    middle.append(&stack);

    let credit = Label::new(Some(POWERED_BY));
    credit.set_xalign(0.0);
    credit.set_hexpand(true);
    credit.add_css_class("cedro-muted");
    let guide = Button::with_label("Guida");
    guide.add_css_class("cedro-sec");
    let cancel = Button::with_label("Annulla");
    cancel.add_css_class("cedro-sec");
    let save = Button::with_label("Salva");
    save.add_css_class("cedro-pri");
    let foot = HBox::new(Orientation::Horizontal, 8);
    foot.add_css_class("cedro-foot");
    foot.append(&credit);
    foot.append(&guide);
    foot.append(&cancel);
    foot.append(&save);

    let body = HBox::new(Orientation::Vertical, 0);
    middle.set_vexpand(true);
    body.append(&middle);
    body.append(&foot);
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
            let dest = if fixed.is_active() {
                cedro::Dest::Fixed
            } else if sub.is_active() {
                cedro::Dest::Sub
            } else {
                cedro::Dest::Same
            };
            if dest == cedro::Dest::Fixed && dest_dir.borrow().trim().is_empty() {
                let alert = gtk4::AlertDialog::builder().message("Scegli la cartella di destinazione con «Sfoglia…».").build();
                alert.show(Some(&window));
                return;
            }
            let base = cedro::read_settings();
            let settings = Settings {
                quality: scale.value().round().clamp(40.0, 95.0) as u8,
                notify: notify.is_active(),
                progress: progress.is_active(),
                reveal: reveal.is_active(),
                update_check: updates.is_active(),
                key_shift: shift.is_active(),
                key_ctrl: ctrl.is_active(),
                key_alt: alt.is_active(),
                dialogs: dialogs.is_active(),
                dest,
                dest_dir: dest_dir.borrow().clone(),
                replace: exists.selected() == 1,
                pdf_dpi: *DPI.get(dpi.selected() as usize).unwrap_or(&base.pdf_dpi),
                compress: (compress.selected() as u8).min(2),
                ocr_lang: LANGS.get(lang.selected() as usize).unwrap_or(&"auto").to_string(),
                ocr_max: *MAX_PAGES.get(max_pages.selected() as usize).unwrap_or(&base.ocr_max),
                // Le opzioni che questa finestra non mostra restano come sono.
                ..base
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
    #[cfg(debug_assertions)]
    if let Ok(dir) = std::env::var("CEDRO_SHOTS") {
        shots(&window, &nav, std::path::PathBuf::from(dir));
    }
}

/// Solo nelle build di prova: con `CEDRO_SHOTS=<cartella>` salva un'immagine di ogni scheda e si chiude.
#[cfg(debug_assertions)]
fn shots(window: &Window, nav: &gtk4::ListBox, dir: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    fn step(window: Window, nav: gtk4::ListBox, dir: std::path::PathBuf, at: i32) {
        let Some(row) = nav.row_at_index(at) else {
            let app = window.application();
            glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
                if let Some(app) = app {
                    app.quit();
                }
            });
            return;
        };
        nav.select_row(Some(&row));
        glib::timeout_add_local_once(std::time::Duration::from_millis(700), move || {
            let name = format!("impostazioni-{at}-{}.png", SECTIONS[at as usize]);
            if let Err(e) = crate::linux_conversioni::save_png(&window, &dir.join(name)) {
                eprintln!("screenshot: {e}");
            }
            step(window.clone(), nav.clone(), dir.clone(), at + 1);
        });
    }
    step(window.clone(), nav.clone(), dir, 0);
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
