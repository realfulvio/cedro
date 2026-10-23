//! Disegno della ruota senza dipendenze grafiche: un buffer BGRA con alfa premoltiplicato,
//! con bordi ammorbiditi. Lo usano Windows (UpdateLayeredWindow) e Linux (cairo).
//! Colori e forma sono quelli della versione "Bondeno": fette arrotondate, spazio uniforme.

use crate::wheelgeom::{R0, R1, WHEEL};

/// Colori come (r, g, b).
pub const SLICE: (u8, u8, u8) = (0xEB, 0xEE, 0xF0);
pub const HOVER: (u8, u8, u8) = (0x00, 0x66, 0xCC);
pub const PRIMARY: (u8, u8, u8) = (0x00, 0x5E, 0x8E);
pub const HOLE: (u8, u8, u8) = (0xFF, 0xFF, 0xFF);
pub const TEXT: (u8, u8, u8) = (0x19, 0x19, 0x19);
pub const TEXT_ON_HOVER: (u8, u8, u8) = (0xFF, 0xFF, 0xFF);
pub const MUTED: (u8, u8, u8) = (0x45, 0x5A, 0x64);
pub const NAVY: (u8, u8, u8) = (0x00, 0x1A, 0x4D);

const CORNER: f32 = 7.0;
const HALF_GAP: f32 = 3.0;
const INNER: f32 = R0 + 6.0;
const SHADOW_DY: f32 = 3.0;

/// Distanza (negativa dentro) dal bordo della fetta `n` che contiene il punto, già arrotondata.
fn slice_dist(n: usize, dx: f32, dy: f32) -> (usize, f32) {
    let tau = std::f32::consts::TAU;
    let start = -std::f32::consts::FRAC_PI_2;
    let r = (dx * dx + dy * dy).sqrt();
    let mut angle = dy.atan2(dx) - start;
    angle = angle.rem_euclid(tau);
    let sweep = tau / n as f32;
    let index = ((angle / sweep) as usize).min(n - 1);
    let t = angle - index as f32 * sweep;
    let inset = HALF_GAP + CORNER;
    let radial = (INNER + CORNER - r).max(r - (R1 - CORNER));
    let side = if n > 1 { (inset - r * t.sin()).max(inset - r * (sweep - t).sin()) } else { f32::MIN / 4.0 };
    let outside = (radial.max(0.0).powi(2) + side.max(0.0).powi(2)).sqrt();
    (index, outside + radial.max(side).min(0.0) - CORNER)
}

fn over(dst: &mut [f32; 4], color: (u8, u8, u8), alpha: f32) {
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
pub fn render_scaled(n: usize, hover: i32, scale: u32) -> Vec<u8> {
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
                let (_, shadow) = slice_dist(n, dx, dy - SHADOW_DY);
                let a = (1.0 - shadow / 9.0).clamp(0.0, 1.0);
                over(&mut px, (0, 0x10, 0x30), 0.28 * a * a);
                let (index, d) = slice_dist(n, dx, dy);
                let cover = (0.5 - d * k).clamp(0.0, 1.0);
                let color = if index as i32 == hover { HOVER } else { SLICE };
                over(&mut px, color, cover);
            }
            if r < R0 + 1.0 {
                let ring = if hover >= 0 { HOVER } else { PRIMARY };
                over(&mut px, ring, ((R0 - r) * k + 1.0).clamp(0.0, 1.0));
                over(&mut px, HOLE, ((R0 - 2.0 - r) * k + 0.5).clamp(0.0, 1.0));
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

    #[test]
    fn slice_hole_and_outside() {
        let buf = render(4, 0);
        let c = WHEEL / 2.0;
        let radius = (R0 + R1) / 2.0;
        let a = -std::f32::consts::FRAC_PI_4;
        let hovered = at(&buf, c + radius * a.cos(), c + radius * a.sin());
        assert_eq!(hovered, [0xCC, 0x66, 0x00, 0xFF]);
        let idle = at(&buf, c - radius * a.cos(), c + radius * a.sin());
        assert_eq!(idle, [0xF0, 0xEE, 0xEB, 0xFF]);
        assert_eq!(at(&buf, c, c), [0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(at(&buf, 2.0, 2.0)[3], 0);
        // lo spazio tra due fette (a mezzogiorno) lascia passare lo sfondo
        assert!(at(&buf, c, c - radius)[3] < 0x80);
    }
}
