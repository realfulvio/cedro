//! H.264 e AAC compilati nel programma. Nessun ffmpeg.
//! OpenH264 (BSD-2, Cisco, dal sorgente) e FDK AAC (Fraunhofer).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use fdk_aac::enc::{AudioObjectType, BitRate, ChannelMode, Encoder as AacEncoder, EncoderParams, Transport};
use openh264::decoder::Decoder;
use openh264::encoder::{BitRate as H264Rate, Encoder, EncoderConfig, FrameRate, FrameType, Profile, VuiConfig};
use openh264::formats::{YUVBuffer, YUVSource};
use openh264::OpenH264API;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{decl_codec_type, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::mp4mux::{write_mp4, Movie, Sample};
use crate::sink::VideoSink;

pub(crate) fn to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    let first = to_mp4_with(input, output);
    // Se la scheda video ha sbagliato a metà video, si rifà tutto col processore.
    let shape = crate::sink::shape();
    if first.is_err() && shape.gpu && crate::sink::gpu_used() {
        return crate::sink::with_shape(crate::sink::VideoShape { gpu: false, ..shape }, || to_mp4_with(input, output));
    }
    first
}

/// Android: prima il decodificatore del telefono (hardware, anche HEVC e VP9, con la rotazione dei video verticali);
/// se non sa leggere il file si passa ai decodificatori software di qui sotto.
#[cfg(target_os = "android")]
fn hardware_to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    let mut decoder = crate::mediadec::Decoder::open(input)?;
    let (mut sink, mut queued): (Option<H264Aac>, Vec<EarlyPcm>) = (None, Vec::new());
    let mut last: Option<crate::mediadec::Frame> = None;
    let mut frames = 0u32;
    let mut dur = if decoder.fps > 0.0 { (10_000_000.0 / decoder.fps) as i64 } else { 333_333 };
    while let Some(f) = decoder.next()? {
        if let Some(prev) = last.take() {
            let gap = (f.pts_us - prev.pts_us) * 10;
            if gap > 0 {
                dur = gap;
            }
            let pts = prev.pts_us;
            let owned = OwnedFrame { y: prev.y, u: prev.u, v: prev.v, width: prev.width, height: prev.height };
            feed_frame(&mut sink, &mut queued, output, &owned, dur, pts * 10, false, 0, 0)?;
            frames += 1;
            if decoder.duration_us > 0 {
                crate::progress::report(pts as f32 / decoder.duration_us as f32 * 0.95);
            }
        }
        last = Some(f);
    }
    if let Some(prev) = last {
        let owned = OwnedFrame { y: prev.y, u: prev.u, v: prev.v, width: prev.width, height: prev.height };
        feed_frame(&mut sink, &mut queued, output, &owned, dur, prev.pts_us * 10, false, 0, 0)?;
        frames += 1;
    }
    if frames == 0 {
        return Err("il video non ha fotogrammi".into());
    }
    let mut sink = sink.ok_or_else(|| "il video non ha fotogrammi".to_string())?;
    if decoder.has_audio {
        // L'audio: prima i decodificatori di Cedro (AAC, MP3, FLAC, Vorbis…), poi quello del telefono (Opus, AMR…).
        if write_symphonia_audio(input, &mut sink).is_err() {
            let mut audio = crate::mediadec::AudioDecoder::open(input)?;
            while let Some((samples, channels, rate)) = audio.next()? {
                sink.write_pcm(&samples, channels, rate)?;
            }
        }
    }
    sink.finish()
}

fn to_mp4_with(input: &Path, output: &Path) -> Result<(), String> {
    #[cfg(target_os = "android")]
    match hardware_to_mp4(input, output) {
        Ok(()) => return Ok(()),
        Err(err) => eprintln!("decodifica hardware non usata: {err}"),
    }
    match crate::softvideo::transcode::<H264Aac>(input, output) {
        Ok(()) => Ok(()),
        Err(err) if err == crate::softvideo::FOREIGN => {
            let mut head = [0u8; 12];
            let mut file = File::open(input).map_err(|err| format!("il video non è stato letto: {err}"))?;
            let n = file.read(&mut head).map_err(|err| format!("il video non è stato letto: {err}"))?;
            if head[..n].starts_with(b"RIFF") {
                avi_to_mp4(input, output)
            } else if n >= 8 && &head[4..8] == b"ftyp" {
                iso_to_mp4(input, output)
            } else if n >= 4 && head[..4] == [0x1A, 0x45, 0xDF, 0xA3] {
                mkv_to_mp4(input, output)
            } else {
                container_to_mp4(input, output)
            }
        }
        Err(err) => Err(err),
    }
}

struct AacTrack {
    encoder: AacEncoder,
    channels: usize,
    frame_len: usize,
    rate: u32,
    pending: Vec<i16>,
    packets: Vec<Vec<u8>>,
    asc: Vec<u8>,
    out: Vec<u8>,
}

struct H264Aac {
    path: std::path::PathBuf,
    /// Dimensioni di uscita; quelle dei fotogrammi in ingresso sono `src_w` e `src_h`.
    width: u32,
    height: u32,
    src_w: u32,
    src_h: u32,
    gate: crate::sink::FrameGate,
    muted: bool,
    frame_duration: i64,
    encoder: Option<H264>,
    sps: Vec<u8>,
    pps: Vec<u8>,
    video: Vec<Sample>,
    audio: Option<AacTrack>,
    source_rate: u32,
    source_channels: u16,
}

impl VideoSink for H264Aac {
    fn open(
        output: &Path,
        width: u32,
        height: u32,
        frame_duration: i64,
        audio: Option<(u32, u16)>,
    ) -> Result<Self, String> {
        let (out_w, out_h) = crate::sink::target_size(width, height);
        let shrinking = (out_w, out_h) != (width, height);
        if width < 2 || height < 2 || (!shrinking && (width % 2 == 1 || height % 2 == 1)) {
            return Err(format!("il video è {width}×{height}: H.264 vuole misure pari"));
        }
        let shape = crate::sink::shape();
        let gate = crate::sink::FrameGate::new();
        let audio = if shape.mute { None } else { audio };
        let audio = match audio {
            Some((rate, channels)) => Some(open_aac(rate, channels)?),
            None => None,
        };
        let (source_rate, source_channels) = audio
            .as_ref()
            .map(|track| (track.rate, track.channels as u16))
            .unwrap_or((0, 0));
        Ok(Self {
            path: output.to_path_buf(),
            width: out_w,
            height: out_h,
            src_w: width,
            src_h: height,
            muted: shape.mute,
            frame_duration: frame_duration.max(gate.gap()).max(1),
            gate,
            encoder: None,
            sps: Vec::new(),
            pps: Vec::new(),
            video: Vec::new(),
            audio,
            source_rate,
            source_channels,
        })
    }

    fn write_yuv(
        &mut self,
        y: &[u8],
        u: &[u8],
        v: &[u8],
        width: u32,
        height: u32,
        time: i64,
        duration: i64,
    ) -> Result<(), String> {
        if width != self.src_w || height != self.src_h {
            return Err("il video cambia dimensione".into());
        }
        if !self.gate.pass(time) {
            return Ok(());
        }
        let duration = duration.max(self.gate.gap());
        if self.encoder.is_none() {
            self.encoder = Some(open_h264(self.width, self.height, self.frame_duration)?);
        }
        let packed = if (width, height) != (self.width, self.height) {
            let (ry, ru, rv) = crate::sink::resize_i420(y, u, v, width, height, self.width, self.height);
            pack_i420(&ry, &ru, &rv, self.width as usize, self.height as usize)?
        } else {
            pack_i420(y, u, v, self.width as usize, self.height as usize)?
        };
        if let Some(H264::Gpu(gpu)) = self.encoder.as_mut() {
            match gpu.encode(&packed) {
                Ok((bytes, sync)) => {
                    crate::sink::note_gpu();
                    return self.push_annex(&bytes, duration, sync);
                }
                // Al primo fotogramma si passa al processore senza perdere niente.
                Err(_) if self.video.is_empty() => {
                    self.encoder = Some(H264::Soft(open_soft_h264(self.width, self.height, self.frame_duration)?));
                }
                Err(err) => return Err(err),
            }
        }
        let Some(H264::Soft(encoder)) = self.encoder.as_mut() else { unreachable!("codificatore") };
        let picture = YUVBuffer::from_vec(packed, self.width as usize, self.height as usize);
        let encoded = encoder.encode(&picture).map_err(|err| format!("H.264 non scritto: {err}"))?;
        if matches!(encoded.frame_type(), FrameType::Skip | FrameType::Invalid) && encoded.to_vec().is_empty() {
            return Ok(());
        }
        let bytes = encoded.to_vec();
        let sync = matches!(encoded.frame_type(), FrameType::IDR | FrameType::I);
        drop(encoded);
        self.push_annex(&bytes, duration, sync)
    }

    fn write_pcm(&mut self, samples: &[i16], channels: u16, rate: u32) -> Result<(), String> {
        if samples.is_empty() || self.muted {
            return Ok(());
        }
        if self.audio.is_none() {
            let track = open_aac(rate, channels)?;
            self.source_rate = track.rate;
            self.source_channels = track.channels as u16;
            self.audio = Some(track);
        }
        let prepared = shape_pcm(samples, channels, rate, self.source_channels, self.source_rate);
        let track = self.audio.as_mut().expect("audio");
        track.pending.extend(prepared);
        track.drain(false)
    }

    fn finish(mut self) -> Result<(), String> {
        // Il codificatore della scheda video risponde con qualche fotogramma di ritardo: si svuota.
        #[cfg(target_os = "android")]
        {
            let rest = match self.encoder.as_mut() {
                Some(H264::Gpu(gpu)) => gpu.finish()?,
                _ => Vec::new(),
            };
            for (bytes, sync) in rest {
                let duration = self.frame_duration;
                self.push_annex(&bytes, duration, sync)?;
            }
        }
        if let Some(track) = self.audio.as_mut() {
            track.drain(true)?;
        }
        if self.video.is_empty() {
            return Err("il video non ha fotogrammi".into());
        }
        let audio = self.audio.take();
        let (audio_rate, audio_channels, audio_asc, audio_samples) = match audio {
            Some(track) if !track.packets.is_empty() => {
                let samples = track
                    .packets
                    .into_iter()
                    .map(|data| Sample { data, duration: track.frame_len as u32, sync: true })
                    .collect();
                (track.rate, track.channels as u16, track.asc, samples)
            }
            _ => (0, 0, Vec::new(), Vec::new()),
        };
        write_mp4(
            &self.path,
            &Movie {
                width: self.width,
                height: self.height,
                sps: self.sps,
                pps: self.pps,
                video: self.video,
                audio_rate,
                audio_channels,
                audio_asc,
                audio: audio_samples,
            },
        )
    }
}

impl H264Aac {
    fn push_annex(&mut self, bytes: &[u8], duration: i64, mut sync: bool) -> Result<(), String> {
        let mut payload = Vec::new();
        let mut vcl = false;
        for nal in annex_nals(bytes) {
            if nal.is_empty() {
                continue;
            }
            let kind = nal[0] & 0x1f;
            if kind == 7 && self.sps.is_empty() {
                self.sps = nal.to_vec();
            } else if kind == 8 && self.pps.is_empty() {
                self.pps = nal.to_vec();
            }
            if kind == 9 {
                continue;
            }
            if kind == 5 {
                sync = true;
            }
            if (1..=5).contains(&kind) {
                vcl = true;
            }
            payload.extend_from_slice(&(nal.len() as u32).to_be_bytes());
            payload.extend_from_slice(nal);
        }
        if !vcl || payload.is_empty() {
            return Ok(());
        }
        let ticks = if duration > 0 { duration } else { self.frame_duration };
        self.video.push(Sample {
            data: payload,
            duration: duration_90k(ticks),
            sync,
        });
        Ok(())
    }
}

/// Il codificatore H.264: la scheda video (VA-API) se c'è ed è permessa, altrimenti OpenH264.
enum H264 {
    Gpu(crate::gpuenc::H264Encoder),
    Soft(Encoder),
}

fn fps_of(frame_duration: i64) -> f32 {
    if frame_duration > 0 {
        (10_000_000.0 / frame_duration as f32).clamp(1.0, 240.0)
    } else {
        30.0
    }
}

fn open_h264(width: u32, height: u32, frame_duration: i64) -> Result<H264, String> {
    let shape = crate::sink::shape();
    if shape.gpu {
        let bitrate = crate::sink::bitrate(width, height);
        if let Ok(gpu) = crate::gpuenc::H264Encoder::new(width, height, fps_of(frame_duration), bitrate, shape.quality) {
            return Ok(H264::Gpu(gpu));
        }
    }
    open_soft_h264(width, height, frame_duration).map(H264::Soft)
}

fn open_soft_h264(width: u32, height: u32, frame_duration: i64) -> Result<Encoder, String> {
    let fps = fps_of(frame_duration);
    let bitrate = crate::sink::bitrate(width, height);
    let config = EncoderConfig::new()
        .bitrate(H264Rate::from_bps(bitrate))
        .max_frame_rate(FrameRate::from_hz(fps))
        .profile(Profile::Baseline)
        .skip_frames(false)
        .num_threads(1)
        .vui(VuiConfig::bt601());
    Encoder::with_api_config(OpenH264API::from_source(), config).map_err(|err| format!("H.264 non pronto: {err}"))
}

fn open_aac(rate: u32, channels: u16) -> Result<AacTrack, String> {
    let channels = if channels > 2 { 2 } else { channels.max(1) };
    let rate = if rate == 48_000 || rate == 44_100 { rate } else { 44_100 };
    let encoder = AacEncoder::new(EncoderParams {
        bit_rate: BitRate::Cbr(if channels == 1 { 96_000 } else { 128_000 }),
        sample_rate: rate,
        transport: Transport::Raw,
        channels: if channels == 1 { ChannelMode::Mono } else { ChannelMode::Stereo },
        audio_object_type: AudioObjectType::Mpeg4LowComplexity,
    })
    .map_err(|err| format!("AAC non pronto: {err}"))?;
    let info = encoder.info().map_err(|err| format!("AAC non pronto: {err}"))?;
    let frame_len = info.frameLength.max(1) as usize;
    let asc = if info.confSize > 0 {
        info.confBuf[..info.confSize as usize].to_vec()
    } else {
        aac_asc(rate, channels)
    };
    let out_len = (info.maxOutBufBytes as usize).max(2048);
    Ok(AacTrack {
        encoder,
        channels: channels as usize,
        frame_len,
        rate,
        pending: Vec::new(),
        packets: Vec::new(),
        asc,
        out: vec![0; out_len],
    })
}

impl AacTrack {
    fn drain(&mut self, finish: bool) -> Result<(), String> {
        let frame = self.frame_len * self.channels;
        while self.pending.len() >= frame {
            let chunk: Vec<i16> = self.pending.drain(..frame).collect();
            self.encode_frame(&chunk)?;
        }
        if finish && !self.pending.is_empty() {
            self.pending.resize(frame, 0);
            let chunk = std::mem::take(&mut self.pending);
            self.encode_frame(&chunk)?;
        }
        if finish {
            let silence = vec![0i16; frame];
            for _ in 0..2 {
                let before = self.packets.len();
                self.encode_frame(&silence)?;
                if self.packets.len() == before {
                    break;
                }
            }
        }
        Ok(())
    }

    fn encode_frame(&mut self, samples: &[i16]) -> Result<(), String> {
        let info = self
            .encoder
            .encode(samples, &mut self.out)
            .map_err(|err| format!("AAC non scritto: {err}"))?;
        if info.output_size == 0 {
            return Ok(());
        }
        let mut packet = self.out[..info.output_size].to_vec();
        if packet.len() > 7 && packet[0] == 0xff && packet[1] & 0xf0 == 0xf0 {
            packet.drain(..7);
        }
        if !packet.is_empty() {
            self.packets.push(packet);
        }
        Ok(())
    }
}

fn aac_asc(rate: u32, channels: u16) -> Vec<u8> {
    let freq = match rate {
        48_000 => 3,
        _ => 4,
    };
    let channels = channels.min(2) as u8;
    vec![(2 << 3) | (freq >> 1), ((freq & 1) << 7) | (channels << 3)]
}

fn shape_pcm(samples: &[i16], channels: u16, rate: u32, target_channels: u16, target_rate: u32) -> Vec<i16> {
    let channels = channels.max(1);
    let folded = if channels > 2 { fold_stereo(samples, channels) } else { samples.to_vec() };
    let folded_channels = if channels > 2 { 2 } else { channels };
    let fitted = if folded_channels != target_channels && target_channels > 0 {
        refit_channels(&folded, folded_channels, target_channels)
    } else {
        folded
    };
    let used = if target_channels > 0 { target_channels } else { folded_channels };
    resample_pcm(&fitted, used, rate, target_rate)
}

fn refit_channels(samples: &[i16], from: u16, to: u16) -> Vec<i16> {
    let from = from as usize;
    let to = to as usize;
    if from == 0 || to == 0 || from == to {
        return samples.to_vec();
    }
    let frames = samples.len() / from;
    let mut out = Vec::with_capacity(frames * to);
    for frame in 0..frames {
        let base = frame * from;
        if to == 1 {
            let sum: i32 = samples[base..base + from].iter().map(|v| *v as i32).sum();
            out.push((sum / from as i32) as i16);
        } else {
            out.push(samples[base]);
            out.push(samples[base + from.min(2) - 1]);
        }
    }
    out
}

fn fold_stereo(samples: &[i16], channels: u16) -> Vec<i16> {
    let channels = channels as usize;
    let frames = samples.len() / channels;
    let mut out = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let base = frame * channels;
        out.push(samples[base]);
        out.push(samples[base + 1]);
    }
    out
}

fn resample_pcm(samples: &[i16], channels: u16, from: u32, to: u32) -> Vec<i16> {
    if from == 0 || to == 0 || from == to {
        return samples.to_vec();
    }
    let ch = channels as usize;
    let frames = samples.len() / ch.max(1);
    if frames == 0 {
        return Vec::new();
    }
    let out_frames = ((frames as u64 * to as u64) / from as u64).max(1) as usize;
    let mut out = Vec::with_capacity(out_frames * ch);
    for frame in 0..out_frames {
        let src = ((frame as u64 * from as u64) / to as u64) as usize;
        let src = src.min(frames - 1);
        out.extend_from_slice(&samples[src * ch..(src + 1) * ch]);
    }
    out
}

fn pack_i420(y: &[u8], u: &[u8], v: &[u8], w: usize, h: usize) -> Result<Vec<u8>, String> {
    let cw = w / 2;
    let ch = h / 2;
    if w < 2 || h < 2 || y.len() < w * h || u.len() < cw * ch || v.len() < cw * ch {
        return Err("fotogramma troppo corto".into());
    }
    let mut out = Vec::with_capacity(w * h * 3 / 2);
    for row in 0..h {
        out.extend_from_slice(&y[row * w..row * w + w]);
    }
    for row in 0..ch {
        out.extend_from_slice(&u[row * cw..row * cw + cw]);
    }
    for row in 0..ch {
        out.extend_from_slice(&v[row * cw..row * cw + cw]);
    }
    Ok(out)
}

fn duration_90k(ticks_100ns: i64) -> u32 {
    let ticks = ticks_100ns.max(1) as u64;
    let value = ticks.saturating_mul(9) / 1000;
    u32::try_from(value).unwrap_or(u32::MAX).max(1)
}

fn annex_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut marks = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if i + 4 <= data.len() && data[i..i + 4] == [0, 0, 0, 1] {
            marks.push((i, i + 4));
            i += 4;
            continue;
        }
        if data[i..i + 3] == [0, 0, 1] {
            marks.push((i, i + 3));
            i += 3;
            continue;
        }
        i += 1;
    }
    marks
        .iter()
        .enumerate()
        .filter_map(|(index, (_, start))| {
            let end = marks.get(index + 1).map(|(at, _)| *at).unwrap_or(data.len());
            if *start < end { Some(&data[*start..end]) } else { None }
        })
        .collect()
}

fn yuv(r: i32, g: i32, b: i32) -> (u8, u8, u8) {
    let y = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
    let u = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
    let v = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
    (y, u, v)
}

struct OwnedFrame {
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
    width: u32,
    height: u32,
}

fn owned_frame(frame: &impl YUVSource) -> Result<OwnedFrame, String> {
    let (w, h) = frame.dimensions();
    let (sy, su, sv) = frame.strides();
    if w < 2 || h < 2 {
        return Err("fotogramma troppo corto".into());
    }
    Ok(OwnedFrame {
        y: copy_plane(frame.y(), w, h, sy)?,
        u: copy_plane(frame.u(), w / 2, h / 2, su)?,
        v: copy_plane(frame.v(), w / 2, h / 2, sv)?,
        width: w as u32,
        height: h as u32,
    })
}

fn copy_plane(src: &[u8], width: usize, height: usize, stride: usize) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || stride < width {
        return Err("fotogramma troppo corto".into());
    }
    let mut out = Vec::with_capacity(width * height);
    for row in 0..height {
        let at = row.saturating_mul(stride);
        let end = at + width;
        if end > src.len() {
            return Err("fotogramma troppo corto".into());
        }
        out.extend_from_slice(&src[at..end]);
    }
    Ok(out)
}

fn push_decoded(sink: &mut H264Aac, frame: &OwnedFrame, duration: i64, time: i64) -> Result<(), String> {
    sink.write_yuv(&frame.y, &frame.u, &frame.v, frame.width, frame.height, time, duration)
}

struct EarlyPcm {
    samples: Vec<i16>,
    channels: u16,
    rate: u32,
}

fn iso_to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    let file = File::open(input).map_err(|err| format!("il video non è stato letto: {err}"))?;
    let size = file.metadata().map_err(|err| format!("il video non è stato letto: {err}"))?.len();
    let mut reader = match mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size) {
        Ok(reader) => reader,
        // Il crate mp4 scambia i campi della descrizione audio QuickTime v1 per una scatola.
        Err(err) => {
            return iso_lenient(input, output).map_err(|fallback| {
                if fallback == "il video non è stato letto" {
                    format!("il video non è stato letto: {err}")
                } else {
                    fallback
                }
            });
        }
    };
    let (track_id, timescale, count, sps, pps, wants_audio) = {
        let tracks = reader.tracks();
        let mut video_info = None;
        let mut wants_audio = false;
        for track in tracks.values() {
            match track.media_type().ok() {
                Some(mp4::MediaType::H264) if video_info.is_none() => {
                    let sps = track.sequence_parameter_set().map_err(|_| "H.264 senza parametri".to_string())?.to_vec();
                    let pps = track.picture_parameter_set().map_err(|_| "H.264 senza parametri".to_string())?.to_vec();
                    video_info = Some((track.track_id(), track.timescale().max(1), track.sample_count(), sps, pps));
                }
                Some(mp4::MediaType::AAC) => wants_audio = true,
                _ => {}
            }
        }
        let (id, scale, count, sps, pps) = video_info.ok_or_else(|| "il video non è stato letto".to_string())?;
        (id, scale, count, sps, pps, wants_audio)
    };
    let mut header = Vec::new();
    for nal in [&sps, &pps] {
        if !nal.is_empty() {
            header.extend_from_slice(&[0, 0, 0, 1]);
            header.extend_from_slice(nal);
        }
    }
    let mut decoder = Decoder::new().map_err(|err| format!("H.264 non letto: {err}"))?;
    if !header.is_empty() {
        let _ = decoder.decode(&header);
    }
    let mut sink = None;
    let mut queued = Vec::new();
    let mut shown = 0u32;
    for index in 1..=count {
        let Some(sample) = reader
            .read_sample(track_id, index)
            .map_err(|err| format!("il video non è stato letto: {err}"))?
        else {
            continue;
        };
        let annex = annex_sample(&sample.bytes);
        if annex.is_empty() {
            continue;
        }
        let duration = ticks(u64::from(sample.duration), 1, timescale).max(1);
        let time = ticks(sample.start_time, 1, timescale);
        if let Some(frame) = decoder.decode(&annex).map_err(|err| format!("H.264 non letto: {err}"))? {
            let owned = owned_frame(&frame)?;
            feed_frame(&mut sink, &mut queued, output, &owned, duration, time, false, 0, 0)?;
            shown += 1;
        }
    }
    if let Ok(rest) = decoder.flush_remaining() {
        for frame in &rest {
            let owned = owned_frame(frame)?;
            feed_frame(&mut sink, &mut queued, output, &owned, 333_333, 0, false, 0, 0)?;
            shown += 1;
        }
    }
    if shown == 0 {
        return Err("il video non ha fotogrammi".into());
    }
    let mut sink = sink.ok_or_else(|| "il video non ha fotogrammi".to_string())?;
    if wants_audio {
        write_symphonia_audio(input, &mut sink)?;
    }
    sink.finish()
}

struct IsoSpan {
    kind: [u8; 4],
    payload: u64,
    end: u64,
}

struct IsoSample {
    offset: u64,
    size: u32,
    duration: u32,
    start: u64,
}

struct IsoTrack {
    handler: [u8; 4],
    timescale: u32,
    sps: Vec<u8>,
    pps: Vec<u8>,
    samples: Vec<IsoSample>,
}

fn iso_lenient(input: &Path, output: &Path) -> Result<(), String> {
    let mut file = File::open(input).map_err(|err| format!("il video non è stato letto: {err}"))?;
    let len = file.metadata().map_err(|err| format!("il video non è stato letto: {err}"))?.len();
    let mut video = None;
    let mut wants_audio = false;
    while let Some(span) = read_span(&mut file, len)? {
        if span.kind == *b"moov" {
            file.seek(SeekFrom::Start(span.payload)).map_err(|err| format!("il video non è stato letto: {err}"))?;
            while let Some(child) = read_span(&mut file, span.end)? {
                if child.kind == *b"trak" {
                    let trak = parse_trak(&mut file, &child)?;
                    if &trak.handler == b"vide" && video.is_none() {
                        video = Some(trak);
                    } else if &trak.handler == b"soun" {
                        wants_audio = true;
                    }
                }
                file.seek(SeekFrom::Start(child.end)).map_err(|err| format!("il video non è stato letto: {err}"))?;
            }
        }
        file.seek(SeekFrom::Start(span.end)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    }
    let video = video.ok_or("il video non è stato letto")?;
    let mut header = Vec::new();
    for nal in [&video.sps, &video.pps] {
        if !nal.is_empty() {
            header.extend_from_slice(&[0, 0, 0, 1]);
            header.extend_from_slice(nal);
        }
    }
    let mut decoder = Decoder::new().map_err(|err| format!("H.264 non letto: {err}"))?;
    if !header.is_empty() {
        let _ = decoder.decode(&header);
    }
    let mut sink = None;
    let mut queued = Vec::new();
    let mut shown = 0u32;
    let mut sample_bytes = Vec::new();
    for sample in &video.samples {
        sample_bytes.resize(sample.size as usize, 0);
        file.seek(SeekFrom::Start(sample.offset)).map_err(|err| format!("il video non è stato letto: {err}"))?;
        file.read_exact(&mut sample_bytes).map_err(|err| format!("il video non è stato letto: {err}"))?;
        let annex = annex_sample(&sample_bytes);
        if annex.is_empty() {
            continue;
        }
        let duration = ticks(u64::from(sample.duration), 1, video.timescale).max(1);
        let time = ticks(sample.start, 1, video.timescale);
        if let Some(frame) = decoder.decode(&annex).map_err(|err| format!("H.264 non letto: {err}"))? {
            let owned = owned_frame(&frame)?;
            feed_frame(&mut sink, &mut queued, output, &owned, duration, time, false, 0, 0)?;
            shown += 1;
        }
    }
    if let Ok(rest) = decoder.flush_remaining() {
        for frame in &rest {
            let owned = owned_frame(frame)?;
            feed_frame(&mut sink, &mut queued, output, &owned, 333_333, 0, false, 0, 0)?;
            shown += 1;
        }
    }
    if shown == 0 {
        return Err("il video non ha fotogrammi".into());
    }
    let mut sink = sink.ok_or_else(|| "il video non ha fotogrammi".to_string())?;
    if wants_audio {
        write_symphonia_audio(input, &mut sink)?;
    }
    sink.finish()
}

fn parse_trak(file: &mut File, trak: &IsoSpan) -> Result<IsoTrack, String> {
    let mut found = None;
    file.seek(SeekFrom::Start(trak.payload)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    while let Some(child) = read_span(file, trak.end)? {
        if child.kind == *b"mdia" {
            found = Some(parse_mdia(file, &child)?);
        }
        file.seek(SeekFrom::Start(child.end)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    }
    found.ok_or_else(|| "il video non è stato letto".to_string())
}

fn parse_mdia(file: &mut File, mdia: &IsoSpan) -> Result<IsoTrack, String> {
    let mut timescale = 1u32;
    let mut handler = [0u8; 4];
    let mut stbl = None;
    file.seek(SeekFrom::Start(mdia.payload)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    while let Some(child) = read_span(file, mdia.end)? {
        match &child.kind {
            b"mdhd" => timescale = timescale_of(&read_payload(file, &child)?),
            b"hdlr" => handler = handler_of(&read_payload(file, &child)?),
            b"minf" => stbl = find_stbl(file, &child)?,
            _ => {}
        }
        file.seek(SeekFrom::Start(child.end)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    }
    if &handler != b"vide" {
        return Ok(IsoTrack { handler, timescale, sps: Vec::new(), pps: Vec::new(), samples: Vec::new() });
    }
    let stbl = stbl.ok_or("il video non è stato letto")?;
    let (sps, pps) = avc_from_stsd(file, &stbl)?;
    let samples = sample_index(file, &stbl)?;
    Ok(IsoTrack { handler, timescale, sps, pps, samples })
}

fn find_stbl(file: &mut File, minf: &IsoSpan) -> Result<Option<IsoSpan>, String> {
    let mut stbl = None;
    file.seek(SeekFrom::Start(minf.payload)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    while let Some(child) = read_span(file, minf.end)? {
        if child.kind == *b"stbl" {
            stbl = Some(child);
            break;
        }
        file.seek(SeekFrom::Start(child.end)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    }
    Ok(stbl)
}

fn avc_from_stsd(file: &mut File, stbl: &IsoSpan) -> Result<(Vec<u8>, Vec<u8>), String> {
    file.seek(SeekFrom::Start(stbl.payload)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    while let Some(child) = read_span(file, stbl.end)? {
        if child.kind == *b"stsd" {
            file.seek(SeekFrom::Start(child.payload + 8)).map_err(|err| format!("il video non è stato letto: {err}"))?;
            let Some(entry) = read_span(file, child.end)? else {
                return Err("il video non è stato letto".into());
            };
            if &entry.kind != b"avc1" && &entry.kind != b"avc3" {
                return Err("il video non è stato letto".into());
            }
            let visual = 78u64;
            if entry.payload + visual > entry.end {
                return Err("il video non è stato letto".into());
            }
            file.seek(SeekFrom::Start(entry.payload + visual)).map_err(|err| format!("il video non è stato letto: {err}"))?;
            while let Some(inner) = read_span(file, entry.end)? {
                if inner.kind == *b"avcC" {
                    return split_avcc(&read_payload(file, &inner)?);
                }
                file.seek(SeekFrom::Start(inner.end)).map_err(|err| format!("il video non è stato letto: {err}"))?;
            }
            return Err("il video non è stato letto".into());
        }
        file.seek(SeekFrom::Start(child.end)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    }
    Err("il video non è stato letto".into())
}

fn split_avcc(raw: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    if raw.len() < 7 {
        return Err("H.264 senza parametri".into());
    }
    let mut i = 5usize;
    let sps_count = (raw[i] & 0x1f) as usize;
    i += 1;
    let mut sps = Vec::new();
    for n in 0..sps_count {
        let nal = take_be_nal(raw, &mut i)?;
        if n == 0 {
            sps = nal;
        }
    }
    if i >= raw.len() {
        return Err("H.264 senza parametri".into());
    }
    let pps_count = raw[i] as usize;
    i += 1;
    let mut pps = Vec::new();
    for n in 0..pps_count {
        let nal = take_be_nal(raw, &mut i)?;
        if n == 0 {
            pps = nal;
        }
    }
    if sps.is_empty() || pps.is_empty() {
        return Err("H.264 senza parametri".into());
    }
    Ok((sps, pps))
}

fn take_be_nal(raw: &[u8], i: &mut usize) -> Result<Vec<u8>, String> {
    if *i + 2 > raw.len() {
        return Err("H.264 senza parametri".into());
    }
    let len = u16::from_be_bytes([raw[*i], raw[*i + 1]]) as usize;
    *i += 2;
    if *i + len > raw.len() {
        return Err("H.264 senza parametri".into());
    }
    let nal = raw[*i..*i + len].to_vec();
    *i += len;
    Ok(nal)
}

fn sample_index(file: &mut File, stbl: &IsoSpan) -> Result<Vec<IsoSample>, String> {
    let mut stts = None;
    let mut stsc = None;
    let mut stsz = None;
    let mut stco = None;
    let mut co64 = None;
    file.seek(SeekFrom::Start(stbl.payload)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    while let Some(child) = read_span(file, stbl.end)? {
        let raw = read_payload(file, &child)?;
        match &child.kind {
            b"stts" => stts = Some(raw),
            b"stsc" => stsc = Some(raw),
            b"stsz" => stsz = Some(raw),
            b"stco" => stco = Some(raw),
            b"co64" => co64 = Some(raw),
            _ => {}
        }
        file.seek(SeekFrom::Start(child.end)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    }
    let sizes = sample_sizes(&stsz.ok_or("il video non è stato letto")?)?;
    let chunks = chunk_offsets(stco.as_deref(), co64.as_deref())?;
    let per_chunk = samples_per_chunk(&stsc.ok_or("il video non è stato letto")?, chunks.len())?;
    let runs = time_runs(&stts.ok_or("il video non è stato letto")?)?;
    let mut samples = Vec::with_capacity(sizes.len());
    let mut index = 0usize;
    for (chunk, base) in chunks.iter().enumerate() {
        let mut offset = *base;
        let count = per_chunk.get(chunk).copied().unwrap_or(0) as usize;
        for _ in 0..count {
            if index >= sizes.len() {
                break;
            }
            samples.push(IsoSample { offset, size: sizes[index], duration: 0, start: 0 });
            offset = offset.saturating_add(u64::from(sizes[index]));
            index += 1;
        }
    }
    if samples.len() != sizes.len() {
        return Err("il video non è stato letto".into());
    }
    let mut start = 0u64;
    let mut run_at = 0usize;
    let mut left = 0u32;
    let mut delta = 1u32;
    for sample in &mut samples {
        if left == 0 {
            if run_at < runs.len() {
                left = runs[run_at].0;
                delta = runs[run_at].1.max(1);
                run_at += 1;
            } else {
                left = u32::MAX;
            }
        }
        sample.duration = delta;
        sample.start = start;
        start = start.saturating_add(u64::from(delta));
        left = left.saturating_sub(1);
    }
    Ok(samples)
}

fn sample_sizes(raw: &[u8]) -> Result<Vec<u32>, String> {
    let sample_size = be_u32(raw, 4).ok_or("il video non è stato letto")?;
    let count = be_u32(raw, 8).ok_or("il video non è stato letto")? as usize;
    if sample_size != 0 {
        return Ok(vec![sample_size; count]);
    }
    let mut sizes = Vec::with_capacity(count);
    for n in 0..count {
        sizes.push(be_u32(raw, 12 + n * 4).ok_or("il video non è stato letto")?);
    }
    Ok(sizes)
}

fn chunk_offsets(stco: Option<&[u8]>, co64: Option<&[u8]>) -> Result<Vec<u64>, String> {
    if let Some(raw) = co64 {
        let count = be_u32(raw, 4).ok_or("il video non è stato letto")? as usize;
        let mut offsets = Vec::with_capacity(count);
        for n in 0..count {
            let at = 8 + n * 8;
            if at + 8 > raw.len() {
                return Err("il video non è stato letto".into());
            }
            offsets.push(u64::from_be_bytes(raw[at..at + 8].try_into().unwrap()));
        }
        return Ok(offsets);
    }
    let raw = stco.ok_or("il video non è stato letto")?;
    let count = be_u32(raw, 4).ok_or("il video non è stato letto")? as usize;
    let mut offsets = Vec::with_capacity(count);
    for n in 0..count {
        offsets.push(u64::from(be_u32(raw, 8 + n * 4).ok_or("il video non è stato letto")?));
    }
    Ok(offsets)
}

fn samples_per_chunk(raw: &[u8], chunks: usize) -> Result<Vec<u32>, String> {
    let count = be_u32(raw, 4).ok_or("il video non è stato letto")? as usize;
    let mut entries = Vec::with_capacity(count);
    for n in 0..count {
        let at = 8 + n * 12;
        let first = be_u32(raw, at).ok_or("il video non è stato letto")?;
        let samples = be_u32(raw, at + 4).ok_or("il video non è stato letto")?;
        entries.push((first, samples));
    }
    let mut per_chunk = vec![0u32; chunks];
    for (n, (first, samples)) in entries.iter().enumerate() {
        let from = first.saturating_sub(1) as usize;
        let to = entries.get(n + 1).map(|(next, _)| next.saturating_sub(1) as usize).unwrap_or(chunks);
        for slot in per_chunk.iter_mut().take(to.min(chunks)).skip(from.min(chunks)) {
            *slot = *samples;
        }
    }
    Ok(per_chunk)
}

fn time_runs(raw: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let count = be_u32(raw, 4).ok_or("il video non è stato letto")? as usize;
    let mut runs = Vec::with_capacity(count);
    for n in 0..count {
        let at = 8 + n * 8;
        runs.push((
            be_u32(raw, at).ok_or("il video non è stato letto")?,
            be_u32(raw, at + 4).ok_or("il video non è stato letto")?,
        ));
    }
    Ok(runs)
}

fn timescale_of(raw: &[u8]) -> u32 {
    let at = if raw.first() == Some(&1) { 20 } else { 12 };
    be_u32(raw, at).unwrap_or(1).max(1)
}

fn handler_of(raw: &[u8]) -> [u8; 4] {
    if raw.len() < 12 {
        return [0; 4];
    }
    raw[8..12].try_into().unwrap()
}

fn be_u32(raw: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(raw.get(at..at + 4)?.try_into().ok()?))
}

fn read_payload(file: &mut File, span: &IsoSpan) -> Result<Vec<u8>, String> {
    let len = span.end.saturating_sub(span.payload);
    if len > 32 * 1024 * 1024 {
        return Err("il video non è stato letto".into());
    }
    let mut buf = vec![0u8; len as usize];
    file.seek(SeekFrom::Start(span.payload)).map_err(|err| format!("il video non è stato letto: {err}"))?;
    file.read_exact(&mut buf).map_err(|err| format!("il video non è stato letto: {err}"))?;
    Ok(buf)
}

fn read_span(file: &mut File, limit: u64) -> Result<Option<IsoSpan>, String> {
    let pos = file.stream_position().map_err(|err| format!("il video non è stato letto: {err}"))?;
    if pos.saturating_add(8) > limit {
        return Ok(None);
    }
    let mut head = [0u8; 8];
    file.read_exact(&mut head).map_err(|err| format!("il video non è stato letto: {err}"))?;
    let mut size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as u64;
    let kind = [head[4], head[5], head[6], head[7]];
    let header = if size == 1 {
        let mut wide = [0u8; 8];
        file.read_exact(&mut wide).map_err(|err| format!("il video non è stato letto: {err}"))?;
        size = u64::from_be_bytes(wide);
        if size < 16 {
            return Err("il video non è stato letto".into());
        }
        16u64
    } else {
        if size == 0 {
            size = limit.saturating_sub(pos);
        }
        if size < 8 {
            return Err("il video non è stato letto".into());
        }
        8u64
    };
    let end = pos.saturating_add(size);
    if end > limit {
        return Err("il video non è stato letto".into());
    }
    Ok(Some(IsoSpan { kind, payload: pos + header, end }))
}

fn annex_sample(sample: &[u8]) -> Vec<u8> {
    if !annex_nals(sample).is_empty() {
        return sample.to_vec();
    }
    for nal_len in [4usize, 3, 2, 1] {
        let annex = length_to_annex(sample, nal_len);
        if !annex.is_empty() {
            return annex;
        }
    }
    Vec::new()
}

fn write_symphonia_audio(input: &Path, sink: &mut H264Aac) -> Result<(), String> {
    let file = File::open(input).map_err(|err| format!("l'audio del video non è stato letto: {err}"))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|_| "l'audio del video non è stato letto".to_string())?;
    let mut format = probed.format;
    let mut chosen = None;
    for track in format.tracks() {
        if symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).is_ok() {
            let dec = symphonia::default::get_codecs()
                .make(&track.codec_params, &DecoderOptions::default())
                .map_err(|err| format!("audio non decodificato: {err}"))?;
            chosen = Some((track.id, dec));
            break;
        }
    }
    let Some((audio_id, mut dec)) = chosen else {
        return Err("l'audio del video non è stato letto".into());
    };
    let mut packets = 0u32;
    let mut samples = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymError::ResetRequired) => {
                dec.reset();
                continue;
            }
            Err(_) => break,
        };
        if packet.track_id() != audio_id {
            continue;
        }
        packets += 1;
        let decoded = match dec.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymError::DecodeError(_)) => continue,
            Err(SymError::ResetRequired) => {
                dec.reset();
                continue;
            }
            Err(err) => return Err(format!("audio non decodificato: {err}")),
        };
        let spec = *decoded.spec();
        let mut buf = SampleBuffer::<i16>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        if buf.samples().is_empty() {
            continue;
        }
        samples += buf.samples().len();
        sink.write_pcm(buf.samples(), spec.channels.count() as u16, spec.rate)?;
    }
    if packets > 0 && samples == 0 {
        return Err("l'audio del video non è stato letto".into());
    }
    Ok(())
}

fn mkv_to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    let mut src = crate::mkv::H264Mkv::open(input)?;
    let (nal_len, header) = h264_private(&src.private_data);
    let wants_audio = src.wants_audio;
    let fallback = if src.default_100ns > 0 { src.default_100ns } else { 333_333 };
    let mut decoder = Decoder::new().map_err(|err| format!("H.264 non letto: {err}"))?;
    if !header.is_empty() {
        let _ = decoder.decode(&header);
    }
    let mut sink = None;
    let mut queued = Vec::new();
    let mut shown = 0u32;
    let mut pending: Option<(OwnedFrame, i64)> = None;
    let mut last_dur = fallback;
    src.each(&mut |sample, time, duration| {
        let annex = annex_pref(sample, nal_len);
        if annex.is_empty() {
            return Ok(());
        }
        if let Some(frame) = decoder.decode(&annex).map_err(|err| format!("H.264 non letto: {err}"))? {
            let owned = owned_frame(&frame)?;
            if let Some((prev, prev_time)) = pending.take() {
                let step = if duration > 0 {
                    duration
                } else if time > prev_time {
                    time - prev_time
                } else {
                    last_dur
                };
                last_dur = step;
                feed_frame(&mut sink, &mut queued, output, &prev, step, prev_time, false, 0, 0)?;
                shown += 1;
            }
            pending = Some((owned, time));
        }
        Ok(())
    })?;
    if let Ok(rest) = decoder.flush_remaining() {
        for frame in &rest {
            let owned = owned_frame(frame)?;
            if let Some((prev, prev_time)) = pending.take() {
                feed_frame(&mut sink, &mut queued, output, &prev, last_dur, prev_time, false, 0, 0)?;
                shown += 1;
            }
            pending = Some((owned, 0));
        }
    }
    if let Some((prev, prev_time)) = pending.take() {
        feed_frame(&mut sink, &mut queued, output, &prev, last_dur, prev_time, false, 0, 0)?;
        shown += 1;
    }
    if shown == 0 {
        return Err("il video non ha fotogrammi".into());
    }
    let mut sink = sink.ok_or_else(|| "il video non ha fotogrammi".to_string())?;
    if wants_audio {
        write_symphonia_audio(input, &mut sink)?;
    }
    sink.finish()
}

fn h264_private(private: &[u8]) -> (usize, Vec<u8>) {
    if private.first() == Some(&1) {
        return avcc_prefix(Some(private));
    }
    if !annex_nals(private).is_empty() {
        return (4, private.to_vec());
    }
    (4, Vec::new())
}

fn annex_pref(sample: &[u8], nal_len: usize) -> Vec<u8> {
    if !annex_nals(sample).is_empty() {
        return sample.to_vec();
    }
    let annex = length_to_annex(sample, nal_len);
    if !annex.is_empty() {
        return annex;
    }
    annex_sample(sample)
}

fn container_to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    let file = File::open(input).map_err(|err| format!("il video non è stato letto: {err}"))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|_| "il video non è stato letto".to_string())?;
    let mut format = probed.format;
    let video_id = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec == decl_codec_type(b"avc1"))
        .map(|track| track.id)
        .ok_or_else(|| "il video non è stato letto".to_string())?;
    let video_params = format
        .tracks()
        .iter()
        .find(|track| track.id == video_id)
        .map(|track| track.codec_params.clone())
        .ok_or_else(|| "il video non è stato letto".to_string())?;
    let (nal_len, header) = avcc_prefix(video_params.extra_data.as_deref());
    let (numer, denom) = video_params.time_base.map(|base| (base.numer, base.denom)).unwrap_or((1, 30));
    let mut audio_dec = None;
    let mut audio_id = None;
    let mut audio_rate = 0u32;
    let mut audio_channels = 0u16;
    for track in format.tracks() {
        if track.id == video_id {
            continue;
        }
        if symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).is_ok() {
            audio_id = Some(track.id);
            audio_rate = track.codec_params.sample_rate.unwrap_or(44_100);
            audio_channels = track.codec_params.channels.map(|ch| ch.count() as u16).unwrap_or(1).max(1);
            audio_dec = Some(
                symphonia::default::get_codecs()
                    .make(&track.codec_params, &DecoderOptions::default())
                    .map_err(|err| format!("audio non decodificato: {err}"))?,
            );
            break;
        }
    }
    let mut decoder = Decoder::new().map_err(|err| format!("H.264 non letto: {err}"))?;
    if !header.is_empty() {
        let _ = decoder.decode(&header);
    }
    let mut sink: Option<H264Aac> = None;
    let mut queued: Vec<EarlyPcm> = Vec::new();
    let mut shown = 0u32;
    let mut audio_packets = 0u32;
    let mut audio_samples = 0usize;
    let audio_asked = audio_dec.is_some();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymError::ResetRequired) => {
                if let Some(dec) = audio_dec.as_mut() {
                    dec.reset();
                }
                continue;
            }
            Err(_) => break,
        };
        if packet.track_id() == video_id {
            let annex = if annex_nals(packet.buf()).is_empty() {
                length_to_annex(packet.buf(), nal_len)
            } else {
                packet.buf().to_vec()
            };
            let time = ticks(packet.ts(), numer, denom);
            let packet_dur = ticks(packet.dur(), numer, denom);
            let duration = if packet_dur > 0 { packet_dur } else { 333_333 };
            if let Some(frame) = decoder.decode(&annex).map_err(|err| format!("H.264 non letto: {err}"))? {
                let owned = owned_frame(&frame)?;
                feed_frame(&mut sink, &mut queued, output, &owned, duration, time, audio_asked, audio_rate, audio_channels)?;
                shown += 1;
            }
        } else if audio_id == Some(packet.track_id()) {
            audio_packets += 1;
            let Some(dec) = audio_dec.as_mut() else { continue };
            let decoded = match dec.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymError::DecodeError(_)) => continue,
                Err(SymError::ResetRequired) => {
                    dec.reset();
                    continue;
                }
                Err(err) => return Err(format!("audio non decodificato: {err}")),
            };
            let spec = *decoded.spec();
            let mut buf = SampleBuffer::<i16>::new(decoded.capacity() as u64, spec);
            buf.copy_interleaved_ref(decoded);
            if buf.samples().is_empty() {
                continue;
            }
            audio_samples += buf.samples().len();
            let pcm = EarlyPcm {
                samples: buf.samples().to_vec(),
                channels: spec.channels.count() as u16,
                rate: spec.rate,
            };
            if let Some(sink) = sink.as_mut() {
                sink.write_pcm(&pcm.samples, pcm.channels, pcm.rate)?;
            } else {
                queued.push(pcm);
            }
        }
    }
    if let Ok(rest) = decoder.flush_remaining() {
        for frame in &rest {
            let owned = owned_frame(frame)?;
            feed_frame(&mut sink, &mut queued, output, &owned, 333_333, 0, audio_asked, audio_rate, audio_channels)?;
            shown += 1;
        }
    }
    if shown == 0 {
        return Err("il video non ha fotogrammi".into());
    }
    if audio_packets > 0 && audio_samples == 0 {
        return Err("l'audio del video non è stato letto".into());
    }
    sink.ok_or_else(|| "il video non ha fotogrammi".to_string())?.finish()
}

fn feed_frame(
    sink: &mut Option<H264Aac>,
    queued: &mut Vec<EarlyPcm>,
    output: &Path,
    frame: &OwnedFrame,
    duration: i64,
    time: i64,
    audio: bool,
    rate: u32,
    channels: u16,
) -> Result<(), String> {
    if sink.is_none() {
        let opened = H264Aac::open(
            output,
            frame.width,
            frame.height,
            duration,
            if audio { Some((rate, channels)) } else { None },
        )?;
        *sink = Some(opened);
        for pcm in queued.drain(..) {
            sink.as_mut().unwrap().write_pcm(&pcm.samples, pcm.channels, pcm.rate)?;
        }
    }
    push_decoded(sink.as_mut().unwrap(), frame, duration, time)
}

fn avcc_prefix(extra: Option<&[u8]>) -> (usize, Vec<u8>) {
    let Some(extra) = extra else {
        return (4, Vec::new());
    };
    if extra.len() < 7 || extra[0] != 1 {
        return (4, Vec::new());
    }
    let nal_len = ((extra[4] & 3) + 1) as usize;
    let mut annex = Vec::new();
    let mut i = 5;
    if i >= extra.len() {
        return (nal_len, annex);
    }
    let sps_count = (extra[i] & 0x1f) as usize;
    i += 1;
    for _ in 0..sps_count {
        if !take_nal(extra, &mut i, &mut annex) {
            break;
        }
    }
    if i < extra.len() {
        let pps_count = extra[i] as usize;
        i += 1;
        for _ in 0..pps_count {
            if !take_nal(extra, &mut i, &mut annex) {
                break;
            }
        }
    }
    (nal_len.max(1), annex)
}

fn take_nal(extra: &[u8], i: &mut usize, annex: &mut Vec<u8>) -> bool {
    if *i + 2 > extra.len() {
        return false;
    }
    let len = u16::from_be_bytes([extra[*i], extra[*i + 1]]) as usize;
    *i += 2;
    if *i + len > extra.len() {
        return false;
    }
    annex.extend_from_slice(&[0, 0, 0, 1]);
    annex.extend_from_slice(&extra[*i..*i + len]);
    *i += len;
    true
}

fn length_to_annex(sample: &[u8], nal_len: usize) -> Vec<u8> {
    let nal_len = nal_len.clamp(1, 4);
    let mut out = Vec::new();
    let mut i = 0;
    while i + nal_len <= sample.len() {
        let mut len = 0usize;
        for byte in &sample[i..i + nal_len] {
            len = (len << 8) | *byte as usize;
        }
        i += nal_len;
        if len == 0 || i + len > sample.len() {
            break;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&sample[i..i + len]);
        i += len;
    }
    out
}

fn ticks(ts: u64, numer: u32, denom: u32) -> i64 {
    if denom == 0 {
        return 0;
    }
    let value = (ts as u128).saturating_mul(numer as u128).saturating_mul(10_000_000) / denom as u128;
    i64::try_from(value).unwrap_or(i64::MAX)
}

enum Pix {
    Nv12,
    Bgr { bits: u16, bottom_up: bool },
    Mjpeg,
    H264,
}

struct AviStream {
    video: bool,
    scale: u32,
    rate: u32,
    width: u32,
    height: u32,
    pix: Option<Pix>,
    channels: u16,
    sample_rate: u32,
    pcm: bool,
}

struct AviChunk {
    stream: usize,
    video: bool,
    data: Vec<u8>,
}

fn avi_to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    let bytes = std::fs::read(input).map_err(|err| format!("AVI non letto: {err}"))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"AVI " {
        return Err("AVI non letto".into());
    }
    let mut streams = Vec::new();
    let mut chunks = Vec::new();
    walk_riff(&bytes[12..], &mut streams, &mut chunks)?;
    let video_index = streams.iter().position(|stream| stream.video).ok_or_else(|| "il video non è stato letto".to_string())?;
    let video = &streams[video_index];
    let pix = video.pix.as_ref().ok_or_else(|| "il video non è stato letto".to_string())?;
    let duration = avi_duration(video.scale, video.rate);
    let width = video.width as usize;
    let height = video.height as usize;
    let audio_index = streams.iter().position(|stream| !stream.video && stream.pcm);
    let mut frames: Vec<OwnedFrame> = Vec::new();
    let mut decoder = if matches!(pix, Pix::H264) {
        Some(Decoder::new().map_err(|err| format!("H.264 non letto: {err}"))?)
    } else {
        None
    };
    let mut pcm = Vec::new();
    for chunk in chunks {
        if chunk.stream == video_index && chunk.video {
            match pix {
                Pix::Nv12 => frames.push(split_nv12(&chunk.data, width, height)?),
                Pix::Bgr { bits, bottom_up } => frames.push(bgr_at(&chunk.data, width, height, *bits, *bottom_up)?),
                Pix::Mjpeg => frames.push(mjpeg_frame(&chunk.data)?),
                Pix::H264 => {
                    let dec = decoder.as_mut().expect("decoder");
                    let annex = if annex_nals(&chunk.data).is_empty() {
                        length_to_annex(&chunk.data, 4)
                    } else {
                        chunk.data.clone()
                    };
                    if let Some(frame) = dec.decode(&annex).map_err(|err| format!("H.264 non letto: {err}"))? {
                        frames.push(owned_frame(&frame)?);
                    }
                }
            }
        } else if audio_index == Some(chunk.stream) && !chunk.video {
            pcm.extend(pcm16(&chunk.data));
        }
    }
    if let Some(dec) = decoder.as_mut() {
        if let Ok(rest) = dec.flush_remaining() {
            for frame in &rest {
                frames.push(owned_frame(frame)?);
            }
        }
    }
    if frames.is_empty() {
        return Err("il video non ha fotogrammi".into());
    }
    let audio = audio_index.map(|index| {
        let stream = &streams[index];
        (stream.sample_rate, stream.channels.max(1))
    });
    let mut sink = H264Aac::open(output, frames[0].width, frames[0].height, duration, audio)?;
    for (index, frame) in frames.iter().enumerate() {
        push_decoded(&mut sink, frame, duration, duration.saturating_mul(index as i64))?;
    }
    if let Some((rate, channels)) = audio {
        if !pcm.is_empty() {
            sink.write_pcm(&pcm, channels, rate)?;
        }
    }
    sink.finish()
}

fn avi_duration(scale: u32, rate: u32) -> i64 {
    let rate = rate.max(1) as u128;
    let value = (scale.max(1) as u128).saturating_mul(10_000_000) / rate;
    i64::try_from(value).unwrap_or(333_333).max(1)
}

fn walk_riff(data: &[u8], streams: &mut Vec<AviStream>, chunks: &mut Vec<AviChunk>) -> Result<(), String> {
    let mut i = 0;
    while i + 8 <= data.len() {
        let id = [data[i], data[i + 1], data[i + 2], data[i + 3]];
        let size = u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as usize;
        i += 8;
        if i + size > data.len() {
            return Err("AVI non letto".into());
        }
        let payload = &data[i..i + size];
        i += size + (size & 1);
        if &id == b"LIST" {
            if payload.len() < 4 {
                return Err("AVI non letto".into());
            }
            let kind = &payload[0..4];
            let inner = &payload[4..];
            if kind == b"strl" {
                streams.push(parse_strl(inner)?);
            } else if kind == b"hdrl" {
                walk_riff(inner, streams, chunks)?;
            } else {
                walk_riff(inner, streams, chunks)?;
            }
        } else if let Some(stream) = stream_of(id) {
            let video = id[2] == b'd' && (id[3] == b'c' || id[3] == b'b');
            let audio = &id[2..4] == b"wb";
            if video || audio {
                chunks.push(AviChunk { stream, video, data: payload.to_vec() });
            }
        }
    }
    Ok(())
}

fn stream_of(id: [u8; 4]) -> Option<usize> {
    if id[0].is_ascii_digit() && id[1].is_ascii_digit() {
        Some(((id[0] - b'0') * 10 + (id[1] - b'0')) as usize)
    } else {
        None
    }
}

fn parse_strl(data: &[u8]) -> Result<AviStream, String> {
    let mut strh = None;
    let mut strf = None;
    let mut i = 0;
    while i + 8 <= data.len() {
        let id = &data[i..i + 4];
        let size = u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as usize;
        i += 8;
        if i + size > data.len() {
            break;
        }
        let payload = &data[i..i + size];
        if id == b"strh" {
            strh = Some(payload.to_vec());
        } else if id == b"strf" {
            strf = Some(payload.to_vec());
        }
        i += size + (size & 1);
    }
    let strh = strh.ok_or_else(|| "AVI non letto".to_string())?;
    if strh.len() < 28 {
        return Err("AVI non letto".into());
    }
    let video = &strh[0..4] == b"vids";
    let scale = u32::from_le_bytes([strh[20], strh[21], strh[22], strh[23]]);
    let rate = u32::from_le_bytes([strh[24], strh[25], strh[26], strh[27]]);
    let handler = if strh.len() >= 8 { [strh[4], strh[5], strh[6], strh[7]] } else { [0; 4] };
    let strf = strf.unwrap_or_default();
    if video {
        if strf.len() < 16 {
            return Err("AVI non letto".into());
        }
        let width = u32::from_le_bytes([strf[4], strf[5], strf[6], strf[7]]);
        let height = i32::from_le_bytes([strf[8], strf[9], strf[10], strf[11]]);
        let bits = u16::from_le_bytes([strf[14], strf[15]]);
        let compression = if strf.len() >= 20 { [strf[16], strf[17], strf[18], strf[19]] } else { [0; 4] };
        let pix = avi_pix(handler, compression, bits, height)?;
        Ok(AviStream {
            video: true,
            scale,
            rate,
            width,
            height: height.unsigned_abs(),
            pix: Some(pix),
            channels: 0,
            sample_rate: 0,
            pcm: false,
        })
    } else {
        let format = if strf.len() >= 2 { u16::from_le_bytes([strf[0], strf[1]]) } else { 0 };
        let channels = if strf.len() >= 4 { u16::from_le_bytes([strf[2], strf[3]]) } else { 0 };
        let sample_rate = if strf.len() >= 8 { u32::from_le_bytes([strf[4], strf[5], strf[6], strf[7]]) } else { 0 };
        let bits = if strf.len() >= 16 { u16::from_le_bytes([strf[14], strf[15]]) } else { 0 };
        let pcm = format == 1 && bits == 16 && channels > 0;
        Ok(AviStream {
            video: false,
            scale,
            rate,
            width: 0,
            height: 0,
            pix: None,
            channels,
            sample_rate,
            pcm,
        })
    }
}

fn avi_pix(handler: [u8; 4], compression: [u8; 4], bits: u16, height: i32) -> Result<Pix, String> {
    if same_cc(compression, b"NV12") || same_cc(handler, b"NV12") {
        return Ok(Pix::Nv12);
    }
    if same_cc(compression, b"MJPG") || same_cc(handler, b"MJPG") {
        return Ok(Pix::Mjpeg);
    }
    if same_cc(compression, b"H264") || same_cc(compression, b"X264") || same_cc(compression, b"AVC1")
        || same_cc(handler, b"H264") || same_cc(handler, b"X264") || same_cc(handler, b"AVC1")
    {
        return Ok(Pix::H264);
    }
    let uncompressed = compression == [0; 4] || same_cc(compression, b"DIB ") || same_cc(compression, b"RGB ")
        || same_cc(handler, b"DIB ");
    if uncompressed && (bits == 24 || bits == 32) {
        return Ok(Pix::Bgr { bits, bottom_up: height > 0 });
    }
    Err("il video non è stato letto".into())
}

fn same_cc(value: [u8; 4], name: &[u8; 4]) -> bool {
    value.iter().zip(name.iter()).all(|(a, b)| a.to_ascii_uppercase() == b.to_ascii_uppercase())
}

fn split_nv12(frame: &[u8], w: usize, h: usize) -> Result<OwnedFrame, String> {
    let y_size = w * h;
    let uv_size = y_size / 2;
    if frame.len() < y_size + uv_size {
        return Err("fotogramma troppo corto".into());
    }
    let mut u = vec![128u8; uv_size / 2];
    let mut v = vec![128u8; uv_size / 2];
    let uv = &frame[y_size..y_size + uv_size];
    for i in 0..u.len() {
        u[i] = uv[i * 2];
        v[i] = uv[i * 2 + 1];
    }
    Ok(OwnedFrame { y: frame[..y_size].to_vec(), u, v, width: w as u32, height: h as u32 })
}

fn bgr_at(frame: &[u8], w: usize, h: usize, bits: u16, bottom_up: bool) -> Result<OwnedFrame, String> {
    let bytes = (bits / 8) as usize;
    if bytes == 0 || w < 2 || h < 2 {
        return Err("fotogramma troppo corto".into());
    }
    let packed = w * bytes;
    let aligned = ((w as u32 * bits as u32 + 31) / 32 * 4) as usize;
    let stride = if frame.len() >= packed * h && (aligned == packed || frame.len() < aligned * h) {
        packed
    } else {
        aligned
    };
    rgb_planes(frame, w, h, stride, bytes, true, bottom_up)
}

fn mjpeg_frame(frame: &[u8]) -> Result<OwnedFrame, String> {
    let image = image::load_from_memory(frame).map_err(|err| format!("MJPEG non letto: {err}"))?;
    let rgb = image.to_rgb8();
    let w = rgb.width() as usize;
    let h = rgb.height() as usize;
    if w < 2 || h < 2 || w % 2 == 1 || h % 2 == 1 {
        return Err(format!("il video è {w}×{h}: H.264 vuole misure pari"));
    }
    rgb_planes(rgb.as_raw(), w, h, w * 3, 3, false, false)
}

fn rgb_planes(
    src: &[u8],
    w: usize,
    h: usize,
    stride: usize,
    bytes: usize,
    bgr: bool,
    bottom_up: bool,
) -> Result<OwnedFrame, String> {
    if src.len() < stride * h || stride < w * bytes {
        return Err("fotogramma troppo corto".into());
    }
    let mut y = vec![16u8; w * h];
    let mut u = vec![128u8; (w / 2) * (h / 2)];
    let mut v = vec![128u8; (w / 2) * (h / 2)];
    for row in 0..h {
        let src_row = if bottom_up { h - 1 - row } else { row };
        for col in 0..w {
            let at = src_row * stride + col * bytes;
            let (r, g, b) = if bgr {
                (src[at + 2] as i32, src[at + 1] as i32, src[at] as i32)
            } else {
                (src[at] as i32, src[at + 1] as i32, src[at + 2] as i32)
            };
            let (yy, uu, vv) = yuv(r, g, b);
            y[row * w + col] = yy;
            if row % 2 == 0 && col % 2 == 0 {
                let c = (row / 2) * (w / 2) + col / 2;
                u[c] = uu;
                v[c] = vv;
            }
        }
    }
    Ok(OwnedFrame { y, u, v, width: w as u32, height: h as u32 })
}

fn pcm16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}
