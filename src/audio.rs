use std::fs::File;
use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};
use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, MonoPcm};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub(crate) struct Pcm {
    pub(crate) rate: u32,
    pub(crate) channels: u16,
    pub(crate) samples: Vec<i16>,
    tags: Tags,
}

/// Titolo, artista e album letti dal file di partenza.
#[derive(Default, Clone)]
struct Tags {
    title: String,
    artist: String,
    album: String,
}

/// Le opzioni audio del pannello Conversioni: taglio dell'inizio, canali, volume.
fn shape(mut pcm: Pcm, opts: &crate::engine::Options) -> Result<Pcm, String> {
    let channels = pcm.channels.max(1) as usize;
    if opts.trim_start_secs > 0 {
        let skip = (opts.trim_start_secs as u64 * pcm.rate as u64) as usize * channels;
        if skip >= pcm.samples.len() {
            return Err("il taglio dell'inizio è più lungo dell'audio".into());
        }
        pcm.samples.drain(..skip);
    }
    if let Some(db) = opts.trim_silence_db {
        let threshold = (32768.0 * 10f32.powf(-(db as f32) / 20.0)) as i32;
        let frames = pcm.samples.len() / channels;
        let loud = (0..frames).find(|f| pcm.samples[f * channels..(f + 1) * channels].iter().any(|s| (*s as i32).abs() > threshold));
        let Some(first) = loud else {
            return Err(format!("l'audio è tutto sotto -{db} dB: niente da tenere"));
        };
        // Si tengono 50 ms prima del primo suono, perché l'attacco non suoni tagliato.
        let start = first.saturating_sub(pcm.rate as usize / 20);
        pcm.samples.drain(..start * channels);
    }
    match (opts.channels, pcm.channels) {
        (Some(1), c) if c > 1 => {
            let c = c as usize;
            pcm.samples = pcm
                .samples
                .chunks_exact(c)
                .map(|f| (f.iter().map(|s| *s as i32).sum::<i32>() / c as i32) as i16)
                .collect();
            pcm.channels = 1;
        }
        (Some(2), 1) => {
            pcm.samples = pcm.samples.iter().flat_map(|s| [*s, *s]).collect();
            pcm.channels = 2;
        }
        (Some(2), c) if c > 2 => pcm = fold_stereo(pcm),
        _ => {}
    }
    if opts.normalize {
        let peak = pcm.samples.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0);
        if peak > 0 {
            let gain = 32000.0 / peak as f32;
            for s in &mut pcm.samples {
                *s = (*s as f32 * gain).round().clamp(-32768.0, 32767.0) as i16;
            }
        }
    }
    Ok(pcm)
}

pub fn to_wav(input: &Path, output: &Path, opts: &crate::engine::Options) -> Result<(), String> {
    let pcm = shape(decode(input)?, opts)?;
    write_wav(output, &pcm)
}

pub fn to_mp3(input: &Path, output: &Path, quality: u8, opts: &crate::engine::Options) -> Result<(), String> {
    let mut pcm = shape(decode(input)?, opts)?;
    if !matches!(pcm.rate, 8000 | 11025 | 12000 | 16000 | 22050 | 24000 | 32000 | 44100 | 48000) {
        pcm = resample(pcm, 44100);
    }
    if pcm.channels > 2 {
        pcm = fold_stereo(pcm);
    }
    let channels = pcm.channels.max(1) as u8;
    let bitrate = match opts.mp3_kbps {
        Some(96) => Bitrate::Kbps96,
        Some(128) => Bitrate::Kbps128,
        Some(160) => Bitrate::Kbps160,
        Some(192) => Bitrate::Kbps192,
        Some(256) => Bitrate::Kbps256,
        Some(320) => Bitrate::Kbps320,
        _ if quality < 60 => Bitrate::Kbps96,
        _ if quality < 85 => Bitrate::Kbps128,
        _ => Bitrate::Kbps192,
    };
    let mut builder = Builder::new().ok_or("il codificatore MP3 non parte")?;
    builder.set_num_channels(channels).map_err(|e| e.to_string())?;
    builder.set_sample_rate(pcm.rate).map_err(|e| e.to_string())?;
    builder.set_brate(bitrate).map_err(|e| e.to_string())?;
    let mut encoder = builder.build().map_err(|e| e.to_string())?;
    let mut mp3 = Vec::new();
    let chunk = 1152;
    if channels == 1 {
        let mut at = 0;
        while at < pcm.samples.len() {
            let end = (at + chunk).min(pcm.samples.len());
            encode_block(&mut encoder, MonoPcm(&pcm.samples[at..end]), &mut mp3)?;
            at = end;
            crate::progress::report(at as f32 / pcm.samples.len().max(1) as f32);
        }
    } else {
        let frames = pcm.samples.len() / 2;
        let mut left = Vec::with_capacity(chunk);
        let mut right = Vec::with_capacity(chunk);
        let mut at = 0;
        while at < frames {
            let end = (at + chunk).min(frames);
            left.clear();
            right.clear();
            for frame in at..end {
                left.push(pcm.samples[frame * 2]);
                right.push(pcm.samples[frame * 2 + 1]);
            }
            encode_block(&mut encoder, DualPcm { left: &left, right: &right }, &mut mp3)?;
            at = end;
            if (at / chunk) % 64 == 0 {
                crate::progress::report(at as f32 / frames.max(1) as f32);
            }
        }
    }
    append_mp3(&mut mp3, mp3lame_encoder::max_required_buffer_size(chunk), |buf| {
        encoder.flush::<FlushNoGap>(buf)
    })?;
    let mut file = if opts.keep_tags { id3v2(&pcm.tags) } else { Vec::new() };
    file.extend_from_slice(&mp3);
    std::fs::write(output, file).map_err(|e| e.to_string())
}

/// Il blocco ID3v2.4 (testo UTF-8) con titolo, artista e album; vuoto se non c'è niente da scrivere.
fn id3v2(tags: &Tags) -> Vec<u8> {
    let mut frames = Vec::new();
    for (id, text) in [(b"TIT2", &tags.title), (b"TPE1", &tags.artist), (b"TALB", &tags.album)] {
        if text.is_empty() {
            continue;
        }
        let body_len = 1 + text.len();
        frames.extend_from_slice(id);
        frames.extend_from_slice(&synchsafe(body_len as u32));
        frames.extend_from_slice(&[0, 0, 3]);
        frames.extend_from_slice(text.as_bytes());
    }
    if frames.is_empty() {
        return frames;
    }
    let mut out = b"ID3\x04\x00\x00".to_vec();
    out.extend_from_slice(&synchsafe(frames.len() as u32));
    out.extend_from_slice(&frames);
    out
}

fn synchsafe(n: u32) -> [u8; 4] {
    [((n >> 21) & 0x7F) as u8, ((n >> 14) & 0x7F) as u8, ((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8]
}

fn encode_block(
    encoder: &mut mp3lame_encoder::Encoder,
    input: impl mp3lame_encoder::EncoderInput,
    mp3: &mut Vec<u8>,
) -> Result<(), String> {
    let spare = mp3lame_encoder::max_required_buffer_size(4096);
    append_mp3(mp3, spare, |buf| encoder.encode(input, buf))
}

fn append_mp3(
    mp3: &mut Vec<u8>,
    spare: usize,
    write: impl FnOnce(&mut [std::mem::MaybeUninit<u8>]) -> Result<usize, mp3lame_encoder::EncodeError>,
) -> Result<(), String> {
    mp3.reserve(spare);
    let start = mp3.len();
    let n = write(mp3.spare_capacity_mut()).map_err(|e| e.to_string())?;
    if start + n > mp3.capacity() {
        return Err("buffer MP3 troppo corto".into());
    }
    // LAME initializes the first `n` bytes of the spare capacity.
    unsafe { mp3.set_len(start + n) };
    Ok(())
}

/// L'onda di un file audio in `n` colonne (0..1), per l'anteprima: un colpo d'occhio su dove c'è suono.
pub fn waveform(path: &Path, n: usize) -> Option<Vec<f32>> {
    let pcm = decode(path).ok()?;
    let ch = pcm.channels.max(1) as usize;
    let frames = pcm.samples.len() / ch;
    if frames == 0 || n == 0 {
        return None;
    }
    let mut v: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = (i * frames / n, ((i + 1) * frames / n).max(i * frames / n + 1).min(frames));
            let s = &pcm.samples[a * ch..b * ch];
            let sum: f64 = s.iter().map(|x| (*x as f64).powi(2)).sum();
            (sum / s.len().max(1) as f64).sqrt() as f32
        })
        .collect();
    let max = v.iter().cloned().fold(0.0f32, f32::max).max(1.0);
    v.iter_mut().for_each(|x| *x = (*x / max).clamp(0.0, 1.0));
    Some(v)
}

pub(crate) fn decode(path: &Path) -> Result<Pcm, String> {
    let first = decode_symphonia(path);
    // Android: i formati che Cedro non sa leggere (Opus, AMR…) li decodifica il telefono.
    #[cfg(target_os = "android")]
    if first.is_err() {
        if let Ok(pcm) = decode_telefono(path) {
            return Ok(pcm);
        }
    }
    first
}

#[cfg(target_os = "android")]
fn decode_telefono(path: &Path) -> Result<Pcm, String> {
    let mut decoder = crate::mediadec::AudioDecoder::open(path)?;
    let (mut channels, mut rate) = (2u16, 44_100u32);
    let mut samples = Vec::new();
    while let Some((chunk, c, r)) = decoder.next()? {
        (channels, rate) = (c, r);
        samples.extend(chunk);
    }
    if samples.is_empty() {
        return Err("audio non decodificato".into());
    }
    Ok(Pcm { rate, channels, samples, tags: Tags::default() })
}

fn decode_symphonia(path: &Path) -> Result<Pcm, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut probed = symphonia::default::get_probe()
        .format(&hint, source, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("audio non letto: {e}"))?;
    let mut tags = Tags::default();
    let mut collect = |rev: &symphonia::core::meta::MetadataRevision| {
        use symphonia::core::meta::StandardTagKey::*;
        for tag in rev.tags() {
            let slot = match tag.std_key {
                Some(TrackTitle) => &mut tags.title,
                Some(Artist) => &mut tags.artist,
                Some(Album) => &mut tags.album,
                _ => continue,
            };
            if slot.is_empty() {
                *slot = tag.value.to_string();
            }
        }
    };
    if let Some(mut log) = probed.metadata.get() {
        if let Some(rev) = log.skip_to_latest() {
            collect(rev);
        }
    }
    if let Some(rev) = probed.format.metadata().skip_to_latest() {
        collect(rev);
    }
    let mut format = probed.format;
    // Nei video la traccia predefinita può essere quella delle immagini: si prende la prima con un audio.
    let track = format
        .default_track()
        .filter(|t| t.codec_params.sample_rate.is_some())
        .or_else(|| format.tracks().iter().find(|t| t.codec_params.sample_rate.is_some()))
        .or_else(|| format.default_track())
        .ok_or("l'audio non ha una traccia")?
        .clone();
    let track_id = track.id;
    #[cfg(any(windows, target_os = "linux", target_os = "android"))]
    if track.codec_params.codec == symphonia::core::codecs::CODEC_TYPE_OPUS {
        let head = track.codec_params.extra_data.clone().unwrap_or_default();
        let (samples, channels) = decode_opus(format.as_mut(), track_id, &head)?;
        return Ok(Pcm { rate: 48_000, channels, samples, tags });
    }
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("audio non decodificato: {e}"))?;
    let mut samples = Vec::new();
    let mut rate = track.codec_params.sample_rate.unwrap_or(44100);
    let mut channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(0);
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(SymError::IoError(_)) => break,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymError::DecodeError(_)) => continue,
            Err(_) => break,
        };
        rate = decoded.spec().rate;
        channels = decoded.spec().channels.count() as u16;
        let mut buf = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        buf.copy_interleaved_ref(decoded);
        samples.extend_from_slice(buf.samples());
    }
    if samples.is_empty() || channels == 0 {
        return Err("l'audio non ha campioni".into());
    }
    Ok(Pcm { rate, channels, samples, tags })
}

/// Opus (Ogg, WebM, MKV): i pacchetti li dà symphonia, la decodifica è libopus. Esce a 48 kHz.
#[cfg(any(windows, target_os = "linux", target_os = "android"))]
fn decode_opus(f: &mut dyn symphonia::core::formats::FormatReader, track_id: u32, head: &[u8]) -> Result<(Vec<i16>, u16), String> {
    use audiopus_sys as sys;
    // OpusHead: 8 byte di firma, versione, canali, pre-skip (u16), …
    let channels = match head.get(9) {
        Some(c) if matches!(*c, 1 | 2) => *c as usize,
        _ => return Err("Opus con più di due canali non letto".into()),
    };
    let preskip = if head.len() >= 12 { u16::from_le_bytes([head[10], head[11]]) as usize } else { 0 };
    let mut err = 0;
    let dec = unsafe { sys::opus_decoder_create(48_000, channels as i32, &mut err) };
    if dec.is_null() || err != 0 {
        return Err(format!("decodificatore Opus non creato ({err})"));
    }
    let mut samples: Vec<i16> = Vec::new();
    let mut pcm = vec![0i16; 5760 * channels];
    loop {
        let packet = match f.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let data = packet.buf();
        let n = unsafe { sys::opus_decode(dec, data.as_ptr(), data.len() as i32, pcm.as_mut_ptr(), 5760, 0) };
        if n > 0 {
            samples.extend_from_slice(&pcm[..n as usize * channels]);
        }
    }
    unsafe { sys::opus_decoder_destroy(dec) };
    let skip = (preskip * channels).min(samples.len());
    samples.drain(..skip);
    if samples.is_empty() {
        return Err("l'audio non ha campioni".into());
    }
    Ok((samples, channels as u16))
}

fn write_wav(path: &Path, pcm: &Pcm) -> Result<(), String> {
    let spec = WavSpec {
        channels: pcm.channels,
        sample_rate: pcm.rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for sample in &pcm.samples {
        writer.write_sample(*sample).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())
}

fn resample(pcm: Pcm, to: u32) -> Pcm {
    if pcm.rate == to || pcm.rate == 0 {
        return pcm;
    }
    let channels = pcm.channels as usize;
    let frames = pcm.samples.len() / channels;
    let out_frames = ((frames as u64 * to as u64) / pcm.rate as u64).max(1) as usize;
    let mut samples = Vec::with_capacity(out_frames * channels);
    for frame in 0..out_frames {
        let src = (frame as u64 * pcm.rate as u64) / to as u64;
        let src = (src as usize).min(frames.saturating_sub(1));
        for ch in 0..channels {
            samples.push(pcm.samples[src * channels + ch]);
        }
    }
    Pcm { rate: to, channels: pcm.channels, samples, tags: pcm.tags }
}

fn fold_stereo(pcm: Pcm) -> Pcm {
    let channels = pcm.channels as usize;
    let frames = pcm.samples.len() / channels;
    let mut samples = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let base = frame * channels;
        samples.push(pcm.samples[base]);
        samples.push(pcm.samples[base + 1]);
    }
    Pcm { rate: pcm.rate, channels: 2, samples, tags: pcm.tags }
}
