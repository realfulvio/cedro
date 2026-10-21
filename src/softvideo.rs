use std::fs::File;
use std::io::Read;
use std::path::Path;

use symphonia::core::audio::{Layout, SampleBuffer};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;

use oxideav_vp8::state::Vp8DecoderState;
use oxideav_vp9::{split_superframe, Vp9SequenceDecoder};

use crate::sink::VideoSink;

pub(crate) const FOREIGN: &str = "non è un webm vp8 o vp9";

enum Codec {
    Vp8,
    Vp9,
}

struct Frame {
    width: u32,
    height: u32,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

struct AudioIn {
    id: u32,
    rate: u32,
    channels: u16,
    params: symphonia::core::codecs::CodecParameters,
}

enum VideoDec {
    Vp8(Vp8DecoderState),
    Vp9(Vp9SequenceDecoder),
}

pub(crate) fn transcode<S: VideoSink>(input: &Path, output: &Path) -> Result<(), String> {
    let codec = codec_of(input)?;
    let geometry = first_frame(input, &codec)?;
    let mut opened = open_media(input)?;
    let mut encoder = S::open(
        output,
        geometry.width,
        geometry.height,
        geometry.duration,
        opened.audio.as_ref().map(|audio| (audio.rate, audio.channels)),
    )?;
    let mut video = decoder(&codec);
    let mut audio_dec = match opened.audio.as_ref() {
        Some(audio) => Some(
            symphonia::default::get_codecs()
                .make(&audio.params, &DecoderOptions::default())
                .map_err(|err| format!("audio non decodificato: {err}"))?,
        ),
        None => None,
    };
    let audio_id = opened.audio.as_ref().map(|audio| audio.id);
    let mut shown = 0u32;
    let mut audio_packets = 0u32;
    let mut audio_samples = 0usize;
    loop {
        let packet = match opened.format.next_packet() {
            Ok(packet) => packet,
            Err(SymError::ResetRequired) => {
                if let Some(dec) = audio_dec.as_mut() {
                    dec.reset();
                }
                continue;
            }
            Err(_) => break,
        };
        if packet.track_id() == opened.video_id {
            let time = ticks(packet.ts(), opened.numer, opened.denom);
            let packet_dur = ticks(packet.dur(), opened.numer, opened.denom);
            let duration = if packet_dur > 0 { packet_dur } else { geometry.duration.max(1) };
            for frame in shown_frames(&mut video, packet.buf())? {
                encoder.write_yuv(
                    &frame.y,
                    &frame.u,
                    &frame.v,
                    frame.width,
                    frame.height,
                    time,
                    duration,
                )?;
                shown += 1;
            }
        } else if audio_id == Some(packet.track_id()) {
            audio_packets += 1;
            let Some(dec) = audio_dec.as_mut() else {
                continue;
            };
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
            let samples = buf.samples();
            if samples.is_empty() {
                continue;
            }
            audio_samples += samples.len();
            encoder.write_pcm(samples, spec.channels.count() as u16, spec.rate)?;
        }
    }
    if shown == 0 {
        return Err("il video non ha fotogrammi".into());
    }
    if audio_packets > 0 && audio_samples == 0 {
        return Err("l'audio del video non è stato letto".into());
    }
    encoder.finish()
}

struct Geometry {
    width: u32,
    height: u32,
    duration: i64,
}

fn first_frame(input: &Path, codec: &Codec) -> Result<Geometry, String> {
    let mut opened = open_media(input)?;
    let mut video = decoder(codec);
    loop {
        let packet = match opened.format.next_packet() {
            Ok(packet) => packet,
            Err(_) => break,
        };
        if packet.track_id() != opened.video_id {
            continue;
        }
        let duration = ticks(packet.dur(), opened.numer, opened.denom);
        if let Some(frame) = shown_frames(&mut video, packet.buf())?.into_iter().next() {
            if frame.width < 2 || frame.height < 2 {
                return Err("il video non ha una dimensione".into());
            }
            return Ok(Geometry {
                width: frame.width,
                height: frame.height,
                duration,
            });
        }
    }
    Err("il video non ha fotogrammi".into())
}

struct Opened {
    format: Box<dyn FormatReader>,
    video_id: u32,
    numer: u32,
    denom: u32,
    audio: Option<AudioIn>,
}

fn open_media(path: &Path) -> Result<Opened, String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, source, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|err| format!("WebM non letto: {err}"))?;
    let format = probed.format;
    let video = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec == CODEC_TYPE_NULL)
        .ok_or("il WebM non ha un video")?;
    let video_id = video.id;
    let time_base = video
        .codec_params
        .time_base
        .unwrap_or_else(|| TimeBase::new(1_000_000, 1_000_000_000));
    let audio = format.tracks().iter().find_map(|track| {
        if track.id == video_id || track.codec_params.codec == CODEC_TYPE_NULL {
            return None;
        }
        if symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .is_err()
        {
            return None;
        }
        Some(AudioIn {
            id: track.id,
            rate: track.codec_params.sample_rate.unwrap_or(44100),
            channels: channel_count(&track.codec_params),
            params: track.codec_params.clone(),
        })
    });
    Ok(Opened {
        format,
        video_id,
        numer: time_base.numer,
        denom: time_base.denom,
        audio,
    })
}

fn channel_count(params: &symphonia::core::codecs::CodecParameters) -> u16 {
    if let Some(channels) = params.channels {
        return channels.count().max(1) as u16;
    }
    match params.channel_layout {
        Some(Layout::Mono) => 1,
        Some(_) => 2,
        None => 1,
    }
}

fn decoder(codec: &Codec) -> VideoDec {
    match codec {
        Codec::Vp8 => VideoDec::Vp8(Vp8DecoderState::new()),
        Codec::Vp9 => VideoDec::Vp9(Vp9SequenceDecoder::new()),
    }
}

fn shown_frames(dec: &mut VideoDec, bytes: &[u8]) -> Result<Vec<Frame>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    match dec {
        VideoDec::Vp8(state) => {
            let frame = state
                .decode_frame(bytes)
                .map_err(|err| format!("VP8 non letto: {err:?}"))?;
            if state.last_frame_shown() == Some(false) {
                return Ok(Vec::new());
            }
            Ok(vec![Frame {
                width: frame.width,
                height: frame.height,
                y: frame.y,
                u: frame.u,
                v: frame.v,
            }])
        }
        VideoDec::Vp9(state) => {
            let mut frames = Vec::new();
            for part in split_superframe(bytes) {
                let shown = state
                    .push_frame(part)
                    .map_err(|err| format!("VP9 non letto: {err:?}"))?;
                if let Some(frame) = shown {
                    let (y, u, v) = vp9_planes(&frame)?;
                    frames.push(Frame {
                        width: frame.width,
                        height: frame.height,
                        y,
                        u,
                        v,
                    });
                }
            }
            Ok(frames)
        }
    }
}

fn vp9_planes(frame: &oxideav_vp9::Vp9DecodedFrame) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), String> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    let y = narrow(&frame.y, w.saturating_mul(h), frame.bit_depth)?;
    let sub_x = frame.subsampling_x;
    let sub_y = frame.subsampling_y;
    let src_w = if sub_x { w.div_ceil(2) } else { w };
    let src_h = if sub_y { h.div_ceil(2) } else { h };
    let u = narrow(&frame.u, src_w.saturating_mul(src_h), frame.bit_depth)?;
    let v = narrow(&frame.v, src_w.saturating_mul(src_h), frame.bit_depth)?;
    if sub_x && sub_y {
        return Ok((y, u, v));
    }
    Ok((y, to_420(&u, w, h, sub_x, sub_y), to_420(&v, w, h, sub_x, sub_y)))
}

fn narrow(samples: &[u16], n: usize, depth: u8) -> Result<Vec<u8>, String> {
    if n == 0 || samples.len() < n {
        return Err("fotogramma troppo corto".into());
    }
    let shift = depth.saturating_sub(8);
    Ok(samples[..n].iter().map(|sample| (*sample >> shift) as u8).collect())
}

fn to_420(plane: &[u8], width: usize, height: usize, sub_x: bool, sub_y: bool) -> Vec<u8> {
    let cw = width.div_ceil(2);
    let ch = height.div_ceil(2);
    let src_w = if sub_x { cw } else { width.max(1) };
    let src_h = if sub_y { ch } else { height.max(1) };
    let mut out = vec![128u8; cw * ch];
    if plane.len() < src_w * src_h || cw == 0 || ch == 0 {
        return out;
    }
    for y in 0..ch {
        for x in 0..cw {
            let x0 = if sub_x { x } else { (x * 2).min(src_w - 1) };
            let y0 = if sub_y { y } else { (y * 2).min(src_h - 1) };
            let x1 = if sub_x { x0 } else { (x0 + 1).min(src_w - 1) };
            let y1 = if sub_y { y0 } else { (y0 + 1).min(src_h - 1) };
            let sum = plane[y0 * src_w + x0] as u16
                + plane[y0 * src_w + x1] as u16
                + plane[y1 * src_w + x0] as u16
                + plane[y1 * src_w + x1] as u16;
            out[y * cw + x] = (sum / 4) as u8;
        }
    }
    out
}

fn codec_of(path: &Path) -> Result<Codec, String> {
    let mut file = File::open(path).map_err(|err| err.to_string())?;
    let mut buf = Vec::new();
    file.by_ref().take(8 * 1024 * 1024).read_to_end(&mut buf).map_err(|err| err.to_string())?;
    let vp8 = buf.windows(5).position(|window| window == b"V_VP8");
    let vp9 = buf.windows(5).position(|window| window == b"V_VP9");
    match (vp8, vp9) {
        (Some(vp8_at), Some(vp9_at)) if vp8_at <= vp9_at => Ok(Codec::Vp8),
        (Some(_), Some(_)) => Ok(Codec::Vp9),
        (Some(_), None) => Ok(Codec::Vp8),
        (None, Some(_)) => Ok(Codec::Vp9),
        (None, None) => Err(FOREIGN.into()),
    }
}

fn ticks(ts: u64, numer: u32, denom: u32) -> i64 {
    if denom == 0 {
        return 0;
    }
    let value = (ts as u128).saturating_mul(numer as u128).saturating_mul(10_000_000) / denom as u128;
    i64::try_from(value).unwrap_or(i64::MAX)
}
