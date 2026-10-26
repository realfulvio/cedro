use std::path::Path;

pub(crate) trait VideoSink: Sized {
    fn open(
        output: &Path,
        width: u32,
        height: u32,
        frame_duration: i64,
        audio: Option<(u32, u16)>,
    ) -> Result<Self, String>;

    fn write_yuv(
        &mut self,
        y: &[u8],
        u: &[u8],
        v: &[u8],
        width: u32,
        height: u32,
        time: i64,
        duration: i64,
    ) -> Result<(), String>;

    fn write_pcm(&mut self, samples: &[i16], channels: u16, rate: u32) -> Result<(), String>;

    fn finish(self) -> Result<(), String>;
}

/// Le opzioni video del pannello Conversioni, lette dagli encoder durante la conversione.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VideoShape {
    /// Altezza massima in pixel: i video più grandi si rimpiccioliscono, gli altri restano.
    pub height: Option<u32>,
    /// Fotogrammi al secondo massimi.
    pub fps: Option<u32>,
    pub mute: bool,
    /// 40-95, come il cursore della qualità.
    pub quality: u8,
}

impl Default for VideoShape {
    fn default() -> Self {
        VideoShape { height: None, fps: None, mute: false, quality: 82 }
    }
}

thread_local! {
    static SHAPE: std::cell::Cell<VideoShape> = std::cell::Cell::new(VideoShape::default());
}

/// Le opzioni video in vigore su questo thread.
pub(crate) fn shape() -> VideoShape {
    SHAPE.with(|s| s.get())
}

/// Esegue `f` con le opzioni video date e poi ripristina quelle di prima.
pub(crate) fn with_shape<T>(shape: VideoShape, f: impl FnOnce() -> T) -> T {
    let before = SHAPE.with(|s| s.replace(shape));
    let out = f();
    SHAPE.with(|s| s.set(before));
    out
}

/// Bit al secondo per un video di `width`×`height`: circa 4 bit per pixel a qualità 82.
pub(crate) fn bitrate(width: u32, height: u32) -> u32 {
    let q = shape().quality.clamp(40, 95) as f64 / 82.0;
    let bits = (width as f64) * (height as f64) * 4.0 * q * q;
    bits.clamp(60_000.0, 20_000_000.0) as u32
}

/// Le dimensioni di uscita: pari, ridotte a `height` se il video è più alto, mai ingrandite.
pub(crate) fn target_size(width: u32, height: u32) -> (u32, u32) {
    match shape().height {
        Some(h) if h >= 2 && h < height => {
            let out_h = (h & !1).max(2);
            let exact = width as f64 * out_h as f64 / height as f64;
            let out_w = (((exact / 2.0).round() as u32) * 2).max(2);
            (out_w, out_h)
        }
        _ => (width + (width & 1), height + (height & 1)),
    }
}

/// Ammette un fotogramma ogni tanto per non superare gli fps scelti; `time` e le durate sono in 100 ns.
pub(crate) struct FrameGate {
    gap: i64,
    last: Option<i64>,
}

impl FrameGate {
    pub fn new() -> Self {
        let gap = shape().fps.filter(|f| *f > 0).map(|f| 10_000_000 / f as i64).unwrap_or(0);
        FrameGate { gap, last: None }
    }

    /// Il tempo minimo di un fotogramma scritto: con il limite di fps, mai meno di così.
    pub fn gap(&self) -> i64 {
        self.gap
    }

    pub fn pass(&mut self, time: i64) -> bool {
        if self.gap == 0 {
            return true;
        }
        match self.last {
            // Tolleranza del 10%: un 30 fps da 33,3 ms non perde un fotogramma su due a 30.
            Some(last) if time - last < self.gap * 9 / 10 => false,
            _ => {
                self.last = Some(time);
                true
            }
        }
    }
}

/// Un piano 8 bit rimpicciolito con la media dei pixel coperti (box filter).
fn shrink_plane(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh];
    for oy in 0..dh {
        let y0 = oy * sh / dh;
        let y1 = ((oy + 1) * sh / dh).clamp(y0 + 1, sh);
        for ox in 0..dw {
            let x0 = ox * sw / dw;
            let x1 = ((ox + 1) * sw / dw).clamp(x0 + 1, sw);
            let mut sum = 0u32;
            for y in y0..y1 {
                let row = &src[y * sw..y * sw + sw];
                for v in &row[x0..x1] {
                    sum += *v as u32;
                }
            }
            out[oy * dw + ox] = (sum / ((y1 - y0) * (x1 - x0)) as u32) as u8;
        }
    }
    out
}

/// Porta un fotogramma I420 (`y`, `u`, `v` senza padding) alle dimensioni `dw`×`dh`.
pub(crate) fn resize_i420(y: &[u8], u: &[u8], v: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (sw, sh, dw, dh) = (sw as usize, sh as usize, dw as usize, dh as usize);
    let (scw, sch) = (sw.div_ceil(2), sh.div_ceil(2));
    let (dcw, dch) = (dw.div_ceil(2), dh.div_ceil(2));
    (
        shrink_plane(y, sw, sh, dw, dh),
        shrink_plane(u, scw, sch, dcw, dch),
        shrink_plane(v, scw, sch, dcw, dch),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_size_only_shrinks_and_stays_even() {
        let shape = VideoShape { height: Some(480), ..Default::default() };
        with_shape(shape, || {
            assert_eq!(target_size(1920, 1080), (854, 480));
            assert_eq!(target_size(640, 360), (640, 360));
            assert_eq!(target_size(641, 361), (642, 362));
        });
        assert_eq!(target_size(1921, 1081), (1922, 1082));
    }

    #[test]
    fn gate_halves_a_30_fps_stream_at_15() {
        let shape = VideoShape { fps: Some(15), ..Default::default() };
        with_shape(shape, || {
            let mut gate = FrameGate::new();
            let kept = (0..30).filter(|i| gate.pass(i * 333_333)).count();
            assert_eq!(kept, 15);
        });
        let mut open = FrameGate::new();
        assert!((0..30).all(|i| open.pass(i * 333_333)));
    }

    #[test]
    fn resize_keeps_the_average_color() {
        let (y, u, v) = resize_i420(&[200; 16 * 16], &[100; 8 * 8], &[50; 8 * 8], 16, 16, 8, 8);
        assert_eq!((y.len(), u.len(), v.len()), (64, 16, 16));
        assert!(y.iter().all(|p| *p == 200) && u.iter().all(|p| *p == 100) && v.iter().all(|p| *p == 50));
    }

    #[test]
    fn bitrate_grows_with_quality() {
        let low = with_shape(VideoShape { quality: 40, ..Default::default() }, || bitrate(1280, 720));
        let high = with_shape(VideoShape { quality: 95, ..Default::default() }, || bitrate(1280, 720));
        assert!(high > low * 3, "{low} {high}");
    }
}
