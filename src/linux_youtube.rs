//! La scheda YouTube del pannello Conversioni (GNOME e Plasma): link, Video o Solo audio, un pulsante.
//! Le altre opzioni e il registro stanno in due riquadri chiusi.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use cedro::youtube::{self, Event, Request};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as HBox, Button, DropDown, Entry as TextEntry, Expander, FileDialog, Label, ListBox, Orientation, ProgressBar, ScrolledWindow, SelectionMode, Switch,
    TextView, ToggleButton, Window,
};

pub const CSS: &str = "\
window.cedro-panel togglebutton.cedro-big { background: #ffffff; border: 2px solid #d6dee5; padding: 14px 18px; }\
window.cedro-panel togglebutton.cedro-big:hover { border-color: #0066CC; }\
window.cedro-panel togglebutton.cedro-big:checked { background: #e3f0f8; border-color: #005E8E; }\
window.cedro-panel togglebutton.cedro-big:checked label { color: #005E8E; }\
window.cedro-panel progressbar.cedro-thin trough, window.cedro-panel progressbar.cedro-thin progress { min-height: 3px; }\
window.cedro-panel progressbar.cedro-thin progress { background: #005E8E; }\
window.cedro-panel textview.cedro-log { font-family: monospace; }";

const VIDEO_QUALITY: &[(&str, u32)] = &[("Migliore", 0), ("Fino a 1080p · consigliato", 1080), ("720p · file leggero", 720), ("480p · file piccolo", 480)];
const AUDIO_QUALITY: &[(&str, u32)] = &[("Migliore", 0), ("Buona · consigliata", 1), ("Leggera", 2)];
const VIDEO_FORMATS: &[&str] = &["mp4", "mkv", "webm"];
const AUDIO_FORMATS: &[&str] = &["mp3", "m4a", "opus", "wav", "flac"];
const SUBTITLES: &[(&str, u32)] = &[("Nessuno", 0), ("Italiano", 1), ("Italiano e inglese", 2), ("Tutti", 3)];

struct Row {
    url: String,
    title: String,
    detail: String,
    status: String,
    class: &'static str,
}

struct Page {
    window: Window,
    entry: TextEntry,
    queue: ListBox,
    count: Label,
    rows: RefCell<Vec<Row>>,
    audio: Cell<bool>,
    quality: DropDown,
    format: DropDown,
    subtitles: DropDown,
    subtitles_row: HBox,
    thumbnail: Switch,
    chapters: Switch,
    split: Switch,
    sponsors: Switch,
    normalize: Switch,
    normalize_row: HBox,
    playlist: Switch,
    skip: Switch,
    log: TextView,
    note: Label,
    dest_label: Label,
    dest: RefCell<PathBuf>,
    go: Button,
    bar: ProgressBar,
    running: RefCell<Option<Receiver<Msg>>>,
    cancel: RefCell<Arc<AtomicBool>>,
}

enum Msg {
    Event(Event),
    Finished(Result<Vec<PathBuf>, String>),
}

pub struct YoutubePage {
    pub widget: gtk4::Widget,
    pub busy: Rc<dyn Fn() -> bool>,
}

fn caption(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.set_xalign(0.0);
    l.add_css_class("cedro-cap");
    l
}

fn option_row(title: &str, control: &impl IsA<gtk4::Widget>) -> HBox {
    let row = HBox::new(Orientation::Horizontal, 12);
    let label = Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row.append(&label);
    row.append(control);
    row
}

fn switch(on: bool) -> Switch {
    let s = Switch::new();
    s.set_active(on);
    s.set_valign(gtk4::Align::Center);
    s
}

fn downloads_dir() -> PathBuf {
    glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")).unwrap_or_else(|| PathBuf::from(".")))
}

fn big(title: &str, note: &str) -> ToggleButton {
    let b = ToggleButton::new();
    b.add_css_class("cedro-big");
    b.set_hexpand(true);
    let inner = HBox::new(Orientation::Vertical, 2);
    let t = Label::new(Some(title));
    t.set_xalign(0.0);
    t.add_css_class("cedro-title");
    let n = Label::new(Some(note));
    n.set_xalign(0.0);
    n.add_css_class("cedro-muted");
    inner.append(&t);
    inner.append(&n);
    b.set_child(Some(&inner));
    b
}

fn set_choices(drop: &DropDown, labels: &[String], selected: u32) {
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    drop.set_model(Some(&gtk4::StringList::new(&refs)));
    drop.set_selected(selected);
}

/// Le scelte che cambiano tra Video e Solo audio.
fn refresh(page: &Rc<Page>) {
    let audio = page.audio.get();
    let quality: Vec<String> = if audio { AUDIO_QUALITY.iter().map(|q| q.0.to_string()).collect() } else { VIDEO_QUALITY.iter().map(|q| q.0.to_string()).collect() };
    set_choices(&page.quality, &quality, 1);
    let formats: Vec<String> = if audio { AUDIO_FORMATS } else { VIDEO_FORMATS }.iter().map(|f| f.to_uppercase()).collect();
    set_choices(&page.format, &formats, 0);
    page.subtitles_row.set_visible(!audio);
    page.normalize_row.set_visible(audio);
    label_go(page);
}

fn label_go(page: &Page) {
    if page.running.borrow().is_some() {
        page.go.set_label("Annulla");
        return;
    }
    let n = page.rows.borrow().len();
    let what = if page.audio.get() { "audio" } else { "video" };
    page.go.set_label(&match n {
        0 | 1 => format!("Scarica {what}"),
        n => format!("Scarica {n} link ({what})"),
    });
}

fn render_queue(page: &Page) {
    while let Some(child) = page.queue.first_child() {
        page.queue.remove(&child);
    }
    for row in page.rows.borrow().iter() {
        let line = HBox::new(Orientation::Horizontal, 12);
        line.set_margin_top(6);
        line.set_margin_bottom(6);
        line.set_margin_start(10);
        line.set_margin_end(10);
        let text = HBox::new(Orientation::Vertical, 0);
        text.set_hexpand(true);
        let title = Label::new(Some(if row.title.is_empty() { &row.url } else { &row.title }));
        title.set_xalign(0.0);
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        text.append(&title);
        if !row.detail.is_empty() {
            let d = Label::new(Some(&row.detail));
            d.set_xalign(0.0);
            d.add_css_class("cedro-muted");
            d.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            text.append(&d);
        }
        let status = Label::new(Some(&row.status));
        if !row.class.is_empty() {
            status.add_css_class(row.class);
        }
        status.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        status.set_max_width_chars(34);
        line.append(&text);
        line.append(&status);
        page.queue.append(&line);
    }
    let n = page.rows.borrow().len();
    page.count.set_text(&if n == 0 { "CODA · nessun link".to_string() } else { format!("CODA · {n} link") });
    label_go(page);
}

fn log(page: &Page, text: &str) {
    let buffer = page.log.buffer();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, &format!("{}  {text}\n", glib::DateTime::now_local().ok().and_then(|d| d.format("%H:%M:%S").ok()).map(|s| s.to_string()).unwrap_or_default()));
    let mut end = buffer.end_iter();
    page.log.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
}

fn add_links(page: &Rc<Page>, text: &str) -> usize {
    let mut added = 0;
    for url in youtube::urls_in(text) {
        if page.rows.borrow().iter().any(|r| r.url == url) {
            continue;
        }
        page.rows.borrow_mut().push(Row { url, title: String::new(), detail: String::new(), status: "In attesa".into(), class: "" });
        added += 1;
    }
    render_queue(page);
    added
}

fn request(page: &Page) -> Request {
    let audio = page.audio.get();
    let mut r = Request::new(page.rows.borrow().iter().map(|r| r.url.clone()).collect(), audio, page.dest.borrow().clone());
    let q = page.quality.selected() as usize;
    r.quality = if audio { AUDIO_QUALITY.get(q).map(|x| x.1).unwrap_or(1) } else { VIDEO_QUALITY.get(q).map(|x| x.1).unwrap_or(1080) };
    let formats = if audio { AUDIO_FORMATS } else { VIDEO_FORMATS };
    r.format = formats.get(page.format.selected() as usize).copied().unwrap_or(formats[0]).to_string();
    r.subtitles = if audio { 0 } else { SUBTITLES.get(page.subtitles.selected() as usize).map(|x| x.1).unwrap_or(0) };
    r.thumbnail = page.thumbnail.is_active();
    r.chapters = page.chapters.is_active();
    r.split_chapters = page.split.is_active();
    r.skip_sponsors = page.sponsors.is_active();
    r.normalize = audio && page.normalize.is_active();
    r.playlist = page.playlist.is_active();
    r.skip_existing = page.skip.is_active();
    r
}

fn start(page: &Rc<Page>) {
    if let Some(_running) = page.running.borrow().as_ref() {
        page.cancel.borrow().store(true, Ordering::Relaxed);
        page.go.set_sensitive(false);
        page.go.set_label("Annullo…");
        return;
    }
    // Un link incollato ma non ancora aggiunto vale come aggiunto.
    let typed = page.entry.text().to_string();
    if !typed.trim().is_empty() && add_links(page, &typed) > 0 {
        page.entry.set_text("");
    }
    if page.rows.borrow().is_empty() {
        page.note.set_text("Incolla prima un link di YouTube.");
        return;
    }
    let req = request(page);
    for row in page.rows.borrow_mut().iter_mut() {
        row.status = "In attesa".into();
        row.class = "";
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *page.cancel.borrow_mut() = cancel.clone();
    let (tx, rx) = channel();
    *page.running.borrow_mut() = Some(rx);
    page.bar.set_visible(true);
    page.bar.set_fraction(0.0);
    let tx2 = tx.clone();
    std::thread::spawn(move || {
        let result = youtube::run(&req, &mut |e| drop(tx2.send(Msg::Event(e))), &cancel);
        let _ = tx.send(Msg::Finished(result));
    });
    render_queue(page);
    let page = page.clone();
    glib::timeout_add_local(Duration::from_millis(120), move || {
        let mut changed = false;
        let mut finished = false;
        if let Some(rx) = page.running.borrow().as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(Msg::Event(e)) => {
                        changed |= handle(&page, e);
                    }
                    Ok(Msg::Finished(result)) => {
                        finished = true;
                        match result {
                            Ok(files) => {
                                page.note.set_text(&format!("Fatto: {} file in {}", files.len(), page.dest.borrow().display()));
                                log(&page, "Finito");
                            }
                            Err(e) => {
                                page.note.set_text(&e);
                                log(&page, &e);
                            }
                        }
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        if finished {
            page.running.borrow_mut().take();
            page.go.set_sensitive(true);
            page.bar.set_visible(false);
            changed = true;
        }
        if changed {
            render_queue(&page);
        }
        if finished {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// Applica un evento del motore alla scheda; vero se la coda va ridisegnata.
fn handle(page: &Rc<Page>, event: Event) -> bool {
    match event {
        Event::Log(text) => {
            log(page, &text);
            false
        }
        Event::Progress { index, total, percent, label } => {
            let whole = (index as f64 + percent as f64 / 100.0) / total.max(1) as f64;
            page.bar.set_fraction(whole.clamp(0.0, 1.0));
            if let Some(r) = page.rows.borrow_mut().get_mut(index) {
                r.status = format!("{percent:.0}%");
                r.class = "";
            }
            page.note.set_text(&label);
            true
        }
        Event::File(_) => false,
        Event::Info { index, info } => {
            if let Some(r) = page.rows.borrow_mut().get_mut(index) {
                r.title = info.title;
                r.detail = match info.entries {
                    Some(n) => format!("{} · playlist di {n} video", info.author),
                    None => format!("{} · {}", info.author, youtube::clock(info.seconds)),
                };
            }
            true
        }
        Event::LinkDone { index, result } => {
            if let Some(r) = page.rows.borrow_mut().get_mut(index) {
                match result {
                    Ok(n) => (r.status, r.class) = (if n > 1 { format!("Fatto · {n} file") } else { "Fatto".into() }, "cedro-ok"),
                    Err(e) => (r.status, r.class) = (format!("Errore · {e}"), "cedro-fail"),
                }
            }
            true
        }
    }
}

pub fn build(window: &Window) -> YoutubePage {
    let entry = TextEntry::new();
    entry.set_placeholder_text(Some("Incolla qui il link di YouTube"));
    entry.set_hexpand(true);
    let add = Button::with_label("Aggiungi");
    add.add_css_class("suggested-action");
    let link_row = HBox::new(Orientation::Horizontal, 8);
    link_row.append(&entry);
    link_row.append(&add);

    let queue = ListBox::new();
    queue.set_selection_mode(SelectionMode::None);
    let queue_scroll = ScrolledWindow::builder().child(&queue).min_content_height(90).vexpand(true).has_frame(true).build();
    let count = caption("CODA · nessun link");

    let video_btn = big("Video", "Immagini e suono, come su YouTube");
    let audio_btn = big("Solo audio", "Per ascoltare, senza immagini");
    audio_btn.set_group(Some(&video_btn));
    video_btn.set_active(true);
    let choose = HBox::new(Orientation::Horizontal, 12);
    choose.set_homogeneous(true);
    choose.append(&video_btn);
    choose.append(&audio_btn);

    let quality = DropDown::from_strings(&[]);
    let format = DropDown::from_strings(&[]);
    let subtitles = DropDown::from_strings(&SUBTITLES.iter().map(|s| s.0).collect::<Vec<_>>());
    let thumbnail = switch(true);
    let chapters = switch(true);
    let split = switch(false);
    let sponsors = switch(false);
    let normalize = switch(false);
    let playlist = switch(true);
    let skip = switch(true);
    let subtitles_row = option_row("Sottotitoli", &subtitles);
    let normalize_row = option_row("Livella il volume", &normalize);
    let more = HBox::new(Orientation::Vertical, 8);
    more.set_margin_top(8);
    more.set_margin_bottom(8);
    for w in [
        option_row("Qualità", &quality),
        option_row("Salva come", &format),
        subtitles_row.clone(),
        option_row("Copertina / miniatura nel file", &thumbnail),
        option_row("Capitoli e descrizione nel file", &chapters),
        option_row("Dividi per capitoli (un file ciascuno)", &split),
        option_row("Salta le parti sponsorizzate", &sponsors),
        normalize_row.clone(),
        option_row("Playlist: scarica tutta", &playlist),
        option_row("Salta i file già scaricati", &skip),
    ] {
        more.append(&w);
    }
    let more_box = Expander::new(Some("Altre opzioni"));
    more_box.set_child(Some(&more));

    let log_view = TextView::new();
    log_view.set_editable(false);
    log_view.set_cursor_visible(false);
    log_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    log_view.add_css_class("cedro-log");
    let log_scroll = ScrolledWindow::builder().child(&log_view).min_content_height(140).has_frame(true).build();
    let log_box = Expander::new(Some("Cosa sta succedendo"));
    log_box.set_child(Some(&log_scroll));

    let note = Label::new(Some("Al primo uso Cedro scarica da solo due piccoli programmi di aiuto (circa 120 MB, una volta sola) e poi li tiene aggiornati."));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("cedro-muted");

    let dest_label = Label::new(None);
    dest_label.set_xalign(0.0);
    dest_label.set_hexpand(true);
    dest_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    let change = Button::with_label("Cambia…");
    let go = Button::with_label("Scarica video");
    go.add_css_class("suggested-action");
    let dest_row = HBox::new(Orientation::Horizontal, 8);
    dest_row.append(&dest_label);
    dest_row.append(&change);
    dest_row.append(&go);

    let bar = ProgressBar::new();
    bar.add_css_class("cedro-thin");
    bar.set_visible(false);

    let top = HBox::new(Orientation::Vertical, 10);
    top.set_margin_top(16);
    top.set_margin_bottom(12);
    top.set_margin_start(20);
    top.set_margin_end(20);
    top.set_hexpand(true);
    let title = Label::new(Some("YouTube"));
    title.set_xalign(0.0);
    title.add_css_class("cedro-title");
    let hint = Label::new(Some("Incolla un link e scarica il video oppure solo l'audio."));
    hint.set_xalign(0.0);
    hint.add_css_class("cedro-muted");
    let signature = Label::new(Some("Powered by Luca \"Realhcok\" A."));
    signature.set_xalign(1.0);
    signature.add_css_class("cedro-muted");
    for w in [
        title.upcast_ref::<gtk4::Widget>(),
        hint.upcast_ref(),
        link_row.upcast_ref(),
        count.upcast_ref(),
        queue_scroll.upcast_ref(),
        caption("COSA VUOI SCARICARE").upcast_ref(),
        choose.upcast_ref(),
        more_box.upcast_ref(),
        log_box.upcast_ref(),
        note.upcast_ref(),
        caption("DOVE SALVARE").upcast_ref(),
        dest_row.upcast_ref(),
        bar.upcast_ref(),
        signature.upcast_ref(),
    ] {
        top.append(w);
    }

    let page = Rc::new(Page {
        window: window.clone(),
        entry: entry.clone(),
        queue,
        count,
        rows: RefCell::new(Vec::new()),
        audio: Cell::new(false),
        quality,
        format,
        subtitles,
        subtitles_row,
        thumbnail,
        chapters,
        split,
        sponsors,
        normalize,
        normalize_row,
        playlist,
        skip,
        log: log_view,
        note,
        dest_label: dest_label.clone(),
        dest: RefCell::new(downloads_dir()),
        go: go.clone(),
        bar,
        running: RefCell::new(None),
        cancel: RefCell::new(Arc::new(AtomicBool::new(false))),
    });
    dest_label.set_text(&page.dest.borrow().display().to_string());
    refresh(&page);
    render_queue(&page);

    {
        let page = page.clone();
        audio_btn.connect_toggled(move |b| {
            page.audio.set(b.is_active());
            refresh(&page);
        });
    }
    {
        let page = page.clone();
        add.connect_clicked(move |_| {
            let text = page.entry.text().to_string();
            if add_links(&page, &text) > 0 {
                page.entry.set_text("");
                page.note.set_text("");
            } else {
                page.note.set_text("Questo non sembra un link di YouTube.");
            }
        });
    }
    {
        let page = page.clone();
        entry.connect_activate(move |_| {
            let text = page.entry.text().to_string();
            if add_links(&page, &text) > 0 {
                page.entry.set_text("");
            }
        });
    }
    {
        let page = page.clone();
        change.connect_clicked(move |_| {
            let dialog = FileDialog::new();
            dialog.set_title("Cartella di destinazione");
            let page2 = page.clone();
            dialog.select_folder(Some(&page.window), gio::Cancellable::NONE, move |res| {
                if let Some(path) = res.ok().and_then(|f| f.path()) {
                    page2.dest_label.set_text(&path.display().to_string());
                    *page2.dest.borrow_mut() = path;
                }
            });
        });
    }
    {
        let page = page.clone();
        go.connect_clicked(move |_| start(&page));
    }

    let busy_page = page.clone();
    YoutubePage { widget: top.upcast(), busy: Rc::new(move || busy_page.running.borrow().is_some()) }
}
