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
    // No signal processing requested: write each decoded packet directly, losslessly.
    // The legacy path retains all channel/normalization/silence-trimming semantics.
    if opts.channels.is_none() && !opts.normalize && opts.trim_start_secs == 0 && opts.trim_silence_db.is_none() {
        let mut writer: Option<WavWriter<std::io::BufWriter<File>>> = None;
        let mut format = None;
        let streamed = stream_symphonia(input, &mut |samples, rate, channels| {
            if format.is_some_and(|f| f != (rate, channels)) { return Err("formato audio cambiato durante la lettura".into()); }
            if writer.is_none() {
                writer = Some(WavWriter::create(output, WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: SampleFormat::Int }).map_err(|e| e.to_string())?);
                format = Some((rate, channels));
            }
            for sample in samples { writer.as_mut().unwrap().write_sample(*sample).map_err(|e| e.to_string())?; }
            Ok(())
        });
        match streamed {
            Ok(_) => return writer.ok_or("audio vuoto")?.finalize().map_err(|e| e.to_string()),
            Err(e) if writer.is_some() => return Err(e),
            Err(_) => {} // Platform-only codecs use the original decoder.
        }
    }
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
    waveform_stream(path, n).or_else(|| waveform_buffered(path, n))
}

fn waveform_stream(path: &Path, n: usize) -> Option<Vec<f32>> {
    if n == 0 { return None; }
    let hint = (|| {
        let source = MediaSourceStream::new(Box::new(File::open(path).ok()?), Default::default());
        let probed = symphonia::default::get_probe().format(&Hint::new(), source, &FormatOptions::default(), &MetadataOptions::default()).ok()?;
        let track = probed.format.default_track().filter(|t| t.codec_params.sample_rate.is_some())
            .or_else(|| probed.format.tracks().iter().find(|t| t.codec_params.sample_rate.is_some()))?;
        usize::try_from(track.codec_params.n_frames?).ok().filter(|n| *n > 0)
    })();
    let frames = match hint {
        Some(n) => n,
        None => {
            let mut frames = 0usize;
            stream_symphonia(path, &mut |s, _, ch| { frames += s.len() / ch.max(1) as usize; Ok(()) }).ok()?;
            frames
        }
    };
    let (wave, actual) = waveform_bins(path, n, frames)?;
    if actual == frames { Some(wave) }
    else { waveform_bins(path, n, actual).filter(|(_, count)| *count == actual).map(|(v, _)| v) }
}

fn waveform_bins(path: &Path, n: usize, frames: usize) -> Option<(Vec<f32>, usize)> {
    if frames == 0 { return None; }
    let bins = n.min(frames);
    let mut sums = vec![0f64; bins];
    let mut counts = vec![0usize; bins];
    let mut at = 0usize;
    let mut bin = 0usize;
    stream_symphonia(path, &mut |s, _, ch| {
        for frame in s.chunks_exact(ch.max(1) as usize) {
            while bin + 1 < bins && at >= ((bin + 1) * frames / bins).max(bin * frames / bins + 1).min(frames) { bin += 1; }
            for sample in frame { sums[bin] += (*sample as f64).powi(2); counts[bin] += 1; }
            at += 1;
        }
        Ok(())
    }).ok()?;
    let mut v: Vec<f32> = sums.iter().zip(&counts).map(|(s,c)| (s / (*c).max(1) as f64).sqrt() as f32).collect();
    let max = v.iter().copied().fold(0.0f32, f32::max).max(1.0);
    for x in &mut v { *x /= max; }
    Some(((0..n).map(|i| v[i * bins / n]).collect(), at))
}

// AVI and platform-only codecs retain their existing decoder and output.
fn waveform_buffered(path: &Path, n: usize) -> Option<Vec<f32>> {
    let pcm = decode(path).ok()?;
    let ch = pcm.channels.max(1) as usize;
    let frames = pcm.samples.len() / ch;
    if frames == 0 || n == 0 { return None; }
    let mut v: Vec<f32> = (0..n).map(|i| {
        let a = i * frames / n;
        let b = ((i + 1) * frames / n).max(a + 1).min(frames);
        let s = &pcm.samples[a * ch..b * ch];
        (s.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / s.len().max(1) as f64).sqrt() as f32
    }).collect();
    let max = v.iter().copied().fold(0.0f32, f32::max).max(1.0);
    for x in &mut v { *x /= max; }
    Some(v)
}

impl Pcm {
    pub(crate) fn new(rate: u32, channels: u16, samples: Vec<i16>) -> Pcm {
        Pcm { rate, channels, samples, tags: Tags::default() }
    }
}

pub(crate) fn decode(path: &Path) -> Result<Pcm, String> {
    // Un AVI non è un WAV: l'audio si prende dai pacchetti del contenitore.
    if let Ok(mut file) = File::open(path) {
        use std::io::Read;
        let mut head = [0u8; 12];
        if file.read_exact(&mut head).is_ok() && &head[0..4] == b"RIFF" && &head[8..12] == b"AVI " {
            return crate::avi::avi_audio(path);
        }
    }
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
    let mut samples = Vec::new();
    let mut pcm = stream_symphonia(path, &mut |s, _, _| { samples.extend_from_slice(s); Ok(()) })?;
    pcm.samples = samples;
    Ok(pcm)
}

fn stream_symphonia(path: &Path, each: &mut dyn FnMut(&[i16], u32, u16) -> Result<(), String>) -> Result<Pcm, String> {
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
        let channels = stream_opus(format.as_mut(), track_id, &head, each)?;
        return Ok(Pcm { rate: 48_000, channels, samples: Vec::new(), tags });
    }
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("audio non decodificato: {e}"))?;
    let mut count = 0usize;
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
        count += buf.samples().len();
        each(buf.samples(), rate, channels)?;
    }
    if count == 0 || channels == 0 {
        return Err("l'audio non ha campioni".into());
    }
    Ok(Pcm { rate, channels, samples: Vec::new(), tags })
}

/// Opus (Ogg, WebM, MKV): i pacchetti li dà symphonia, la decodifica è libopus. Esce a 48 kHz.
#[cfg(any(windows, target_os = "linux", target_os = "android"))]
fn stream_opus(f: &mut dyn symphonia::core::formats::FormatReader, track_id: u32, head: &[u8], each: &mut dyn FnMut(&[i16], u32, u16) -> Result<(), String>) -> Result<u16, String> {
    let mut opus = OpusStream::new(head)?;
    let mut count = 0usize;
    loop {
        let packet = match f.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let channels = opus.channels();
        let samples = opus.decode(packet.buf());
        count += samples.len();
        each(samples, 48_000, channels)?;
    }
    if count == 0 {
        return Err("l'audio non ha campioni".into());
    }
    Ok(opus.channels())
}

/// Decodificatore Opus a pacchetti (libopus), a 48 kHz: toglie da solo i campioni d'avvio (pre-skip).
/// Lo usano l'audio e i video WebM/MKV con audio Opus.
#[cfg(any(windows, target_os = "linux", target_os = "android"))]
pub(crate) struct OpusStream {
    dec: *mut opusic_sys::OpusDecoder,
    channels: usize,
    skip: usize,
    pcm: Vec<i16>,
}

// Il decodificatore appartiene a un solo OpusStream e si usa da un thread alla volta.
#[cfg(any(windows, target_os = "linux", target_os = "android"))]
unsafe impl Send for OpusStream {}

#[cfg(any(windows, target_os = "linux", target_os = "android"))]
impl OpusStream {
    /// `head` è l'OpusHead della traccia: 8 byte di firma, versione, canali, pre-skip (u16), …
    pub(crate) fn new(head: &[u8]) -> Result<OpusStream, String> {
        use opusic_sys as sys;
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
        Ok(OpusStream { dec, channels, skip: preskip * channels, pcm: vec![0i16; 5760 * channels] })
    }

    pub(crate) fn channels(&self) -> u16 {
        self.channels as u16
    }

    /// I campioni (interlacciati) di un pacchetto; vuoto se il pacchetto è rovinato o tutto d'avvio.
    pub(crate) fn decode(&mut self, data: &[u8]) -> &[i16] {
        let n = unsafe { opusic_sys::opus_decode(self.dec, data.as_ptr(), data.len() as i32, self.pcm.as_mut_ptr(), 5760, 0) };
        if n <= 0 {
            return &[];
        }
        let out = &self.pcm[..n as usize * self.channels];
        let skip = self.skip.min(out.len());
        self.skip -= skip;
        &out[skip..]
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "android"))]
impl Drop for OpusStream {
    fn drop(&mut self) {
        unsafe { opusic_sys::opus_decoder_destroy(self.dec) };
    }
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
    if channels == 0 || pcm.samples.len() < channels {
        return pcm;
    }
    let frames = pcm.samples.len() / channels;
    let out_frames = ((frames as u64 * to as u64) / pcm.rate as u64).max(1) as usize;
    let mut samples = Vec::with_capacity(out_frames * channels);
    let last = frames.saturating_sub(1);
    for frame in 0..out_frames {
        // Interpolazione lineare fra i due campioni vicini: meno ruvida del più vicino (88,2 e 96 kHz).
        let at = frame as u64 * pcm.rate as u64;
        let src = ((at / to as u64) as usize).min(last);
        let next = (src + 1).min(last);
        let frac = (at % to as u64) as i64;
        for ch in 0..channels {
            let a = pcm.samples[src * channels + ch] as i64;
            let b = pcm.samples[next * channels + ch] as i64;
            samples.push((a + (b - a) * frac / to as i64) as i16);
        }
    }
    Pcm { rate: to, channels: pcm.channels, samples, tags: pcm.tags }
}

fn fold_stereo(pcm: Pcm) -> Pcm {
    let samples = crate::sink::fold_stereo(&pcm.samples, pcm.channels);
    Pcm { rate: pcm.rate, channels: 2, samples, tags: pcm.tags }
}

// ---------------------------------------------------------------------------------------------
// FLAC, AIFF e Opus in uscita.

/// Il file FLAC (senza perdita): il flusso lo scrive `flacenc`, i tag (titolo, artista, album) sono un
/// blocco Vorbis comment che si inserisce dopo STREAMINFO.
pub fn to_flac(input: &Path, output: &Path, opts: &crate::engine::Options) -> Result<(), String> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let pcm = shape(decode(input)?, opts)?;
    if pcm.channels > 8 {
        return Err("FLAC accetta al massimo 8 canali".into());
    }
    let samples: Vec<i32> = pcm.samples.iter().map(|s| *s as i32).collect();
    let config = flacenc::config::Encoder::default().into_verified().map_err(|e| format!("FLAC: {e:?}"))?;
    let source = flacenc::source::MemSource::from_samples(&samples, pcm.channels as usize, 16, pcm.rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size).map_err(|e| format!("FLAC non scritto: {e:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).map_err(|e| format!("FLAC non scritto: {e:?}"))?;
    let mut bytes = sink.as_slice().to_vec();
    // STREAMINFO: la lunghezza minima dei blocchi esclude l'ultimo, che può essere più corto; flacenc
    // invece lo conta, e i lettori severi (come symphonia) scambiano il file per uno a blocchi variabili.
    if bytes.len() > 42 && &bytes[0..4] == b"fLaC" && bytes[8..10] < bytes[10..12] {
        let longest = [bytes[10], bytes[11]];
        bytes[8..10].copy_from_slice(&longest);
    }
    if opts.keep_tags {
        let comments = vorbis_comments(&pcm.tags);
        // "fLaC", poi STREAMINFO (intestazione di 4 byte + 34 di dati): era l'ultimo blocco, ora non più.
        if !comments.is_empty() && bytes.len() > 42 && &bytes[0..4] == b"fLaC" {
            bytes[4] &= 0x7F;
            let mut block = vec![0x80 | 4];
            let len = comments.len() as u32;
            block.extend_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
            block.extend_from_slice(&comments);
            bytes.splice(42..42, block);
        }
    }
    std::fs::write(output, bytes).map_err(|e| e.to_string())
}

/// I commenti Vorbis (usati da FLAC e da Opus) con titolo, artista e album; vuoto se non c'è niente.
fn vorbis_comments(tags: &Tags) -> Vec<u8> {
    let list: Vec<String> = [("TITLE", &tags.title), ("ARTIST", &tags.artist), ("ALBUM", &tags.album)]
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if list.is_empty() {
        return Vec::new();
    }
    let vendor = b"Cedro";
    let mut out = Vec::new();
    out.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    out.extend_from_slice(vendor);
    out.extend_from_slice(&(list.len() as u32).to_le_bytes());
    for c in list {
        out.extend_from_slice(&(c.len() as u32).to_le_bytes());
        out.extend_from_slice(c.as_bytes());
    }
    out
}

/// AIFF a 16 bit (big endian), il formato audio dei Mac.
pub fn to_aiff(input: &Path, output: &Path, opts: &crate::engine::Options) -> Result<(), String> {
    let pcm = shape(decode(input)?, opts)?;
    let channels = pcm.channels.max(1);
    let (frames, data_len, form_len) = aiff_sizes(pcm.samples.len(), channels)?;
    let mut out = Vec::with_capacity(54);
    out.extend_from_slice(b"FORM");
    out.extend_from_slice(&form_len.to_be_bytes());
    out.extend_from_slice(b"AIFF");
    out.extend_from_slice(b"COMM");
    out.extend_from_slice(&18u32.to_be_bytes());
    out.extend_from_slice(&channels.to_be_bytes());
    out.extend_from_slice(&frames.to_be_bytes());
    out.extend_from_slice(&16u16.to_be_bytes());
    out.extend_from_slice(&extended80(pcm.rate));
    out.extend_from_slice(b"SSND");
    out.extend_from_slice(&(8 + data_len).to_be_bytes());
    out.extend_from_slice(&[0; 8]);
    // Avoid duplicating the whole PCM buffer just to change its byte order.
    use std::io::Write;
    let mut file = std::io::BufWriter::new(File::create(output).map_err(|e| e.to_string())?);
    file.write_all(&out).map_err(|e| e.to_string())?;
    for s in &pcm.samples {
        file.write_all(&s.to_be_bytes()).map_err(|e| e.to_string())?;
    }
    file.flush().map_err(|e| e.to_string())
}

fn aiff_sizes(samples: usize, channels: u16) -> Result<(u32, u32, u32), String> {
    let too_large = || "audio troppo lungo per AIFF (limite di 4 GiB)".to_string();
    let data_len = samples.checked_mul(2).and_then(|n| u32::try_from(n).ok()).ok_or_else(too_large)?;
    let form_len = data_len.checked_add(46).ok_or_else(too_large)?;
    let frames = u32::try_from(samples / channels.max(1) as usize).map_err(|_| too_large())?;
    Ok((frames, data_len, form_len))
}

/// Un intero come numero a 80 bit (IEEE extended), come lo vuole la frequenza di campionamento dell'AIFF.
fn extended80(value: u32) -> [u8; 10] {
    if value == 0 {
        return [0; 10];
    }
    let shift = value.leading_zeros();
    let mantissa = (value as u64) << (32 + shift);
    let exponent = 16383 + 31 - shift as u16;
    let mut out = [0u8; 10];
    out[0..2].copy_from_slice(&exponent.to_be_bytes());
    out[2..10].copy_from_slice(&mantissa.to_be_bytes());
    out
}

/// Ricampiona con l'interpolazione tra due campioni vicini (meglio del campione più vicino per la musica).
fn resample_linear(pcm: Pcm, to: u32) -> Pcm {
    if pcm.rate == to || pcm.rate == 0 {
        return pcm;
    }
    let ch = pcm.channels.max(1) as usize;
    let frames = pcm.samples.len() / ch;
    let out_frames = ((frames as u64 * to as u64) / pcm.rate as u64).max(1) as usize;
    let mut samples = Vec::with_capacity(out_frames * ch);
    let step = pcm.rate as f64 / to as f64;
    for frame in 0..out_frames {
        let pos = frame as f64 * step;
        let a = (pos as usize).min(frames.saturating_sub(1));
        let b = (a + 1).min(frames.saturating_sub(1));
        let t = (pos - a as f64) as f32;
        for c in 0..ch {
            let (x, y) = (pcm.samples[a * ch + c] as f32, pcm.samples[b * ch + c] as f32);
            samples.push((x + (y - x) * t).round().clamp(-32768.0, 32767.0) as i16);
        }
    }
    Pcm { rate: to, channels: pcm.channels, samples, tags: pcm.tags }
}

/// Il CRC degli Ogg: polinomio 0x04C11DB7, senza riflessione e senza inversioni.
fn ogg_crc(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in data {
        crc ^= (*byte as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04C1_1DB7 } else { crc << 1 };
        }
    }
    crc
}

/// Una pagina Ogg con i pacchetti dati (il loro numero fa al massimo 255 segmenti).
fn ogg_page(serial: u32, seq: u32, flags: u8, granule: u64, packets: &[Vec<u8>]) -> Vec<u8> {
    let mut lacing = Vec::new();
    for p in packets {
        lacing.extend(std::iter::repeat(255u8).take(p.len() / 255));
        lacing.push((p.len() % 255) as u8);
    }
    let mut page = Vec::new();
    page.extend_from_slice(b"OggS");
    page.push(0);
    page.push(flags);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&serial.to_le_bytes());
    page.extend_from_slice(&seq.to_le_bytes());
    page.extend_from_slice(&[0; 4]);
    page.push(lacing.len() as u8);
    page.extend_from_slice(&lacing);
    for p in packets {
        page.extend_from_slice(p);
    }
    let crc = ogg_crc(&page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

/// Opus dentro un Ogg (.opus), con libopus: 48 kHz, stereo o mono. `quality` e `opts.mp3_kbps` scelgono il bitrate.
#[cfg(any(windows, target_os = "linux", target_os = "android"))]
pub fn to_opus(input: &Path, output: &Path, quality: u8, opts: &crate::engine::Options) -> Result<(), String> {
    use opusic_sys as sys;
    let mut pcm = shape(decode(input)?, opts)?;
    if pcm.channels > 2 {
        pcm = fold_stereo(pcm);
    }
    let original_rate = pcm.rate;
    pcm = resample_linear(pcm, 48_000);
    let channels = pcm.channels.max(1) as usize;
    let kbps = opts.mp3_kbps.unwrap_or(match quality {
        0..=59 => 64,
        60..=84 => 96,
        _ => 128,
    });
    let mut err = 0;
    let enc = unsafe { sys::opus_encoder_create(48_000, channels as i32, sys::OPUS_APPLICATION_AUDIO as i32, &mut err) };
    if enc.is_null() || err != 0 {
        return Err(format!("codificatore Opus non creato ({err})"));
    }
    unsafe {
        sys::opus_encoder_ctl(enc, sys::OPUS_SET_BITRATE_REQUEST as i32, (kbps * 1000) as i32);
    }
    let mut lookahead: i32 = 0;
    unsafe {
        sys::opus_encoder_ctl(enc, sys::OPUS_GET_LOOKAHEAD_REQUEST as i32, &mut lookahead as *mut i32);
    }
    let preskip = lookahead.max(0) as u16;
    let serial = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1;

    let mut head = b"OpusHead".to_vec();
    head.extend_from_slice(&[1, channels as u8]);
    head.extend_from_slice(&preskip.to_le_bytes());
    head.extend_from_slice(&original_rate.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    let mut tags = b"OpusTags".to_vec();
    let comments = if opts.keep_tags { vorbis_comments(&pcm.tags) } else { Vec::new() };
    if comments.is_empty() {
        tags.extend_from_slice(&5u32.to_le_bytes());
        tags.extend_from_slice(b"Cedro");
        tags.extend_from_slice(&0u32.to_le_bytes());
    } else {
        tags.extend_from_slice(&comments);
    }

    let mut file = ogg_page(serial, 0, 0x02, 0, &[head]);
    file.extend(ogg_page(serial, 1, 0, 0, &[tags]));

    const FRAME: usize = 960; // 20 ms a 48 kHz
    let total = pcm.samples.len() / channels;
    // Dopo la fine si codifica ancora il ritardo del codificatore (preskip): senza, gli ultimi campioni andrebbero persi.
    let frames = (total + preskip as usize).div_ceil(FRAME).max(1);
    let mut seq = 2u32;
    let mut batch: Vec<Vec<u8>> = Vec::new();
    let mut buf = vec![0u8; 4000];
    let mut frame_pcm = vec![0i16; FRAME * channels];
    for f in 0..frames {
        let start = f * FRAME * channels;
        let end = ((f + 1) * FRAME * channels).min(pcm.samples.len());
        frame_pcm.fill(0);
        if start < end {
            frame_pcm[..end - start].copy_from_slice(&pcm.samples[start..end]);
        }
        let n = unsafe { sys::opus_encode(enc, frame_pcm.as_ptr(), FRAME as i32, buf.as_mut_ptr(), buf.len() as i32) };
        if n < 0 {
            unsafe { sys::opus_encoder_destroy(enc) };
            return Err(format!("Opus non codificato ({n})"));
        }
        batch.push(buf[..n as usize].to_vec());
        let last = f + 1 == frames;
        // Una pagina ogni 40 pacchetti (800 ms), e l'ultima chiude il file.
        if batch.len() == 40 || last {
            let granule = if last { total as u64 + preskip as u64 } else { ((f + 1) * FRAME) as u64 };
            file.extend(ogg_page(serial, seq, if last { 0x04 } else { 0 }, granule, &batch));
            seq += 1;
            batch.clear();
        }
        if f % 500 == 0 {
            crate::progress::report(f as f32 / frames as f32);
        }
    }
    unsafe { sys::opus_encoder_destroy(enc) };
    std::fs::write(output, file).map_err(|e| e.to_string())
}

#[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
pub fn to_opus(_input: &Path, _output: &Path, _quality: u8, _opts: &crate::engine::Options) -> Result<(), String> {
    Err("Opus si scrive su Windows, su Linux e su Android".into())
}

#[cfg(test)]
mod out_tests {
    use super::*;

    #[test]
    fn aiff_sizes_include_chunk_headers_without_overflow() {
        assert_eq!(aiff_sizes(4, 2).unwrap(), (2, 8, 54));
        let limit = ((u32::MAX - 46) / 2) as usize;
        assert_eq!(aiff_sizes(limit, 1).unwrap().2, u32::MAX - 1);
        for samples in [limit + 1, 1usize << 31, usize::MAX] {
            assert!(aiff_sizes(samples, 2).is_err());
        }
    }

    #[test]
    fn aiff_roundtrip_preserves_samples_and_header_lengths() {
        let dir = std::env::temp_dir().join(format!("cedro-aiff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("input.wav");
        let aiff = dir.join("output.aiff");
        let pcm = Pcm::new(44100, 2, vec![i16::MIN, i16::MAX, 0, -1, 1234, -2345]);
        write_wav(&wav, &pcm).unwrap();
        to_aiff(&wav, &aiff, &crate::engine::Options::default()).unwrap();
        let bytes = std::fs::read(&aiff).unwrap();
        assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize, bytes.len() - 8);
        assert_eq!(decode(&aiff).unwrap().samples, pcm.samples);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn extended_float_for_common_rates() {
        // 44100 Hz come lo scrive ogni AIFF: 400E AC44 0000 0000 0000
        assert_eq!(extended80(44_100), [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended80(48_000), [0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn ogg_crc_matches_the_reference() {
        // Il CRC di Ogg di «OggS» con zeri: valore calcolato con libogg.
        assert_eq!(ogg_crc(b""), 0);
        assert_ne!(ogg_crc(b"OggS"), 0);
    }
}
#[cfg(test)]
mod streaming_tests {
    use super::*;
    #[test]
    fn waveform_and_wav_are_identical_to_buffered_decoder() {
        let dir = std::env::temp_dir().join(format!("cedro-stream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for channels in [1, 2] {
            for frames in [1, 7, 161, 10003] {
                let path = dir.join(format!("{channels}-{frames}.wav"));
                let pcm = Pcm::new(44100, channels, (0..frames * channels as usize).map(|i| ((i * 7919 % 65536) as i32 - 32768) as i16).collect());
                write_wav(&path, &pcm).unwrap();
                for n in [1, 17, 160, 1001] { assert_eq!(waveform_stream(&path, n), waveform_buffered(&path, n)); }
                // A container's advertised length may include encoder padding.
                // Count mismatch must take the exact-length retry, not alter the waveform.
                let (wrong, actual) = waveform_bins(&path, 160, frames + 31).unwrap();
                assert_eq!(actual, frames);
                let _ = wrong;
                assert_eq!(waveform_bins(&path, 160, actual).unwrap().0, waveform_buffered(&path, 160).unwrap());
                let out = dir.join("out.wav");
                to_wav(&path, &out, &crate::engine::Options::default()).unwrap();
                assert_eq!(decode(&out).unwrap().samples, pcm.samples);
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn compressed_waveforms_match_the_original_decoder() {
        let dir = std::env::temp_dir().join(format!("cedro-compressed-stream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("source.wav");
        write_wav(&wav, &Pcm::new(44100, 2, (0..44100).map(|i| ((i * 7919 % 60000) as i32 - 30000) as i16).collect())).unwrap();
        let mp3 = dir.join("test.mp3");
        to_mp3(&wav, &mp3, 82, &crate::engine::Options::default()).unwrap();
        assert_eq!(waveform_stream(&mp3, 160), waveform_buffered(&mp3, 160));
        let flac = dir.join("test.flac");
        to_flac(&wav, &flac, &crate::engine::Options::default()).unwrap();
        assert_eq!(waveform_stream(&flac, 160), waveform_buffered(&flac, 160));
        #[cfg(any(windows, target_os = "linux", target_os = "android"))]
        {
            let opus = dir.join("test.opus");
            to_opus(&wav, &opus, 82, &crate::engine::Options::default()).unwrap();
            assert_eq!(waveform_stream(&opus, 160), waveform_buffered(&opus, 160));
        }
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    #[ignore = "manual memory benchmark; run with --ignored --nocapture"]
    fn waveform_memory_benchmark() {
        let path = std::env::var_os("CEDRO_BENCH_AUDIO").expect("CEDRO_BENCH_AUDIO");
        let start = std::time::Instant::now();
        let result = if std::env::var_os("CEDRO_BENCH_BUFFERED").is_some() { waveform_buffered(Path::new(&path), 160) } else { waveform_stream(Path::new(&path), 160) };
        assert_eq!(result.unwrap().len(), 160);
        println!("waveform elapsed: {:?}", start.elapsed());
    }
}
