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

use crate::Failure;

pub const RATE: u32 = 16_000;

pub fn to_16k_mono(path: &Path) -> Result<Vec<f32>, Failure> {
    let mut resampler: Option<StreamResampler> = None;
    decode_mono(path, &mut |chunk, rate| {
        if resampler.is_none() {
            resampler = Some(StreamResampler::new(rate, RATE).map_err(Failure::Other)?);
        }
        let r = resampler.as_mut().expect("resampler initialized");
        if r.from != rate { return Err(Failure::Other("frequenza audio cambiata durante la lettura".into())); }
        r.push(chunk);
        Ok(())
    })?;
    Ok(resampler.map(|r| r.finish()).unwrap_or_default())
}

fn decode_mono(path: &Path, each: &mut dyn FnMut(&[f32], u32) -> Result<(), Failure>) -> Result<(), Failure> {
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
        return opus_mono(format.as_mut(), track.id, track.codec_params.delay.unwrap_or(0) as usize, each);
    }
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
        mono.clear();
        mono.extend(buf.samples().chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
        each(&mono, spec.rate)?;
    }
    Ok(())
}

/// Opus in Ogg o in MKV/WebM: libopus decodifica già in mono a 48 kHz.
fn opus_mono(format: &mut dyn symphonia::core::formats::FormatReader, track: u32, mut pre_skip: usize, each: &mut dyn FnMut(&[f32], u32) -> Result<(), Failure>) -> Result<(), Failure> {
    let mut decoder = opus::Decoder::new(48_000, opus::Channels::Mono).map_err(|e| Failure::Other(format!("Opus non parte: {e}")))?;
    // 120 ms a 48 kHz: il pacchetto Opus più lungo possibile.
    let mut frame = vec![0f32; 5760];
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(Failure::Other(format!("lettura interrotta: {e}"))),
        };
        if packet.track_id() != track || packet.data.is_empty() {
            continue;
        }
        match decoder.decode_float(&packet.data, &mut frame, false) {
            Ok(n) => { let skip = pre_skip.min(n); pre_skip -= skip; each(&frame[skip..n], 48_000)?; },
            Err(_) => continue,
        }
    }
    Ok(())
}

/// Ricampionamento con un filtro sinc a finestra di Hann, a tabella: il passa-basso toglie quello che
/// a 16 kHz non entra senza fare eco (aliasing), e la tabella tiene veloce anche un'ora di audio.

#[path = "resample.rs"]
mod resample;
use resample::StreamResampler;
