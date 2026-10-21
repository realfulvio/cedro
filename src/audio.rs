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

struct Pcm {
    rate: u32,
    channels: u16,
    samples: Vec<i16>,
}

pub fn to_wav(input: &Path, output: &Path) -> Result<(), String> {
    let pcm = decode(input)?;
    write_wav(output, &pcm)
}

pub fn to_mp3(input: &Path, output: &Path, quality: u8) -> Result<(), String> {
    let mut pcm = decode(input)?;
    if !matches!(pcm.rate, 8000 | 11025 | 12000 | 16000 | 22050 | 24000 | 32000 | 44100 | 48000) {
        pcm = resample(pcm, 44100);
    }
    if pcm.channels > 2 {
        pcm = fold_stereo(pcm);
    }
    let channels = pcm.channels.max(1) as u8;
    let bitrate = if quality < 60 {
        Bitrate::Kbps96
    } else if quality < 85 {
        Bitrate::Kbps128
    } else {
        Bitrate::Kbps192
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
        }
    }
    append_mp3(&mut mp3, mp3lame_encoder::max_required_buffer_size(chunk), |buf| {
        encoder.flush::<FlushNoGap>(buf)
    })?;
    std::fs::write(output, mp3).map_err(|e| e.to_string())
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

fn decode(path: &Path) -> Result<Pcm, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, source, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("audio non letto: {e}"))?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("l'audio non ha una traccia")?.clone();
    let track_id = track.id;
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
    Ok(Pcm { rate, channels, samples })
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
    Pcm { rate: to, channels: pcm.channels, samples }
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
    Pcm { rate: pcm.rate, channels: 2, samples }
}
