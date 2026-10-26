//! Il pannello Conversioni su GNOME e Plasma, con i colori del Comune di Bondeno.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use cedro::conversioni::{self, Control, Values, CATEGORIES};
use cedro::{extension, kind_of, read_settings, Kind};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, Box as HBox, Button, CheckButton, CssProvider, DropDown, DropTarget, FileDialog, FlowBox, Grid, Label,
    ListBox, Orientation, PasswordEntry, Scale, ScrolledWindow, SelectionMode, StringList, ToggleButton,
    STYLE_PROVIDER_PRIORITY_APPLICATION,
};

const TITLE: &str = "Conversioni di Cedro";
const POWERED_BY: &str = "Powered by Luca \"Realhcok\" A.";

const CSS: &str = "\
window.cedro-panel list.cedro-side { background: #f3f6f9; }\
window.cedro-panel list.cedro-side row { padding: 8px 14px; }\
window.cedro-panel list.cedro-side row:selected { background: #005E8E; }\
window.cedro-panel list.cedro-side row:selected label { color: #ffffff; font-weight: bold; }\
window.cedro-panel label.cedro-title { color: #005E8E; font-weight: bold; font-size: 150%; }\
window.cedro-panel label.cedro-cap { color: #455A64; font-weight: bold; font-size: 85%; }\
window.cedro-panel button.cedro-drop { background: #e3f0f8; color: #005E8E; border: 2px dashed #005E8E; padding: 14px; }\
window.cedro-panel button.cedro-drop label { color: #005E8E; font-weight: bold; }\
window.cedro-panel togglebutton.cedro-chip { background: #ffffff; border: 1px solid #d6dee5; padding: 6px 14px; }\
window.cedro-panel togglebutton.cedro-chip:hover { border-color: #0066CC; }\
window.cedro-panel togglebutton.cedro-chip:checked { background: #005E8E; border-color: #005E8E; }\
window.cedro-panel togglebutton.cedro-chip:checked label { color: #ffffff; }\
window.cedro-panel label.cedro-ok { color: #1d7a46; font-weight: bold; }\
window.cedro-panel label.cedro-fail { color: #b3261e; font-weight: bold; }";

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    status: String,
    class: &'static str,
}

struct Ui {
    window: gtk4::Window,
    side: ListBox,
    title: Label,
    hint: Label,
    queue: ListBox,
    count: Label,
    formats: FlowBox,
    tools: FlowBox,
    tools_caption: Label,
    options: Grid,
    options_caption: Label,
    values: RefCell<Values>,
    quality: Scale,
    dest_label: Label,
    go: Button,
    files: RefCell<Vec<Entry>>,
    category: Cell<usize>,
    action: RefCell<String>,
    dest: RefCell<Option<PathBuf>>,
    running: RefCell<Option<Receiver<(PathBuf, Result<String, String>)>>>,
}

fn caption(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.set_xalign(0.0);
    l.add_css_class("cedro-cap");
    l
}

fn clear_flow(flow: &FlowBox) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }
}

fn clear_list(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn chips(flow: &FlowBox, ui: &Rc<Ui>, items: &[cedro::Slice], first: Option<&ToggleButton>) -> Option<ToggleButton> {
    let mut lead: Option<ToggleButton> = first.cloned();
    for item in items {
        let chip = ToggleButton::with_label(item.label);
        chip.add_css_class("cedro-chip");
        if let Some(l) = &lead {
            chip.set_group(Some(l));
        } else {
            lead = Some(chip.clone());
        }
        let ui = ui.clone();
        let id = item.id.to_string();
        chip.connect_toggled(move |b| {
            if b.is_active() {
                *ui.action.borrow_mut() = id.clone();
                refresh_options(&ui);
                label_go(&ui);
            }
        });
        flow.insert(&chip, -1);
    }
    lead
}

/// Ridisegna la zona Opzioni per la voce scelta: interruttori, elenchi, cursori, password.
/// I valori già scelti restano se la nuova voce li offre ancora.
fn refresh_options(ui: &Rc<Ui>) {
    while let Some(child) = ui.options.first_child() {
        ui.options.remove(&child);
    }
    let kind = CATEGORIES[ui.category.get()].kind;
    let action = ui.action.borrow().clone();
    let defs = conversioni::option_defs(kind, &action);
    ui.options.set_visible(!defs.is_empty());
    ui.options_caption.set_visible(!defs.is_empty());
    for (i, def) in defs.iter().copied().enumerate() {
        let (col, row) = ((i % 2) as i32 * 2, (i / 2) as i32);
        let value = ui.values.borrow().get(def);
        match &def.control {
            Control::Switch => {
                let check = CheckButton::with_label(def.title);
                check.set_active(value != 0);
                check.set_tooltip_text(Some(def.note).filter(|n| !n.is_empty()));
                let owner = ui.clone();
                check.connect_toggled(move |c| owner.values.borrow_mut().set(def, c.is_active() as u32));
                ui.options.attach(&check, col, row, 2, 1);
            }
            Control::Choice(choices) => {
                let label = Label::new(Some(def.title));
                label.set_xalign(0.0);
                let names: Vec<&str> = choices.iter().map(|c| c.label).collect();
                let drop = DropDown::new(Some(StringList::new(&names)), gtk4::Expression::NONE);
                drop.set_selected(choices.iter().position(|c| c.value == value).unwrap_or(0) as u32);
                drop.set_hexpand(true);
                drop.set_tooltip_text(Some(def.note).filter(|n| !n.is_empty()));
                let owner = ui.clone();
                drop.connect_selected_notify(move |d| {
                    if let Some(c) = choices.get(d.selected() as usize) {
                        owner.values.borrow_mut().set(def, c.value);
                    }
                });
                ui.options.attach(&label, col, row, 1, 1);
                ui.options.attach(&drop, col + 1, row, 1, 1);
            }
            Control::Range { min, max, unit } => {
                let label = Label::new(Some(def.title));
                label.set_xalign(0.0);
                let scale = Scale::with_range(Orientation::Horizontal, *min as f64, *max as f64, 1.0);
                scale.set_value(value as f64);
                scale.set_draw_value(true);
                scale.set_format_value_func({
                    let unit = unit.to_string();
                    move |_, v| format!("{}{unit}", v.round() as u32)
                });
                scale.set_hexpand(true);
                scale.set_tooltip_text(Some(def.note).filter(|n| !n.is_empty()));
                let owner = ui.clone();
                scale.connect_value_changed(move |sc| owner.values.borrow_mut().set(def, sc.value().round() as u32));
                ui.options.attach(&label, col, row, 1, 1);
                ui.options.attach(&scale, col + 1, row, 1, 1);
            }
            Control::Password => {
                let label = Label::new(Some(def.title));
                label.set_xalign(0.0);
                let entry = PasswordEntry::new();
                entry.set_show_peek_icon(true);
                entry.set_text(&ui.values.borrow().password);
                entry.set_tooltip_text(Some(def.note).filter(|n| !n.is_empty()));
                let owner = ui.clone();
                entry.connect_changed(move |e| owner.values.borrow_mut().password = e.text().to_string());
                ui.options.attach(&label, col, row, 1, 1);
                ui.options.attach(&entry, col + 1, row, 1, 1);
            }
        }
    }
}

fn current_files(ui: &Ui) -> Vec<Entry> {
    let kind = CATEGORIES[ui.category.get()].kind;
    ui.files.borrow().iter().filter(|e| kind_of(&e.path) == kind).cloned().collect()
}

fn label_go(ui: &Ui) {
    let n = current_files(ui).len();
    let action = ui.action.borrow().clone();
    let kind = CATEGORIES[ui.category.get()].kind;
    let name = conversioni::formats(kind)
        .iter()
        .chain(conversioni::tools(kind).iter())
        .find(|s| s.id == action)
        .map(|s| s.label)
        .unwrap_or("");
    let busy = ui.running.borrow().is_some();
    ui.go.set_sensitive(n > 0 && !busy && !action.is_empty());
    ui.go.set_label(&match (busy, n) {
        (true, _) => "Conversione in corso…".to_string(),
        (_, 0) => "Converti".to_string(),
        (_, 1) => format!("Converti 1 file · {name}"),
        _ => format!("Converti {n} file · {name}"),
    });
}

fn render_queue(ui: &Ui) {
    clear_list(&ui.queue);
    let files = current_files(ui);
    ui.count.set_text(&match files.len() {
        0 => "Coda vuota".to_string(),
        1 => "Coda · 1 file".to_string(),
        n => format!("Coda · {n} file"),
    });
    for e in files {
        let row = HBox::new(Orientation::Horizontal, 12);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        row.set_margin_start(10);
        row.set_margin_end(10);
        let ext = Label::new(Some(&extension(&e.path).to_uppercase()));
        ext.add_css_class("cedro-cap");
        ext.set_width_chars(5);
        let name = Label::new(e.path.file_name().and_then(|n| n.to_str()));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        let status = Label::new(Some(&e.status));
        if !e.class.is_empty() {
            status.add_css_class(e.class);
        }
        row.append(&ext);
        row.append(&name);
        row.append(&status);
        ui.queue.append(&row);
    }
    label_go(ui);
}

fn render_category(ui: &Rc<Ui>) {
    let cat = &CATEGORIES[ui.category.get()];
    ui.title.set_text(cat.name);
    ui.hint.set_text(&format!("{} Accetta: {}.", cat.hint, cat.accepts));
    clear_flow(&ui.formats);
    clear_flow(&ui.tools);
    let first = chips(&ui.formats, ui, conversioni::formats(cat.kind), None);
    let tools = conversioni::tools(cat.kind);
    ui.tools.set_visible(!tools.is_empty());
    ui.tools_caption.set_visible(!tools.is_empty());
    chips(&ui.tools, ui, tools, first.as_ref());
    if let Some(first) = first {
        first.set_active(true);
    }
    render_queue(ui);
}

fn add_files(ui: &Rc<Ui>, paths: Vec<PathBuf>) {
    let mut first = None;
    for path in paths.into_iter().filter(|p| p.is_file()) {
        if kind_of(&path) == Kind::Other || ui.files.borrow().iter().any(|e| e.path == path) {
            continue;
        }
        first.get_or_insert(kind_of(&path));
        ui.files.borrow_mut().push(Entry { path, status: "In attesa".into(), class: "" });
    }
    // Se i file appena aggiunti sono di un'altra categoria, si passa a quella: altrimenti restano invisibili.
    if let Some(kind) = first {
        if CATEGORIES[ui.category.get()].kind != kind {
            if let Some(i) = CATEGORIES.iter().position(|c| c.kind == kind) {
                ui.side.select_row(ui.side.row_at_index(i as i32).as_ref());
                return;
            }
        }
    }
    render_queue(ui);
}

fn paths_of(list: &gio::ListModel) -> Vec<PathBuf> {
    (0..list.n_items())
        .filter_map(|i| list.item(i).and_downcast::<gio::File>())
        .filter_map(|f| f.path())
        .collect()
}

fn start(ui: &Rc<Ui>) {
    let files: Vec<PathBuf> = current_files(ui).into_iter().map(|e| e.path).collect();
    if files.is_empty() || ui.running.borrow().is_some() {
        return;
    }
    for e in ui.files.borrow_mut().iter_mut().filter(|e| files.contains(&e.path)) {
        e.status = "In attesa".into();
        e.class = "";
    }
    let action = ui.action.borrow().clone();
    let quality = ui.quality.value().round().clamp(40.0, 95.0) as u8;
    let dest = ui.dest.borrow().clone();
    let opts = conversioni::options_from(&ui.values.borrow());
    let (tx, rx) = channel();
    *ui.running.borrow_mut() = Some(rx);
    std::thread::spawn(move || {
        for path in files {
            let result = conversioni::convert_one(&path, &action, quality, dest.as_deref(), &opts);
            if tx.send((path, result)).is_err() {
                break;
            }
        }
    });
    render_queue(ui);
    let ui = ui.clone();
    glib::timeout_add_local(Duration::from_millis(150), move || {
        let mut changed = false;
        let mut finished = false;
        if let Some(rx) = ui.running.borrow().as_ref() {
            loop {
                match rx.try_recv() {
                    Ok((path, result)) => {
                        if let Some(e) = ui.files.borrow_mut().iter_mut().find(|e| e.path == path) {
                            match result {
                                Ok(msg) => (e.status, e.class) = (format!("Fatto · {msg}"), "cedro-ok"),
                                Err(msg) => (e.status, e.class) = (format!("Errore · {msg}"), "cedro-fail"),
                            }
                        }
                        changed = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        if finished {
            ui.running.borrow_mut().take();
        }
        if changed || finished {
            render_queue(&ui);
        }
        if finished {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// Apre il pannello Conversioni, o porta avanti quello già aperto.
pub fn open_conversions(app: &Application) {
    for window in app.windows() {
        if window.title().is_some_and(|t| t == TITLE) {
            window.present();
            return;
        }
    }
    if let Some(display) = gtk4::gdk::Display::default() {
        let provider = CssProvider::new();
        provider.load_from_string(CSS);
        gtk4::style_context_add_provider_for_display(&display, &provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    let window = crate::linux_settings::titled(app, TITLE, 940, 780);
    window.set_resizable(true);

    let side = ListBox::new();
    side.add_css_class("cedro-side");
    side.set_selection_mode(SelectionMode::Single);
    for cat in CATEGORIES {
        let l = Label::new(Some(cat.name));
        l.set_xalign(0.0);
        side.append(&l);
    }
    let side_scroll = ScrolledWindow::builder().child(&side).min_content_width(180).vexpand(true).build();

    let title = Label::new(None);
    title.set_xalign(0.0);
    title.add_css_class("cedro-title");
    let hint = Label::new(None);
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    hint.add_css_class("cedro-muted");

    let drop = Button::with_label("Trascina qui i file o clicca per sceglierli");
    drop.add_css_class("cedro-drop");
    let queue = ListBox::new();
    queue.set_selection_mode(SelectionMode::None);
    let queue_scroll = ScrolledWindow::builder().child(&queue).min_content_height(130).vexpand(true).has_frame(true).build();
    let count = caption("");

    let formats = FlowBox::new();
    formats.set_selection_mode(SelectionMode::None);
    formats.set_homogeneous(false);
    formats.set_max_children_per_line(12);
    let tools = FlowBox::new();
    tools.set_selection_mode(SelectionMode::None);
    tools.set_max_children_per_line(12);
    let tools_caption = caption("STRUMENTI");
    let options = Grid::new();
    options.set_column_spacing(12);
    options.set_row_spacing(6);
    options.set_column_homogeneous(false);
    let options_caption = caption("OPZIONI");

    let quality = Scale::with_range(Orientation::Horizontal, 40.0, 95.0, 1.0);
    quality.set_value(read_settings().quality as f64);
    quality.set_hexpand(true);
    quality.set_draw_value(true);
    quality.set_value_pos(gtk4::PositionType::Right);
    let quality_note = Label::new(Some("JPEG, WebP, PDF in JPG e video. Più alta, file più grandi."));
    quality_note.set_xalign(0.0);
    quality_note.add_css_class("cedro-muted");

    let dest_label = Label::new(Some("Accanto all'originale"));
    dest_label.set_xalign(0.0);
    dest_label.set_hexpand(true);
    dest_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    let change = Button::with_label("Cambia…");
    let reset = Button::with_label("Accanto all'originale");
    let go = Button::with_label("Converti");
    go.add_css_class("suggested-action");

    let overwrite = CheckButton::with_label("Se il nome esiste già, sostituisci il file");
    let dest_row = HBox::new(Orientation::Horizontal, 8);
    dest_row.append(&dest_label);
    dest_row.append(&change);
    dest_row.append(&reset);
    dest_row.append(&go);

    let credit = Label::new(Some("Tutto sul tuo PC. L'originale non si tocca.   ·   "));
    credit.set_xalign(0.0);
    credit.add_css_class("cedro-muted");
    let signature = Label::new(Some(POWERED_BY));
    signature.add_css_class("cedro-muted");
    let foot = HBox::new(Orientation::Horizontal, 0);
    foot.append(&credit);
    signature.set_hexpand(true);
    signature.set_xalign(1.0);
    foot.append(&signature);

    let main = HBox::new(Orientation::Vertical, 10);
    main.set_margin_top(16);
    main.set_margin_bottom(12);
    main.set_margin_start(20);
    main.set_margin_end(20);
    main.set_hexpand(true);
    for w in [
        title.upcast_ref::<gtk4::Widget>(),
        hint.upcast_ref(),
        drop.upcast_ref(),
        count.upcast_ref(),
        queue_scroll.upcast_ref(),
        caption("CONVERTI IN").upcast_ref(),
        formats.upcast_ref(),
        tools_caption.upcast_ref(),
        tools.upcast_ref(),
        options_caption.upcast_ref(),
        options.upcast_ref(),
        caption("QUALITÀ").upcast_ref(),
        quality.upcast_ref(),
        quality_note.upcast_ref(),
        caption("DOVE SALVARE").upcast_ref(),
        dest_row.upcast_ref(),
        overwrite.upcast_ref(),
        foot.upcast_ref(),
    ] {
        main.append(w);
    }

    let root = HBox::new(Orientation::Horizontal, 0);
    root.append(&side_scroll);
    root.append(&main);
    window.set_child(Some(&root));

    let ui = Rc::new(Ui {
        window: window.clone(),
        side: side.clone(),
        title,
        hint,
        queue,
        count,
        formats,
        tools,
        tools_caption,
        options,
        options_caption,
        values: RefCell::new(Values::new()),
        quality,
        dest_label,
        go: go.clone(),
        files: RefCell::new(Vec::new()),
        category: Cell::new(0),
        action: RefCell::new(String::new()),
        dest: RefCell::new(None),
        running: RefCell::new(None),
    });

    {
        // Finché la coda lavora la finestra resta aperta: chiuderla fermerebbe il processo a metà scrittura.
        let ui = ui.clone();
        window.connect_close_request(move |_| {
            if ui.running.borrow().is_some() {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    {
        let ui = ui.clone();
        side.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                ui.category.set(row.index() as usize);
                render_category(&ui);
            }
        });
    }
    {
        let ui = ui.clone();
        drop.connect_clicked(move |_| {
            let dialog = FileDialog::new();
            dialog.set_title("Scegli i file da convertire");
            let ui2 = ui.clone();
            dialog.open_multiple(Some(&ui.window), gio::Cancellable::NONE, move |res| {
                if let Ok(list) = res {
                    add_files(&ui2, paths_of(&list));
                }
            });
        });
    }
    {
        let target = DropTarget::new(gtk4::gdk::FileList::static_type(), gtk4::gdk::DragAction::COPY);
        let ui = ui.clone();
        target.connect_drop(move |_, value, _, _| {
            let Ok(list) = value.get::<gtk4::gdk::FileList>() else { return false };
            add_files(&ui, list.files().iter().filter_map(|f| f.path()).collect());
            true
        });
        window.add_controller(target);
    }
    {
        let ui = ui.clone();
        change.connect_clicked(move |_| {
            let dialog = FileDialog::new();
            dialog.set_title("Cartella di destinazione");
            let ui2 = ui.clone();
            dialog.select_folder(Some(&ui.window), gio::Cancellable::NONE, move |res| {
                if let Some(path) = res.ok().and_then(|f| f.path()) {
                    ui2.dest_label.set_text(&path.display().to_string());
                    *ui2.dest.borrow_mut() = Some(path);
                }
            });
        });
    }
    {
        let ui = ui.clone();
        reset.connect_clicked(move |_| {
            ui.dest_label.set_text("Accanto all'originale");
            *ui.dest.borrow_mut() = None;
        });
    }
    {
        let ui = ui.clone();
        overwrite.connect_toggled(move |c| ui.values.borrow_mut().overwrite = c.is_active());
    }
    {
        let ui = ui.clone();
        go.connect_clicked(move |_| start(&ui));
    }

    side.select_row(side.row_at_index(0).as_ref());
    window.present();
    #[cfg(debug_assertions)]
    if let Ok(dir) = std::env::var("CEDRO_SHOTS") {
        shots(&ui, std::path::PathBuf::from(dir));
    }
}

/// Solo nelle build di prova: con `CEDRO_SHOTS=<cartella>` il pannello salva da solo un'immagine di ogni
/// categoria (e di qualche voce) e poi si chiude. Con `CEDRO_SHOT_FILES=a;b` aggiunge prima quei file.
#[cfg(debug_assertions)]
fn shots(ui: &Rc<Ui>, dir: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(list) = std::env::var("CEDRO_SHOT_FILES") {
        add_files(ui, list.split(';').filter(|p| !p.is_empty()).map(PathBuf::from).collect());
    }
    // (categoria, voce da scegliere): None = la prima.
    let steps: Vec<(usize, Option<&'static str>)> = vec![
        (0, None),
        (0, Some("JPEG")),
        (1, None),
        (2, None),
        (2, Some("Word OCR")),
        (3, None),
        (3, Some("JSON")),
        (4, None),
        (4, Some("MP3")),
        (5, None),
        (5, Some("Solo audio (MP3)")),
    ];
    fn step(ui: Rc<Ui>, dir: std::path::PathBuf, steps: Vec<(usize, Option<&'static str>)>, at: usize) {
        let Some((cat, label)) = steps.get(at).copied() else {
            let app = ui.window.application();
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                if let Some(app) = app {
                    app.quit();
                }
            });
            return;
        };
        ui.side.select_row(ui.side.row_at_index(cat as i32).as_ref());
        if let Some(label) = label {
            for flow in [&ui.formats, &ui.tools] {
                let mut child = flow.first_child();
                while let Some(c) = child {
                    if let Some(chip) = c.first_child().and_then(|w| w.downcast::<ToggleButton>().ok()) {
                        if chip.label().is_some_and(|l| l == label) {
                            chip.set_active(true);
                        }
                    }
                    child = c.next_sibling();
                }
            }
        }
        let next = ui.clone();
        glib::timeout_add_local_once(Duration::from_millis(700), move || {
            let name = format!("{at:02}-{}{}.png", CATEGORIES[cat].name, label.map(|l| format!("-{}", l.replace(' ', "_"))).unwrap_or_default());
            let path = dir.join(name);
            // Appena cambiata la voce il disegno può non essere pronto: si riprova qualche volta.
            let mut tries = 0;
            glib::timeout_add_local(Duration::from_millis(300), move || {
                tries += 1;
                match save_png(&next.window, &path) {
                    Ok(()) => {}
                    Err(e) if tries < 6 => {
                        let _ = e;
                        return glib::ControlFlow::Continue;
                    }
                    Err(e) => eprintln!("screenshot: {e}"),
                }
                step(next.clone(), dir.clone(), steps.clone(), at + 1);
                glib::ControlFlow::Break
            });
        });
    }
    step(ui.clone(), dir, steps, 0);
}

#[cfg(debug_assertions)]
fn save_png(window: &gtk4::Window, path: &std::path::Path) -> Result<(), String> {
    use gtk4::gsk::prelude::*;
    let (w, h) = (window.width(), window.height());
    let paintable = gtk4::WidgetPaintable::new(Some(window));
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, w as f64, h as f64);
    let node = snapshot.to_node().ok_or("niente da disegnare")?;
    let renderer = window.native().and_then(|n| n.renderer()).ok_or("nessun renderer")?;
    let texture = renderer.render_texture(&node, None);
    texture.save_to_png(path).map_err(|e| e.to_string())
}
