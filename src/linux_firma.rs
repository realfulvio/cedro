//! La firma in fondo alle finestre: «Cedro 0.17.1 · Powered by Luca real>hcok_ A.», col logo disegnato
//! alla misura del testo (vedi `cedro::logo`).

use cedro::edizione::{self, Stemma};
use cedro::logo::{self, Colors};
use gtk4::cairo::{Context, Filter, Format, ImageSurface, SurfacePattern};
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

/// Uno stemma dell'edizione Bondeno come superficie cairo; `None` nell'edizione standard.
pub fn stemma_surface(stemma: Stemma) -> Option<ImageSurface> {
    let img = edizione::immagine(stemma)?;
    ImageSurface::create_for_data(img.bgra.clone(), Format::ARgb32, img.w as i32, img.h as i32, img.w as i32 * 4).ok()
}

/// Disegna `img` nel riquadro dato, ricampionata bene.
pub fn paint_surface(cr: &Context, img: &ImageSurface, x: f64, y: f64, w: f64, h: f64) {
    let _ = cr.save();
    cr.translate(x, y);
    cr.scale(w / img.width() as f64, h / img.height() as f64);
    let pattern = SurfacePattern::create(img);
    pattern.set_filter(Filter::Best);
    let _ = cr.set_source(&pattern);
    let _ = cr.paint();
    let _ = cr.restore();
}

/// Uno stemma dell'edizione Bondeno grande `side` pixel, come elemento dell'interfaccia.
pub fn stemma_area(stemma: Stemma, side: i32) -> DrawingArea {
    let area = DrawingArea::new();
    area.set_content_width(side);
    area.set_content_height(side);
    area.set_valign(gtk4::Align::Center);
    let surface = stemma_surface(stemma);
    area.set_draw_func(move |_, cr, w, h| {
        if let Some(img) = &surface {
            paint_surface(cr, img, 0.0, 0.0, w as f64, h as f64);
        }
    });
    area
}

/// La firma: `version` aggiunge «Cedro x.y.z ·» davanti, `suffix` va in coda.
/// Con `ente`, nell'edizione Bondeno, davanti c'è lo stemma; senza `version` anche «Comune di Bondeno ·».
pub fn firma(version: bool, suffix: &str, ente: bool) -> gtk4::Box {
    let row = gtk4::Box::new(Orientation::Horizontal, 0);
    let ente = ente && edizione::BONDENO;
    if ente {
        let stemma = stemma_area(Stemma::Solo, 20);
        stemma.set_margin_end(6);
        row.append(&stemma);
    }
    let head = match (version, ente) {
        (true, _) => format!("Cedro {}  ·  Powered by Luca ", cedro::update::CURRENT),
        (false, true) => "Comune di Bondeno  ·  Powered by Luca ".to_string(),
        (false, false) => "Powered by Luca ".to_string(),
    };
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
