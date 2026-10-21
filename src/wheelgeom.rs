//! Geometria della ruota, uguale su Windows e su Linux.
//! L'angolo 0 è a destra. Le fette partono da mezzogiorno e vanno in senso orario
//! sullo schermo, dove l'asse Y punta in basso.

pub const WHEEL: f32 = 440.0;
pub const R0: f32 = 58.0;
pub const R1: f32 = 168.0;

/// Indice della fetta sotto il punto, `-1` nel buco o nello stacco, `-2` fuori dall'anello.
pub fn hover_at(cx: f32, cy: f32, x: f32, y: f32, n: usize) -> i32 {
    if n == 0 {
        return -1;
    }
    let dx = x - cx;
    let dy = y - cy;
    let radius = (dx * dx + dy * dy).sqrt();
    if radius > R1 + 2.0 {
        return -2;
    }
    if radius < R0 - 2.0 {
        return -1;
    }
    let start = -std::f32::consts::FRAC_PI_2;
    let mut angle = dy.atan2(dx);
    while angle < start {
        angle += std::f32::consts::TAU;
    }
    while angle >= start + std::f32::consts::TAU {
        angle -= std::f32::consts::TAU;
    }
    let sweep = std::f32::consts::TAU / n as f32;
    let index = ((angle - start) / sweep) as usize;
    if index >= n {
        return -1;
    }
    let gap = if n > 1 { 0.05 } else { 0.0 };
    let local = angle - start - index as f32 * sweep;
    if local < gap / 2.0 || local > sweep - gap / 2.0 {
        return -1;
    }
    index as i32
}

pub fn slice_bounds(index: usize, n: usize) -> (f32, f32) {
    let count = n.max(1) as f32;
    let sweep = std::f32::consts::TAU / count;
    let gap = if n > 1 { 0.05 } else { 0.0 };
    let a0 = -std::f32::consts::FRAC_PI_2 + index as f32 * sweep + gap / 2.0;
    let a1 = a0 + sweep - gap;
    (a0, a1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_zero_sits_up_right() {
        let cx = 220.0;
        let cy = 220.0;
        let radius = (R0 + R1) / 2.0;
        let angle = -std::f32::consts::FRAC_PI_4;
        let x = cx + radius * angle.cos();
        let y = cy + radius * angle.sin();
        assert_eq!(hover_at(cx, cy, x, y, 4), 0);
        assert_eq!(hover_at(cx, cy, cx, cy, 4), -1);
        assert_eq!(hover_at(cx, cy, cx + 400.0, cy, 4), -2);
        assert_eq!(hover_at(cx, cy, cx, cy - radius, 4), -1);
        assert_eq!(hover_at(cx, cy, cx, cy - radius, 1), 0);
    }
}
