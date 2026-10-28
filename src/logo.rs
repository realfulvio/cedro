//! Il logo di «hcok», la firma di Luca: un prompt, le lettere a tratto e il cursore giallo.
//! È fatto solo di linee, archi e rettangoli, su un piano alto 44 unità con la linea di base a y = 44,
//! così ogni finestra lo ridisegna alla sua scala (GDI+ su Windows, cairo su Linux).
//! «real» si scrive attaccato davanti al logo, mai dentro. Il kit completo (SVG, PNG) è nel progetto.

/// Il colore di un tratto: l'inchiostro del logo, il cursore giallo o la scritta «real».
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Ink,
    Accent,
    Real,
}

#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Line(f32, f32, f32, f32),
    /// Centro, raggio, angolo di partenza e ampiezza in gradi (in senso orario sullo schermo; negativa = antiorario).
    Arc(f32, f32, f32, f32, f32),
    Circle(f32, f32, f32),
    Curve([(f32, f32); 4]),
    /// Linea spezzata con gli angoli a punta.
    Poly([(f32, f32); 3]),
    /// Rettangolo pieno.
    Rect(f32, f32, f32, f32),
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub shape: Shape,
    pub width: f32,
    pub tone: Tone,
}

const fn ink(shape: Shape, width: f32) -> Item {
    Item { shape, width, tone: Tone::Ink }
}
const fn real(shape: Shape) -> Item {
    Item { shape, width: 7.0, tone: Tone::Real }
}

/// `>hcok_`, con la «h» minuscola.
pub const MARK: &[Item] = &[
    ink(Shape::Poly([(-21.0, 18.0), (-9.0, 30.0), (-21.0, 42.0)]), 6.0),
    // h
    ink(Shape::Line(7.5, 4.0, 7.5, 44.0), 7.0),
    ink(Shape::Arc(18.0, 30.0, 10.5, 180.0, 180.0), 7.0),
    ink(Shape::Line(28.5, 30.0, 28.5, 44.0), 7.0),
    // c
    ink(Shape::Arc(51.0, 30.0, 10.5, 45.0, 270.0), 7.0),
    // o
    ink(Shape::Circle(79.0, 30.0, 10.5), 7.0),
    // k
    ink(Shape::Line(100.0, 4.0, 100.0, 44.0), 7.0),
    ink(Shape::Line(100.0, 33.0, 114.5, 17.5), 7.0),
    ink(Shape::Line(106.5, 27.5, 117.0, 44.0), 7.0),
    // il cursore
    Item { shape: Shape::Rect(126.0, 35.0, 18.0, 9.0), width: 0.0, tone: Tone::Accent },
];

/// «real», con le stesse linee del logo.
pub const REAL: &[Item] = &[
    real(Shape::Line(7.5, 16.0, 7.5, 44.0)),
    real(Shape::Curve([(7.5, 31.0), (7.5, 23.0), (12.5, 19.5), (20.5, 19.5)])),
    real(Shape::Line(33.5, 30.0, 54.5, 30.0)),
    real(Shape::Arc(44.0, 30.0, 10.5, 0.0, -315.0)),
    real(Shape::Circle(76.0, 30.0, 10.5)),
    real(Shape::Line(86.5, 16.0, 86.5, 44.0)),
    real(Shape::Line(97.0, 4.0, 97.0, 44.0)),
];

/// Altezza del piano e quota della cima: y va da `TOP` a `TOP + HEIGHT`, la linea di base è y = 44.
pub const TOP: f32 = 2.0;
pub const HEIGHT: f32 = 44.0;
pub const BASELINE: f32 = 44.0;
/// Dove sta «real» e quanto il logo scorre a destra quando c'è «real» davanti.
const MARK_OFFSET: f32 = 130.0;
const MARK_LEFT: f32 = -24.0;
const MARK_RIGHT: f32 = 144.0;
const REAL_LEFT: f32 = 4.0;

/// Il piano occupato dal disegno, da sinistra a destra.
pub fn span(with_real: bool) -> (f32, f32) {
    if with_real {
        (REAL_LEFT, MARK_OFFSET + MARK_RIGHT)
    } else {
        (MARK_LEFT, MARK_RIGHT)
    }
}

pub fn width(with_real: bool) -> f32 {
    let (l, r) = span(with_real);
    r - l
}

/// I tratti da disegnare, già spostati: «real» davanti se richiesto, poi il logo.
pub fn items(with_real: bool) -> impl Iterator<Item = (Item, f32)> {
    let real = if with_real { REAL } else { &[] }.iter().map(|i| (*i, 0.0));
    let dx = if with_real { MARK_OFFSET } else { 0.0 };
    real.chain(MARK.iter().map(move |i| (*i, dx)))
}

/// Ridisegna il logo con cairo. `left` e `baseline` sono in pixel: il bordo sinistro del disegno e la linea di base.
#[cfg(target_os = "linux")]
pub fn draw_cairo(cr: &gtk4::cairo::Context, left: f64, baseline: f64, scale: f64, with_real: bool, colors: Colors) {
    use gtk4::cairo::LineJoin;
    let (l, _) = span(with_real);
    let _ = cr.save();
    cr.translate(left - l as f64 * scale, baseline - BASELINE as f64 * scale);
    cr.scale(scale, scale);
    for (item, dx) in items(with_real) {
        let (r, g, b) = colors.of(item.tone);
        cr.set_source_rgb(r, g, b);
        cr.set_line_width(item.width as f64);
        cr.set_line_join(LineJoin::Miter);
        let x = |v: f32| (v + dx) as f64;
        match item.shape {
            Shape::Line(x1, y1, x2, y2) => {
                cr.move_to(x(x1), y1 as f64);
                cr.line_to(x(x2), y2 as f64);
                let _ = cr.stroke();
            }
            Shape::Arc(cx, cy, rad, start, sweep) => {
                let (a1, a2) = ((start as f64).to_radians(), ((start + sweep) as f64).to_radians());
                if sweep >= 0.0 {
                    cr.arc(x(cx), cy as f64, rad as f64, a1, a2);
                } else {
                    cr.arc_negative(x(cx), cy as f64, rad as f64, a1, a2);
                }
                let _ = cr.stroke();
            }
            Shape::Circle(cx, cy, rad) => {
                cr.new_sub_path();
                cr.arc(x(cx), cy as f64, rad as f64, 0.0, std::f64::consts::TAU);
                let _ = cr.stroke();
            }
            Shape::Curve(p) => {
                cr.move_to(x(p[0].0), p[0].1 as f64);
                cr.curve_to(x(p[1].0), p[1].1 as f64, x(p[2].0), p[2].1 as f64, x(p[3].0), p[3].1 as f64);
                let _ = cr.stroke();
            }
            Shape::Poly(p) => {
                cr.move_to(x(p[0].0), p[0].1 as f64);
                cr.line_to(x(p[1].0), p[1].1 as f64);
                cr.line_to(x(p[2].0), p[2].1 as f64);
                let _ = cr.stroke();
            }
            Shape::Rect(rx, ry, w, h) => {
                cr.rectangle(x(rx), ry as f64, w as f64, h as f64);
                let _ = cr.fill();
            }
        }
    }
    let _ = cr.restore();
}

/// I tre colori del logo, come (rosso, verde, blu) da 0 a 1.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
pub struct Colors {
    pub ink: (f64, f64, f64),
    pub accent: (f64, f64, f64),
    pub real: (f64, f64, f64),
}

#[cfg(target_os = "linux")]
impl Colors {
    fn of(&self, tone: Tone) -> (f64, f64, f64) {
        match tone {
            Tone::Ink => self.ink,
            Tone::Accent => self.accent,
            Tone::Real => self.real,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn il_logo_sta_nel_suo_piano() {
        for with_real in [false, true] {
            let (l, r) = span(with_real);
            for (item, dx) in items(with_real) {
                let half = item.width / 2.0;
                let (x0, x1, y0, y1) = match item.shape {
                    Shape::Line(a, b, c, d) => (a.min(c), a.max(c), b.min(d), b.max(d)),
                    Shape::Arc(cx, cy, rad, ..) | Shape::Circle(cx, cy, rad) => (cx - rad, cx + rad, cy - rad, cy + rad),
                    Shape::Curve(p) => (p.iter().map(|q| q.0).fold(f32::MAX, f32::min), p.iter().map(|q| q.0).fold(f32::MIN, f32::max), p.iter().map(|q| q.1).fold(f32::MAX, f32::min), p.iter().map(|q| q.1).fold(f32::MIN, f32::max)),
                    Shape::Poly(p) => (p.iter().map(|q| q.0).fold(f32::MAX, f32::min), p.iter().map(|q| q.0).fold(f32::MIN, f32::max), p.iter().map(|q| q.1).fold(f32::MAX, f32::min), p.iter().map(|q| q.1).fold(f32::MIN, f32::max)),
                    Shape::Rect(x, y, w, h) => (x, x + w, y, y + h),
                };
                assert!(x0 + dx - half >= l - 3.0 && x1 + dx + half <= r + 3.0, "{item:?} esce a destra o a sinistra");
                assert!(y0 - half >= TOP - 0.5 - 3.0 && y1 + half <= TOP + HEIGHT + 3.0, "{item:?} esce in alto o in basso");
            }
        }
    }
}
