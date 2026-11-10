//! symphonia 0.6: le parti comuni a audio, video e anteprima (aprire un file, trovare la traccia audio,
//! creare il decodificatore, avere i campioni in i16 interleaved).

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, Track, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::packet::Packet;
use symphonia::core::units::TimeBase;

pub(crate) use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;

/// Apre un file con symphonia; l'estensione aiuta a riconoscere il formato.
pub(crate) fn open(path: &Path) -> Result<Box<dyn FormatReader>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    open_source(MediaSourceStream::new(Box::new(file), Default::default()), &hint)
}

/// Come `open`, ma da una sorgente già pronta.
pub(crate) fn open_source(source: MediaSourceStream<'static>, hint: &Hint) -> Result<Box<dyn FormatReader>, String> {
    symphonia::default::get_probe()
        .probe(hint, source, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("audio non letto: {e}"))
}

/// I parametri audio di una traccia, se è audio.
pub(crate) fn audio_params(track: &Track) -> Option<&AudioCodecParameters> {
    match track.codec_params.as_ref()? {
        CodecParameters::Audio(params) => Some(params),
        _ => None,
    }
}

/// La traccia audio da leggere: nei video la predefinita può essere quella delle immagini, quindi si
/// prende la prima con un audio vero (con la frequenza), altrimenti la predefinita audio.
pub(crate) fn audio_track(format: &dyn FormatReader) -> Option<Track> {
    let has_rate = |t: &&Track| audio_params(t).is_some_and(|p| p.sample_rate.is_some());
    format
        .default_track(TrackType::Audio)
        .filter(has_rate)
        .or_else(|| format.tracks().iter().find(has_rate))
        .or_else(|| format.default_track(TrackType::Audio))
        .cloned()
}

/// Il decodificatore per i parametri di una traccia audio.
pub(crate) fn decoder(params: &AudioCodecParameters) -> Result<Box<dyn AudioDecoder>, String> {
    symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| format!("audio non decodificato: {e}"))
}

/// I campioni di un blocco decodificato, interleaved a 16 bit, in `out` (che viene svuotato prima).
pub(crate) fn interleaved_i16(decoded: &GenericAudioBufferRef<'_>, out: &mut Vec<i16>) {
    out.clear();
    decoded.copy_to_vec_interleaved(out);
}

/// Frequenza e canali di un blocco decodificato.
pub(crate) fn rate_channels(decoded: &GenericAudioBufferRef<'_>) -> (u32, u16) {
    let spec = decoded.spec();
    (spec.rate(), spec.channels().count() as u16)
}

/// Il silenzio al posto di un pacchetto audio rovinato che il decodificatore ha saltato: senza, tutto l'audio
/// dopo arriverebbe in anticipo sul video (21 ms per ogni pacchetto AAC perso). Vuoto se la durata del
/// pacchetto non si sa o non è credibile (oltre 10 secondi).
pub(crate) fn silence_for(packet: &Packet, time_base: Option<TimeBase>, rate: u32, channels: u16) -> Vec<i16> {
    let Some(base) = time_base else { return Vec::new() };
    let frames = u128::from(packet.dur.get()) * u128::from(base.numer.get()) * u128::from(rate) / u128::from(base.denom.get());
    if frames == 0 || frames > u128::from(rate) * 10 {
        return Vec::new();
    }
    vec![0; frames as usize * channels.max(1) as usize]
}

/// La durata di una traccia in secondi, se il contenitore la dice.
pub(crate) fn track_secs(track: &Track) -> Option<f64> {
    let base = track.time_base?;
    let dur = track.duration.or_else(|| track.num_frames.map(symphonia::core::units::Duration::new))?;
    let secs = base.calc_duration(dur)?.as_secs_f64();
    (secs > 0.0).then_some(secs)
}

/// Il nome breve del codec audio (MP3, AAC, FLAC…), se symphonia lo sa leggere.
pub(crate) fn codec_name(params: &AudioCodecParameters) -> Option<&'static str> {
    symphonia::default::get_codecs().get_audio_decoder(params.codec).map(|d| d.codec.info.short_name)
}

/// La durata del file in secondi: quella del contenitore (MKV e WebM la danno solo così) o la traccia
/// più lunga.
pub(crate) fn media_secs(format: &dyn FormatReader) -> Option<f64> {
    let info = format.media_info();
    let whole = info.time_base.zip(info.duration).and_then(|(base, dur)| base.calc_duration(dur)).map(|t| t.as_secs_f64());
    let longest = format.tracks().iter().filter_map(track_secs).fold(None, |m: Option<f64>, s| Some(m.map_or(s, |m| m.max(s))));
    let secs = whole.into_iter().chain(longest).fold(0.0, f64::max);
    (secs > 0.0).then_some(secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;

    #[test]
    fn silence_matches_the_lost_packet() {
        let packet = Packet::new(1, 0.into(), symphonia::core::units::Duration::new(1024), Vec::new());
        let base = TimeBase { numer: NonZero::new(1).unwrap(), denom: NonZero::new(44_100).unwrap() };
        assert_eq!(silence_for(&packet, Some(base), 44_100, 2).len(), 2048);
        assert!(silence_for(&packet, None, 44_100, 2).is_empty());
        let zero = Packet::new(1, 0.into(), symphonia::core::units::Duration::new(0), Vec::new());
        assert!(silence_for(&zero, Some(base), 44_100, 2).is_empty());
    }
}
