//! Decodifica del video con MediaCodec (hardware) e MediaExtractor, per Android: legge MP4, MOV, MKV, WebM, 3GP con
//! H.264, HEVC, VP8, VP9, AV1 dove il telefono li sa fare, e consegna i fotogrammi in I420, già ruotati come
//! dice il file (i video verticali dei telefoni hanno un'indicazione di rotazione). Se il telefono non decodifica quel
//! formato, `open` dà un errore e chi chiama passa al decodificatore software.

use std::ffi::CString;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::NonNull;
use std::time::Duration;

use ndk::media::media_codec::{DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection};
use ndk::media::media_format::MediaFormat;

const FLAG_END: u32 = 4;
/// COLOR_FormatYUV420Planar / SemiPlanar / Flexible e il semiplanare Qualcomm a 32 byte.
const PLANAR: i32 = 19;
const SEMIPLANAR: i32 = 21;
const FLEXIBLE: i32 = 0x7F42_0888;
const QCOM_SEMI: i32 = 0x7FA3_0C04;

pub(crate) struct Frame {
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Istante di presentazione, in microsecondi.
    pub pts_us: i64,
}

struct Extractor(*mut ndk_sys::AMediaExtractor);

impl Drop for Extractor {
    fn drop(&mut self) {
        unsafe {
            ndk_sys::AMediaExtractor_delete(self.0);
        }
    }
}

pub(crate) struct Decoder {
    extractor: Extractor,
    codec: MediaCodec,
    _file: File,
    rotation: i32,
    pub has_audio: bool,
    /// Fotogrammi al secondo dichiarati dal file (0 se non lo dice).
    pub fps: f32,
    /// Durata dichiarata, in microsecondi (0 se ignota).
    pub duration_us: i64,
    input_done: bool,
    ended: bool,
    format: Option<OutFormat>,
    /// Quando è uscito l'ultimo fotogramma, e quanti ne sono usciti.
    ultimo: std::time::Instant,
    usciti: u64,
}

#[derive(Clone, Copy)]
struct OutFormat {
    width: usize,
    height: usize,
    stride: usize,
    slice: usize,
    color: i32,
    crop: (usize, usize),
}

fn chiave(k: &str) -> CString {
    CString::new(k).unwrap()
}

fn stringa(f: &MediaFormat, k: &str) -> Option<String> {
    let mut out: *const std::os::raw::c_char = std::ptr::null();
    let ok = unsafe { ndk_sys::AMediaFormat_getString(f.as_ptr(), chiave(k).as_ptr(), &mut out) };
    (ok && !out.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(out) }.to_string_lossy().into_owned())
}

impl Decoder {
    /// Apre il file e il decodificatore del primo video. `Err` se il telefono non sa leggerlo.
    pub(crate) fn open(path: &Path) -> Result<Decoder, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        let ex = unsafe { ndk_sys::AMediaExtractor_new() };
        if ex.is_null() {
            return Err("MediaExtractor non disponibile".into());
        }
        let extractor = Extractor(ex);
        let st = unsafe { ndk_sys::AMediaExtractor_setDataSourceFd(ex, file.as_raw_fd(), 0, len as i64) };
        if st != ndk_sys::media_status_t::AMEDIA_OK {
            return Err(format!("MediaExtractor non legge il file ({st:?})"));
        }
        let tracce = unsafe { ndk_sys::AMediaExtractor_getTrackCount(ex) };
        let (mut video, mut has_audio) = (None, false);
        for i in 0..tracce {
            let Some(ptr) = NonNull::new(unsafe { ndk_sys::AMediaExtractor_getTrackFormat(ex, i) }) else { continue };
            let f = unsafe { MediaFormat::from_ptr(ptr) };
            match stringa(&f, "mime") {
                Some(m) if m.starts_with("video/") && video.is_none() => video = Some((i, m, f)),
                Some(m) if m.starts_with("audio/") => has_audio = true,
                _ => {}
            }
        }
        let (indice, mime, mut format) = video.ok_or("nessuna traccia video")?;
        let rotation = format.i32("rotation-degrees").unwrap_or(0).rem_euclid(360);
        let duration_us = format.i64("durationUs").unwrap_or(0);
        let fps = format.i32("frame-rate").map(|f| f as f32).or_else(|| format.f32("frame-rate")).unwrap_or(0.0);
        let codec = MediaCodec::from_decoder_type(&mime).ok_or_else(|| format!("il telefono non decodifica {mime}"))?;
        // Si chiede il formato flessibile: i codificatori lo scrivono nel loro formato vero, che si legge dall'uscita.
        format.set_i32("color-format", FLEXIBLE);
        codec.configure(&format, None, MediaCodecDirection::Decoder).map_err(|e| format!("decodificatore non configurato: {e}"))?;
        codec.start().map_err(|e| format!("decodificatore non avviato: {e}"))?;
        if unsafe { ndk_sys::AMediaExtractor_selectTrack(ex, indice) } != ndk_sys::media_status_t::AMEDIA_OK {
            return Err("traccia video non scelta".into());
        }
        Ok(Decoder { extractor, codec, _file: file, rotation, has_audio, fps, duration_us, input_done: false, ended: false, format: None, ultimo: std::time::Instant::now(), usciti: 0 })
    }
}

fn leggi_formato(codec: &MediaCodec) -> Result<OutFormat, String> {
        let f = codec.output_format();
        let w = f.i32("width").ok_or("larghezza ignota")? as usize;
        let h = f.i32("height").ok_or("altezza ignota")? as usize;
        let crop = match (f.i32("crop-left"), f.i32("crop-top"), f.i32("crop-right"), f.i32("crop-bottom")) {
            (Some(l), Some(t), Some(r), Some(b)) if r >= l && b >= t => ((r - l + 1) as usize, (b - t + 1) as usize),
            _ => (w, h),
        };
        let stride = f.i32("stride").map(|s| s as usize).filter(|s| *s >= w).unwrap_or(w);
        let slice = f.i32("slice-height").map(|s| s as usize).filter(|s| *s >= h).unwrap_or(h);
        let color = f.i32("color-format").unwrap_or(SEMIPLANAR);
        if !matches!(color, PLANAR | SEMIPLANAR | FLEXIBLE | QCOM_SEMI) {
            return Err(format!("formato dei colori {color:#x} non gestito"));
        }
        Ok(OutFormat { width: w, height: h, stride, slice, color, crop })
}

impl Decoder {
    /// Il prossimo fotogramma, in ordine di presentazione. `None` a fine video.
    pub(crate) fn next(&mut self) -> Result<Option<Frame>, String> {
        loop {
            if self.ended {
                return Ok(None);
            }
            if !self.input_done {
                if let DequeuedInputBufferResult::Buffer(mut input) = self.codec.dequeue_input_buffer(Duration::from_millis(0)).map_err(|e| format!("{e}"))? {
                    let dest = input.buffer_mut();
                    let n = unsafe { ndk_sys::AMediaExtractor_readSampleData(self.extractor.0, dest.as_mut_ptr() as *mut u8, dest.len()) };
                    if n < 0 {
                        self.codec.queue_input_buffer(input, 0, 0, 0, FLAG_END).map_err(|e| format!("{e}"))?;
                        self.input_done = true;
                    } else {
                        let t = unsafe { ndk_sys::AMediaExtractor_getSampleTime(self.extractor.0) }.max(0) as u64;
                        self.codec.queue_input_buffer(input, 0, n as usize, t, 0).map_err(|e| format!("{e}"))?;
                        unsafe {
                            ndk_sys::AMediaExtractor_advance(self.extractor.0);
                        }
                    }
                }
            }
            match self.codec.dequeue_output_buffer(Duration::from_millis(5)).map_err(|e| format!("{e}"))? {
                DequeuedOutputBufferInfoResult::Buffer(out) => {
                    let info = *out.info();
                    let flags = info.flags();
                    if self.format.is_none() {
                        self.format = Some(leggi_formato(&self.codec)?);
                    }
                    let fmt = self.format.unwrap();
                    let (from, len) = (info.offset().max(0) as usize, info.size().max(0) as usize);
                    let frame = if len == 0 { None } else { out.buffer().get(from..from + len).map(|d| converti(d, &fmt, self.rotation, info.presentation_time_us())) };
                    self.codec.release_output_buffer(out, false).map_err(|e| format!("{e}"))?;
                    if flags & FLAG_END != 0 {
                        self.ended = true;
                    }
                    match frame {
                        Some(Ok(f)) => {
                            self.ultimo = std::time::Instant::now();
                            self.usciti += 1;
                            return Ok(Some(f));
                        }
                        Some(Err(e)) => return Err(e),
                        None => {}
                    }
                }
                DequeuedOutputBufferInfoResult::OutputFormatChanged => self.format = Some(leggi_formato(&self.codec)?),
                DequeuedOutputBufferInfoResult::OutputBuffersChanged => {}
                DequeuedOutputBufferInfoResult::TryAgainLater => {
                    // Fermo da troppo: se ha già dato fotogrammi il video è finito (alcuni decodificatori non
                    // mandano la fine del flusso sui video piccolissimi), altrimenti non sa leggerlo.
                    let fermo = self.ultimo.elapsed();
                    if (self.input_done && fermo > Duration::from_millis(1500)) || fermo > Duration::from_millis(2500) {
                        if self.usciti > 0 {
                            self.ended = true;
                        } else {
                            return Err("il decodificatore non risponde".into());
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.codec.stop();
    }
}

/// Un buffer di uscita del decodificatore in tre piani I420, tagliato e ruotato.
fn converti(d: &[u8], f: &OutFormat, rotation: i32, pts_us: i64) -> Result<Frame, String> {
    let (w, h) = (f.crop.0 & !1, f.crop.1 & !1);
    let need = f.stride * f.slice * 3 / 2;
    if d.len() < need.min(f.stride * (h + h / 2)) {
        return Err("buffer di decodifica troppo corto".into());
    }
    let mut y = Vec::with_capacity(w * h);
    for r in 0..h {
        y.extend_from_slice(&d[r * f.stride..r * f.stride + w]);
    }
    let (mut u, mut v) = (Vec::with_capacity(w * h / 4), Vec::with_capacity(w * h / 4));
    let base = f.stride * f.slice;
    if f.color == PLANAR {
        let (cs, cslice) = (f.stride / 2, f.slice / 2);
        for r in 0..h / 2 {
            u.extend_from_slice(&d[base + r * cs..base + r * cs + w / 2]);
        }
        for r in 0..h / 2 {
            let o = base + cs * cslice + r * cs;
            v.extend_from_slice(&d[o..o + w / 2]);
        }
    } else {
        for r in 0..h / 2 {
            let row = &d[base + r * f.stride..base + r * f.stride + w];
            for c in 0..w / 2 {
                u.push(row[2 * c]);
                v.push(row[2 * c + 1]);
            }
        }
    }
    let frame = Frame { y, u, v, width: w as u32, height: h as u32, pts_us };
    Ok(match rotation {
        90 => ruota(frame, 90),
        180 => ruota(frame, 180),
        270 => ruota(frame, 270),
        _ => frame,
    })
}

fn ruota_piano(p: &[u8], w: usize, h: usize, gradi: i32) -> Vec<u8> {
    let mut o = vec![0u8; p.len()];
    for y in 0..h {
        for x in 0..w {
            let s = p[y * w + x];
            let d = match gradi {
                90 => x * h + (h - 1 - y),
                180 => (h - 1 - y) * w + (w - 1 - x),
                _ => (w - 1 - x) * h + y,
            };
            o[d] = s;
        }
    }
    o
}

/// Ruota il fotogramma di 90°, 180° o 270° in senso orario.
fn ruota(f: Frame, gradi: i32) -> Frame {
    let (w, h) = (f.width as usize, f.height as usize);
    let (y, u, v) = (ruota_piano(&f.y, w, h, gradi), ruota_piano(&f.u, w / 2, h / 2, gradi), ruota_piano(&f.v, w / 2, h / 2, gradi));
    let (nw, nh) = if gradi == 180 { (f.width, f.height) } else { (f.height, f.width) };
    Frame { y, u, v, width: nw, height: nh, pts_us: f.pts_us }
}

/// Decodifica dell'audio con MediaCodec: per i formati che i decodificatori software di Cedro non leggono (Opus, AMR…).
pub(crate) struct AudioDecoder {
    extractor: Extractor,
    codec: MediaCodec,
    _file: File,
    input_done: bool,
    ended: bool,
    ultimo: std::time::Instant,
    usciti: u64,
}

impl AudioDecoder {
    pub(crate) fn open(path: &Path) -> Result<AudioDecoder, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        let ex = unsafe { ndk_sys::AMediaExtractor_new() };
        if ex.is_null() {
            return Err("MediaExtractor non disponibile".into());
        }
        let extractor = Extractor(ex);
        if unsafe { ndk_sys::AMediaExtractor_setDataSourceFd(ex, file.as_raw_fd(), 0, len as i64) } != ndk_sys::media_status_t::AMEDIA_OK {
            return Err("MediaExtractor non legge il file".into());
        }
        let mut scelta = None;
        for i in 0..unsafe { ndk_sys::AMediaExtractor_getTrackCount(ex) } {
            let Some(ptr) = NonNull::new(unsafe { ndk_sys::AMediaExtractor_getTrackFormat(ex, i) }) else { continue };
            let f = unsafe { MediaFormat::from_ptr(ptr) };
            if let Some(m) = stringa(&f, "mime").filter(|m| m.starts_with("audio/")) {
                scelta = Some((i, m, f));
                break;
            }
        }
        let (indice, mime, format) = scelta.ok_or("nessuna traccia audio")?;
        let codec = MediaCodec::from_decoder_type(&mime).ok_or_else(|| format!("il telefono non decodifica {mime}"))?;
        codec.configure(&format, None, MediaCodecDirection::Decoder).map_err(|e| format!("decodificatore audio non configurato: {e}"))?;
        codec.start().map_err(|e| format!("decodificatore audio non avviato: {e}"))?;
        if unsafe { ndk_sys::AMediaExtractor_selectTrack(ex, indice) } != ndk_sys::media_status_t::AMEDIA_OK {
            return Err("traccia audio non scelta".into());
        }
        Ok(AudioDecoder { extractor, codec, _file: file, input_done: false, ended: false, ultimo: std::time::Instant::now(), usciti: 0 })
    }

    /// Il prossimo pezzo di audio: campioni a 16 bit intercalati, canali e frequenza.
    pub(crate) fn next(&mut self) -> Result<Option<(Vec<i16>, u16, u32)>, String> {
        loop {
            if self.ended {
                return Ok(None);
            }
            if !self.input_done {
                if let DequeuedInputBufferResult::Buffer(mut input) = self.codec.dequeue_input_buffer(Duration::from_millis(0)).map_err(|e| format!("{e}"))? {
                    let dest = input.buffer_mut();
                    let n = unsafe { ndk_sys::AMediaExtractor_readSampleData(self.extractor.0, dest.as_mut_ptr() as *mut u8, dest.len()) };
                    if n < 0 {
                        self.codec.queue_input_buffer(input, 0, 0, 0, FLAG_END).map_err(|e| format!("{e}"))?;
                        self.input_done = true;
                    } else {
                        let t = unsafe { ndk_sys::AMediaExtractor_getSampleTime(self.extractor.0) }.max(0) as u64;
                        self.codec.queue_input_buffer(input, 0, n as usize, t, 0).map_err(|e| format!("{e}"))?;
                        unsafe {
                            ndk_sys::AMediaExtractor_advance(self.extractor.0);
                        }
                    }
                }
            }
            match self.codec.dequeue_output_buffer(Duration::from_millis(5)).map_err(|e| format!("{e}"))? {
                DequeuedOutputBufferInfoResult::Buffer(out) => {
                    let info = *out.info();
                    let f = self.codec.output_format();
                    let (rate, channels) = (f.i32("sample-rate").unwrap_or(44_100) as u32, f.i32("channel-count").unwrap_or(2).max(1) as u16);
                    let float = f.i32("pcm-encoding") == Some(4);
                    let (from, len) = (info.offset().max(0) as usize, info.size().max(0) as usize);
                    let raw = out.buffer().get(from..from + len).unwrap_or(&[]);
                    let samples: Vec<i16> = if float {
                        raw.chunks_exact(4).map(|b| (f32::from_le_bytes([b[0], b[1], b[2], b[3]]).clamp(-1.0, 1.0) * 32767.0) as i16).collect()
                    } else {
                        raw.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
                    };
                    let flags = info.flags();
                    self.codec.release_output_buffer(out, false).map_err(|e| format!("{e}"))?;
                    if flags & FLAG_END != 0 {
                        self.ended = true;
                    }
                    if !samples.is_empty() {
                        self.ultimo = std::time::Instant::now();
                        self.usciti += 1;
                        return Ok(Some((samples, channels, rate)));
                    }
                }
                DequeuedOutputBufferInfoResult::TryAgainLater => {
                    let fermo = self.ultimo.elapsed();
                    if (self.input_done && fermo > Duration::from_millis(1500)) || fermo > Duration::from_millis(2500) {
                        if self.usciti > 0 {
                            self.ended = true;
                        } else {
                            return Err("il decodificatore audio non risponde".into());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl Drop for AudioDecoder {
    fn drop(&mut self) {
        let _ = self.codec.stop();
    }
}
