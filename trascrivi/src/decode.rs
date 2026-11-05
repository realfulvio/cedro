//! Da un file audio (o dalla traccia audio di un video) ai campioni che whisper vuole: 16 kHz, mono, f32.
//! Symphonia legge MP3, AAC/M4A, FLAC, Ogg Vorbis, WAV, AIFF, ALAC, MKV e MP4; l'Opus (i messaggi vocali
//! di WhatsApp e Telegram) passa per libopus.

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

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
    let mut format = symphonia::default::get_probe()
        .probe(&hint, stream, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| Failure::Unsupported(format!("formato non riconosciuto ({e})")))?;
    // In un video la prima traccia è spesso l'immagine: si prende la prima che ha un audio.
    let (track, params) = format
        .tracks()
        .iter()
        .find_map(|t| match &t.codec_params {
            Some(CodecParameters::Audio(p)) if p.sample_rate.is_some() => Some((t.clone(), p.clone())),
            _ => None,
        })
        .ok_or_else(|| Failure::Unsupported("nel file non c'è una traccia audio".into()))?;
    if params.codec == CODEC_ID_OPUS {
        return opus_mono(format.as_mut(), track.id, track.delay.unwrap_or(0) as usize, each);
    }
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| Failure::Unsupported(format!("codifica audio non supportata ({e})")))?;
    let mut mono = Vec::new();
    let mut buf: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(Failure::Other(format!("lettura interrotta: {e}"))),
        };
        if packet.track_id != track.id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // Un pacchetto rovinato si salta, come fanno i lettori.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(Failure::Other(format!("decodifica interrotta: {e}"))),
        };
        let rate = decoded.spec().rate();
        let channels = decoded.spec().channels().count().max(1);
        decoded.copy_to_vec_interleaved(&mut buf);
        mono.clear();
        mono.extend(buf.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
        each(&mono, rate)?;
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
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(Failure::Other(format!("lettura interrotta: {e}"))),
        };
        if packet.track_id != track || packet.data.is_empty() {
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
