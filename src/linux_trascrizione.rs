//! La scheda «Trascrizione» della categoria Audio nel pannello Conversioni su GNOME e Plasma:
//! «Salva come», «Precisione» e la finestra «Cosa sta succedendo» a destra, come su Windows.
//! Il lavoro vero lo fa `cedro::trascrizione`; qui ci sono i controlli e il registro colorato.

use std::cell::Cell;
use std::rc::Rc;

use cedro::trascrizione::{self, Format, Line, Tone, MODELS};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box as HBox, Button, FlowBox, Label, Orientation, ScrolledWindow, TextBuffer, TextView, ToggleButton};

pub const CSS: &str = "\
window.cedro-panel button.cedro-tab { background: #ffffff; background-image: none; border: none; border-bottom: 1px solid #E5E5E5; border-radius: 0; box-shadow: none; padding: 7px 22px; }\
window.cedro-panel button.cedro-tab label { color: #455A64; font-weight: bold; }\
window.cedro-panel button.cedro-tab:checked { border-bottom: 3px solid #005E8E; }\
window.cedro-panel button.cedro-tab:checked label { color: #005E8E; }\
window.cedro-panel button.cedro-model { background: #ffffff; background-image: none; border: 1px solid #E5E5E5; border-radius: 4px; box-shadow: none; padding: 6px 12px; }\
window.cedro-panel button.cedro-model:hover { border-color: #0066CC; }\
window.cedro-panel button.cedro-model:checked { background: #E6EEFF; border: 2px solid #005E8E; padding: 5px 11px; }\
window.cedro-panel label.cedro-ready { color: #1d7a46; font-size: 90%; }\
window.cedro-panel label.cedro-fetch { color: #a15c00; font-size: 90%; }\
window.cedro-panel textview.cedro-log { background: #0d1b24; color: #cfe3f0; font-family: monospace; font-size: 90%; padding: 6px; }\
window.cedro-panel textview.cedro-log text { background: #0d1b24; color: #cfe3f0; }";

/// Il testo sopra la coda quando si vede la trascrizione.
pub const HINT: &str = "Voce in testo, sul tuo PC. Accetta: MP3, WAV, M4A, OGG, OPUS, FLAC, WMA, AMR e video.";

const FORMATS: [(Format, &str); 2] = [(Format::Word, "Word"), (Format::Text, "Testo semplice")];

/// Il colore di ogni tono nella finestra, come nell'anteprima.
fn colors(tone: Tone) -> (&'static str, &'static str) {
    match tone {
        Tone::Step => ("#7fc4ee", "#cfe3f0"),
        Tone::Speech => ("#7fdca5", "#cfe3f0"),
        Tone::Note => ("#f0c36d", "#cfe3f0"),
        Tone::Error => ("#ff8080", "#ff8080"),
        Tone::Muted => ("#7d93a3", "#7d93a3"),
    }
}

/// Cosa è cambiato nella scheda.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// «Cambia formato» o «Trascrizione».
    Tab,
    /// Word o testo semplice: il pulsante in fondo lo scrive.
    Format,
    /// Un altro modello: la finestra dice se si scarica.
    Model,
}

/// Le scelte e i pezzi della scheda che gli altri devono poter toccare.
pub struct Panel {
    /// «Cambia formato» e «Trascrizione», sopra le opzioni: solo con la categoria Audio.
    pub tabs: HBox,
    pub convert_tab: ToggleButton,
    pub transcribe_tab: ToggleButton,
    /// Salva come, Precisione e il registro.
    pub body: HBox,
    format: Cell<Format>,
    model: Cell<usize>,
    log: TextBuffer,
    view: TextView,
    cards: Vec<(Label, ToggleButton)>,
}

fn caption(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.set_xalign(0.0);
    l.add_css_class("cedro-cap");
    l
}

impl Panel {
    /// `on_change` si chiama quando cambia la scheda, il formato o il modello.
    pub fn build(on_change: impl Fn(Change) + Clone + 'static) -> Rc<Panel> {
        let convert_tab = ToggleButton::with_label("Cambia formato");
        let transcribe_tab = ToggleButton::with_label("Trascrizione");
        transcribe_tab.set_group(Some(&convert_tab));
        convert_tab.set_active(true);
        for tab in [&convert_tab, &transcribe_tab] {
            tab.add_css_class("cedro-tab");
        }
        let tabs = HBox::new(Orientation::Horizontal, 4);
        tabs.append(&convert_tab);
        tabs.append(&transcribe_tab);
        tabs.set_visible(false);

        // Sinistra: Salva come e Precisione.
        let save_as = FlowBox::new();
        save_as.set_selection_mode(gtk4::SelectionMode::None);
        save_as.set_homogeneous(false);
        let mut lead: Option<ToggleButton> = None;
        let mut format_buttons = Vec::new();
        for (_, label) in FORMATS {
            let chip = ToggleButton::with_label(label);
            chip.add_css_class("cedro-chip");
            if let Some(first) = &lead {
                chip.set_group(Some(first));
            } else {
                lead = Some(chip.clone());
            }
            save_as.insert(&chip, -1);
            format_buttons.push(chip);
        }
        format_buttons[0].set_active(true);

        let models = HBox::new(Orientation::Vertical, 6);
        let mut cards = Vec::new();
        let mut first_card: Option<ToggleButton> = None;
        for (i, m) in MODELS.iter().enumerate() {
            let button = ToggleButton::new();
            button.add_css_class("cedro-model");
            if let Some(first) = &first_card {
                button.set_group(Some(first));
            } else {
                first_card = Some(button.clone());
            }
            let top = HBox::new(Orientation::Horizontal, 8);
            let name = Label::new(Some(m.name));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.add_css_class("cedro-rt");
            let status = Label::new(None);
            top.append(&name);
            top.append(&status);
            let note = Label::new(Some(&format!("{} · {}", m.note, trascrizione::size_text(m.megabytes))));
            note.set_xalign(0.0);
            note.add_css_class("cedro-muted");
            let card = HBox::new(Orientation::Vertical, 2);
            card.append(&top);
            card.append(&note);
            button.set_child(Some(&card));
            models.append(&button);
            if i == trascrizione::DEFAULT_MODEL {
                button.set_active(true);
            }
            cards.push((status, button));
        }
        let left = HBox::new(Orientation::Vertical, 6);
        left.set_size_request(330, -1);
        left.append(&caption("SALVA COME"));
        left.append(&save_as);
        let precision = caption("PRECISIONE");
        precision.set_margin_top(8);
        left.append(&precision);
        left.append(&models);

        // Destra: la finestra dei messaggi.
        let buffer = TextBuffer::new(None);
        for tone in [Tone::Step, Tone::Speech, Tone::Note, Tone::Error, Tone::Muted] {
            let (key, text) = colors(tone);
            for (suffix, color) in [("k", key), ("t", text)] {
                buffer.tag_table().add(&gtk4::TextTag::builder().name(format!("{tone:?}-{suffix}")).foreground(color).build());
            }
        }
        buffer.tag_table().add(&gtk4::TextTag::builder().name("stamp").foreground("#7d93a3").build());
        let view = TextView::with_buffer(&buffer);
        view.set_editable(false);
        view.set_cursor_visible(false);
        view.set_wrap_mode(gtk4::WrapMode::WordChar);
        view.add_css_class("cedro-log");
        let scroll = ScrolledWindow::builder().child(&view).min_content_height(220).vexpand(true).hexpand(true).build();
        let copy = Button::with_label("Copia");
        let log_head = HBox::new(Orientation::Horizontal, 8);
        let log_caption = caption("COSA STA SUCCEDENDO");
        log_caption.set_hexpand(true);
        log_head.append(&log_caption);
        log_head.append(&copy);
        let right = HBox::new(Orientation::Vertical, 6);
        right.set_hexpand(true);
        right.append(&log_head);
        right.append(&scroll);

        let body = HBox::new(Orientation::Horizontal, 18);
        body.append(&left);
        body.append(&right);
        body.set_visible(false);

        let panel = Rc::new(Panel {
            tabs,
            convert_tab: convert_tab.clone(),
            transcribe_tab: transcribe_tab.clone(),
            body,
            format: Cell::new(Format::Word),
            model: Cell::new(trascrizione::DEFAULT_MODEL),
            log: buffer,
            view,
            cards,
        });
        panel.refresh_models();
        for (i, chip) in format_buttons.iter().enumerate() {
            let (panel, change) = (panel.clone(), on_change.clone());
            chip.connect_toggled(move |c| {
                if c.is_active() {
                    panel.format.set(FORMATS[i].0);
                    change(Change::Format);
                }
            });
        }
        for (i, (_, button)) in panel.cards.iter().enumerate() {
            let (p, change) = (panel.clone(), on_change.clone());
            button.connect_toggled(move |b| {
                if b.is_active() {
                    p.model.set(i);
                    change(Change::Model);
                }
            });
        }
        {
            let change = on_change.clone();
            transcribe_tab.connect_toggled(move |_| change(Change::Tab));
        }
        {
            let p = panel.clone();
            copy.connect_clicked(move |b| {
                let (start, end) = p.log.bounds();
                b.clipboard().set_text(&p.log.text(&start, &end, false));
            });
        }
        panel
    }

    /// Vero se è scelta la scheda «Trascrizione».
    pub fn selected(&self) -> bool {
        self.transcribe_tab.is_active()
    }

    pub fn select(&self, transcription: bool) {
        if transcription {
            self.transcribe_tab.set_active(true);
        } else {
            self.convert_tab.set_active(true);
        }
    }

    pub fn format(&self) -> Format {
        self.format.get()
    }

    pub fn model(&self) -> usize {
        self.model.get()
    }

    /// «Sul PC» o «Si scarica la prima volta» su ogni scheda di modello.
    pub fn refresh_models(&self) {
        for (i, (status, _)) in self.cards.iter().enumerate() {
            let ready = trascrizione::model_ready(i);
            status.set_text(if ready { "Sul PC" } else { "Si scarica la prima volta" });
            status.remove_css_class("cedro-ready");
            status.remove_css_class("cedro-fetch");
            status.add_css_class(if ready { "cedro-ready" } else { "cedro-fetch" });
        }
    }

    /// Il testo di quando non lavora: cosa succederà premendo il pulsante.
    pub fn idle_text(&self) {
        self.log.set_text("");
        let model = &MODELS[self.model.get()];
        let first = if trascrizione::model_ready(self.model.get()) {
            format!("Il modello «{}» è già sul PC.", model.name)
        } else {
            format!("Alla prima trascrizione Cedro scarica il motore e il modello «{}» ({}).", model.name, trascrizione::size_text(model.megabytes))
        };
        for text in [first.as_str(), "Qui vedrai i passi e le frasi man mano che le scrive. A fine lavoro il motore si spegne e libera la memoria."] {
            self.append(&Line { tone: Tone::Muted, key: String::new(), text: text.to_string() }, false);
        }
    }

    /// Svuota la finestra: sta per partire una trascrizione.
    pub fn clear(&self) {
        self.log.set_text("");
    }

    /// Aggiunge una riga in fondo, con l'ora davanti, e scorre fino a lei.
    pub fn append(&self, line: &Line, stamp: bool) {
        let mut end = self.log.end_iter();
        if stamp {
            let now = glib::DateTime::now_local().ok().and_then(|t| t.format("%H:%M:%S ").ok()).map(|s| s.to_string()).unwrap_or_default();
            self.log.insert_with_tags_by_name(&mut end, &now, &["stamp"]);
        }
        if !line.key.is_empty() {
            self.log.insert_with_tags_by_name(&mut end, &format!("{} ", line.key), &[&format!("{:?}-k", line.tone)]);
        }
        self.log.insert_with_tags_by_name(&mut end, &format!("{}\n", line.text), &[&format!("{:?}-t", line.tone)]);
        let mark = self.log.create_mark(None, &self.log.end_iter(), false);
        self.view.scroll_to_mark(&mark, 0.0, false, 0.0, 1.0);
        self.log.delete_mark(&mark);
    }
}
