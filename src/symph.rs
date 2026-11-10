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
use symphonia::core::codecs::audio::well_known::CODEC_ID_AAC;

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

/// I campioni d'avvio («priming») di un AAC: il decodificatore li produce come primi della traccia ma non fanno
/// parte del suono, e se restano l'audio parte ~21 ms (1024 campioni) dopo il video. symphonia non li toglie.
const AAC_PRIMING: u64 = 1024;
/// Oltre questo (circa 90 ms a 44,1 kHz) il valore dell'edit list non è più un priming ma un taglio fatto da un
/// editor: lì non si toglie niente.
const MAX_PRIMING: u64 = 4096;

/// I fotogrammi audio ancora da scartare in testa alla traccia.
pub(crate) struct Priming(usize);

impl Priming {
    /// Niente da scartare.
    pub(crate) fn none() -> Priming {
        Priming(0)
    }

    /// Per una traccia: solo l'AAC ha il priming. In un MP4/MOV lo dice l'edit list (`media_time` della traccia,
    /// 1024 con ffmpeg, 2112 con Apple); senza edit list, o in altri contenitori (MKV…), vale quello standard
    /// dell'AAC LC, 1024.
    pub(crate) fn for_track(path: &Path, track: &Track) -> Priming {
        let Some(params) = audio_params(track) else { return Priming(0) };
        if params.codec != CODEC_ID_AAC {
            return Priming(0);
        }
        let rate = u64::from(params.sample_rate.unwrap_or(0));
        let frames = match mp4_edit_start(path, track.id) {
            Some((media_time, scale)) if rate > 0 && scale > 0 => media_time.saturating_mul(rate) / u64::from(scale),
            _ => AAC_PRIMING,
        };
        Priming(if frames > MAX_PRIMING { 0 } else { frames as usize })
    }

    /// I campioni (interleaved) senza quelli da scartare, che si tolgono da questo e dai blocchi seguenti.
    pub(crate) fn trim<'a>(&mut self, samples: &'a [i16], channels: u16) -> &'a [i16] {
        if self.0 == 0 {
            return samples;
        }
        let channels = usize::from(channels.max(1));
        let drop = (self.0 * channels).min(samples.len());
        self.0 -= drop / channels;
        &samples[drop..]
    }
}

/// Il `media_time` (con la scala dei tempi del media) della prima modifica non vuota dell'edit list della
/// traccia `track_id` di un MP4/MOV; `None` se il file non è un MP4, non ha edit list o non si legge.
fn mp4_edit_start(path: &Path, track_id: u32) -> Option<(u64, u32)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut at = 0u64;
    let mut head = [0u8; 16];
    // Le scatole di primo livello: serve solo `moov`.
    while at + 8 <= len {
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(&mut head[..8]).ok()?;
        let kind = [head[4], head[5], head[6], head[7]];
        if at == 0 && !matches!(&kind, b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free" | b"skip" | b"pnot") {
            return None;
        }
        let mut size = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let mut header = 8u64;
        if size == 1 {
            file.read_exact(&mut head[8..16]).ok()?;
            size = u64::from_be_bytes(head[8..16].try_into().ok()?);
            header = 16;
        } else if size == 0 {
            size = len - at;
        }
        if size < header {
            return None;
        }
        if &kind == b"moov" {
            let payload = size - header;
            if payload > 64 << 20 || at + size > len {
                return None;
            }
            let mut moov = vec![0u8; payload as usize];
            file.read_exact(&mut moov).ok()?;
            return elst_media_time(&moov, track_id);
        }
        at = at.checked_add(size)?;
    }
    None
}

/// Le scatole figlie di un blocco: (tipo, contenuto).
fn child_boxes(mut data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    while data.len() >= 8 {
        let size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let size = if size == 0 { data.len() } else { size };
        if size < 8 || size > data.len() {
            break;
        }
        out.push(([data[4], data[5], data[6], data[7]], &data[8..size]));
        data = &data[size..];
    }
    out
}

fn find_box<'a>(boxes: &[([u8; 4], &'a [u8])], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes.iter().find(|(k, _)| k == kind).map(|(_, body)| *body)
}

/// Dal contenuto di `moov`: `media_time` e scala dei tempi (mdhd) per la traccia `track_id`.
fn elst_media_time(moov: &[u8], track_id: u32) -> Option<(u64, u32)> {
    let be32 = |b: &[u8], at: usize| b.get(at..at + 4).map(|v| u32::from_be_bytes([v[0], v[1], v[2], v[3]]));
    let be64 = |b: &[u8], at: usize| b.get(at..at + 8).map(|v| u64::from_be_bytes(v.try_into().unwrap_or([0; 8])));
    for (kind, trak) in child_boxes(moov) {
        if &kind != b"trak" {
            continue;
        }
        let parts = child_boxes(trak);
        let tkhd = find_box(&parts, b"tkhd")?;
        let id = if *tkhd.first()? == 1 { be32(tkhd, 20)? } else { be32(tkhd, 12)? };
        if id != track_id {
            continue;
        }
        let mdia = child_boxes(find_box(&parts, b"mdia")?);
        let mdhd = find_box(&mdia, b"mdhd")?;
        let scale = if *mdhd.first()? == 1 { be32(mdhd, 20)? } else { be32(mdhd, 12)? };
        let edts = child_boxes(find_box(&parts, b"edts")?);
        let elst = find_box(&edts, b"elst")?;
        let wide = *elst.first()? == 1;
        let count = be32(elst, 4)? as usize;
        let entry = if wide { 20 } else { 12 };
        for k in 0..count.min(64) {
            let base = 8 + k * entry;
            let media_time = if wide { be64(elst, base + 8)? as i64 } else { be32(elst, base + 4)? as i32 as i64 };
            // -1: pausa iniziale (modifica vuota), non c'entra col priming.
            if media_time >= 0 {
                return Some((media_time as u64, scale));
            }
        }
        return None;
    }
    None
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

    fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn trak(id: u32, scale: u32, entries: &[(u32, i32)]) -> Vec<u8> {
        let mut tkhd = vec![0u8; 12];
        tkhd.extend_from_slice(&id.to_be_bytes());
        let mut mdhd = vec![0u8; 12];
        mdhd.extend_from_slice(&scale.to_be_bytes());
        let mut elst = vec![0, 0, 0, 0];
        elst.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (dur, time) in entries {
            elst.extend_from_slice(&dur.to_be_bytes());
            elst.extend_from_slice(&time.to_be_bytes());
            elst.extend_from_slice(&[0, 1, 0, 0]);
        }
        let mut body = mp4_box(b"tkhd", &tkhd);
        body.extend(mp4_box(b"edts", &mp4_box(b"elst", &elst)));
        body.extend(mp4_box(b"mdia", &mp4_box(b"mdhd", &mdhd)));
        mp4_box(b"trak", &body)
    }

    #[test]
    fn edit_list_gives_the_priming() {
        let mut moov = trak(1, 12_800, &[(51_200, 1024)]);
        moov.extend(trak(2, 44_100, &[(1000, -1), (176_400, 2112)]));
        moov.extend(trak(3, 44_100, &[]));
        assert_eq!(elst_media_time(&moov, 1), Some((1024, 12_800)));
        // La pausa iniziale (modifica vuota) non è il priming.
        assert_eq!(elst_media_time(&moov, 2), Some((2112, 44_100)));
        assert_eq!(elst_media_time(&moov, 3), None);
        assert_eq!(elst_media_time(&moov, 9), None);
        assert_eq!(elst_media_time(&moov[..20], 1), None);
    }

    #[test]
    fn priming_is_dropped_across_blocks() {
        let mut priming = Priming(3);
        // Stereo: 2 fotogrammi (4 campioni) poi 2 (4 campioni): ne vanno via 3 fotogrammi in tutto.
        assert!(priming.trim(&[1, 1, 2, 2], 2).is_empty());
        assert_eq!(priming.trim(&[3, 3, 4, 4], 2), &[4, 4]);
        assert_eq!(priming.trim(&[5, 5], 2), &[5, 5]);
        assert_eq!(Priming::none().trim(&[7, 7], 2), &[7, 7]);
    }
}
