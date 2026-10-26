//! La firma in fondo alle finestre: «Cedro 0.17.1 · Powered by Luca real>hcok_ A.», col logo disegnato
//! alla misura del testo (vedi `cedro::logo`).

use cedro::logo::{self, Colors};
use gtk4::prelude::*;
use gtk4::{DrawingArea, Label, Orientation};

/// Il logo con «real» davanti, alto quanto una riga di testo piccolo.
fn logo_area() -> DrawingArea {
    const H: i32 = 18;
    let scale = 0.27;
    let area = DrawingArea::new();
    area.set_content_height(H);
    area.set_content_width((logo::width(true) as f64 * scale).ceil() as i32);
    area.set_valign(gtk4::Align::Center);
    area.set_draw_func(move |a, cr, _, _| {
        // Su sfondo scuro il testo è chiaro: il blu si schiarisce e «real» segue il testo.
        let text = a.color();
        let dark = (text.red() + text.green() + text.blue()) / 3.0 > 0.5;
        let colors = Colors {
            ink: if dark { (0.29, 0.64, 0.84) } else { (0.0, 0.369, 0.557) },
            accent: (0.949, 0.761, 0.188),
            real: (text.red() as f64, text.green() as f64, text.blue() as f64),
        };
        logo::draw_cairo(cr, 0.0, 13.5, scale, true, colors);
    });
    area
}

/// La firma: `version` aggiunge «Cedro x.y.z ·» davanti, `suffix` va in coda.
pub fn firma(version: bool, suffix: &str) -> gtk4::Box {
    let row = gtk4::Box::new(Orientation::Horizontal, 0);
    let head = if version { format!("Cedro {}  ·  Powered by Luca ", cedro::update::CURRENT) } else { "Powered by Luca ".to_string() };
    for part in [Some(head), None, Some(format!(" A.{suffix}"))] {
        match part {
            Some(text) => {
                let label = Label::new(Some(&text));
                label.add_css_class("cedro-muted");
                row.append(&label);
            }
            None => row.append(&logo_area()),
        }
    }
    row
}
