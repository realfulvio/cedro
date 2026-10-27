//! Da un file audio (o dalla traccia audio di un video) ai campioni che whisper vuole: 16 kHz, mono, f32.
//! Symphonia legge MP3, AAC/M4A, FLAC, Ogg Vorbis, WAV, AIFF, ALAC, MKV e MP4; l'Opus (i messaggi vocali
//! di WhatsApp e Telegram) passa per libopus.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL, CODEC_TYPE_OPUS};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::Failure;

pub const RATE: u32 = 16_000;

pub fn to_16k_mono(path: &Path) -> Result<Vec<f32>, Failure> {
    let (mono, rate) = decode_mono(path)?;
    Ok(resample(&mono, rate, RATE))
}

fn decode_mono(path: &Path) -> Result<(Vec<f32>, u32), Failure> {
    let file = File::open(path).map_err(|e| Failure::Other(format!("il file non si apre: {e}")))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(&ext.to_lowercase());
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| Failure::Unsupported(format!("formato non riconosciuto ({e})")))?;
    let mut format = probed.format;
    // In un video la prima traccia è spesso l'immagine: si prende la prima che ha un audio.
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL && t.codec_params.sample_rate.is_some())
        .ok_or_else(|| Failure::Unsupported("nel file non c'è una traccia audio".into()))?
        .clone();
    if track.codec_params.codec == CODEC_TYPE_OPUS {
        let head = track.codec_params.extra_data.clone().unwrap_or_default();
        return opus_mono(format.as_mut(), track.id, track.codec_params.delay.unwrap_or(0) as usize, &head);
    }
    let rate = track.codec_params.sample_rate.unwrap_or(RATE);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| Failure::Unsupported(format!("codifica audio non supportata ({e})")))?;
    let mut mono = Vec::new();
    let mut buffer: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(Failure::Other(format!("lettura interrotta: {e}"))),
        };
        if packet.track_id() != track.id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // Un pacchetto rovinato si salta, come fanno i lettori.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(Failure::Other(format!("decodifica interrotta: {e}"))),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let buf = buffer.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        if buf.capacity() < decoded.capacity() * channels {
            *buf = SampleBuffer::new(decoded.capacity() as u64, spec);
        }
        buf.copy_interleaved_ref(decoded);
        mono.extend(buf.samples().chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
    }
    Ok((mono, rate))
}

/// Opus (Ogg): i pacchetti li dà symphonia, la decodifica è libopus (statica, dentro l'APK). Esce a 48 kHz mono.
fn opus_mono(f: &mut dyn symphonia::core::formats::FormatReader, track_id: u32, skip: usize, head: &[u8]) -> Result<(Vec<f32>, u32), Failure> {
    use audiopus_sys as sys;
    // OpusHead: 8 byte di firma, versione, canali, pre-skip (u16), …
    let channels = match head.get(9) {
        Some(c) if matches!(*c, 1 | 2) => *c as i32,
        _ => return Err(Failure::Unsupported("Opus con più di due canali".into())),
    };
    let preskip = if head.len() >= 12 { u16::from_le_bytes([head[10], head[11]]) as usize } else { skip };
    let mut err = 0;
    let dec = unsafe { sys::opus_decoder_create(48_000, channels, &mut err) };
    if dec.is_null() || err != 0 {
        return Err(Failure::Other(format!("decodificatore Opus non creato ({err})")));
    }
    let mut mono: Vec<f32> = Vec::new();
    let mut pcm = vec![0f32; 5760 * channels as usize];
    loop {
        let packet = match f.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => {
                unsafe { sys::opus_decoder_destroy(dec) };
                return Err(Failure::Other(format!("lettura interrotta: {e}")));
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        let data = packet.buf();
        let n = unsafe { sys::opus_decode_float(dec, data.as_ptr(), data.len() as i32, pcm.as_mut_ptr(), 5760, 0) };
        if n <= 0 {
            continue; // un pacchetto rovinato si salta
        }
        let ch = channels as usize;
        mono.extend(pcm[..n as usize * ch].chunks_exact(ch).map(|fr| fr.iter().sum::<f32>() / ch as f32));
    }
    unsafe { sys::opus_decoder_destroy(dec) };
    if mono.len() > preskip {
        mono.drain(..preskip);
    }
    Ok((mono, 48_000))
}

/// Ricampionamento con un filtro sinc a finestra di Hann, a tabella: il passa-basso toglie quello che
/// a 16 kHz non entra senza fare eco (aliasing), e la tabella tiene veloce anche un'ora di audio.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    const PHASES: usize = 256;
    let ratio = from as f64 / to as f64;
    let cutoff = (to as f64 / from as f64).min(1.0) * 0.94;
    let half = (8.0 / cutoff).ceil() as isize;
    let taps = (2 * half) as usize;
    let mut table = vec![0f32; PHASES * taps];
    for p in 0..PHASES {
        let frac = p as f64 / PHASES as f64;
        let row = &mut table[p * taps..(p + 1) * taps];
        let mut sum = 0.0;
        for (k, w) in row.iter_mut().enumerate() {
            let t = (k as isize - half + 1) as f64 - frac;
            let x = cutoff * t;
            let sinc = if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
            let hann = 0.5 + 0.5 * (std::f64::consts::PI * t / half as f64).cos();
            let v = if t.abs() >= half as f64 { 0.0 } else { sinc * hann };
            *w = v as f32;
            sum += v;
        }
        for w in row.iter_mut() {
            *w /= sum as f32;
        }
    }
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let last = input.len() as isize - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let center = i as f64 * ratio;
        let base = center.floor() as isize;
        let phase = (((center - base as f64) * PHASES as f64) as usize).min(PHASES - 1);
        let row = &table[phase * taps..(phase + 1) * taps];
        let first = base - half + 1;
        let mut acc = 0f32;
        if first >= 0 && first + taps as isize - 1 <= last {
            let window = &input[first as usize..first as usize + taps];
            for (x, w) in window.iter().zip(row) {
                acc += x * w;
            }
        } else {
            for (k, w) in row.iter().enumerate() {
                let at = (first + k as isize).clamp(0, last) as usize;
                acc += input[at] * w;
            }
        }
        out.push(acc);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f64, rate: u32, secs: f64) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize).map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32 * 0.5).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn speech_band_passes_and_high_tones_are_removed() {
        let low = resample(&tone(1000.0, 48_000, 1.0), 48_000, 16_000);
        assert_eq!(low.len(), 16_000);
        assert!((rms(&low[200..15_800]) - 0.3535).abs() < 0.01, "1 kHz deve restare: {}", rms(&low));
        // 12 kHz a 16 kHz diventerebbe un'eco a 4 kHz: il filtro lo toglie.
        let high = resample(&tone(12_000.0, 48_000, 1.0), 48_000, 16_000);
        assert!(rms(&high[200..15_800]) < 0.01, "12 kHz deve sparire: {}", rms(&high));
        let odd = resample(&tone(440.0, 44_100, 1.0), 44_100, 16_000);
        assert!((odd.len() as i64 - 16_000).abs() <= 1);
        assert!((rms(&odd[200..15_800]) - 0.3535).abs() < 0.01);
    }
}
