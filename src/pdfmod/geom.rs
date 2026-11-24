//! Matrici del PDF e passaggio tra lo spazio della pagina nel file e la pagina come la si vede.

use lopdf::{Document, ObjectId};

pub type Mat = [f32; 6];

pub const IDENT: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a` poi `b` (come «a b cm» nel PDF: i punti sono righe).
pub fn mul(a: &Mat, b: &Mat) -> Mat {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

pub fn apply(m: &Mat, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

pub fn invert(m: &Mat) -> Option<Mat> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-9 {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(m[4] * a + m[5] * c), -(m[4] * b + m[5] * d)])
}

/// Una pagina: com'è nel file e com'è vista.
#[derive(Clone, Copy, Debug)]
pub struct Geom {
    /// Dalla pagina vista (origine in basso a sinistra) allo spazio del file.
    pub m: Mat,
    inv: Mat,
    pub w: f32,
    pub h: f32,
}

impl Geom {
    pub fn of(doc: &Document, page: ObjectId) -> Geom {
        let (frame, rot) = crate::pdfedit::page_frame(doc, page);
        let (m, w, h) = crate::pdfedit::display_matrix(frame, rot);
        Geom { m, inv: invert(&m).unwrap_or(IDENT), w, h }
    }

    /// Uno spazio locale (un disegno di un campo o di un timbro): gli assi del file, origine in basso a sinistra.
    pub fn local(w: f32, h: f32) -> Geom {
        Geom { m: IDENT, inv: IDENT, w, h }
    }

    /// Un punto del file → un punto della pagina vista (origine in alto a sinistra).
    pub fn to_view(&self, x: f32, y: f32) -> (f32, f32) {
        let (vx, vy) = apply(&self.inv, x, y);
        (vx, self.h - vy)
    }

    /// Un punto della pagina vista → un punto del file.
    pub fn to_file(&self, x: f32, y: f32) -> (f32, f32) {
        apply(&self.m, x, self.h - y)
    }

    /// Il rettangolo del file (due angoli opposti) → il rettangolo visto.
    pub fn rect_to_view(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> super::Rect {
        let a = self.to_view(x0, y0);
        let b = self.to_view(x1, y1);
        super::Rect::between(a, b)
    }

    /// Il rettangolo visto → il rettangolo nel file (x0, y0, x1, y1 con x0<x1, y0<y1), per «Rect» e «BBox».
    pub fn rect_to_file(&self, r: &super::Rect) -> [f32; 4] {
        let a = self.to_file(r.x, r.y);
        let b = self.to_file(r.right(), r.bottom());
        [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
    }

    /// Quanto è girata la pagina nel file: la matrice senza traslazione.
    pub fn rotation_quarters(&self) -> i32 {
        let (a, b) = (self.m[0], self.m[1]);
        if a > 0.5 {
            0
        } else if b > 0.5 {
            1
        } else if a < -0.5 {
            2
        } else {
            3
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_round_trip() {
        let m: Mat = [0.0, 1.0, -1.0, 0.0, 595.0, 10.0];
        let i = invert(&m).unwrap();
        let (x, y) = apply(&m, 12.0, 34.0);
        let (bx, by) = apply(&i, x, y);
        assert!((bx - 12.0).abs() < 1e-3 && (by - 34.0).abs() < 1e-3);
        let n = mul(&m, &i);
        assert!((n[0] - 1.0).abs() < 1e-4 && n[4].abs() < 1e-3);
    }
}
