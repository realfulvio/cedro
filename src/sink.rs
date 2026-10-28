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
    /// Codifica con la scheda video, se c'è (vedi `accel::use_gpu`).
    pub gpu: bool,
}

impl Default for VideoShape {
    fn default() -> Self {
        VideoShape { height: None, fps: None, mute: false, quality: 82, gpu: false }
    }
}

thread_local! {
    static SHAPE: std::cell::Cell<VideoShape> = std::cell::Cell::new(VideoShape::default());
    static GPU_USED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Durata del video in lavoro (100 ns) e bit al secondo del file di partenza; 0 = non noti.
    static TOTAL: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    static SOURCE_BPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Tetto al bitrate di uscita (bit/s alla qualità 82); 0 = nessuno.
    static BITRATE_CAP: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Durata e bitrate del video che si sta per convertire, per la barra di avanzamento e per non gonfiare i file.
pub(crate) fn set_source(total_100ns: i64, bps: u64) {
    TOTAL.with(|t| t.set(total_100ns.max(0)));
    SOURCE_BPS.with(|b| b.set(bps));
}

/// Legge dal file la durata e il bitrate (per la barra di avanzamento e per il tetto al bitrate di uscita).
pub(crate) fn note_source(input: &std::path::Path) {
    let size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let total = probe_total(input).unwrap_or(0);
    let bps = if total > 0 { (size as f64 * 8.0 / (total as f64 / 10_000_000.0)) as u64 } else { 0 };
    set_source(total, bps);
}

/// La durata in 100 ns, dalle tracce che il lettore dei contenitori conosce (MP4, MKV, WebM…).
fn probe_total(input: &std::path::Path) -> Option<i64> {
    use symphonia::core::{formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint};
    let file = std::fs::File::open(input).ok()?;
    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, MediaSourceStream::new(Box::new(file), Default::default()), &FormatOptions::default(), &MetadataOptions::default())
        .ok()?;
    let secs = probed
        .format
        .tracks()
        .iter()
        .filter_map(|t| {
            let time = t.codec_params.time_base?.calc_time(t.codec_params.n_frames?);
            Some(time.seconds as f64 + time.frac)
        })
        .fold(0.0, f64::max);
    (secs > 0.0).then(|| (secs * 10_000_000.0) as i64)
}

/// Un fotogramma al tempo `time` (100 ns) è stato preso in carico: avanza la barra.
pub(crate) fn frame_progress(time: i64) {
    let total = TOTAL.with(|t| t.get());
    if total > 0 {
        crate::progress::report((time as f64 / total as f64 * 0.99) as f32);
    }
}

/// L'uscita è più piccola dell'ingresso (`out`/`src` pixel): col file di partenza già compresso, a 720p il bitrate
/// non va oltre quello di prima in proporzione (H.264 Baseline ne chiede circa una volta e mezza).
pub(crate) fn cap_for(out_pixels: u64, src_pixels: u64) {
    let bps = SOURCE_BPS.with(|b| b.get());
    let cap = if bps > 0 && src_pixels > 0 { (bps as f64 * (out_pixels.min(src_pixels) as f64 / src_pixels as f64) * 1.5) as u64 } else { 0 };
    BITRATE_CAP.with(|c| c.set(cap));
}

/// Il video appena fatto su questo thread è passato dalla scheda video.
pub(crate) fn gpu_used() -> bool {
    GPU_USED.with(|g| g.get())
}

/// Lo dice il codificatore quando la scheda video ha preso il lavoro.
pub(crate) fn note_gpu() {
    GPU_USED.with(|g| g.set(true));
    let name = crate::accel::hardware().gpu.clone().unwrap_or_else(|| "codifica video".into());
    crate::accel::set_in_use(true, name);
}

/// Le opzioni video in vigore su questo thread.
pub(crate) fn shape() -> VideoShape {
    SHAPE.with(|s| s.get())
}

/// Esegue `f` con le opzioni video date e poi ripristina quelle di prima.
pub(crate) fn with_shape<T>(shape: VideoShape, f: impl FnOnce() -> T) -> T {
    let before = SHAPE.with(|s| s.replace(shape));
    GPU_USED.with(|g| g.set(false));
    TOTAL.with(|t| t.set(0));
    SOURCE_BPS.with(|b| b.set(0));
    BITRATE_CAP.with(|c| c.set(0));
    if !shape.gpu {
        crate::accel::set_in_use(false, "H.264 software");
    }
    let out = f();
    SHAPE.with(|s| s.set(before));
    out
}

/// Bit al secondo per un video di `width`×`height`: circa 4 bit per pixel a qualità 82.
pub(crate) fn bitrate(width: u32, height: u32) -> u32 {
    let q = shape().quality.clamp(40, 95) as f64 / 82.0;
    let mut bits = (width as f64) * (height as f64) * 4.0 * q * q;
    let cap = BITRATE_CAP.with(|c| c.get());
    if cap > 0 {
        bits = bits.min(cap as f64 * q * q);
    }
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
/// I confini delle colonne si calcolano una volta sola e le righe d'uscita si dividono fra i thread:
/// il risultato è identico, ma su un video grande questo lavoro non è più il collo di bottiglia.
fn shrink_plane(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh];
    if dw == 0 || dh == 0 {
        return out;
    }
    let cols: Vec<(usize, usize)> = (0..dw)
        .map(|ox| {
            let x0 = ox * sw / dw;
            (x0, ((ox + 1) * sw / dw).clamp(x0 + 1, sw))
        })
        .collect();
    let rows_of = |first: usize, chunk: &mut [u8]| {
        for (k, line) in chunk.chunks_mut(dw).enumerate() {
            let oy = first + k;
            let y0 = oy * sh / dh;
            let y1 = ((oy + 1) * sh / dh).clamp(y0 + 1, sh);
            for (ox, &(x0, x1)) in cols.iter().enumerate() {
                let mut sum = 0u32;
                for y in y0..y1 {
                    let row = &src[y * sw..y * sw + sw];
                    for v in &row[x0..x1] {
                        sum += *v as u32;
                    }
                }
                line[ox] = (sum / ((y1 - y0) * (x1 - x0)) as u32) as u8;
            }
        }
    };
    let workers = if dw * dh >= 200_000 { std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 4) } else { 1 };
    if workers == 1 {
        rows_of(0, &mut out);
    } else {
        let per = dh.div_ceil(workers);
        std::thread::scope(|scope| {
            for (n, chunk) in out.chunks_mut(per * dw).enumerate() {
                let rows_of = &rows_of;
                scope.spawn(move || rows_of(n * per, chunk));
            }
        });
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

/// Da 3-8 canali a stereo (downmix ITU, normalizzato): il canale centrale, dove stanno i dialoghi,
/// e i surround vanno in entrambi i lati; l'LFE si lascia. Ordine dei canali di symphonia e di
/// WAVEFORMATEXTENSIBLE: FL FR FC LFE BL BR SL SR (5 canali: FL FR FC BL BR; 4: FL FR BL BR).
pub(crate) fn fold_stereo(samples: &[i16], channels: u16) -> Vec<i16> {
    const K: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let ch = channels as usize;
    if ch < 3 {
        return samples.to_vec();
    }
    let norm = match ch {
        3 | 4 => 1.0 / (1.0 + K),
        5..=7 => 1.0 / (1.0 + 2.0 * K),
        _ => 1.0 / (1.0 + 3.0 * K),
    };
    let mut out = Vec::with_capacity(samples.len() / ch * 2);
    for f in samples.chunks_exact(ch) {
        let s = |i: usize| f[i] as f32;
        // Centro, dietro a sinistra, dietro a destra.
        let (c, bl, br) = match ch {
            3 => (s(2), 0.0, 0.0),
            4 => (0.0, s(2), s(3)),
            5 => (s(2), s(3), s(4)),
            _ => (s(2), s(4), s(5)),
        };
        let (sl, sr) = if ch >= 8 { (s(6), s(7)) } else { (0.0, 0.0) };
        let l = (s(0) + K * c + K * (bl + sl)) * norm;
        let r = (s(1) + K * c + K * (br + sr)) * norm;
        out.push(l.round().clamp(-32768.0, 32767.0) as i16);
        out.push(r.round().clamp(-32768.0, 32767.0) as i16);
    }
    out
}

/// Cambio di frequenza dell'audio a pacchetti, con il resto portato da un pacchetto all'altro:
/// ricampionando ogni pacchetto da solo si perdeva una frazione di campione ogni volta, e l'audio
/// si accorciava (fino a qualche secondo all'ora) andando fuori sincrono col video.
#[derive(Default)]
pub(crate) struct Resampler {
    /// Posizione del prossimo campione d'uscita dall'inizio del pacchetto, in campioni d'ingresso × `to`.
    pos: u64,
    from: u32,
}

impl Resampler {
    /// Aggiunge a `out` i campioni di `input` (interleaved, `channels` canali) portati da `from` a `to` Hz.
    pub(crate) fn push(&mut self, input: &[i16], channels: u16, from: u32, to: u32, out: &mut Vec<i16>) {
        let ch = channels.max(1) as usize;
        if from == 0 || to == 0 || from == to {
            out.extend_from_slice(input);
            return;
        }
        if from != self.from {
            self.from = from;
            self.pos = 0;
        }
        let frames = (input.len() / ch) as u64;
        let limit = frames * to as u64;
        out.reserve((limit / from as u64 + 1) as usize * ch);
        while self.pos < limit {
            let src = (self.pos / to as u64) as usize;
            out.extend_from_slice(&input[src * ch..(src + 1) * ch]);
            self.pos += from as u64;
        }
        self.pos -= limit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compressed_source_caps_the_bitrate_but_a_rich_one_does_not() {
        let plain = with_shape(VideoShape::default(), || bitrate(1280, 720));
        // 1080p a 3,9 Mbit/s portato a 720p: il tetto è 1,5 volte la sua parte di bitrate.
        let capped = with_shape(VideoShape::default(), || {
            set_source(10_000_000_000, 3_900_000);
            cap_for(1280 * 720, 1920 * 1080);
            bitrate(1280, 720)
        });
        assert!(capped < plain && (2_400_000..=2_700_000).contains(&capped), "{plain} {capped}");
        // Un 1080p a 20 Mbit/s: il tetto sta sopra la formula, che resta com'è.
        let rich = with_shape(VideoShape::default(), || {
            set_source(10_000_000_000, 20_000_000);
            cap_for(1280 * 720, 1920 * 1080);
            bitrate(1280, 720)
        });
        assert_eq!(rich, plain);
        // Il tetto non passa da un lavoro all'altro.
        assert_eq!(with_shape(VideoShape::default(), || bitrate(1280, 720)), plain);
    }

    #[test]
    fn downmix_keeps_the_center_channel() {
        // 5.1 con la voce solo sul centrale: deve sentirsi a sinistra e a destra.
        let out = fold_stereo(&[0, 0, 10_000, 0, 0, 0], 6);
        assert!(out.len() == 2 && out[0] > 2_000 && out[0] == out[1], "{out:?}");
        // Tutto al massimo: si resta nel limite.
        assert_eq!(fold_stereo(&[i16::MAX; 8], 8), vec![i16::MAX, i16::MAX]);
        // La sinistra resta a sinistra.
        let left = fold_stereo(&[10_000, 0, 0], 3);
        assert!(left[0] > 5_000 && left[1] == 0, "{left:?}");
    }

    #[test]
    fn resampling_by_packets_keeps_the_length() {
        // Da 24 kHz a 44,1 kHz a pacchetti AAC da 1024 campioni, per un minuto: nessun campione perso.
        let mut r = Resampler::default();
        let packet = vec![0i16; 1024 * 2];
        let packets = 24_000 * 60 / 1024;
        let mut total = 0u64;
        let mut out = Vec::new();
        for _ in 0..packets {
            out.clear();
            r.push(&packet, 2, 24_000, 44_100, &mut out);
            total += (out.len() / 2) as u64;
        }
        let want = packets as u64 * 1024 * 44_100 / 24_000;
        assert!(total.abs_diff(want) <= 1, "{total} {want}");
        // Stessa frequenza: i campioni passano come sono.
        out.clear();
        r.push(&[1, 2, 3, 4], 2, 48_000, 48_000, &mut out);
        assert_eq!(out, vec![1, 2, 3, 4]);
    }

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
