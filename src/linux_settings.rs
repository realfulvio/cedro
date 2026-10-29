//! Finestre Impostazioni e Guida su GNOME e Plasma, con i colori del Comune di Bondeno.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use cedro::guida;
use cedro::wheelgeom::WHEEL;
use cedro::wheelgfx;
use cedro::{append_log, read_settings, write_settings, Settings};
use gtk4::cairo::{Context, FontSlant, FontWeight, ImageSurface};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, Box as HBox, Button, CheckButton, CssProvider, DrawingArea, HeaderBar, Label, Orientation,
    Scale, Window, STYLE_PROVIDER_PRIORITY_APPLICATION,
};


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
window.cedro-panel box.cedro-side { background: #F3F5F7; border-right: 1px solid #E5E5E5; }\
window.cedro-panel box.cedro-side list.cedro-nav { border-right: none; padding-top: 4px; }\
window.cedro-panel box.cedro-ente { margin: 14px 14px 4px; padding-bottom: 12px; border-bottom: 1px solid #E5E5E5; }\
window.cedro-panel label.cedro-ente-nome { color: #005E8E; font-weight: bold; }\
window.cedro-panel box.cedro-ente-about { background: #F2F7FB; border: 1px solid #D4E3EE; border-radius: 6px; padding: 8px 12px; margin-top: 14px; }\
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
window.cedro-panel switch.cedro-sw label { font-size: 0; min-width: 0; }\
window.cedro-panel label.cedro-cap { color: #455A64; font-weight: bold; font-size: 85%; }\
window.cedro-panel label.cedro-isa { font-family: monospace; font-weight: bold; padding: 3px 10px; border-radius: 12px; background: #E6F2EA; color: #1B5E20; }\
window.cedro-panel label.cedro-isa-off { background: #F3F5F7; color: #8A96A0; }\
window.cedro-panel label.cedro-tag { font-weight: bold; font-size: 90%; padding: 2px 9px; border-radius: 4px; background: #E6F2EA; color: #1B5E20; }\
window.cedro-panel label.cedro-tag-off { background: #F3F5F7; color: #455A64; }\
window.cedro-panel switch.cedro-sw:disabled { opacity: 0.45; }\
window.cedro-panel entry.cedro-search { background: #ffffff; border: 1px solid #d6dee5; border-radius: 6px; padding: 5px 10px; box-shadow: none; }\
window.cedro-panel list.cedro-credits { background: transparent; }\
window.cedro-panel list.cedro-credits row { background: transparent; border-bottom: 1px solid #E5E5E5; padding: 0; }\
window.cedro-panel label.cedro-license { font-family: monospace; font-size: 80%; background: #F3F5F7; border-radius: 5px; padding: 8px; color: #455A64; }\
window.cedro-panel label.cedro-soon { font-size: 75%; font-weight: bold; padding: 1px 6px; border-radius: 4px; background: #E6EEFF; color: #005E8E; }";

/// Il foglio di stile nei colori del tema: quelli scritti sono del Comune di Bondeno.
fn css() -> String {
    let t = cedro::tema::attivo();
    if cedro::edizione::bondeno() {
        return CSS.to_string();
    }
    let hex = |c: (u8, u8, u8)| format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2);
    CSS.replace("#005E8E", &hex(t.acc)).replace("#455A64", &hex(t.mute)).replace("#F3F5F7", &hex(t.surf)).replace("#E5E5E5", &hex(t.line)).replace("#191919", &hex(t.fg))
}

fn style() {
    let Some(display) = gtk4::gdk::Display::default() else {
        return;
    };
    let provider = CssProvider::new();
    provider.load_from_string(&css());
    gtk4::style_context_add_provider_for_display(&display, &provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
    // Il carattere segue il tema (Titillium Web con quello del Comune).
    crate::linux_font::apply();
}

pub(crate) fn titled(app: &Application, title: &str, width: i32, height: i32) -> Window {
    style();
    let window = Window::builder()
        .application(app)
        .title(t(title))
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

/// Il testo nella lingua dell'interfaccia.
fn t(s: &str) -> String {
    cedro::i18n::tr(s).into_owned()
}

const DPI: [u32; 4] = [100, 144, 200, 300];
const MAX_PAGES: [u32; 4] = [50, 100, 300, 0];
const LANGS: [&str; 3] = ["auto", "it", "en"];
/// Le scelte della lingua dell'interfaccia, nell'ordine dell'elenco.
const UI_LANGS: [&str; 3] = ["auto", "it", "en"];
const SECTIONS: [&str; 8] = ["Generale", "Ruota", "Salvataggio", "Qualità", "OCR", "Aggiornamenti", "Sistema", "Informazioni"];
const MARK: &[u8] = cedro::edizione::ICONA;
const POWERED_BY: &str = "Powered by Luca \"Realhcok\" A.";

/// Una riga di impostazione: titolo e descrizione a sinistra, il controllo a destra.
fn setting(title: &str, desc: &str, control: &impl IsA<gtk4::Widget>) -> HBox {
    let text = HBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk4::Align::Center);
    let head = Label::new(Some(&t(title)));
    head.set_xalign(0.0);
    head.add_css_class("cedro-rt");
    let d = Label::new(Some(&t(desc)));
    d.set_xalign(0.0);
    d.set_wrap(true);
    d.add_css_class("cedro-muted");
    text.append(&head);
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
    let heading = Label::new(Some(&t(title)));
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
    let shown: Vec<String> = items.iter().map(|i| t(i)).collect();
    let shown: Vec<&str> = shown.iter().map(String::as_str).collect();
    let dd = gtk4::DropDown::from_strings(&shown);
    dd.set_selected(selected as u32);
    dd.set_size_request(190, -1);
    dd
}

fn short_path(path: &str) -> String {
    if path.is_empty() { "Nessuna cartella scelta".into() } else { path.to_string() }
}

fn caption(text: &str) -> Label {
    let l = Label::new(Some(&t(text)));
    l.set_xalign(0.0);
    l.set_margin_top(14);
    l.set_margin_bottom(6);
    l.add_css_class("cedro-cap");
    l
}

/// Come `setting`, con sotto la descrizione l'etichetta di quello che Cedro ha trovato.
fn setting_tagged(title: &str, desc: &str, found: &str, available: bool, control: &impl IsA<gtk4::Widget>) -> HBox {
    let row = setting(title, desc, control);
    if let Some(text) = row.first_child().and_then(|w| w.downcast::<HBox>().ok()) {
        let tag = Label::new(Some(&t(found)));
        tag.set_halign(gtk4::Align::Start);
        tag.set_margin_top(4);
        tag.add_css_class("cedro-tag");
        if !available {
            tag.add_css_class("cedro-tag-off");
        }
        text.append(&tag);
    }
    row
}

/// Scheda «Sistema»: com'è fatto il PC, le istruzioni del processore e le due accelerazioni.
fn system_page(current: &Settings, window: &Window) -> (HBox, gtk4::Switch, gtk4::Switch) {
    let info = cedro::sysinfo::info();
    let hw = cedro::accel::hardware();
    let page = HBox::new(Orientation::Vertical, 0);
    page.set_margin_top(18);
    page.set_margin_start(28);
    page.set_margin_end(28);
    let heading = Label::new(Some(&t("Sistema")));
    heading.set_xalign(0.0);
    heading.add_css_class("cedro-h");
    page.append(&heading);

    page.append(&caption("QUESTO PC"));
    let grid = gtk4::Grid::new();
    grid.set_row_spacing(4);
    grid.set_column_spacing(16);
    let mut row = 0;
    for (label, value, detail) in [
        ("Processore", info.cpu.as_str(), info.cpu_detail.as_str()),
        ("Memoria", info.ram.as_str(), ""),
        ("Scheda video", info.gpu.as_str(), info.gpu_detail.as_str()),
        ("Sistema", info.os.as_str(), ""),
    ] {
        let name = Label::new(Some(&t(label)));
        name.set_xalign(0.0);
        name.set_valign(gtk4::Align::Start);
        name.set_width_chars(13);
        name.add_css_class("cedro-muted");
        let text = HBox::new(Orientation::Vertical, 0);
        let value = Label::new(Some(&t(value)));
        value.set_xalign(0.0);
        value.set_wrap(true);
        value.set_max_width_chars(50);
        value.set_selectable(true);
        value.add_css_class("cedro-rt");
        text.append(&value);
        if !detail.is_empty() {
            let d = Label::new(Some(&t(detail)));
            d.set_xalign(0.0);
            d.set_wrap(true);
            d.set_max_width_chars(60);
            d.add_css_class("cedro-muted");
            text.append(&d);
        }
        text.set_hexpand(true);
        grid.attach(&name, 0, row, 1, 1);
        grid.attach(&text, 1, row, 1, 1);
        row += 1;
    }
    page.append(&grid);

    page.append(&caption("ISTRUZIONI DEL PROCESSORE"));
    let chips = gtk4::FlowBox::new();
    chips.set_selection_mode(gtk4::SelectionMode::None);
    chips.set_max_children_per_line(8);
    chips.set_column_spacing(6);
    chips.set_row_spacing(6);
    chips.set_homogeneous(false);
    for (name, yes) in &info.isa {
        let chip = Label::new(Some(&t(&format!("{} {name}", if *yes { "✓" } else { "✗" }))));
        chip.add_css_class("cedro-isa");
        if !*yes {
            chip.add_css_class("cedro-isa-off");
        }
        chip.set_halign(gtk4::Align::Start);
        chips.insert(&chip, -1);
    }
    if info.isa.is_empty() {
        let none = Label::new(Some(&t("Non rilevate su questo processore.")));
        none.set_xalign(0.0);
        none.add_css_class("cedro-muted");
        page.append(&none);
    } else {
        page.append(&chips);
    }

    page.append(&caption("ACCELERAZIONI"));
    let gpu = switch(current.gpu && hw.gpu.is_some());
    gpu.set_sensitive(hw.gpu.is_some());
    let avx = switch(current.avx512 && hw.avx512);
    avx.set_sensitive(hw.avx512);
    let gpu_row = setting_tagged(
        "Accelerazione GPU",
        "Video in MP4 con la scheda video o la grafica integrata. Se la scheda rifiuta un video, Cedro lo rifà col processore.",
        &match &hw.gpu {
            Some(found) => format!("Trovata: {found}"),
            None => "Non disponibile su questo PC".to_string(),
        },
        hw.gpu.is_some(),
        &gpu,
    );
    let avx_row = setting_tagged(
        "Accelerazione AVX-512",
        "Calcoli più veloci per OCR e trascrizione dell'audio.",
        if hw.avx512 { "Trovato: AVX-512 F, VL, BW, DQ" } else { "Il processore non ha AVX-512" },
        hw.avx512,
        &avx,
    );
    page.append(&gpu_row);
    page.append(&gtk4::Separator::new(Orientation::Horizontal));
    page.append(&avx_row);

    let copy = Button::with_label(&t("Copia informazioni"));
    copy.add_css_class("cedro-sec");
    let copied = Label::new(Some(&t("Letto all'avvio di Cedro.")));
    copied.add_css_class("cedro-muted");
    copied.set_xalign(0.0);
    let line = HBox::new(Orientation::Horizontal, 12);
    line.set_margin_top(12);
    line.append(&copy);
    line.append(&copied);
    page.append(&line);
    {
        let copied = copied.clone();
        let window = window.clone();
        copy.connect_clicked(move |_| {
            window.clipboard().set_text(&cedro::sysinfo::report());
            copied.set_text(&t("Copiato: incollalo nella segnalazione."));
        });
    }
    (page, gpu, avx)
}

/// Una voce dell'elenco dei componenti: nome, autore, a cosa serve; un clic mostra la licenza.
fn credit_row(item: &cedro::credits::Component) -> gtk4::ListBoxRow {
    let expander = gtk4::Expander::new(None);
    let head = HBox::new(Orientation::Vertical, 1);
    let top = HBox::new(Orientation::Horizontal, 8);
    let name = Label::new(Some(&t(item.name)));
    name.add_css_class("cedro-rt");
    name.set_xalign(0.0);
    top.append(&name);
    if item.soon {
        let soon = Label::new(Some(&t("in arrivo")));
        soon.add_css_class("cedro-soon");
        top.append(&soon);
    }
    let by = Label::new(Some(&format!("{} · {}", item.author, item.license)));
    by.set_xalign(0.0);
    by.add_css_class("cedro-muted");
    let what = Label::new(Some(&t(item.what)));
    what.set_xalign(0.0);
    what.set_wrap(true);
    what.set_max_width_chars(60);
    head.append(&top);
    head.append(&what);
    head.append(&by);
    head.set_margin_top(6);
    head.set_margin_bottom(6);
    expander.set_label_widget(Some(&head));
    let body = HBox::new(Orientation::Vertical, 6);
    body.set_margin_bottom(8);
    let license = Label::new(Some(cedro::credits::license_text(item.license)));
    license.set_wrap(true);
    license.set_max_width_chars(70);
    license.set_xalign(0.0);
    license.set_selectable(true);
    license.add_css_class("cedro-license");
    let site = gtk4::LinkButton::with_label(&format!("https://{}", item.url), item.url);
    site.set_halign(gtk4::Align::Start);
    body.append(&license);
    body.append(&site);
    expander.set_child(Some(&body));
    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&expander));
    row.set_activatable(false);
    row.set_selectable(false);
    row
}

fn credit_matches(item: &cedro::credits::Component, query: &str) -> bool {
    query.is_empty() || [item.name, item.author, item.license, item.what].iter().any(|t| t.to_lowercase().contains(query))
}

/// Riempie l'elenco con i componenti che contengono `query`; restituisce quanti sono.
fn fill_credits(list: &gtk4::ListBox, query: &str) -> usize {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let query = query.trim().to_lowercase();
    let mut count = 0;
    for (title, items) in cedro::credits::visible() {
        let found: Vec<_> = items.into_iter().filter(|i| credit_matches(i, &query)).collect();
        if found.is_empty() {
            continue;
        }
        let group = Label::new(Some(&t(title).to_uppercase()));
        group.set_xalign(0.0);
        group.set_margin_top(14);
        group.set_margin_bottom(4);
        group.add_css_class("cedro-cap");
        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&group));
        row.set_activatable(false);
        row.set_selectable(false);
        list.append(&row);
        count += found.len();
        for item in found {
            list.append(&credit_row(item));
        }
    }
    count
}

/// Scheda «Informazioni»: chi siamo e, sotto, tutto il codice aperto che Cedro usa.
fn about_page() -> HBox {
    let page = HBox::new(Orientation::Vertical, 0);
    page.set_margin_top(18);
    page.set_margin_start(28);
    page.set_margin_end(28);

    let head = HBox::new(Orientation::Horizontal, 16);
    if let Ok(texture) = gtk4::gdk::Texture::from_bytes(&glib::Bytes::from_static(MARK)) {
        let logo = gtk4::Image::from_paintable(Some(&texture));
        logo.set_pixel_size(56);
        logo.set_valign(gtk4::Align::Start);
        head.append(&logo);
    }
    let who = HBox::new(Orientation::Vertical, 2);
    let title = Label::new(Some(&format!("Cedro {}", cedro::update::CURRENT)));
    title.set_xalign(0.0);
    title.add_css_class("cedro-h");
    let line1 = Label::new(Some(&t("Convertitore di file, con una ruota al cursore · Licenza MIT")));
    line1.set_xalign(0.0);
    line1.add_css_class("cedro-muted");
    let line2 = Label::new(Some(&format!("{POWERED_BY} · github.com/realfulvio/cedro")));
    line2.set_xalign(0.0);
    line2.add_css_class("cedro-muted");
    who.append(&title);
    who.append(&line1);
    who.append(&line2);
    head.append(&who);
    page.append(&head);
    if cedro::edizione::bondeno() {
        // Il riquadro del tema, con lo stemma con la scritta.
        let ente = HBox::new(Orientation::Horizontal, 14);
        ente.add_css_class("cedro-ente-about");
        ente.append(&crate::linux_firma::stemma_area(cedro::edizione::Stemma::Scritta, 60));
        let text = HBox::new(Orientation::Vertical, 2);
        text.set_valign(gtk4::Align::Center);
        let nome = Label::new(Some(&t("Tema Comune di Bondeno")));
        nome.set_xalign(0.0);
        nome.add_css_class("cedro-ente-nome");
        let nota = Label::new(Some(&t("Stesse funzioni di Cedro, con i colori, il carattere e lo stemma del Comune. Si cambia in Aspetto.")));
        nota.set_xalign(0.0);
        nota.set_wrap(true);
        nota.add_css_class("cedro-muted");
        text.append(&nome);
        text.append(&nota);
        ente.append(&text);
        page.append(&ente);
    }

    page.append(&caption("CODICE APERTO USATO DA CEDRO"));
    let thanks = Label::new(Some(&t("Cedro esiste grazie a queste persone e ai loro progetti. Grazie.")));
    thanks.set_xalign(0.0);
    thanks.add_css_class("cedro-muted");
    page.append(&thanks);

    let search_line = HBox::new(Orientation::Horizontal, 10);
    search_line.set_margin_top(10);
    search_line.set_margin_bottom(8);
    let search = gtk4::SearchEntry::new();
    search.set_placeholder_text(Some(&t("Cerca un componente o una licenza")));
    search.set_hexpand(true);
    search.add_css_class("cedro-search");
    let count = Label::new(None);
    count.add_css_class("cedro-muted");
    search_line.append(&search);
    search_line.append(&count);
    page.append(&search_line);

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("cedro-credits");
    let show = {
        let list = list.clone();
        let count = count.clone();
        move |query: &str| {
            let n = fill_credits(&list, query);
            count.set_text(&t(&format!("{n} {}", if n == 1 { "componente" } else { "componenti" })));
        }
    };
    show("");
    search.connect_search_changed(move |entry| show(&entry.text()));
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_min_content_height(160);
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_child(Some(&list));
    page.append(&scroll);

    let full = Button::with_label(&t("Apri le licenze complete"));
    full.add_css_class("cedro-sec");
    full.set_halign(gtk4::Align::Start);
    full.set_margin_top(10);
    full.set_margin_bottom(10);
    let note = match cedro::credits::full_count() {
        Some(n) => format!("Tutti i {n} componenti, anche quelli minori, nel file «Licenze di terze parti»."),
        None => "Tutti i componenti, anche quelli minori, nel file «Licenze di terze parti».".to_string(),
    };
    let note = Label::new(Some(&t(&note)));
    note.set_wrap(true);
    note.set_max_width_chars(40);
    note.set_xalign(0.0);
    note.set_hexpand(true);
    note.add_css_class("cedro-muted");
    let foot = HBox::new(Orientation::Horizontal, 12);
    foot.append(&full);
    foot.append(&note);
    page.append(&foot);
    full.connect_clicked(|_| {
        // Il file sta accanto a cedro; se manca si apre la pagina del progetto.
        let target = match cedro::credits::full_file() {
            // L'URI va codificato: un percorso con spazi o «#» non si aprirebbe.
            Some(path) => gtk4::gio::File::for_path(&path).uri().to_string(),
            None => "https://github.com/realfulvio/cedro".to_string(),
        };
        let _ = gtk4::gio::AppInfo::launch_default_for_uri(&target, None::<&gtk4::gio::AppLaunchContext>);
    });
    page
}

/// La scheda «Aspetto»: modalità e temi con le anteprime, disegnati da `cedro::ui::aspetto`.
fn aspetto_page(current: &Settings) -> (Rc<RefCell<cedro::ui::aspetto::Aspetto>>, HBox) {
    use cedro::ui::aspetto::Aspetto;
    use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let dirty = Arc::new(AtomicBool::new(false));
    let wake = {
        let dirty = dirty.clone();
        Arc::new(move || dirty.store(true, Ordering::Relaxed))
    };
    let state = Rc::new(RefCell::new(Aspetto::new(cedro::ui::theme::ThemeId::from_key(&current.ui_theme), current.simple, wake)));
    let area = DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.set_focusable(true);
    area.set_content_width(420);
    area.set_content_height(300);
    let cache = Rc::new(RefCell::new(GlyphCache::new()));
    {
        let (state, cache) = (state.clone(), cache.clone());
        area.set_draw_func(move |a, cr, w, h| {
            let sf = a.scale_factor().max(1);
            let (pw, ph) = (w * sf, h * sf);
            if pw <= 0 || ph <= 0 {
                return;
            }
            let mut cache = cache.borrow_mut();
            let mut painter = Painter::new(pw as u32, ph as u32, sf as f32, Fonts::current(), &mut cache);
            state.borrow_mut().draw(&mut painter, w as f32, h as f32);
            let data = painter.bgra();
            if let Ok(img) = ImageSurface::create_for_data(data, gtk4::cairo::Format::ARgb32, pw, ph, pw * 4) {
                cr.scale(1.0 / sf as f64, 1.0 / sf as f64);
                let _ = cr.set_source_surface(&img, 0.0, 0.0);
                let _ = cr.paint();
            }
        });
    }
    let motion = gtk4::EventControllerMotion::new();
    {
        let (st, ar) = (state.clone(), area.clone());
        motion.connect_motion(move |_, x, y| {
            let mut s = st.borrow_mut();
            if s.pointer_move(x as f32, y as f32) {
                ar.queue_draw();
            }
            ar.set_cursor_from_name(Some(if s.is_hand() { "pointer" } else { "default" }));
        });
        let (st, ar) = (state.clone(), area.clone());
        motion.connect_leave(move |_| {
            if st.borrow_mut().pointer_leave() {
                ar.queue_draw();
            }
        });
    }
    area.add_controller(motion);
    let click = gtk4::GestureClick::new();
    {
        let (state, area2) = (state.clone(), area.clone());
        click.connect_pressed(move |_, _, x, y| {
            area2.grab_focus();
            if state.borrow_mut().pointer_down(x as f32, y as f32) {
                area2.queue_draw();
            }
        });
    }
    area.add_controller(click);
    let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    {
        let (state, area2) = (state.clone(), area.clone());
        scroll.connect_scroll(move |_, _, dy| {
            if state.borrow_mut().wheel(dy as f32 * 60.0) {
                area2.queue_draw();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    area.add_controller(scroll);
    let keys = gtk4::EventControllerKey::new();
    {
        let (state, area2) = (state.clone(), area.clone());
        keys.connect_key_pressed(move |_, key, _, _| {
            use cedro::ui::screen::Key;
            let k = match key {
                gtk4::gdk::Key::Left => Key::Left,
                gtk4::gdk::Key::Right => Key::Right,
                gtk4::gdk::Key::Up => Key::Up,
                gtk4::gdk::Key::Down => Key::Down,
                _ => return glib::Propagation::Proceed,
            };
            if state.borrow_mut().key(k) {
                area2.queue_draw();
            }
            glib::Propagation::Stop
        });
    }
    area.add_controller(keys);
    // Quando un'anteprima è pronta (la fa un altro thread) si ridisegna.
    {
        let weak = area.downgrade();
        glib::timeout_add_local(std::time::Duration::from_millis(150), move || match weak.upgrade() {
            Some(a) => {
                if dirty.swap(false, Ordering::Relaxed) {
                    a.queue_draw();
                }
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
    }
    let page = HBox::new(Orientation::Vertical, 0);
    page.set_margin_top(18);
    page.set_margin_start(28);
    page.set_margin_end(28);
    let heading = Label::new(Some(&t("Aspetto")));
    heading.set_xalign(0.0);
    heading.set_margin_bottom(10);
    heading.add_css_class("cedro-h");
    page.append(&heading);
    page.append(&area);
    (state, page)
}

/// Apre le Impostazioni, o porta avanti quelle già aperte.
pub fn open_settings(app: &Application, on_saved: impl Fn(&Settings) + 'static) {
    for window in app.windows() {
        if window.title().is_some_and(|name| name == t("Impostazioni di Cedro")) {
            window.present();
            return;
        }
    }
    let current = read_settings();
    let window = titled(app, "Impostazioni di Cedro", 780, 720);
    window.set_resizable(true);

    // Generale
    let autostart = switch(crate::linux_ui::autostart_enabled());
    let notify = switch(current.notify);
    let reveal = switch(current.reveal);
    let progress = switch(current.progress);
    let (aspetto, aspetto_page) = aspetto_page(&current);
    let log = Button::with_label(&t("Apri cartella"));
    log.add_css_class("cedro-sec");
    log.connect_clicked(|_| {
        let dir = cedro::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let uri = gtk4::gio::File::for_path(&dir).uri();
        let _ = gtk4::gio::AppInfo::launch_default_for_uri(&uri, None::<&gtk4::gio::AppLaunchContext>);
    });
    let autostart_row = setting("Avvia Cedro con il computer", "La stessa voce del menu dell'icona.", &autostart);
    let notify_row = setting("Avviso a fine conversione", "Una notifica con il nome del file creato.", &notify);
    let reveal_row = setting("Mostra il file nuovo", "A lavoro finito apre la cartella e seleziona la copia.", &reveal);
    let progress_row = setting("Avanzamento mentre converte", "Una linea sottile nel pannello e, su KDE, tra le notifiche.", &progress);
    let log_row = setting("Registro attività", "Il file log.txt e config.txt stanno nella cartella di Cedro.", &log);
    // I nomi delle lingue restano nella loro lingua: non passano dalla traduzione.
    let ui_lang = {
        let items = [t("Automatica"), "Italiano".to_string(), "English".to_string()];
        let shown: Vec<&str> = items.iter().map(String::as_str).collect();
        let dd = gtk4::DropDown::from_strings(&shown);
        dd.set_selected(UI_LANGS.iter().position(|l| *l == current.language).unwrap_or(0) as u32);
        dd.set_size_request(190, -1);
        dd
    };
    let lang_row = setting("Lingua dell'interfaccia", "Automatica segue la lingua del sistema. Vale subito dopo «Salva».", &ui_lang);
    // La modalità Semplice e il tema si scelgono in «Aspetto».
    let general = page("Generale", &[&autostart_row, &notify_row, &reveal_row, &progress_row, &log_row, &lang_row]);

    // Ruota
    let shift = switch(current.key_shift);
    let ctrl = switch(current.key_ctrl);
    let alt = switch(current.key_alt);
    // Wayland non lascia a un programma agganciarsi alle finestre Apri/Salva di un altro: l'interruttore resta spento e bloccato.
    let dialogs = switch(false);
    dialogs.set_sensitive(false);
    let wheel = page(
        "Ruota",
        &[
            &setting("Shift apre le conversioni", "Tieni premuto Shift e fai clic destro sul file.", &shift),
            &setting("Ctrl apre gli strumenti", "Comprimi, ruota, OCR e le altre fette di Ctrl.", &ctrl),
            &setting("Alt apre le azioni", "Duplica, percorso e ZIP. Spegnilo se Alt dà fastidio ad altri programmi.", &alt),
            &setting("Apri nelle finestre Apri/Salva", "Non disponibile su Wayland: il desktop non lascia a Cedro intervenire nelle finestre degli altri programmi. Usa il menu del file.", &dialogs),
        ],
    );

    // Salvataggio
    let same = CheckButton::with_label(&t("Nella stessa cartella del file"));
    let sub = CheckButton::with_label(&t("In una sottocartella «Cedro»"));
    let fixed = CheckButton::with_label(&t("Sempre in questa cartella:"));
    sub.set_group(Some(&same));
    fixed.set_group(Some(&same));
    match current.dest {
        cedro::Dest::Same => same.set_active(true),
        cedro::Dest::Sub => sub.set_active(true),
        cedro::Dest::Fixed => fixed.set_active(true),
    }
    let dest_dir = Rc::new(RefCell::new(current.dest_dir.clone()));
    let path = Label::new(Some(&t(&short_path(&current.dest_dir))));
    path.set_xalign(0.0);
    path.set_hexpand(true);
    path.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
    path.set_max_width_chars(26);
    path.add_css_class("cedro-muted");
    let browse = Button::with_label(&t("Sfoglia…"));
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
            let dialog = gtk4::FileDialog::builder().title(t("Cartella dove salvare le copie")).build();
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
    let check = Button::with_label(&t("Controlla ora"));
    check.add_css_class("cedro-sec");
    check.connect_clicked(|_| crate::linux_ui::check_updates(true));
    let updates = switch(current.update_check);
    let unskip = Button::with_label(&t("Proponila di nuovo"));
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

    let (system, gpu_switch, avx_switch) = system_page(&current, &window);
    let about = about_page();

    // Colonna delle schede e contenuto
    let stack = gtk4::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    for (name, content) in [("0", &general), ("1", &wheel), ("2", &saving), ("3", &quality), ("4", &ocr), ("5", &upd), ("6", &system), ("7", &about), ("8", &aspetto_page)] {
        stack.add_named(content, Some(name));
    }
    // Le schede nell'ordine in cui si vedono: nome e pagina dello stack. «Aspetto» è la seconda.
    let order: Vec<(&'static str, String)> = {
        let mut v: Vec<(&'static str, String)> = SECTIONS.iter().enumerate().map(|(i, n)| (*n, i.to_string())).collect();
        v.insert(1, ("Aspetto", "8".to_string()));
        v
    };
    let nav = gtk4::ListBox::new();
    nav.add_css_class("cedro-nav");
    nav.set_size_request(190, -1);
    for (name, _) in &order {
        let label = Label::new(Some(&t(name)));
        label.set_xalign(0.0);
        nav.append(&label);
    }
    {
        let stack = stack.clone();
        let order = order.clone();
        nav.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                if let Some((_, page)) = order.get(row.index() as usize) {
                    stack.set_visible_child_name(page);
                }
            }
        });
    }
    if let Some(first) = nav.row_at_index(0) {
        nav.select_row(Some(&first));
    }

    let middle = HBox::new(Orientation::Horizontal, 0);
    if cedro::edizione::bondeno() {
        // Tema Bondeno: stemma e «Comune di Bondeno» in cima alle sezioni.
        let side = HBox::new(Orientation::Vertical, 0);
        side.add_css_class("cedro-side");
        let ente = HBox::new(Orientation::Horizontal, 10);
        ente.add_css_class("cedro-ente");
        ente.append(&crate::linux_firma::stemma_area(cedro::edizione::Stemma::Solo, 38));
        let nome = Label::new(Some(&t("Comune di\nBondeno")));
        nome.set_xalign(0.0);
        nome.add_css_class("cedro-ente-nome");
        ente.append(&nome);
        side.append(&ente);
        nav.set_vexpand(true);
        side.append(&nav);
        middle.append(&side);
    } else {
        middle.append(&nav);
    }
    // Le pagine si accorciano a piacere: se la finestra è bassa, scorrono.
    let scrolled = gtk4::ScrolledWindow::new();
    scrolled.set_hexpand(true);
    scrolled.set_vexpand(true);
    scrolled.set_min_content_height(280);
    scrolled.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scrolled.set_child(Some(&stack));
    middle.append(&scrolled);

    let credit = crate::linux_firma::firma(false, "", true);
    credit.set_halign(gtk4::Align::Start);
    credit.set_hexpand(true);
    let guide = Button::with_label(&t("Guida"));
    guide.add_css_class("cedro-sec");
    let cancel = Button::with_label(&t("Annulla"));
    cancel.add_css_class("cedro-sec");
    let save = Button::with_label(&t("Salva"));
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
                let alert = gtk4::AlertDialog::builder().message(t("Scegli la cartella di destinazione con «Sfoglia…».")).build();
                alert.show(Some(&window));
                return;
            }
            let base = cedro::read_settings();
            let hw = cedro::accel::hardware();
            let settings = Settings {
                quality: scale.value().round().clamp(40.0, 95.0) as u8,
                notify: notify.is_active(),
                progress: progress.is_active(),
                simple: aspetto.borrow().simple,
                ui_theme: aspetto.borrow().theme.key().to_string(),
                view_asked: base.view_asked,
                reveal: reveal.is_active(),
                update_check: updates.is_active(),
                key_shift: shift.is_active(),
                key_ctrl: ctrl.is_active(),
                key_alt: alt.is_active(),
                dest,
                dest_dir: dest_dir.borrow().clone(),
                replace: exists.selected() == 1,
                pdf_dpi: *DPI.get(dpi.selected() as usize).unwrap_or(&base.pdf_dpi),
                compress: (compress.selected() as u8).min(2),
                ocr_lang: LANGS.get(lang.selected() as usize).unwrap_or(&"auto").to_string(),
                language: UI_LANGS.get(ui_lang.selected() as usize).unwrap_or(&"auto").to_string(),
                ocr_max: *MAX_PAGES.get(max_pages.selected() as usize).unwrap_or(&base.ocr_max),
                // Dove il PC non ha l'accelerazione l'interruttore è bloccato: resta la scelta di prima.
                gpu: if hw.gpu.is_some() { gpu_switch.is_active() } else { base.gpu },
                avx512: if hw.avx512 { avx_switch.is_active() } else { base.avx512 },
                // Le opzioni che questa finestra non mostra restano come sono.
                ..base
            };
            if let Err(e) = write_settings(&settings) {
                append_log(&format!("impostazioni non salvate: {e}"));
                // Come su Windows: chi preme «Salva» deve sapere che non è andata.
                gtk4::AlertDialog::builder().message(t(&format!("Impostazioni non salvate: {e}"))).build().show(Some(&window));
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
        shots(&window, &nav, order.iter().map(|(n, _)| n.to_string()).collect(), std::path::PathBuf::from(dir));
    }
}

/// Solo nelle build di prova: con `CEDRO_SHOTS=<cartella>` salva un'immagine di ogni scheda e si chiude.
#[cfg(debug_assertions)]
fn shots(window: &Window, nav: &gtk4::ListBox, names: Vec<String>, dir: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    fn step(window: Window, nav: gtk4::ListBox, names: Vec<String>, dir: std::path::PathBuf, at: i32) {
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
        glib::timeout_add_local_once(std::time::Duration::from_millis(if at == 1 { 9000 } else { 700 }), move || {
            let name = format!("impostazioni-{at}-{}.png", names[at as usize]);
            if let Err(e) = save_png(&window, &dir.join(name)) {
                eprintln!("screenshot: {e}");
            }
            step(window.clone(), nav.clone(), names.clone(), dir.clone(), at + 1);
        });
    }
    step(window.clone(), nav.clone(), names, dir, 0);
}

/// Il disegno della Guida su cairo: le primitive di `cedro::guida`.
struct CairoCanvas<'a> {
    cr: &'a Context,
    cache: &'a mut Vec<((usize, i32), ImageSurface)>,
}

fn set_font(cr: &Context, size: f64, bold: bool) {
    cr.select_font_face(cedro::tema::attivo().font, FontSlant::Normal, if bold { FontWeight::Bold } else { FontWeight::Normal });
    cr.set_font_size(size);
}

fn rgb(cr: &Context, c: (u8, u8, u8)) {
    cr.set_source_rgb(c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0);
}

fn rounded(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
    cr.close_path();
}

impl guida::Canvas for CairoCanvas<'_> {
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: guida::Rgb) {
        rgb(self.cr, c);
        self.cr.rectangle(x as f64, y as f64, w as f64, h as f64);
        let _ = self.cr.fill();
    }

    fn round(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, fill: Option<guida::Rgb>, line: Option<guida::Rgb>) {
        let cr = self.cr;
        // Il bordo sta dentro il riquadro, come su Windows.
        let (x, y, w, h) = (x as f64 + 0.5, y as f64 + 0.5, w as f64 - 1.0, h as f64 - 1.0);
        rounded(cr, x, y, w, h, radius as f64);
        if let Some(c) = fill {
            rgb(cr, c);
            let _ = cr.fill_preserve();
        }
        if let Some(c) = line {
            rgb(cr, c);
            cr.set_line_width(1.0);
            let _ = cr.stroke_preserve();
        }
        cr.new_path();
    }

    fn text(&mut self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, bold: bool, c: guida::Rgb, align: guida::Align, wrap: bool) {
        let s = &*cedro::i18n::tr(s);
        let cr = self.cr;
        set_font(cr, size as f64, bold);
        rgb(cr, c);
        let (x, y, w, h) = (x as f64, y as f64, w as f64, h as f64);
        let line_h = size as f64 * 1.3;
        let mut lines: Vec<String> = Vec::new();
        if wrap {
            let mut cur = String::new();
            for word in s.split(' ') {
                let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if !cur.is_empty() && cr.text_extents(&next).map(|e| e.x_advance() > w).unwrap_or(false) {
                    lines.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = next;
                }
            }
            lines.push(cur);
        } else {
            lines.push(s.to_string());
        }
        let fe = cr.font_extents().ok();
        let ascent = fe.map(|f| f.ascent()).unwrap_or(size as f64 * 0.8);
        let descent = fe.map(|f| f.descent()).unwrap_or(size as f64 * 0.2);
        let top = if wrap { y } else { y + (h - (ascent + descent)) / 2.0 };
        for (i, line) in lines.iter().enumerate() {
            let adv = cr.text_extents(line).map(|e| e.x_advance()).unwrap_or(0.0);
            let lx = match align {
                guida::Align::Left => x,
                guida::Align::Center => x + (w - adv) / 2.0,
                guida::Align::Right => x + w - adv,
            };
            cr.move_to(lx, top + ascent + i as f64 * line_h);
            let _ = cr.show_text(line);
        }
    }

    fn text_width(&mut self, s: &str, size: f32, bold: bool) -> f32 {
        let s = &*cedro::i18n::tr(s);
        set_font(self.cr, size as f64, bold);
        self.cr.text_extents(s).map(|e| e.x_advance() as f32).unwrap_or(0.0)
    }

    fn poly(&mut self, pts: &[(f32, f32)], fill: guida::Rgb, line: guida::Rgb) {
        let cr = self.cr;
        for (i, p) in pts.iter().enumerate() {
            if i == 0 {
                cr.move_to(p.0 as f64, p.1 as f64);
            } else {
                cr.line_to(p.0 as f64, p.1 as f64);
            }
        }
        cr.close_path();
        rgb(cr, fill);
        let _ = cr.fill_preserve();
        rgb(cr, line);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
    }

    fn clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let _ = self.cr.save();
        self.cr.rectangle(x as f64, y as f64, w as f64, h as f64);
        self.cr.clip();
    }

    fn unclip(&mut self) {
        let _ = self.cr.restore();
    }

    fn wheel(&mut self, n: usize, hover: i32, cx: f32, cy: f32, size: f32, alpha: f32) {
        let key = (n, hover);
        if !self.cache.iter().any(|(k, _)| *k == key) {
            if let Some(img) = wheel_image(n, hover, 1) {
                self.cache.push((key, img));
            }
        }
        let Some((_, img)) = self.cache.iter().find(|(k, _)| *k == key) else { return };
        let k = size as f64 / WHEEL as f64;
        let _ = self.cr.save();
        self.cr.translate(cx as f64 - size as f64 / 2.0, cy as f64 - size as f64 / 2.0);
        self.cr.scale(k, k);
        let _ = self.cr.set_source_surface(img, 0.0, 0.0);
        let _ = self.cr.paint_with_alpha(alpha as f64);
        let _ = self.cr.restore();
    }

    fn logo(&mut self, left: f32, baseline: f32, scale: f32, ink: guida::Rgb, accent: guida::Rgb, real: guida::Rgb) {
        let f = |c: guida::Rgb| (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0);
        cedro::logo::draw_cairo(self.cr, left as f64, baseline as f64, scale as f64, true, cedro::logo::Colors { ink: f(ink), accent: f(accent), real: f(real) });
    }

    fn stemma(&mut self, stemma: cedro::edizione::Stemma, x: f32, y: f32, w: f32, h: f32) {
        let key = (1000 + stemma as usize, -1);
        if !self.cache.iter().any(|(k, _)| *k == key) {
            if let Some(img) = crate::linux_firma::stemma_surface(stemma) {
                self.cache.push((key, img));
            }
        }
        let Some((_, img)) = self.cache.iter().find(|(k, _)| *k == key) else { return };
        crate::linux_firma::paint_surface(self.cr, img, x as f64, y as f64, w as f64, h as f64);
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

/// Apre la Guida animata, o porta avanti quella già aperta.
pub fn open_guide(app: &Application) {
    for window in app.windows() {
        if window.title().is_some_and(|name| name == t("Guida di Cedro")) {
            window.present();
            return;
        }
    }
    let window = titled(app, "Guida di Cedro", guida::W as i32, guida::H as i32);
    let area = DrawingArea::new();
    area.set_content_width(guida::W as i32);
    area.set_content_height(guida::H as i32);
    let started = Instant::now();
    let now = move || started.elapsed().as_millis() as u64;
    let tour = Rc::new(Cell::new(guida::Tour::new(0)));
    let cache: Rc<RefCell<Vec<((usize, i32), ImageSurface)>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let tour = tour.clone();
        area.set_draw_func(move |_, cr, _, _| {
            let mut cache = cache.borrow_mut();
            let mut canvas = CairoCanvas { cr, cache: &mut cache };
            guida::draw(&mut canvas, &tour.get(), now());
        });
    }
    let click = gtk4::GestureClick::new();
    {
        let tour = tour.clone();
        let window = window.clone();
        click.connect_released(move |_, _, x, y| {
            let mut t = tour.get();
            match guida::hit(x as f32, y as f32, t.page) {
                Some(guida::Hit::Page(i)) => t.goto(i, now()),
                Some(guida::Hit::Prev) => t.goto(t.page + guida::PAGE_COUNT - 1, now()),
                Some(guida::Hit::Next) if t.is_last() => {
                    window.close();
                    return;
                }
                Some(guida::Hit::Next) => t.goto(t.page + 1, now()),
                Some(guida::Hit::Row(i)) => t.show_row(i, now()),
                None => {}
            }
            tour.set(t);
        });
    }
    area.add_controller(click);
    window.set_child(Some(&area));
    let weak = area.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(33), move || match weak.upgrade() {
        Some(area) if area.is_mapped() => {
            let mut t = tour.get();
            t.tick(now());
            tour.set(t);
            area.queue_draw();
            glib::ControlFlow::Continue
        }
        Some(_) => glib::ControlFlow::Continue,
        None => glib::ControlFlow::Break,
    });
    window.present();
}

/// «Come vuoi usare Cedro?»: le due anteprime animate; si apre al primo avvio.
pub fn open_view_choice(app: &Application) {
    use cedro::vista::{self, Hit};
    let window = titled(app, "Cedro: scegli la vista", vista::W as i32, vista::H as i32);
    let area = DrawingArea::new();
    area.set_content_width(vista::W as i32);
    area.set_content_height(vista::H as i32);
    let started = Instant::now();
    let now = move || started.elapsed().as_millis() as u64;
    let chosen = Rc::new(Cell::new(vista::initial()));
    let cache: Rc<RefCell<Vec<((usize, i32), ImageSurface)>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let chosen = chosen.clone();
        area.set_draw_func(move |_, cr, _, _| {
            let mut cache = cache.borrow_mut();
            let mut canvas = CairoCanvas { cr, cache: &mut cache };
            vista::draw(&mut canvas, chosen.get(), now());
        });
    }
    let done = Rc::new(Cell::new(false));
    let click = gtk4::GestureClick::new();
    {
        let chosen = chosen.clone();
        let window = window.clone();
        let done = done.clone();
        click.connect_released(move |_, _, x, y| match vista::hit(x as f32, y as f32) {
            Some(Hit::Card(c)) => chosen.set(c),
            Some(Hit::Done) => {
                vista::save(chosen.get());
                done.set(true);
                window.close();
            }
            None => {}
        });
    }
    area.add_controller(click);
    // Chiusa senza scegliere: non lo richiediamo a ogni avvio.
    window.connect_close_request(move |_| {
        if !done.get() {
            vista::mark_asked();
        }
        glib::Propagation::Proceed
    });
    window.set_child(Some(&area));
    let weak = area.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(33), move || match weak.upgrade() {
        Some(area) => {
            if area.is_mapped() {
                area.queue_draw();
            }
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });
    window.present();
}

/// Salva la finestra in un PNG (per le fotografie delle prove).
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
