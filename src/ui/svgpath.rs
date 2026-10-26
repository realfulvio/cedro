//! Lettura dell'attributo `d` dei tracciati SVG (le icone del kit) in un tracciato di tiny-skia.
//! Copre i comandi delle icone: M L H V C S Q T A Z, assoluti e relativi.

use tiny_skia::{Path, PathBuilder};

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] == b' ' || self.s[self.i] == b',' || self.s[self.i] == b'\n' || self.s[self.i] == b'\t') {
            self.i += 1;
        }
    }

    fn command(&mut self) -> Option<u8> {
        self.skip();
        let c = *self.s.get(self.i)?;
        if c.is_ascii_alphabetic() {
            self.i += 1;
            Some(c)
        } else {
            None
        }
    }

    fn at_number(&mut self) -> bool {
        self.skip();
        matches!(self.s.get(self.i), Some(c) if c.is_ascii_digit() || *c == b'-' || *c == b'.' || *c == b'+')
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        let start = self.i;
        if matches!(self.s.get(self.i), Some(b'-' | b'+')) {
            self.i += 1;
        }
        let mut dot = false;
        while let Some(&c) = self.s.get(self.i) {
            if c.is_ascii_digit() {
                self.i += 1;
            } else if c == b'.' && !dot {
                dot = true;
                self.i += 1;
            } else if (c == b'e' || c == b'E') && self.i > start {
                self.i += 1;
                if matches!(self.s.get(self.i), Some(b'-' | b'+')) {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }

    /// Le bandierine degli archi possono essere scritte attaccate («0 1 0» o «010»).
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// Un arco ellittico in curve di Bézier (SVG 1.1, appendice F.6).
#[allow(clippy::too_many_arguments)]
fn arc(pb: &mut PathBuilder, x1: f32, y1: f32, mut rx: f32, mut ry: f32, angle: f32, large: bool, sweep: bool, x2: f32, y2: f32) {
    if (x1 - x2).abs() < 1e-6 && (y1 - y2).abs() < 1e-6 {
        return;
    }
    if rx.abs() < 1e-6 || ry.abs() < 1e-6 {
        pb.line_to(x2, y2);
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let phi = angle.to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        rx *= k;
        ry *= k;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let cx = cos * cxp - sin * cyp + (x1 + x2) / 2.0;
    let cy = sin * cxp + cos * cyp + (y1 + y2) / 2.0;
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = ang(1.0, 0.0, ux, uy);
    let mut dtheta = ang(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f32::consts::TAU;
    }
    let segs = (dtheta.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let delta = dtheta / segs as f32;
    let t = 4.0 / 3.0 * (delta / 4.0).tan();
    let mut th = theta1;
    let point = |th: f32| {
        let (s, c) = th.sin_cos();
        (cx + rx * c * cos - ry * s * sin, cy + rx * c * sin + ry * s * cos)
    };
    let deriv = |th: f32| {
        let (s, c) = th.sin_cos();
        (-rx * s * cos - ry * c * sin, -rx * s * sin + ry * c * cos)
    };
    for _ in 0..segs {
        let (p0, d0) = (point(th), deriv(th));
        let th2 = th + delta;
        let (p3, d3) = (point(th2), deriv(th2));
        pb.cubic_to(p0.0 + t * d0.0, p0.1 + t * d0.1, p3.0 - t * d3.0, p3.1 - t * d3.1, p3.0, p3.1);
        th = th2;
    }
}

/// Il tracciato descritto da `d`, o `None` se è vuoto o scritto male.
pub fn parse(d: &str) -> Option<Path> {
    let mut lx = Lexer { s: d.as_bytes(), i: 0 };
    let mut pb = PathBuilder::new();
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    // L'ultimo punto di controllo, per S e T.
    let (mut qx, mut qy) = (0.0f32, 0.0f32);
    let mut last = b' ';
    while let Some(cmd) = lx.command() {
        let rel = cmd.is_ascii_lowercase();
        let up = cmd.to_ascii_uppercase();
        let mut first = true;
        loop {
            if !first && !lx.at_number() {
                break;
            }
            if up == b'Z' {
                pb.close();
                cx = sx;
                cy = sy;
                last = b'Z';
                break;
            }
            let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
            match up {
                b'M' => {
                    let (x, y) = (lx.number()? + ox, lx.number()? + oy);
                    if first {
                        pb.move_to(x, y);
                        sx = x;
                        sy = y;
                    } else {
                        pb.line_to(x, y);
                    }
                    cx = x;
                    cy = y;
                }
                b'L' => {
                    let (x, y) = (lx.number()? + ox, lx.number()? + oy);
                    pb.line_to(x, y);
                    cx = x;
                    cy = y;
                }
                b'H' => {
                    cx = lx.number()? + ox;
                    pb.line_to(cx, cy);
                }
                b'V' => {
                    cy = lx.number()? + oy;
                    pb.line_to(cx, cy);
                }
                b'C' => {
                    let (x1, y1) = (lx.number()? + ox, lx.number()? + oy);
                    let (x2, y2) = (lx.number()? + ox, lx.number()? + oy);
                    let (x, y) = (lx.number()? + ox, lx.number()? + oy);
                    pb.cubic_to(x1, y1, x2, y2, x, y);
                    (qx, qy, cx, cy) = (x2, y2, x, y);
                }
                b'S' => {
                    let (x1, y1) = if matches!(last, b'C' | b'S') { (2.0 * cx - qx, 2.0 * cy - qy) } else { (cx, cy) };
                    let (x2, y2) = (lx.number()? + ox, lx.number()? + oy);
                    let (x, y) = (lx.number()? + ox, lx.number()? + oy);
                    pb.cubic_to(x1, y1, x2, y2, x, y);
                    (qx, qy, cx, cy) = (x2, y2, x, y);
                }
                b'Q' => {
                    let (x1, y1) = (lx.number()? + ox, lx.number()? + oy);
                    let (x, y) = (lx.number()? + ox, lx.number()? + oy);
                    pb.quad_to(x1, y1, x, y);
                    (qx, qy, cx, cy) = (x1, y1, x, y);
                }
                b'T' => {
                    let (x1, y1) = if matches!(last, b'Q' | b'T') { (2.0 * cx - qx, 2.0 * cy - qy) } else { (cx, cy) };
                    let (x, y) = (lx.number()? + ox, lx.number()? + oy);
                    pb.quad_to(x1, y1, x, y);
                    (qx, qy, cx, cy) = (x1, y1, x, y);
                }
                b'A' => {
                    let (rx, ry, rot) = (lx.number()?, lx.number()?, lx.number()?);
                    let (large, sweep) = (lx.flag()?, lx.flag()?);
                    let (x, y) = (lx.number()? + ox, lx.number()? + oy);
                    arc(&mut pb, cx, cy, rx, ry, rot, large, sweep, x, y);
                    cx = x;
                    cy = y;
                }
                _ => return None,
            }
            last = up;
            first = false;
        }
    }
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses() {
        use crate::ui::assets::*;
        for d in [ADD, ARROW_LEFT, ARROW_RIGHT, AUDIO, CHECK, CHEVRON_DOWN, CLOCK, CLOSE, CONVERT, COPY, DOWNLOAD, FILE, FOLDER, HELP, IMAGE, LINK, MENU, SEARCH, SETTINGS, TRANSCRIBE, TRASH, VIDEO, WARNING, WEB, ZOOM_IN] {
            let p = parse(d).unwrap_or_else(|| panic!("{d}"));
            let b = p.bounds();
            assert!(b.left() >= 0.0 && b.right() <= 24.5 && b.top() >= 0.0 && b.bottom() <= 24.5, "{d}: {b:?}");
        }
    }

    #[test]
    fn circle_arc_closes() {
        let p = parse("M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0z").unwrap();
        let b = p.bounds();
        assert!((b.left() - 2.0).abs() < 0.01 && (b.right() - 22.0).abs() < 0.01);
        assert!((b.top() - 2.0).abs() < 0.05 && (b.bottom() - 22.0).abs() < 0.05);
    }
}
