//! Le primitive di disegno (GDI su Windows, cairo su Linux) che usa ancora la finestra «Come vuoi usare Cedro?» (`vista`).
//! La Guida vera è in `ui::guida`, disegnata con lo stesso pittore del resto dell'interfaccia.

pub type Rgb = (u8, u8, u8);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Le primitive che ogni finestra deve saper disegnare. Tutto in pixel logici.
pub trait Canvas {
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb);
    fn round(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, fill: Option<Rgb>, line: Option<Rgb>);
    /// Testo in un riquadro: centrato in verticale, oppure a capo dall'alto se `wrap`.
    fn text(&mut self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, bold: bool, c: Rgb, align: Align, wrap: bool);
    fn text_width(&mut self, s: &str, size: f32, bold: bool) -> f32;
    fn poly(&mut self, pts: &[(f32, f32)], fill: Rgb, line: Rgb);
    fn clip(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn unclip(&mut self);
    /// La ruota con `n` fette (fetta `hover` accesa, -1 nessuna), centrata e larga `size`, con opacità `alpha`.
    fn wheel(&mut self, n: usize, hover: i32, cx: f32, cy: f32, size: f32, alpha: f32);
    fn logo(&mut self, left: f32, baseline: f32, scale: f32, ink: Rgb, accent: Rgb, real: Rgb);
    /// Uno stemma del Comune di Bondeno nel riquadro dato (senza il tema Bondeno non c'è niente da disegnare).
    fn stemma(&mut self, stemma: crate::edizione::Stemma, x: f32, y: f32, w: f32, h: f32);
}

