//! Il pannello Conversioni su GNOME e Plasma, con i colori del Comune di Bondeno.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use cedro::conversioni::{self, Control, Values, CATEGORIES, EXISTS_CHOICES};
use cedro::trascrizione::{self, Event, Line, Request};
use cedro::{Exists, ExistsAsk, ExistsChoice};
use crate::linux_trascrizione::{self as tr, Change, Panel};
use cedro::{extension, kind_of, read_settings, Kind};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    AlertDialog, Application, Box as HBox, Button, CheckButton, CssProvider, Entry as TextEntry, DropDown, DropTarget, FileDialog, FlowBox, Grid, Label,
    ListBox, Orientation, PasswordEntry, Scale, ScrolledWindow, SelectionMode, StringList, ToggleButton,
    STYLE_PROVIDER_PRIORITY_APPLICATION,
};

const TITLE: &str = "Conversioni di Cedro";

const CSS: &str = "\
window.cedro-panel list.cedro-side { background: #f3f6f9; }\
window.cedro-panel list.cedro-side row { padding: 8px 14px; }\
window.cedro-panel list.cedro-side row:selected { background: #005E8E; }\
window.cedro-panel list.cedro-side row:selected label { color: #ffffff; font-weight: bold; }\
window.cedro-panel label.cedro-title { color: #005E8E; font-weight: bold; font-size: 150%; }\
window.cedro-panel label.cedro-cap { color: #455A64; font-weight: bold; font-size: 85%; }\
window.cedro-panel button.cedro-drop { background: #e3f0f8; color: #005E8E; border: 2px dashed #005E8E; padding: 14px; }\
window.cedro-panel button.cedro-drop label { color: #005E8E; font-weight: bold; }\
window.cedro-panel button.cedro-chip { background: #ffffff; background-image: none; border: 1px solid #d6dee5; padding: 6px 14px; }\
window.cedro-panel button.cedro-chip:hover { border-color: #0066CC; }\
window.cedro-panel button.cedro-chip:checked { background: #005E8E; background-image: none; border-color: #005E8E; }\
window.cedro-panel button.cedro-chip:checked label { color: #ffffff; }\
window.cedro-panel label.cedro-ok { color: #1d7a46; font-weight: bold; }\
window.cedro-panel label.cedro-fail { color: #b3261e; font-weight: bold; }\nwindow.cedro-panel label.cedro-skip { color: #8A5A00; font-weight: bold; }\
window.cedro-panel button.cedro-chip.cedro-sbig { padding: 12px 22px; }\
window.cedro-panel button.cedro-chip.cedro-sbig label.cedro-sbt { font-size: 130%; font-weight: bold; }\
window.cedro-panel button.cedro-chip.cedro-sbig label.cedro-sbn { font-size: 90%; color: #5b6b78; }\
window.cedro-panel button.cedro-chip.cedro-sbig:checked label.cedro-sbn { color: #d6ebf6; }\
window.cedro-panel button.cedro-biggo { padding: 14px; font-size: 130%; font-weight: bold; }\
window.cedro-panel button.cedro-link label { color: #005E8E; text-decoration: underline; }\
window.cedro-panel button.cedro-link { background: none; background-image: none; border: none; box-shadow: none; color: #005E8E; text-decoration: underline; padding: 2px 4px; }\
window.cedro-panel progressbar.cedro-progress trough { min-height: 3px; background: #d0dde8; border: none; border-radius: 2px; }\
window.cedro-panel progressbar.cedro-progress progress { min-height: 3px; background: #005E8E; border: none; border-radius: 2px; }\
window.cedro-panel progressbar.cedro-progress.cedro-done progress { background: #1d7a46; }\
window.cedro-panel progressbar.cedro-progress.cedro-error progress { background: #b3261e; }";

/// La voce del menu dopo le categorie: la scheda YouTube, che non lavora su file ma su link.
const YOUTUBE: &str = "YouTube";

/// Cosa il thread della conversione dice alla finestra.
enum Msg {
    Done(PathBuf, Result<String, String>),
    /// Il file di uscita esiste già: la finestra chiede e risponde su questo canale.
    Ask(PathBuf, std::sync::mpsc::Sender<ExistsChoice>),
    /// Una riga per la finestra «Cosa sta succedendo» della trascrizione.
    Log(Line),
}

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
    /// «Cambia formato»: formati, strumenti, opzioni e qualità. Si nasconde con la scheda Trascrizione.
    convert_box: HBox,
    tr: Rc<Panel>,
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
    /// La linea sottile sopra il piede, visibile mentre converte.
    bar: gtk4::ProgressBar,
    /// File del lavoro in corso: quanti in tutto e quanti finiti.
    job: Cell<(usize, usize)>,
    files: RefCell<Vec<Entry>>,
    category: Cell<usize>,
    action: RefCell<String>,
    dest: RefCell<Option<PathBuf>>,
    running: RefCell<Option<Receiver<Msg>>>,
    /// Modalità Semplice: stesso motore, meno cose in vista (vedi `apply_simple`).
    simple: bool,
    advanced: Cell<bool>,
    queue_scroll: ScrolledWindow,
    formats_caption: Label,
    /// Qualità: titolo, cursore e nota.
    quality_widgets: Vec<gtk4::Widget>,
    save_widgets: Vec<gtk4::Widget>,
    more: Button,
    stack: gtk4::Stack,
    /// Le tre schede della modalità Semplice: Converti file, Video dal web, Trascrivi.
    pick: Vec<ToggleButton>,
    /// Vero mentre una scheda Semplice sta cambiando pannello: la sincronizzazione aspetta.
    switching: Cell<bool>,
    syncing: Cell<bool>,
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
            Control::Text { hint } => {
                let label = Label::new(Some(def.title));
                label.set_xalign(0.0);
                let entry = TextEntry::new();
                entry.set_placeholder_text(Some(hint));
                entry.set_hexpand(true);
                entry.set_text(ui.values.borrow().text(def));
                entry.set_tooltip_text(Some(def.note).filter(|n| !n.is_empty()));
                let owner = ui.clone();
                entry.connect_changed(move |e| owner.values.borrow_mut().set_text(def, &e.text()));
                ui.options.attach(&label, col, row, 1, 1);
                ui.options.attach(&entry, col + 1, row, 1, 1);
            }
            Control::Password => {
                let label = Label::new(Some(def.title));
                label.set_xalign(0.0);
                let entry = PasswordEntry::new();
                entry.set_show_peek_icon(true);
                entry.set_hexpand(true);
                entry.set_text(ui.values.borrow().text(def));
                entry.set_tooltip_text(Some(def.note).filter(|n| !n.is_empty()));
                let owner = ui.clone();
                entry.connect_changed(move |e| owner.values.borrow_mut().set_text(def, &e.text()));
                ui.options.attach(&label, col, row, 1, 1);
                ui.options.attach(&entry, col + 1, row, 1, 1);
            }
        }
    }
    apply_simple(ui);
}

/// I formati della modalità Semplice: bottoni grandi con una riga di spiegazione.
fn simple_chips(flow: &FlowBox, ui: &Rc<Ui>, kind: Kind) -> Option<ToggleButton> {
    let mut lead: Option<ToggleButton> = None;
    for item in conversioni::simple_formats(kind) {
        let chip = ToggleButton::new();
        chip.add_css_class("cedro-chip");
        chip.add_css_class("cedro-sbig");
        let (title, note) = (Label::new(Some(item.title)), Label::new(Some(item.note)));
        title.add_css_class("cedro-sbt");
        note.add_css_class("cedro-sbn");
        title.set_xalign(0.0);
        note.set_xalign(0.0);
        let inner = HBox::new(Orientation::Vertical, 0);
        inner.append(&title);
        inner.append(&note);
        chip.set_child(Some(&inner));
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

/// Modalità Semplice: mostra solo l'essenziale e tiene il resto a un clic («Altre opzioni»).
/// Non cambia nulla del motore: i valori consigliati restano quelli impostati.
fn apply_simple(ui: &Ui) {
    if !ui.simple {
        return;
    }
    let voice = transcribing(ui);
    let files = !current_files(ui).is_empty();
    let adv = ui.advanced.get();
    let kind = CATEGORIES[ui.category.get()].kind;
    if voice {
        ui.title.set_text("Cosa vuoi trascrivere?");
        ui.hint.set_text("Trascina una registrazione, un vocale o un video: Cedro scrive il testo.");
    } else {
        ui.title.set_text("Cosa vuoi convertire?");
        ui.hint.set_text("Trascina qui foto, documenti, PDF, audio o video.");
    }
    ui.tr.tabs.set_visible(false);
    ui.tr.options.set_visible(adv);
    ui.queue_scroll.set_visible(files);
    ui.count.set_visible(files);
    let show_formats = !voice && files;
    ui.formats_caption.set_label("IN CHE COSA LI TRASFORMO?");
    ui.formats_caption.set_visible(show_formats);
    ui.formats.set_visible(show_formats);
    let has_tools = !conversioni::tools(kind).is_empty();
    ui.tools.set_visible(adv && has_tools);
    ui.tools_caption.set_visible(adv && has_tools);
    let has_options = ui.options.first_child().is_some();
    ui.options.set_visible(adv && has_options);
    ui.options_caption.set_visible(adv && has_options);
    for w in &ui.quality_widgets {
        w.set_visible(adv && !voice);
    }
    for w in &ui.save_widgets {
        w.set_visible(adv);
    }
    ui.go.set_visible(files);
    ui.more.set_label(if adv { "Meno opzioni" } else { "Altre opzioni" });
    // La scheda in alto segue ciò che si vede (un vocale .opus apre la trascrizione da solo).
    if !ui.switching.get() && ui.stack.visible_child_name().is_some_and(|n| n == "files") {
        ui.syncing.set(true);
        ui.pick[if voice { 2 } else { 0 }].set_active(true);
        ui.syncing.set(false);
    }
}

/// Vero se si vede la scheda Trascrizione (solo con la categoria Audio).
fn transcribing(ui: &Ui) -> bool {
    CATEGORIES[ui.category.get()].kind == Kind::Audio && ui.tr.selected()
}

fn current_files(ui: &Ui) -> Vec<Entry> {
    let kind = CATEGORIES[ui.category.get()].kind;
    // La trascrizione prende anche l'audio dei video e i vocali (.opus, .amr…) che le altre voci non leggono.
    let voice = transcribing(ui);
    ui.files.borrow().iter().filter(|e| if voice { trascrizione::transcribable(&e.path) } else { kind_of(&e.path) == kind }).cloned().collect()
}

/// Mostra le due schede di Audio e, con «Trascrizione», i suoi controlli al posto di quelli del formato.
fn apply_mode(ui: &Ui) {
    let cat = &CATEGORIES[ui.category.get()];
    let on = transcribing(ui);
    ui.tr.tabs.set_visible(cat.kind == Kind::Audio);
    ui.convert_box.set_visible(!on);
    ui.tr.body.set_visible(on);
    ui.hint.set_text(&if on { tr::HINT.to_string() } else { format!("{} Accetta: {}.", cat.hint, cat.accepts) });
    if on && ui.running.borrow().is_none() {
        ui.tr.idle_text();
    }
    render_queue(ui);
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
    ui.tr.tabs.set_sensitive(!busy);
    if transcribing(ui) {
        ui.go.set_sensitive(n > 0 && !busy);
        ui.go.set_label(&match (busy, n) {
            (true, _) => "Trascrizione in corso…".to_string(),
            (_, 0) => "Trascrivi".to_string(),
            _ => format!("Trascrivi {n} file in {}", ui.tr.format().label()),
        });
        return;
    }
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
    apply_simple(ui);
}

fn render_category(ui: &Rc<Ui>) {
    let cat = &CATEGORIES[ui.category.get()];
    ui.title.set_text(cat.name);
    clear_flow(&ui.formats);
    clear_flow(&ui.tools);
    let first = if ui.simple { simple_chips(&ui.formats, ui, cat.kind) } else { chips(&ui.formats, ui, conversioni::formats(cat.kind), None) };
    let tools = conversioni::tools(cat.kind);
    ui.tools.set_visible(!tools.is_empty());
    ui.tools_caption.set_visible(!tools.is_empty());
    chips(&ui.tools, ui, tools, first.as_ref());
    if let Some(first) = first {
        first.set_active(true);
    }
    apply_mode(ui);
}

fn add_files(ui: &Rc<Ui>, paths: Vec<PathBuf>) {
    let mut first = None;
    let mut voice = false;
    let transcribing = transcribing(ui);
    for path in paths.into_iter().filter(|p| p.is_file()) {
        let only = trascrizione::only_transcribable(&path);
        if (kind_of(&path) == Kind::Other && !only) || ui.files.borrow().iter().any(|e| e.path == path) {
            continue;
        }
        // Un vocale .opus o .amr va solo nella trascrizione; un video, con la trascrizione aperta, resta lì.
        if first.is_none() {
            voice = only || (transcribing && trascrizione::transcribable(&path));
        }
        first.get_or_insert(if only { Kind::Audio } else { kind_of(&path) });
        ui.files.borrow_mut().push(Entry { path, status: "In attesa".into(), class: "" });
    }
    if voice {
        ui.tr.select(true);
        if !transcribing {
            if let Some(i) = CATEGORIES.iter().position(|c| c.kind == Kind::Audio) {
                if ui.category.get() != i {
                    ui.side.select_row(ui.side.row_at_index(i as i32).as_ref());
                    return;
                }
            }
        }
        apply_mode(ui);
        return;
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

/// La conversione della coda in un thread, un file dopo l'altro.
fn spawn_conversion(files: Vec<PathBuf>, action: String, quality: u8, dest: Option<PathBuf>, opts: cedro::Options, label: String, tx: std::sync::mpsc::Sender<Msg>) {
std::thread::spawn(move || {
    let total = files.len();
    let name = |p: &std::path::Path| p.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
    let step_label = |i: usize, p: &std::path::Path| {
        if total == 1 { format!("{} → {label}", name(p)) } else { format!("{} di {total} · {}", i + 1, name(p)) }
    };
    if let Some(first) = files.first() {
        crate::linux_progress::start(total, &step_label(0, first));
    }
    let mut failed = None;
    for (i, path) in files.into_iter().enumerate() {
        crate::linux_progress::step(i, &step_label(i, &path));
        let result = conversioni::convert_one(&path, &action, quality, dest.as_deref(), &opts);
        if let Err(e) = &result {
            failed.get_or_insert_with(|| e.clone());
        }
        if tx.send(Msg::Done(path, result)).is_err() {
            break;
        }
    }
    match failed {
        Some(e) => crate::linux_progress::finish(false, &format!("Errore · {e}")),
        None => crate::linux_progress::finish(true, &format!("Fatto · {total} file in {label}")),
    }
});
}

/// La trascrizione della coda in un thread: il motore si accende ora e si spegne a fine coda.
fn spawn_transcription(files: Vec<PathBuf>, request: Request, dest: Option<PathBuf>, opts: cedro::Options, tx: std::sync::mpsc::Sender<Msg>) {
    std::thread::spawn(move || {
        let total = files.len();
        let format = request.format;
        let name = |p: &std::path::Path| p.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
        let label = |i: usize, p: &std::path::Path, pct: u8| {
            let head = if total == 1 { name(p) } else { format!("{} di {total} · {}", i + 1, name(p)) };
            if pct > 0 { format!("{head} · {pct}%") } else { format!("Trascrivo {head}") }
        };
        if let Some(first) = files.first() {
            crate::linux_progress::start(total, &label(0, first, 0));
        }
        let failed = std::sync::Mutex::new(None::<String>);
        trascrizione::run(&files, &request, dest.as_deref(), &opts, &|event| match event {
            Event::Log(line) => {
                let _ = tx.send(Msg::Log(line));
            }
            Event::Percent { index, percent } => {
                if let Some(p) = files.get(index) {
                    crate::linux_progress::step(index, &label(index, p, percent));
                }
            }
            Event::Finished { path, result } => {
                let result = result.map(|(_, message)| message);
                // Un file saltato perché esiste già non fa diventare rossa la barra.
                if let Err(e) = &result {
                    if !e.contains("saltato") {
                        failed.lock().unwrap_or_else(|x| x.into_inner()).get_or_insert_with(|| e.clone());
                    }
                }
                let _ = tx.send(Msg::Done(path, result));
            }
        });
        match failed.into_inner().unwrap_or_else(|e| e.into_inner()) {
            Some(e) => crate::linux_progress::finish(false, &format!("Errore · {e}")),
            None => crate::linux_progress::finish(true, &format!("Fatto · {total} file in {}", format.label())),
        }
    });
}

fn start(ui: &Rc<Ui>) {
    let files: Vec<PathBuf> = current_files(ui).into_iter().map(|e| e.path).collect();
    if files.is_empty() || ui.running.borrow().is_some() {
        return;
    }
    let kind = CATEGORIES[ui.category.get()].kind;
    let voice = transcribing(ui);
    if !voice {
        if let Err(e) = conversioni::validate(&ui.values.borrow(), kind, &ui.action.borrow()) {
            AlertDialog::builder().message("Controlla le pagine").detail(e).modal(true).build().show(Some(&ui.window));
            return;
        }
    }
    for e in ui.files.borrow_mut().iter_mut().filter(|e| files.contains(&e.path)) {
        e.status = "In attesa".into();
        e.class = "";
    }
    let action = ui.action.borrow().clone();
    let quality = ui.quality.value().round().clamp(40.0, 95.0) as u8;
    let dest = ui.dest.borrow().clone();
    let mut opts = conversioni::options_from(&ui.values.borrow());
    let (tx, rx) = channel();
    *ui.running.borrow_mut() = Some(rx);
    if opts.exists == Exists::Ask {
        let asker = tx.clone();
        opts.on_exists = Some(ExistsAsk::new(move |path| {
            let (answer_tx, answer_rx) = std::sync::mpsc::channel();
            if asker.send(Msg::Ask(path.to_path_buf(), answer_tx)).is_err() {
                return ExistsChoice::KeepBoth;
            }
            answer_rx.recv().unwrap_or(ExistsChoice::KeepBoth)
        }));
    }
    let label = conversioni::action_label(kind, &action).to_string();
    ui.job.set((files.len(), 0));
    ui.bar.remove_css_class("cedro-done");
    ui.bar.remove_css_class("cedro-error");
    ui.bar.set_fraction(0.0);
    ui.bar.set_visible(read_settings().progress);
    if voice {
        let settings = read_settings();
        let request = Request { format: ui.tr.format(), model: ui.tr.model(), gpu: settings.gpu, avx512: settings.avx512 };
        ui.tr.clear();
        spawn_transcription(files, request, dest, opts, tx);
    } else {
        spawn_conversion(files, action, quality, dest, opts, label, tx);
    }
    render_queue(ui);
    let ui = ui.clone();
    glib::timeout_add_local(Duration::from_millis(150), move || {
        let mut changed = false;
        let mut finished = false;
        if let Some(rx) = ui.running.borrow().as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(Msg::Done(path, result)) => {
                        let (total, done) = ui.job.get();
                        ui.job.set((total, done + 1));
                        if result.as_ref().is_err_and(|e| !e.contains("saltato")) {
                            ui.bar.add_css_class("cedro-error");
                        }
                        if let Some(e) = ui.files.borrow_mut().iter_mut().find(|e| e.path == path) {
                            match result {
                                Ok(msg) => (e.status, e.class) = (format!("Fatto · {msg}"), "cedro-ok"),
                                // Un file saltato perché esiste già è una scelta, non un errore.
                                Err(msg) if msg.contains("saltato") => (e.status, e.class) = (format!("Saltato · {msg}"), "cedro-skip"),
                                Err(msg) => (e.status, e.class) = (format!("Errore · {msg}"), "cedro-fail"),
                            }
                        }
                        changed = true;
                    }
                    Ok(Msg::Log(line)) => ui.tr.append(&line, true),
                    Ok(Msg::Ask(path, answer)) => {
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("il file").to_string();
                        let dialog = AlertDialog::builder()
                            .message(format!("«{name}» esiste già"))
                            .detail("Cosa faccio con il file nuovo?")
                            .buttons(["Sostituisci", "Tieni entrambi", "Salta"])
                            .default_button(1)
                            .cancel_button(2)
                            .modal(true)
                            .build();
                        dialog.choose(Some(&ui.window), gio::Cancellable::NONE, move |res| {
                            let _ = answer.send(match res {
                                Ok(0) => ExistsChoice::Replace,
                                Ok(1) => ExistsChoice::KeepBoth,
                                _ => ExistsChoice::Skip,
                            });
                        });
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        let (total, done) = ui.job.get();
        if finished {
            ui.running.borrow_mut().take();
            // Il modello è sul PC dopo la prima trascrizione.
            ui.tr.refresh_models();
            ui.bar.set_fraction(1.0);
            if !ui.bar.has_css_class("cedro-error") {
                // A lavoro finito la linea resta verde un secondo, poi sparisce.
                ui.bar.add_css_class("cedro-done");
                // Se nel frattempo è partito un altro lavoro la linea non è più verde e resta.
                let bar = ui.bar.clone();
                glib::timeout_add_local_once(Duration::from_millis(1200), move || {
                    if bar.has_css_class("cedro-done") {
                        bar.set_visible(false);
                    }
                });
            }
        } else if total == 1 {
            // Un file solo non dice quanto manca: la linea scorre.
            ui.bar.pulse();
        } else if total > 0 {
            ui.bar.set_fraction(done as f64 / total as f64);
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
    let simple = read_settings().simple;
    let window = crate::linux_settings::titled(app, TITLE, 940, 780);
    window.set_resizable(true);
    // Dopo i colori di `titled`: GTK fa vincere il gestore di stile aggiunto per ultimo, a parità di priorità.
    if let Some(display) = gtk4::gdk::Display::default() {
        let provider = CssProvider::new();
        provider.load_from_string(&format!("{CSS}{}{}", crate::linux_youtube::CSS, tr::CSS));
        gtk4::style_context_add_provider_for_display(&display, &provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let side = ListBox::new();
    side.add_css_class("cedro-side");
    side.set_selection_mode(SelectionMode::Single);
    for cat in CATEGORIES {
        let l = Label::new(Some(cat.name));
        l.set_xalign(0.0);
        side.append(&l);
    }
    let youtube_label = Label::new(Some(YOUTUBE));
    youtube_label.set_xalign(0.0);
    side.append(&youtube_label);
    let web_label = Label::new(Some("Video dal web"));
    web_label.set_xalign(0.0);
    side.append(&web_label);
    // Niente scorrimento orizzontale: la colonna si allarga quanto la voce più lunga (in grassetto quando è scelta).
    let side_scroll = ScrolledWindow::builder()
        .child(&side)
        .min_content_width(180)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .build();

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
    let queue_scroll = ScrolledWindow::builder().child(&queue).min_content_height(if simple { 90 } else { 130 }).vexpand(!simple).has_frame(true).build();
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

    let overwrite_label = Label::new(Some("Se il nome esiste già"));
    overwrite_label.set_xalign(0.0);
    let overwrite = DropDown::from_strings(&EXISTS_CHOICES.iter().map(|(l, _)| *l).collect::<Vec<_>>());
    let overwrite_row = HBox::new(Orientation::Horizontal, 8);
    overwrite_row.append(&overwrite_label);
    overwrite_row.append(&overwrite);
    let dest_row = HBox::new(Orientation::Horizontal, 8);
    dest_row.append(&dest_label);
    dest_row.append(&change);
    dest_row.append(&reset);
    if !simple {
        dest_row.append(&go);
    }

    let credit = Label::new(Some("Tutto sul tuo PC. L'originale non si tocca.   ·   "));
    credit.set_xalign(0.0);
    credit.add_css_class("cedro-muted");
    let signature = crate::linux_firma::firma(true, "");
    let foot = HBox::new(Orientation::Horizontal, 0);
    foot.append(&credit);
    signature.set_hexpand(true);
    signature.set_halign(gtk4::Align::End);
    foot.append(&signature);

    let bar = gtk4::ProgressBar::new();
    bar.add_css_class("cedro-progress");
    bar.set_pulse_step(0.08);
    bar.set_visible(false);

    // Il pannello della trascrizione chiama `hook` quando cambia qualcosa; la finestra lo collega dopo, a costruzione finita.
    let hook: Rc<RefCell<Option<Box<dyn Fn(Change)>>>> = Rc::new(RefCell::new(None));
    let tr = Panel::build({
        let hook = hook.clone();
        move |change| {
            if let Some(f) = hook.borrow().as_ref() {
                f(change);
            }
        }
    });
    let convert_box = HBox::new(Orientation::Vertical, 10);
    let formats_caption = caption("CONVERTI IN");
    let (quality_caption, dest_caption) = (caption("QUALITÀ"), caption("DOVE SALVARE"));
    for w in [
        formats_caption.clone().upcast::<gtk4::Widget>(),
        formats.clone().upcast(),
        tools_caption.clone().upcast(),
        tools.clone().upcast(),
        options_caption.clone().upcast(),
        options.clone().upcast(),
        quality_caption.clone().upcast(),
        quality.clone().upcast(),
        quality_note.clone().upcast(),
    ] {
        convert_box.append(&w);
    }

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
        tr.tabs.upcast_ref(),
        convert_box.upcast_ref(),
        tr.body.upcast_ref(),
        dest_caption.upcast_ref(),
        dest_row.upcast_ref(),
        overwrite_row.upcast_ref(),
    ] {
        main.append(w);
    }
    // Modalità Semplice: un solo bottone grande, e sotto i due link.
    let more = Button::with_label("Altre opzioni");
    more.add_css_class("cedro-link");
    let full = Button::with_label("Modalità completa");
    full.add_css_class("cedro-link");
    let links = HBox::new(Orientation::Horizontal, 12);
    links.append(&more);
    full.set_hexpand(true);
    full.set_halign(gtk4::Align::End);
    links.append(&full);
    if simple {
        go.add_css_class("cedro-biggo");
        main.append(&go);
        main.append(&links);
    }
    for w in [
        bar.upcast_ref::<gtk4::Widget>(),
        foot.upcast_ref(),
    ] {
        main.append(w);
    }

    let youtube = crate::linux_youtube::build(&window, false, simple);
    let web_video = crate::linux_youtube::build(&window, true, simple);
    let stack = gtk4::Stack::new();
    stack.set_hexpand(true);
    stack.add_named(&main, Some("files"));
    stack.add_named(&youtube.widget, Some("youtube"));
    stack.add_named(&web_video.widget, Some("web"));
    let youtube_busy = {
        let (a, b) = (youtube.busy.clone(), web_video.busy.clone());
        move || a() || b()
    };
    let root = HBox::new(Orientation::Horizontal, 0);
    // Semplice: al posto dell'elenco a sinistra, tre schede in alto.
    let pick: Vec<ToggleButton> = if simple { ["Converti file", "Video dal web", "Trascrivi"].iter().map(|l| ToggleButton::with_label(l)).collect() } else { Vec::new() };
    if simple {
        side_scroll.set_visible(false);
        let tabs = HBox::new(Orientation::Horizontal, 6);
        tabs.set_margin_top(12);
        tabs.set_margin_start(20);
        tabs.set_margin_end(20);
        for (i, t) in pick.iter().enumerate() {
            t.add_css_class("cedro-tab");
            if i > 0 {
                t.set_group(Some(&pick[0]));
            }
            tabs.append(t);
        }
        pick[0].set_active(true);
        let right = HBox::new(Orientation::Vertical, 0);
        right.set_hexpand(true);
        right.append(&tabs);
        right.append(&stack);
        root.append(&side_scroll);
        root.append(&right);
    } else {
        root.append(&side_scroll);
        root.append(&stack);
    }
    window.set_child(Some(&root));

    let ui = Rc::new(Ui {
        window: window.clone(),
        side: side.clone(),
        title,
        hint,
        convert_box: convert_box.clone(),
        tr: tr.clone(),
        queue,
        count,
        formats,
        tools,
        tools_caption,
        options,
        options_caption,
        values: RefCell::new(Values::new()),
        quality: quality.clone(),
        dest_label,
        go: go.clone(),
        bar,
        job: Cell::new((0, 0)),
        files: RefCell::new(Vec::new()),
        category: Cell::new(0),
        action: RefCell::new(String::new()),
        dest: RefCell::new(None),
        running: RefCell::new(None),
        simple,
        advanced: Cell::new(false),
        queue_scroll: queue_scroll.clone(),
        formats_caption,
        quality_widgets: vec![quality_caption.upcast(), quality.upcast(), quality_note.upcast()],
        save_widgets: vec![dest_caption.clone().upcast(), dest_row.clone().upcast(), overwrite_row.clone().upcast()],
        more: more.clone(),
        stack: stack.clone(),
        pick: pick.clone(),
        switching: Cell::new(false),
        syncing: Cell::new(false),
    });

    {
        let ui = Rc::downgrade(&ui);
        *hook.borrow_mut() = Some(Box::new(move |change| {
            let Some(ui) = ui.upgrade() else { return };
            match change {
                Change::Tab => apply_mode(&ui),
                Change::Format => label_go(&ui),
                Change::Model => {
                    if ui.running.borrow().is_none() {
                        ui.tr.idle_text();
                    }
                }
            }
        }));
    }
    {
        // Finché la coda lavora la finestra resta aperta: chiuderla fermerebbe il processo a metà scrittura.
        let ui = ui.clone();
        window.connect_close_request(move |w| {
            if ui.running.borrow().is_some() || youtube_busy() {
                AlertDialog::builder().message("Una conversione è ancora in corso: aspetta che finisca.").modal(true).build().show(Some(w));
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    {
        let ui = ui.clone();
        let stack = stack.clone();
        side.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                let i = row.index() as usize;
                if i >= CATEGORIES.len() {
                    stack.set_visible_child_name(if i > CATEGORIES.len() { "web" } else { "youtube" });
                    return;
                }
                stack.set_visible_child_name("files");
                ui.category.set(i);
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
        overwrite.connect_selected_notify(move |d| {
            if let Some((_, choice)) = EXISTS_CHOICES.get(d.selected() as usize) {
                ui.values.borrow_mut().exists = *choice;
            }
        });
    }
    {
        let ui = ui.clone();
        go.connect_clicked(move |_| start(&ui));
    }
    if simple {
        for (i, tab) in pick.iter().enumerate() {
            let ui = ui.clone();
            tab.connect_toggled(move |t| {
                if !t.is_active() || ui.syncing.get() {
                    return;
                }
                ui.switching.set(true);
                match i {
                    1 => ui.stack.set_visible_child_name("web"),
                    _ => {
                        ui.stack.set_visible_child_name("files");
                        let audio = CATEGORIES.iter().position(|c| c.kind == Kind::Audio).unwrap_or(0);
                        if i == 2 && ui.category.get() != audio {
                            ui.side.select_row(ui.side.row_at_index(audio as i32).as_ref());
                        }
                        ui.tr.select(i == 2);
                        apply_mode(&ui);
                    }
                }
                ui.switching.set(false);
            });
        }
        {
            let ui = ui.clone();
            more.connect_clicked(move |_| {
                ui.advanced.set(!ui.advanced.get());
                apply_simple(&ui);
            });
        }
        {
            // Torna alla vista completa: spegne l'impostazione e riapre il pannello.
            let window = window.clone();
            full.connect_clicked(move |_| {
                let mut s = read_settings();
                s.simple = false;
                let _ = cedro::write_settings(&s);
                let app = window.application();
                window.close();
                if let Some(app) = app {
                    open_conversions(&app);
                }
            });
        }
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
    if ui.simple {
        return shots_simple(ui, dir);
    }
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
        (4, Some("Trascrizione")),
        (5, None),
        (5, Some("Solo audio (MP3)")),
        (6, None),
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
        ui.tr.select(label == Some("Trascrizione"));
        ui.side.select_row(ui.side.row_at_index(cat as i32).as_ref());
        if let Some(label) = label.filter(|l| *l != "Trascrizione") {
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
        // Con CEDRO_SHOT_TRANSCRIBE=1 la scheda Trascrizione parte davvero e la foto si fa a lavoro finito.
        let wait = label == Some("Trascrizione") && std::env::var_os("CEDRO_SHOT_TRANSCRIBE").is_some();
        if wait {
            start(&ui);
        }
        let next = ui.clone();
        glib::timeout_add_local_once(Duration::from_millis(700), move || {
            let name = format!("{at:02}-{}{}.png", CATEGORIES.get(cat).map(|c| c.name).unwrap_or(YOUTUBE), label.map(|l| format!("-{}", l.replace(' ', "_"))).unwrap_or_default());
            let path = dir.join(name);
            // Appena cambiata la voce il disegno può non essere pronto: si riprova qualche volta.
            let mut tries = 0;
            glib::timeout_add_local(Duration::from_millis(300), move || {
                if wait && next.running.borrow().is_some() {
                    return glib::ControlFlow::Continue;
                }
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
pub(crate) fn save_png(window: &gtk4::Window, path: &std::path::Path) -> Result<(), String> {
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

/// Le immagini della modalità Semplice: vuoto, con file, con «Altre opzioni», Video dal web, Trascrivi.
#[cfg(debug_assertions)]
fn shots_simple(ui: &Rc<Ui>, dir: std::path::PathBuf) {
    type Action = Box<dyn Fn(&Rc<Ui>)>;
    let files: Vec<PathBuf> = std::env::var("CEDRO_SHOT_FILES").unwrap_or_default().split(';').filter(|p| !p.is_empty()).map(PathBuf::from).collect();
    let steps: Vec<(&'static str, Action)> = vec![
        ("00-vuoto", Box::new(|_| {})),
        ("01-con-file", Box::new(move |ui| add_files(ui, files.clone()))),
        ("02-altre-opzioni", Box::new(|ui| ui.more.emit_clicked())),
        ("03-video-dal-web", Box::new(|ui| ui.pick[1].set_active(true))),
        ("04-trascrivi", Box::new(|ui| ui.pick[2].set_active(true))),
        ("05-trascrivi-altre", Box::new(|ui| { ui.pick[0].set_active(true); ui.pick[2].set_active(true); })),
    ];
    fn step(ui: Rc<Ui>, dir: std::path::PathBuf, steps: Rc<Vec<(&'static str, Action)>>, at: usize) {
        let Some((name, action)) = steps.get(at) else {
            let app = ui.window.application();
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                if let Some(app) = app {
                    app.quit();
                }
            });
            return;
        };
        action(&ui);
        let path = dir.join(format!("{name}.png"));
        glib::timeout_add_local_once(Duration::from_millis(900), move || {
            if let Err(e) = save_png(&ui.window, &path) {
                eprintln!("screenshot: {e}");
            }
            step(ui.clone(), dir.clone(), steps.clone(), at + 1);
        });
    }
    step(ui.clone(), dir, Rc::new(steps), 0);
}
