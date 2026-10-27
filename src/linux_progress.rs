//! L'avanzamento delle conversioni su Linux.
//!
//! Wayland non lascia a un programma scegliere un angolo dello schermo, quindi la linea in basso
//! a destra la disegna il desktop: su KDE Plasma con il servizio dei lavori in corso
//! (`org.kde.JobViewServer`), che la mostra tra le notifiche con la sua barra. Dove quel servizio
//! non c'è (GNOME) resta l'avviso a fine conversione.

use std::sync::Mutex;

use gtk4::gio::{self, DBusCallFlags, DBusConnection};
use gtk4::glib::prelude::ToVariant;
use gtk4::glib::{Variant, VariantDict, VariantTy};

struct View {
    bus: DBusConnection,
    path: String,
    total: usize,
}

static VIEW: Mutex<Option<View>> = Mutex::new(None);

fn props(pairs: &[(&str, Variant)]) -> Variant {
    let dict = VariantDict::new(None);
    for (key, value) in pairs {
        dict.insert_value(key, value);
    }
    dict.end()
}

fn call(bus: &DBusConnection, dest: &str, path: &str, iface: &str, method: &str, args: Variant, reply: Option<&VariantTy>) -> Option<Variant> {
    bus.call_sync(Some(dest), path, iface, method, Some(&args), reply, DBusCallFlags::NONE, 2000, None::<&gio::Cancellable>).ok()
}

const SERVER: &str = "org.kde.JobViewServer";

/// Parte un lavoro di `total` file. Senza KDE, o con le Impostazioni che lo spengono, non fa niente.
pub fn start(total: usize, title: &str) {
    if !cedro::read_settings().progress || total == 0 {
        return;
    }
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else { return };
    let args = Variant::tuple_from_iter([
        "dev.cedro.Cedro".to_variant(),
        0i32.to_variant(),
        props(&[("title", title.to_variant())]),
    ]);
    let Some(reply) = call(&bus, SERVER, "/JobViewServer", "org.kde.JobViewServerV2", "requestView", args, Some(VariantTy::new("(o)").unwrap())) else {
        return;
    };
    let Some(path) = reply.child_value(0).str().map(str::to_string) else { return };
    let view = View { bus, path, total };
    update(&view, 0, title);
    *VIEW.lock().unwrap_or_else(|e| e.into_inner()) = Some(view);
}

fn update(view: &View, done: usize, info: &str) {
    let percent = (done * 100 / view.total.max(1)) as u32;
    let args = Variant::tuple_from_iter([props(&[
        ("title", "Cedro".to_variant()),
        ("infoMessage", cedro::i18n::tr(info).to_variant()),
        ("percent", percent.to_variant()),
        ("processedAmount", (done as u64).to_variant()),
        ("totalAmount", (view.total as u64).to_variant()),
        ("processedUnit", "files".to_variant()),
    ])]);
    call(&view.bus, SERVER, &view.path, "org.kde.JobViewV3", "update", args, None);
}

/// Il file `index` (da 0) sta per partire.
pub fn step(index: usize, info: &str) {
    if let Some(view) = VIEW.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        update(view, index, info);
    }
}

/// Fine del lavoro: KDE mostra la barra piena, o l'errore, e la toglie da sé.
pub fn finish(ok: bool, info: &str) {
    let Some(view) = VIEW.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    update(&view, view.total, info);
    let (code, message) = if ok { (0u32, String::new()) } else { (1u32, cedro::i18n::tr_string(info)) };
    let args = Variant::tuple_from_iter([code.to_variant(), message.to_variant(), props(&[])]);
    call(&view.bus, SERVER, &view.path, "org.kde.JobViewV3", "terminate", args, None);
}

