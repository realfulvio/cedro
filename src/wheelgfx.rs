//! Disegno della ruota senza dipendenze grafiche: un buffer BGRA con alfa premoltiplicato,
//! con bordi ammorbiditi. Lo usano Windows (UpdateLayeredWindow) e Linux (cairo).
//! Colori e forma vengono dal tema dell'edizione (`tema::attivo`): fette bianche con un filo e
//! centro scuro nella standard, fette arrotondate e staccate in quella del Comune di Bondeno.

use crate::tema::{self, Rgb, Tema};
use crate::wheelgeom::{R0, R1, WHEEL};

/// Spessore del filo attorno alle fette, in unità logiche.
const EDGE: f32 = 1.5;
const INNER: f32 = R0 + 6.0;
const SHADOW_DY: f32 = 3.0;

/// Distanza (negativa dentro) dal bordo della fetta `n` che contiene il punto, già arrotondata.
fn slice_dist(t: &Tema, n: usize, dx: f32, dy: f32) -> (usize, f32) {
    let tau = std::f32::consts::TAU;
    let start = -std::f32::consts::FRAC_PI_2;
    let r = (dx * dx + dy * dy).sqrt();
    let mut angle = dy.atan2(dx) - start;
    angle = angle.rem_euclid(tau);
    let sweep = tau / n as f32;
    let index = ((angle / sweep) as usize).min(n - 1);
    let local = angle - index as f32 * sweep;
    let corner = t.corner;
    let inset = t.half_gap + corner;
    let radial = (INNER + corner - r).max(r - (R1 - corner));
    let side = if n > 1 { (inset - r * local.sin()).max(inset - r * (sweep - local).sin()) } else { f32::MIN / 4.0 };
    let outside = (radial.max(0.0).powi(2) + side.max(0.0).powi(2)).sqrt();
    (index, outside + radial.max(side).min(0.0) - corner)
}

fn over(dst: &mut [f32; 4], color: Rgb, alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let k = 1.0 - alpha;
    dst[0] = dst[0] * k + color.2 as f32 / 255.0 * alpha;
    dst[1] = dst[1] * k + color.1 as f32 / 255.0 * alpha;
    dst[2] = dst[2] * k + color.0 as f32 / 255.0 * alpha;
    dst[3] = dst[3] * k + alpha;
}

/// Forme della ruota (fette, centro, ombra) in BGRA premoltiplicato, `WHEEL x WHEEL` pixel.
pub fn render(n: usize, hover: i32) -> Vec<u8> {
    render_scaled(n, hover, 1)
}

/// Come `render`, ma con `scale` pixel per unità logica (schermi ad alta densità).
/// Nella versione standard le forme sono quelle della ruota nuova (vedi `ui::wheel`).
pub fn render_scaled(n: usize, hover: i32, scale: u32) -> Vec<u8> {
    if crate::edizione::BONDENO {
        return render_tema(tema::attivo(), n, hover, scale);
    }
    render_new(n, hover, scale)
}

/// Le forme della ruota nuova nello spazio di quella vecchia (stessi raggi, stessa finestra).
fn render_new(n: usize, hover: i32, scale: u32) -> Vec<u8> {
    use crate::ui::gfx::{Fonts, GlyphCache, Painter};
    let scale = scale.max(1);
    let side = WHEEL as u32 * scale;
    let mut cache = GlyphCache::new();
    let mut p = Painter::new(side, side, scale as f32, Fonts::shared(), &mut cache);
    let c = WHEEL / 2.0;
    let k = R1 / 230.0;
    crate::ui::wheel::disc_shapes(&mut p, c, c, R0 + 6.0, R1, k, n.max(1), if hover >= 0 { Some(hover as usize) } else { None });
    // Il centro è bianco, con un anello azzurro se una fetta è accesa.
    p.disc_a(c, c + 3.0, R0 + 4.0, (10, 40, 90), 0.10);
    p.disc_a(c, c, R0 + 2.0, if hover >= 0 { (8, 116, 242) } else { (208, 223, 236) }, 1.0);
    p.disc_a(c, c, R0 - 1.0, (255, 255, 255), 1.0);
    p.bgra()
}

fn render_tema(t: &Tema, n: usize, hover: i32, scale: u32) -> Vec<u8> {
    let scale = scale.max(1);
    let k = scale as f32;
    let size = WHEEL as usize * scale as usize;
    let c = WHEEL / 2.0;
    let n = n.max(1);
    let mut out = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / k - c;
            let dy = (y as f32 + 0.5) / k - c;
            let r = (dx * dx + dy * dy).sqrt();
            let mut px = [0.0f32; 4];
            if r > R0 + 2.0 && r < R1 + 14.0 {
                let (_, shadow) = slice_dist(t, n, dx, dy - SHADOW_DY);
                let a = (1.0 - shadow / 9.0).clamp(0.0, 1.0);
                over(&mut px, (0, 0x10, 0x30), 0.28 * a * a);
                let (index, d) = slice_dist(t, n, dx, dy);
                let cover = (0.5 - d * k).clamp(0.0, 1.0);
                let color = if index as i32 == hover { t.hover } else { t.slice };
                over(&mut px, color, cover);
                if t.edge_alpha > 0.0 {
                    // il filo sta dentro il bordo della fetta, largo EDGE
                    let band = (0.5 - ((d + EDGE / 2.0).abs() - EDGE / 2.0) * k).clamp(0.0, 1.0);
                    over(&mut px, t.edge, band * t.edge_alpha);
                }
            }
            if r < R0 + 1.0 {
                let ring = if hover >= 0 { t.hover } else { t.hole_ring };
                over(&mut px, ring, ((R0 - r) * k + 1.0).clamp(0.0, 1.0));
                over(&mut px, t.hole, ((R0 - 2.0 - r) * k + 0.5).clamp(0.0, 1.0));
            }
            let i = (y * size + x) * 4;
            for k in 0..4 {
                out[i + k] = (px[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(buf: &[u8], x: f32, y: f32) -> [u8; 4] {
        let i = ((y as usize) * WHEEL as usize + x as usize) * 4;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    fn bgra(c: Rgb) -> [u8; 4] {
        [c.2, c.1, c.0, 0xFF]
    }

    fn check(t: &Tema) {
        let buf = render_tema(t, 4, 0, 1);
        let c = WHEEL / 2.0;
        let radius = (R0 + R1) / 2.0;
        let a = -std::f32::consts::FRAC_PI_4;
        let hovered = at(&buf, c + radius * a.cos(), c + radius * a.sin());
        assert_eq!(hovered, bgra(t.hover));
        let idle = at(&buf, c - radius * a.cos(), c + radius * a.sin());
        assert_eq!(idle, bgra(t.slice));
        assert_eq!(at(&buf, c, c), bgra(t.hole));
        assert_eq!(at(&buf, 2.0, 2.0)[3], 0);
        if t.half_gap > 0.0 {
            // lo spazio tra due fette (a mezzogiorno) lascia passare lo sfondo
            assert!(at(&buf, c, c - radius)[3] < 0x80);
        } else {
            // fette attaccate: a mezzogiorno c'è il filo, più scuro della fetta
            let seam = at(&buf, c, c - radius);
            assert_eq!(seam[3], 0xFF);
            assert!(seam[0] < bgra(t.slice)[0]);
        }
    }

    #[test]
    fn slice_hole_and_outside() {
        check(&tema::LIMONE);
        check(&tema::BONDENO);
    }
}
