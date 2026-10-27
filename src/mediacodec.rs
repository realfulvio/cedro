//! Codifica H.264 con MediaCodec (la scheda video dei telefoni), per il video su Android.
//! Stessa forma di `vaapi`: `probe`, `H264Encoder::new` e `encode`, più `finish` per i fotogrammi ancora in coda
//! nel codificatore. Se qualcosa manca (nessun codificatore hardware, misura non adatta) `new` dà un errore e
//! si torna a OpenH264, senza perdere funzioni.

use std::collections::VecDeque;
use std::time::Duration;

use ndk::media::media_codec::{DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection};
use ndk::media::media_format::MediaFormat;

const FLAG_KEY: u32 = 1;
const FLAG_CONFIG: u32 = 2;
const FLAG_END: u32 = 4;
/// COLOR_FormatYUV420SemiPlanar (NV12): quello che i codificatori hardware accettano quasi sempre.
const NV12: i32 = 21;

pub(crate) struct H264Encoder {
    codec: MediaCodec,
    width: usize,
    height: usize,
    stride: usize,
    slice: usize,
    fps: f32,
    count: u64,
    ready: VecDeque<(Vec<u8>, bool)>,
    config: Vec<u8>,
}

/// I codificatori H.264 hardware dei costruttori di chip più diffusi (Qualcomm, Samsung, MediaTek, HiSilicon, Google Tensor),
/// provati per nome: l'NDK non dà l'elenco prima di Android 9 e il codificatore «predefinito» può essere quello software.
const HARDWARE: [&str; 9] = [
    "c2.qti.avc.encoder",
    "OMX.qcom.video.encoder.avc",
    "c2.exynos.h264.encoder",
    "OMX.Exynos.AVC.Encoder",
    "c2.mtk.avc.encoder",
    "OMX.MTK.VIDEO.ENCODER.AVC",
    "OMX.hisi.video.encoder.avc",
    "c2.google.avc.encoder",
    "OMX.rk.video_encoder.avc",
];

/// Apre il primo codificatore hardware che c'è; il nome in chiaro con cui si è aperto.
fn apri() -> Option<(MediaCodec, &'static str)> {
    HARDWARE.iter().find_map(|n| MediaCodec::from_codec_name(n).map(|c| (c, *n)))
}

/// Il nome del codificatore hardware H.264, se c'è.
pub(crate) fn probe() -> Option<String> {
    apri().map(|(_, n)| n.to_string())
}

impl H264Encoder {
    /// `width`×`height` pari; `fps` fotogrammi al secondo; `bitrate` in bit al secondo; `quality` 40-95.
    pub(crate) fn new(width: u32, height: u32, fps: f32, bitrate: u32, _quality: u8) -> Result<Self, String> {
        // I codificatori vogliono le misure a multipli di 16: con le altre si passa a OpenH264.
        if width < 128 || height < 128 || width % 16 != 0 || height % 16 != 0 || width > 4096 || height > 4096 {
            return Err(format!("MediaCodec: {width}×{height} non adatto"));
        }
        let (codec, _) = apri().ok_or("nessun codificatore H.264 hardware")?;
        let fps = if fps.is_finite() && fps > 0.0 { fps.clamp(1.0, 240.0) } else { 30.0 };
        let mut format = MediaFormat::new();
        format.set_str("mime", "video/avc");
        format.set_i32("width", width as i32);
        format.set_i32("height", height as i32);
        format.set_i32("bitrate", bitrate.max(100_000) as i32);
        format.set_i32("frame-rate", fps.round() as i32);
        format.set_i32("i-frame-interval", 2);
        format.set_i32("color-format", NV12);
        // Qualità costante a parità di bitrate (VBR) e profilo Baseline: lo leggono tutti i lettori.
        format.set_i32("bitrate-mode", 1);
        format.set_i32("profile", 1);
        format.set_i32("prepend-sps-pps-to-idr-frames", 1);
        codec.configure(&format, None, MediaCodecDirection::Encoder).map_err(|e| format!("MediaCodec non configurato: {e}"))?;
        codec.start().map_err(|e| format!("MediaCodec non avviato: {e}"))?;
        // Passo e altezza della fetta: con le misure a multipli di 16 il buffer NV12 è compatto.
        let (stride, slice) = (width as usize, height as usize);
        Ok(H264Encoder { codec, width: width as usize, height: height as usize, stride, slice, fps, count: 0, ready: VecDeque::new(), config: Vec::new() })
    }

    /// Prende i pacchetti pronti dal codificatore. Vero se è arrivata la fine del flusso.
    fn raccogli(&mut self, attesa: Duration) -> Result<bool, String> {
        loop {
            match self.codec.dequeue_output_buffer(attesa).map_err(|e| format!("MediaCodec: {e}"))? {
                DequeuedOutputBufferInfoResult::Buffer(out) => {
                    let info = *out.info();
                    let (from, len) = (info.offset().max(0) as usize, info.size().max(0) as usize);
                    let data = out.buffer().get(from..from + len).map(|d| d.to_vec()).unwrap_or_default();
                    let flags = info.flags();
                    self.codec.release_output_buffer(out, false).map_err(|e| format!("MediaCodec: {e}"))?;
                    if flags & FLAG_CONFIG != 0 {
                        self.config = data;
                    } else if !data.is_empty() {
                        let key = flags & FLAG_KEY != 0;
                        // SPS e PPS davanti a ogni fotogramma chiave, se il codificatore non li ha già messi.
                        let ha_sps = data.windows(5).take(64).any(|w| (w[..4] == [0, 0, 0, 1] && w[4] & 0x1f == 7) || (w[..3] == [0, 0, 1] && w[3] & 0x1f == 7));
                        let out = if key && !ha_sps && !self.config.is_empty() { [self.config.clone(), data].concat() } else { data };
                        self.ready.push_back((out, key));
                    }
                    if flags & FLAG_END != 0 {
                        return Ok(true);
                    }
                }
                DequeuedOutputBufferInfoResult::TryAgainLater => return Ok(false),
                DequeuedOutputBufferInfoResult::OutputFormatChanged | DequeuedOutputBufferInfoResult::OutputBuffersChanged => {}
            }
        }
    }

    /// Codifica un fotogramma I420 compatto (Y, poi U, poi V). Il codificatore risponde con qualche fotogramma di ritardo:
    /// restituisce il primo pacchetto pronto (anche vuoto, se non ce n'è ancora) e se è un IDR.
    pub(crate) fn encode(&mut self, i420: &[u8]) -> Result<(Vec<u8>, bool), String> {
        let (w, h) = (self.width, self.height);
        if i420.len() < w * h * 3 / 2 {
            return Err("MediaCodec: fotogramma incompleto".into());
        }
        let pts = (self.count as f64 * 1_000_000.0 / self.fps as f64) as u64;
        let mut tentativi = 0;
        loop {
            match self.codec.dequeue_input_buffer(Duration::from_millis(10)).map_err(|e| format!("MediaCodec: {e}"))? {
                DequeuedInputBufferResult::Buffer(mut input) => {
                    let need = self.stride * self.slice * 3 / 2;
                    let dest = input.buffer_mut();
                    if dest.len() < need {
                        return Err(format!("MediaCodec: buffer di {} byte, ne servono {need}", dest.len()));
                    }
                    // Y riga per riga (il passo può essere più largo), poi U e V a coppie.
                    let (y, uv) = i420.split_at(w * h);
                    let (u, v) = uv.split_at(w * h / 4);
                    for r in 0..h {
                        for (d, s) in dest[r * self.stride..r * self.stride + w].iter_mut().zip(&y[r * w..(r + 1) * w]) {
                            d.write(*s);
                        }
                    }
                    let base = self.stride * self.slice;
                    for r in 0..h / 2 {
                        let row = &mut dest[base + r * self.stride..base + r * self.stride + w];
                        for c in 0..w / 2 {
                            row[2 * c].write(u[r * (w / 2) + c]);
                            row[2 * c + 1].write(v[r * (w / 2) + c]);
                        }
                    }
                    self.codec.queue_input_buffer(input, 0, need, pts, 0).map_err(|e| format!("MediaCodec: {e}"))?;
                    break;
                }
                DequeuedInputBufferResult::TryAgainLater => {
                    // Il codificatore è pieno: si svuota l'uscita e si riprova.
                    self.raccogli(Duration::from_millis(10))?;
                    tentativi += 1;
                    if tentativi > 300 {
                        return Err("MediaCodec: il codificatore non risponde".into());
                    }
                }
            }
        }
        self.count += 1;
        self.raccogli(Duration::from_millis(0))?;
        Ok(self.ready.pop_front().unwrap_or_default())
    }

    /// Chiude il flusso e restituisce i pacchetti rimasti nel codificatore.
    pub(crate) fn finish(&mut self) -> Result<Vec<(Vec<u8>, bool)>, String> {
        for _ in 0..100 {
            match self.codec.dequeue_input_buffer(Duration::from_millis(20)).map_err(|e| format!("MediaCodec: {e}"))? {
                DequeuedInputBufferResult::Buffer(input) => {
                    let pts = (self.count as f64 * 1_000_000.0 / self.fps as f64) as u64;
                    self.codec.queue_input_buffer(input, 0, 0, pts, FLAG_END).map_err(|e| format!("MediaCodec: {e}"))?;
                    break;
                }
                DequeuedInputBufferResult::TryAgainLater => {
                    self.raccogli(Duration::from_millis(10))?;
                }
            }
        }
        // Fino alla fine del flusso; se il codificatore tace per 1,5 s si prende quel che c'è.
        let mut ultimo = std::time::Instant::now();
        while ultimo.elapsed() < Duration::from_millis(1500) {
            let prima = self.ready.len();
            if self.raccogli(Duration::from_millis(20))? {
                break;
            }
            if self.ready.len() > prima {
                ultimo = std::time::Instant::now();
            }
        }
        Ok(self.ready.drain(..).collect())
    }
}

impl Drop for H264Encoder {
    fn drop(&mut self) {
        let _ = self.codec.stop();
    }
}
