//! H.264 e AAC compilati nel programma. Nessun ffmpeg.
//! OpenH264 (BSD-2, Cisco, dal sorgente) e FDK AAC (Fraunhofer).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use fdk_aac::enc::{AudioObjectType, BitRate, ChannelMode, Encoder as AacEncoder, EncoderParams, Transport};
use openh264::decoder::Decoder;
use openh264::encoder::{BitRate as H264Rate, Encoder, EncoderConfig, FrameRate, FrameType, Profile, VuiConfig};
use openh264::formats::{YUVSlices, YUVSource};
use openh264::OpenH264API;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{decl_codec_type, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::mp4mux::{Movie, Mp4Writer, Sample};
use crate::nal::{annex_nals, avcc_prefix, is_annex, length_to_annex};
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
    crate::sink::note_source(input);
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
                with_gpu_fallback(|| iso_to_mp4(input, output))
            } else if n >= 4 && head[..4] == [0x1A, 0x45, 0xDF, 0xA3] {
                with_gpu_fallback(|| mkv_to_mp4(input, output))
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
    /// Campioni d'avvio del codificatore in testa all'audio.
    delay: u32,
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
    /// Il file, aperto al primo fotogramma codificato: i fotogrammi ci vanno subito.
    mp4: Option<Mp4Writer>,
    /// Le misure dei fotogrammi già scritti.
    video: Vec<Sample>,
    /// Il tempo scritto finora, in 100 ns e in 1/90000 s: le durate si arrotondano sul totale,
    /// così l'errore non si somma fotogramma dopo fotogramma.
    elapsed_100ns: i64,
    elapsed_90k: u64,
    payload: Vec<u8>,
    audio: Option<AacTrack>,
    source_rate: u32,
    source_channels: u16,
    resampler: crate::sink::Resampler,
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
        crate::sink::cap_for(out_w as u64 * out_h as u64, width as u64 * height as u64);
        let shrinking = (out_w, out_h) != (width, height);
        if width < 2 || height < 2 || (!grabbing() && !shrinking && (width % 2 == 1 || height % 2 == 1)) {
            return Err(format!("il video è {width}×{height}: H.264 vuole misure pari"));
        }
        let shape = crate::sink::shape();
        let gate = crate::sink::FrameGate::new();
        let audio = if shape.mute || grabbing() { None } else { audio };
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
            mp4: None,
            video: Vec::new(),
            elapsed_100ns: 0,
            elapsed_90k: 0,
            payload: Vec::new(),
            audio,
            source_rate,
            source_channels,
            resampler: crate::sink::Resampler::default(),
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
        if grabbing() {
            return grab_frame(y, u, v, width, height, time);
        }
        crate::sink::frame_progress(time);
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
        let resized = ((width, height) != (self.width, self.height))
            .then(|| crate::sink::resize_i420(y, u, v, width, height, self.width, self.height));
        let (y, u, v) = match &resized {
            Some((ry, ru, rv)) => (ry.as_slice(), ru.as_slice(), rv.as_slice()),
            None => (y, u, v),
        };
        let (w, h) = (self.width as usize, self.height as usize);
        if let Some(H264::Gpu(gpu)) = self.encoder.as_mut() {
            let packed = pack_i420(y, u, v, w, h)?;
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
        // I piani vanno al codificatore come sono, senza copiarli in un fotogramma unico.
        let (cw, ch) = (w / 2, h / 2);
        if y.len() < w * h || u.len() < cw * ch || v.len() < cw * ch {
            return Err("fotogramma troppo corto".into());
        }
        let picture = YUVSlices::new((&y[..w * h], &u[..cw * ch], &v[..cw * ch]), (w, h), (w, cw, cw));
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
        if samples.is_empty() || self.muted || grabbing() {
            return Ok(());
        }
        if self.audio.is_none() {
            let track = open_aac(rate, channels)?;
            self.source_rate = track.rate;
            self.source_channels = track.channels as u16;
            self.audio = Some(track);
        }
        let track = self.audio.as_mut().expect("audio");
        shape_pcm(&mut self.resampler, samples, channels, rate, self.source_channels, self.source_rate, &mut track.pending);
        track.drain(false)
    }

    fn finish(mut self) -> Result<(), String> {
        if grabbing() {
            return Err(GRAB_DONE.into());
        }
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
        let (audio_rate, audio_channels, audio_asc, audio_samples, packets, audio_delay) = match audio {
            Some(track) if !track.packets.is_empty() => {
                let samples = track
                    .packets
                    .iter()
                    .map(|data| Sample { size: data.len() as u32, duration: track.frame_len as u32, sync: true })
                    .collect();
                (track.rate, track.channels as u16, track.asc, samples, track.packets, track.delay)
            }
            _ => (0, 0, Vec::new(), Vec::new(), Vec::new(), 0),
        };
        let mp4 = self.mp4.take().ok_or("il video non ha fotogrammi")?;
        mp4.finish(
            &Movie {
                width: self.width,
                height: self.height,
                sps: self.sps,
                pps: self.pps,
                video: self.video,
                audio_rate,
                audio_channels,
                audio_asc,
                audio_delay,
                audio: audio_samples,
            },
            &packets,
        )
    }
}

impl H264Aac {
    fn push_annex(&mut self, bytes: &[u8], duration: i64, mut sync: bool) -> Result<(), String> {
        let mut payload = std::mem::take(&mut self.payload);
        payload.clear();
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
            self.payload = payload;
            return Ok(());
        }
        let ticks = if duration > 0 { duration } else { self.frame_duration };
        let size = u32::try_from(payload.len()).map_err(|_| "fotogramma troppo grande".to_string())?;
        if self.mp4.is_none() {
            self.mp4 = Some(Mp4Writer::create(&self.path)?);
        }
        self.mp4.as_mut().expect("mp4").write_video(&payload)?;
        self.payload = payload;
        let duration = self.push_duration(ticks);
        self.video.push(Sample { size, duration, sync });
        Ok(())
    }

    /// La durata in 1/90000 s del prossimo fotogramma, da quella in 100 ns: si arrotonda il tempo
    /// totale, non la singola durata (troncando, a 30 fps si perdeva 1,2 s di video all'ora).
    fn push_duration(&mut self, ticks_100ns: i64) -> u32 {
        self.elapsed_100ns = self.elapsed_100ns.saturating_add(ticks_100ns.max(1));
        let end = ((self.elapsed_100ns as i128 * 9 + 500) / 1000) as u64;
        let d = end.saturating_sub(self.elapsed_90k).max(1);
        self.elapsed_90k += d;
        u32::try_from(d).unwrap_or(u32::MAX)
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
        delay: info.nDelay,
        out: vec![0; out_len],
    })
}

impl AacTrack {
    fn drain(&mut self, finish: bool) -> Result<(), String> {
        let frame = self.frame_len * self.channels;
        // Un pezzo alla volta dall'inizio e il resto spostato una volta sola: togliendo ogni volta il
        // primo pezzo dal vettore, un audio lungo passato tutto insieme costava ore.
        if self.pending.len() >= frame {
            let pending = std::mem::take(&mut self.pending);
            let mut chunks = pending.chunks_exact(frame);
            for chunk in &mut chunks {
                self.encode_frame(chunk)?;
            }
            self.pending = chunks.remainder().to_vec();
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

/// L'audio portato ai canali e alla frequenza della traccia AAC, aggiunto in fondo a `out`.
fn shape_pcm(
    resampler: &mut crate::sink::Resampler,
    samples: &[i16],
    channels: u16,
    rate: u32,
    target_channels: u16,
    target_rate: u32,
    out: &mut Vec<i16>,
) {
    use std::borrow::Cow;
    let channels = channels.max(1);
    let folded: Cow<[i16]> = if channels > 2 { Cow::Owned(crate::sink::fold_stereo(samples, channels)) } else { Cow::Borrowed(samples) };
    let folded_channels = channels.min(2);
    let fitted = if folded_channels != target_channels && target_channels > 0 {
        Cow::Owned(refit_channels(&folded, folded_channels, target_channels))
    } else {
        folded
    };
    let used = if target_channels > 0 { target_channels } else { folded_channels };
    resampler.push(&fitted, used, rate, target_rate, out);
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

/// Quello che il thread di decodifica passa al codificatore.
enum Decoded {
    /// Fotogramma, durata e tempo (100 ns).
    Frame(OwnedFrame, i64, i64),
    Pcm(EarlyPcm),
}

/// Fotogrammi decodificati in attesa del codificatore.
const DECODED_QUEUE: usize = 4;
/// Chi codifica si è fermato (errore o anteprima finita): chi legge smette.
const STOPPED: &str = "lettura fermata";

/// Decodifica su un altro thread mentre questo codifica: i due lavori vanno insieme invece che a turno.
/// `read` passa ogni fotogramma a `emit` (che risponde `false` se chi codifica si è fermato), `take` li
/// riceve qui nell'ordine. Il codificatore, le opzioni e l'anteprima restano su questo thread.
fn pipelined<R>(read: R, mut take: impl FnMut(Decoded) -> Result<(), String>) -> Result<(), String>
where
    R: FnOnce(&mut dyn FnMut(Decoded) -> bool) -> Result<(), String> + Send,
{
    std::thread::scope(|scope| {
        let (send, receive) = std::sync::mpsc::sync_channel::<Decoded>(DECODED_QUEUE);
        let reader = std::thread::Builder::new()
            .name("cedro-decodifica".into())
            .spawn_scoped(scope, move || read(&mut |item| send.send(item).is_ok()))
            .map_err(|err| format!("il video non è stato letto: {err}"))?;
        let mut taken = Ok(());
        for item in receive.iter() {
            if let Err(err) = take(item) {
                taken = Err(err);
                break;
            }
        }
        // Il canale chiuso ferma chi legge al prossimo fotogramma.
        drop(receive);
        let read = reader.join().unwrap_or_else(|_| Err("il video non è stato letto".into()));
        taken?;
        read
    })
}

/// Per `read` di `pipelined`: `false` da `emit` diventa l'errore che ferma la lettura.
fn sent(ok: bool) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(STOPPED.into())
    }
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
    let mut sink = None;
    let mut queued = Vec::new();
    let mut shown = 0u32;
    pipelined(
        |emit| {
            let mut decoder = SampleDecoder::open(&header)?;
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
                decoder.decode(&annex, duration, time, &mut |frame, d, t| sent(emit(Decoded::Frame(frame, d, t))))?;
            }
            // Con i fotogrammi B il decodificatore ne trattiene qualcuno: escono qui, dopo l'ultimo.
            decoder.finish(&mut |frame, d, t| sent(emit(Decoded::Frame(frame, d, t))))
        },
        |item| {
            if let Decoded::Frame(owned, duration, time) = item {
                feed_frame(&mut sink, &mut queued, output, &owned, duration, time, false, 0, 0)?;
                shown += 1;
            }
            Ok(())
        },
    )?;
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
    let mut sink = None;
    let mut queued = Vec::new();
    let mut shown = 0u32;
    pipelined(
        |emit| {
            let mut decoder = SampleDecoder::open(&header)?;
            let mut sample_bytes = Vec::new();
            for sample in &video.samples {
                // Un campione che esce dal file è un indice rovinato: niente memoria chiesta per niente.
                if sample.offset.saturating_add(u64::from(sample.size)) > len {
                    return Err("il video non è stato letto".into());
                }
                sample_bytes.resize(sample.size as usize, 0);
                file.seek(SeekFrom::Start(sample.offset)).map_err(|err| format!("il video non è stato letto: {err}"))?;
                file.read_exact(&mut sample_bytes).map_err(|err| format!("il video non è stato letto: {err}"))?;
                let annex = annex_sample(&sample_bytes);
                if annex.is_empty() {
                    continue;
                }
                let duration = ticks(u64::from(sample.duration), 1, video.timescale).max(1);
                let time = ticks(sample.start, 1, video.timescale);
                decoder.decode(&annex, duration, time, &mut |frame, d, t| sent(emit(Decoded::Frame(frame, d, t))))?;
            }
            // Con i fotogrammi B il decodificatore ne trattiene qualcuno: escono qui, dopo l'ultimo.
            decoder.finish(&mut |frame, d, t| sent(emit(Decoded::Frame(frame, d, t))))
        },
        |item| {
            if let Decoded::Frame(owned, duration, time) = item {
                feed_frame(&mut sink, &mut queued, output, &owned, duration, time, false, 0, 0)?;
                shown += 1;
            }
            Ok(())
        },
    )?;
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

/// Quante voci da `entry` byte stanno davvero nella tabella dopo `head` byte: i contatori vengono dal
/// file e un file rovinato non deve far chiedere gigabyte di memoria.
fn table_count(raw: &[u8], count: usize, head: usize, entry: usize) -> Result<usize, String> {
    if count > raw.len().saturating_sub(head) / entry {
        return Err("il video non è stato letto".into());
    }
    Ok(count)
}

fn sample_sizes(raw: &[u8]) -> Result<Vec<u32>, String> {
    let sample_size = be_u32(raw, 4).ok_or("il video non è stato letto")?;
    let count = be_u32(raw, 8).ok_or("il video non è stato letto")? as usize;
    if sample_size != 0 {
        // Misura fissa: la tabella è vuota; 50 milioni di campioni sono 46 ore a 300 fps.
        if count > 50_000_000 {
            return Err("il video non è stato letto".into());
        }
        return Ok(vec![sample_size; count]);
    }
    let count = table_count(raw, count, 12, 4)?;
    let mut sizes = Vec::with_capacity(count);
    for n in 0..count {
        sizes.push(be_u32(raw, 12 + n * 4).ok_or("il video non è stato letto")?);
    }
    Ok(sizes)
}

fn chunk_offsets(stco: Option<&[u8]>, co64: Option<&[u8]>) -> Result<Vec<u64>, String> {
    if let Some(raw) = co64 {
        let count = table_count(raw, be_u32(raw, 4).ok_or("il video non è stato letto")? as usize, 8, 8)?;
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
    let count = table_count(raw, be_u32(raw, 4).ok_or("il video non è stato letto")? as usize, 8, 4)?;
    let mut offsets = Vec::with_capacity(count);
    for n in 0..count {
        offsets.push(u64::from(be_u32(raw, 8 + n * 4).ok_or("il video non è stato letto")?));
    }
    Ok(offsets)
}

fn samples_per_chunk(raw: &[u8], chunks: usize) -> Result<Vec<u32>, String> {
    let count = table_count(raw, be_u32(raw, 4).ok_or("il video non è stato letto")? as usize, 8, 12)?;
    let mut entries = Vec::with_capacity(count);
    for n in 0..count {
        let at = 8 + n * 12;
        let first = be_u32(raw, at).ok_or("il video non è stato letto")?;
        let samples = be_u32(raw, at + 4).ok_or("il video non è stato letto")?;
        entries.push((first, samples));
    }
    let mut per_chunk = vec![0u32; chunks];
    for (n, (first, samples)) in entries.iter().enumerate() {
        let from = (first.saturating_sub(1) as usize).min(chunks);
        let to = entries.get(n + 1).map(|(next, _)| next.saturating_sub(1) as usize).unwrap_or(chunks).clamp(from, chunks);
        // Solo l'intervallo della voce: con voci fuori ordine il conto restava voci × blocchi.
        per_chunk[from..to].fill(*samples);
    }
    Ok(per_chunk)
}

fn time_runs(raw: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let count = table_count(raw, be_u32(raw, 4).ok_or("il video non è stato letto")? as usize, 8, 8)?;
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
    if is_annex(sample) {
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
    let fixed = src.default_100ns.max(0);
    let mut sink = None;
    let mut queued = Vec::new();
    let mut shown = 0u32;
    pipelined(
        |emit| {
            let mut decoder = SampleDecoder::open(&header)?;
            let mut emitted = 0i64;
            let mut pending: Option<(OwnedFrame, i64)> = None;
            let mut last_dur = fallback;
            src.each(&mut |sample, time, duration| {
                let annex = annex_pref(sample, nal_len);
                if annex.is_empty() {
                    return Ok(());
                }
                decoder.decode(&annex, duration, time, &mut |owned, duration, time| {
                    // Con la durata fissa dei fotogrammi i tempi si contano: con i fotogrammi B quelli dei blocchi
                    // sono nell'ordine di visione, non in quello in cui escono dal decodificatore.
                    if fixed > 0 {
                        sent(emit(Decoded::Frame(owned, fixed, emitted * fixed)))?;
                        emitted += 1;
                        return Ok(());
                    }
                    if let Some((prev, prev_time)) = pending.take() {
                        let step = if duration > 0 {
                            duration
                        } else if time > prev_time {
                            time - prev_time
                        } else {
                            last_dur
                        };
                        last_dur = step;
                        sent(emit(Decoded::Frame(prev, step, prev_time)))?;
                    }
                    pending = Some((owned, time));
                    Ok(())
                })
            })?;
            decoder.finish(&mut |owned, _, _| {
                if fixed > 0 {
                    sent(emit(Decoded::Frame(owned, fixed, emitted * fixed)))?;
                    emitted += 1;
                    return Ok(());
                }
                let mut next = 0;
                if let Some((prev, prev_time)) = pending.take() {
                    sent(emit(Decoded::Frame(prev, last_dur, prev_time)))?;
                    next = prev_time + last_dur;
                }
                pending = Some((owned, next));
                Ok(())
            })?;
            if let Some((prev, prev_time)) = pending.take() {
                sent(emit(Decoded::Frame(prev, last_dur, prev_time)))?;
            }
            Ok(())
        },
        |item| {
            if let Decoded::Frame(owned, duration, time) = item {
                feed_frame(&mut sink, &mut queued, output, &owned, duration, time, false, 0, 0)?;
                shown += 1;
            }
            Ok(())
        },
    )?;
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
    if is_annex(sample) {
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
    let mut sink: Option<H264Aac> = None;
    let mut queued: Vec<EarlyPcm> = Vec::new();
    let mut shown = 0u32;
    let mut audio_packets = 0u32;
    let mut audio_samples = 0usize;
    let audio_asked = audio_dec.is_some();
    pipelined(
        |emit| {
            let mut decoder = h264_decoder()?;
            if !header.is_empty() {
                let _ = decoder.decode(&header);
            }
            let mut last = (0i64, 333_333i64);
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
                    let annex = if !is_annex(packet.buf()) {
                        length_to_annex(packet.buf(), nal_len)
                    } else {
                        packet.buf().to_vec()
                    };
                    let time = ticks(packet.ts(), numer, denom);
                    let packet_dur = ticks(packet.dur(), numer, denom);
                    let duration = if packet_dur > 0 { packet_dur } else { 333_333 };
                    last = (time, duration);
                    if let Some(frame) = decoder.decode(&annex).map_err(|err| format!("H.264 non letto: {err}"))? {
                        sent(emit(Decoded::Frame(owned_frame(&frame)?, duration, time)))?;
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
                    sent(emit(Decoded::Pcm(pcm)))?;
                }
            }
            if let Ok(rest) = decoder.flush_remaining() {
                for (k, frame) in rest.iter().enumerate() {
                    sent(emit(Decoded::Frame(owned_frame(frame)?, last.1, last.0 + last.1 * (k as i64 + 1))))?;
                }
            }
            Ok(())
        },
        |item| {
            match item {
                Decoded::Frame(owned, duration, time) => {
                    feed_frame(&mut sink, &mut queued, output, &owned, duration, time, audio_asked, audio_rate, audio_channels)?;
                    shown += 1;
                }
                Decoded::Pcm(pcm) => match sink.as_mut() {
                    Some(sink) => sink.write_pcm(&pcm.samples, pcm.channels, pcm.rate)?,
                    None => queued.push(pcm),
                },
            }
            Ok(())
        },
    )?;
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




fn ticks(ts: u64, numer: u32, denom: u32) -> i64 {
    if denom == 0 {
        return 0;
    }
    // Arrotondato, non troncato: ogni troncamento accorcia un poco il video.
    let value = ((ts as u128).saturating_mul(numer as u128).saturating_mul(10_000_000) + denom as u128 / 2) / denom as u128;
    i64::try_from(value).unwrap_or(i64::MAX)
}

enum Pix {
    Nv12,
    Bgr { bits: u16, bottom_up: bool },
    Mjpeg,
    H264,
    /// MPEG-4 parte 2 (XVID, DivX, FMP4…).
    Mpeg4,
}



fn avi_to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    let (streams, chunks) = crate::avi::read(input)?;
    let video_index = streams.iter().position(|stream| stream.video).ok_or_else(|| "il video non è stato letto".to_string())?;
    let video = &streams[video_index];
    let pix = &avi_pix(video.handler, video.compression, video.bits, video.signed_height)?;
    let (frames, secs) = crate::avi::video_timing(&streams, &chunks, video_index);
    let duration = ((secs * 10_000_000.0 / frames as f64) as i64).max(1);
    if secs > 0.0 {
        let size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
        crate::sink::set_source((secs * 10_000_000.0) as i64, (size as f64 * 8.0 / secs) as u64);
    }
    let width = video.width as usize;
    let height = video.height as usize;
    let audio_index = streams.iter().position(|stream| !stream.video && stream.channels > 0);
    // L'audio dell'AVI (PCM, MP3, AAC) si decodifica prima, così il suo formato è noto all'apertura.
    let avi_audio = match audio_index {
        Some(index) => crate::avi::avi_audio_pcm(&streams[index], &chunks, index)?,
        None => None,
    };
    let audio = avi_audio.as_ref().map(|(_, rate, channels)| (*rate, *channels));
    let avc_header = crate::avi::avc_config(&video.extra);
    // Ogni fotogramma va subito al file (o all'anteprima), senza tenerli tutti in memoria.
    let mut sink: Option<H264Aac> = None;
    let mut shown: i64 = 0;
    pipelined(
        |emit| avi_frames(pix, &chunks, video_index, width, height, &avc_header, &mut |frame| sent(emit(Decoded::Frame(frame, 0, 0)))),
        |item| {
            let Decoded::Frame(frame, _, _) = item else { return Ok(()) };
            if sink.is_none() {
                sink = Some(H264Aac::open(output, frame.width, frame.height, duration, audio)?);
            }
            let sink = sink.as_mut().expect("sink");
            push_decoded(sink, &frame, duration, duration.saturating_mul(shown))?;
            shown += 1;
            Ok(())
        },
    )?;
    let Some(mut sink) = sink else {
        return Err("il video non ha fotogrammi".into());
    };
    if let Some((pcm, rate, channels)) = avi_audio {
        // A pezzi: l'audio intero in una volta passava tutto per le copie di `shape_pcm`.
        for part in pcm.chunks(channels.max(1) as usize * 4096) {
            sink.write_pcm(part, channels, rate)?;
        }
    }
    sink.finish()
}

/// I fotogrammi del video di un AVI, decodificati e passati a `emit` nell'ordine.
fn avi_frames(
    pix: &Pix,
    chunks: &[crate::avi::AviChunk],
    video_index: usize,
    width: usize,
    height: usize,
    avc_header: &(usize, Vec<u8>),
    emit: &mut dyn FnMut(OwnedFrame) -> Result<(), String>,
) -> Result<(), String> {
    let mut decoder = if matches!(pix, Pix::H264) {
        let mut dec = h264_decoder()?;
        // Con avcC nei dati di `strf` i parametri non viaggiano nei fotogrammi.
        if !avc_header.1.is_empty() {
            let _ = dec.decode(&avc_header.1);
        }
        Some(dec)
    } else {
        None
    };
    let mut mpeg4 = matches!(pix, Pix::Mpeg4).then(oxideav_mpeg4video::decoder::Mpeg4VideoDecoder::new);
    for chunk in chunks {
        if chunk.stream == video_index && chunk.video {
            if chunk.data.is_empty() {
                continue; // fotogramma saltato
            }
            match pix {
                Pix::Nv12 => emit(split_nv12(&chunk.data, width, height)?)?,
                Pix::Bgr { bits, bottom_up } => emit(bgr_at(&chunk.data, width, height, *bits, *bottom_up)?)?,
                Pix::Mjpeg => emit(mjpeg_frame(&chunk.data)?)?,
                Pix::H264 => {
                    let dec = decoder.as_mut().expect("decoder");
                    let annex = if !is_annex(&chunk.data) {
                        length_to_annex(&chunk.data, avc_header.0)
                    } else {
                        chunk.data.to_vec()
                    };
                    if annex.is_empty() {
                        continue;
                    }
                    if let Some(frame) = dec.decode(&annex).map_err(|err| format!("H.264 non letto: {err}"))? {
                        emit(owned_frame(&frame)?)?;
                    }
                }
                Pix::Mpeg4 => {
                    let dec = mpeg4.as_mut().expect("decoder");
                    let frames = dec.decode(&chunk.data).map_err(|err| format!("MPEG-4 non letto: {err}"))?;
                    for frame in &frames {
                        emit(mpeg4_frame(frame, width, height)?)?;
                    }
                }
            }
        }
    }
    if let Some(dec) = mpeg4.as_mut() {
        for frame in &dec.flush() {
            emit(mpeg4_frame(frame, width, height)?)?;
        }
    }
    if let Some(dec) = decoder.as_mut() {
        if let Ok(rest) = dec.flush_remaining() {
            for frame in &rest {
                emit(owned_frame(frame)?)?;
            }
        }
    }
    Ok(())
}

/// I fotogrammi dell'MPEG-4 hanno le misure dei macroblocchi; l'immagine vera può essere più piccola.
fn mpeg4_frame(frame: &oxideav_mpeg4video::framestore::DecodedFrame, want_w: usize, want_h: usize) -> Result<OwnedFrame, String> {
    let (fw, fh) = (frame.width(), frame.height());
    let (w, h) = (want_w.min(fw) & !1, want_h.min(fh) & !1);
    if w < 2 || h < 2 {
        return Err("fotogramma troppo corto".into());
    }
    let crop = |src: &[u8], stride: usize, cw: usize, ch: usize| -> Vec<u8> {
        (0..ch).flat_map(|row| src[row * stride..row * stride + cw].iter().copied()).collect()
    };
    Ok(OwnedFrame {
        y: crop(frame.luma_samples(), fw, w, h),
        u: crop(frame.cb_samples(), fw / 2, w / 2, h / 2),
        v: crop(frame.cr_samples(), fw / 2, w / 2, h / 2),
        width: w as u32,
        height: h as u32,
    })
}






fn avi_pix(handler: [u8; 4], compression: [u8; 4], bits: u16, height: i32) -> Result<Pix, String> {
    if crate::avi::same_cc(compression, b"NV12") || crate::avi::same_cc(handler, b"NV12") {
        return Ok(Pix::Nv12);
    }
    if crate::avi::same_cc(compression, b"MJPG") || crate::avi::same_cc(handler, b"MJPG") {
        return Ok(Pix::Mjpeg);
    }
    if crate::avi::same_cc(compression, b"H264") || crate::avi::same_cc(compression, b"X264") || crate::avi::same_cc(compression, b"AVC1")
        || crate::avi::same_cc(handler, b"H264") || crate::avi::same_cc(handler, b"X264") || crate::avi::same_cc(handler, b"AVC1")
    {
        return Ok(Pix::H264);
    }
    for cc in [b"FMP4", b"XVID", b"DIVX", b"DX50", b"MP4V", b"M4S2", b"MP4S", b"3IV2", b"RMP4", b"SEDG", b"DIV5"] {
        if crate::avi::same_cc(compression, cc) || crate::avi::same_cc(handler, cc) {
            return Ok(Pix::Mpeg4);
        }
    }
    let uncompressed = compression == [0; 4] || crate::avi::same_cc(compression, b"DIB ") || crate::avi::same_cc(compression, b"RGB ")
        || crate::avi::same_cc(handler, b"DIB ");
    if uncompressed && (bits == 24 || bits == 32) {
        return Ok(Pix::Bgr { bits, bottom_up: height > 0 });
    }
    Err("il video non è stato letto".into())
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
    let aligned = ((w as u32 * bits as u32).div_ceil(32) * 4) as usize;
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


// ---- Fotogrammi per l'anteprima ----

use crate::softvideo::{i420_rgba, Peek, PEEK_BUDGET};

const GRAB_DONE: &str = "fotogrammi dell'anteprima presi";

struct Grab {
    at: Vec<i64>,
    next: usize,
    frames: Vec<crate::softvideo::Rgba>,
    size: (u32, u32),
    start: std::time::Instant,
}

thread_local! {
    /// Con un prelievo in corso i decodificatori di sopra consegnano i fotogrammi qui invece che al codificatore.
    static GRAB: std::cell::RefCell<Option<Grab>> = const { std::cell::RefCell::new(None) };
}

/// Chi guarda il video (il tasto Play): riceve ogni fotogramma con il suo tempo; `false` ferma la lettura.
type Tap = Box<dyn FnMut(&[u8], &[u8], &[u8], u32, u32, i64) -> bool>;

thread_local! {
    static TAP: std::cell::RefCell<Option<Tap>> = const { std::cell::RefCell::new(None) };
}

fn grabbing() -> bool {
    GRAB.with(|g| g.borrow().is_some()) || TAP.with(|t| t.borrow().is_some())
}

fn grab_frame(y: &[u8], u: &[u8], v: &[u8], width: u32, height: u32, time: i64) -> Result<(), String> {
    if let Some(mut tap) = TAP.with(|t| t.borrow_mut().take()) {
        let go = tap(y, u, v, width, height, time);
        TAP.with(|t| *t.borrow_mut() = Some(tap));
        return if go { Ok(()) } else { Err(GRAB_DONE.to_string()) };
    }
    GRAB.with(|g| {
        let mut g = g.borrow_mut();
        let Some(grab) = g.as_mut() else { return Ok(()) };
        if grab.next < grab.at.len() && time >= grab.at[grab.next] {
            grab.size = (width, height);
            if let Some(frame) = i420_rgba(y, u, v, width, height) {
                grab.frames.push(frame);
            }
            while grab.next < grab.at.len() && grab.at[grab.next] <= time {
                grab.next += 1;
            }
        }
        if grab.next >= grab.at.len() || grab.start.elapsed() > PEEK_BUDGET {
            return Err(GRAB_DONE.to_string());
        }
        Ok(())
    })
}

/// I fotogrammi del video ai secondi `at` (in ordine), per l'anteprima. Non scrive niente.
/// MP4 e MOV saltano da un fotogramma chiave all'altro; gli altri formati si leggono dall'inizio.
pub(crate) fn peek_frames(input: &Path, at: &[f64]) -> Result<Peek, String> {
    for quick in [iso_keyframes, crate::softvideo::peek] {
        if let Ok(peek) = quick(input, at) {
            if !peek.frames.is_empty() {
                return Ok(peek);
            }
        }
    }
    let at = at.iter().map(|s| (s.max(0.0) * 10_000_000.0) as i64).collect();
    GRAB.with(|g| *g.borrow_mut() = Some(Grab { at, next: 0, frames: Vec::new(), size: (0, 0), start: std::time::Instant::now() }));
    let out = std::env::temp_dir().join(format!("cedro-anteprima-{}.mp4", std::process::id()));
    let result = to_mp4_with(input, &out);
    let grab = GRAB.with(|g| g.borrow_mut().take());
    let _ = std::fs::remove_file(&out);
    let (frames, size) = grab.map(|g| (g.frames, g.size)).unwrap_or_default();
    if frames.is_empty() {
        return Err(match result {
            Err(err) if err != GRAB_DONE => err,
            _ => "il video non ha fotogrammi".into(),
        });
    }
    Ok(Peek { frames, size })
}

/// Legge il video dall'inizio e passa ogni fotogramma (YUV 4:2:0 e tempo in 100 ns) a `tap`,
/// finché `tap` non risponde `false`. Per il tasto Play dell'anteprima: non scrive niente.
pub(crate) fn play_frames(input: &Path, tap: impl FnMut(&[u8], &[u8], &[u8], u32, u32, i64) -> bool + 'static) -> Result<(), String> {
    TAP.with(|t| *t.borrow_mut() = Some(Box::new(tap)));
    let out = std::env::temp_dir().join(format!("cedro-play-{}-{:?}.mp4", std::process::id(), std::thread::current().id()));
    let result = to_mp4_with(input, &out);
    TAP.with(|t| t.borrow_mut().take());
    let _ = std::fs::remove_file(&out);
    match result {
        Err(err) if err != GRAB_DONE && !err.ends_with(GRAB_DONE) => Err(err),
        _ => Ok(()),
    }
}

fn iso_keyframes(input: &Path, at: &[f64]) -> Result<Peek, String> {
    let file = File::open(input).map_err(|err| err.to_string())?;
    let size = file.metadata().map_err(|err| err.to_string())?.len();
    let mut reader = mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size).map_err(|err| err.to_string())?;
    let (id, count, fps, sync, header) = {
        let track = reader
            .tracks()
            .values()
            .find(|t| matches!(t.media_type(), Ok(mp4::MediaType::H264)))
            .ok_or("niente H.264")?;
        let mut header = Vec::new();
        for nal in [track.sequence_parameter_set().map_err(|e| e.to_string())?, track.picture_parameter_set().map_err(|e| e.to_string())?] {
            header.extend_from_slice(&[0, 0, 0, 1]);
            header.extend_from_slice(nal);
        }
        let count = track.sample_count();
        let secs = track.duration().as_secs_f64();
        let fps = if track.frame_rate() > 0.0 { track.frame_rate() } else if secs > 0.0 { count as f64 / secs } else { 30.0 };
        let sync = track.trak.mdia.minf.stbl.stss.as_ref().map(|s| s.entries.clone()).unwrap_or_default();
        (track.track_id(), count, fps, sync, header)
    };
    let mut out = Peek { frames: Vec::new(), size: (0, 0) };
    if count == 0 {
        return Ok(out);
    }
    let start = std::time::Instant::now();
    for t in at {
        if start.elapsed() > PEEK_BUDGET {
            break;
        }
        let want = ((t.max(0.0) * fps) as u32 + 1).min(count);
        let key = if sync.is_empty() { want } else { sync.iter().rev().find(|&&k| k <= want).or(sync.first()).copied().unwrap_or(1) };
        let mut decoder = h264_decoder()?;
        let _ = decoder.decode(&header);
        let mut got = None;
        for index in key..=(key + 30).min(count) {
            let Some(sample) = reader.read_sample(id, index).map_err(|err| err.to_string())? else { continue };
            let annex = annex_sample(&sample.bytes);
            if annex.is_empty() {
                continue;
            }
            if let Ok(Some(frame)) = decoder.decode(&annex) {
                let o = owned_frame(&frame)?;
                out.size = (o.width, o.height);
                got = i420_rgba(&o.y, &o.u, &o.v, o.width, o.height);
                break;
            }
        }
        if got.is_none() {
            if let Some(frame) = decoder.flush_remaining().ok().and_then(|r| r.into_iter().next()) {
                let o = owned_frame(&frame)?;
                out.size = (o.width, o.height);
                got = i420_rgba(&o.y, &o.u, &o.v, o.width, o.height);
            }
        }
        out.frames.extend(got);
    }
    Ok(out)
}
/// Il decodificatore H.264: senza svuotarlo a ogni pacchetto, così i video con i fotogrammi B
/// (quasi tutti quelli dei telefoni e delle fotocamere) si leggono interi.
fn h264_decoder() -> Result<Decoder, String> {
    use openh264::decoder::{DecoderConfig, Flush};
    Decoder::with_api_config(OpenH264API::from_source(), DecoderConfig::new().flush_after_decode(Flush::NoFlush))
        .map_err(|err| format!("H.264 non letto: {err}"))
}

/// Inizio degli errori della decodifica sulla scheda video: chi li vede rifà il video con OpenH264.
const GPU_DECODE_FAILED: &str = "decodifica sulla scheda video";

thread_local! {
    /// Acceso dopo un errore della scheda: il secondo tentativo decodifica col processore.
    static GPU_DECODE_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Lancia `run`; se la decodifica sulla scheda video fallisce a metà, la rilancia con OpenH264.
fn with_gpu_fallback(run: impl Fn() -> Result<(), String>) -> Result<(), String> {
    match run() {
        Err(err) if err.starts_with(GPU_DECODE_FAILED) => {
            eprintln!("{err}: il video si rifà col processore");
            GPU_DECODE_OFF.with(|off| off.set(true));
            let again = run();
            GPU_DECODE_OFF.with(|off| off.set(false));
            again
        }
        other => other,
    }
}

#[cfg(target_os = "linux")]
fn frame_copy(frame: &crate::gpuenc::I420Frame) -> OwnedFrame {
    OwnedFrame { y: frame.y.clone(), u: frame.u.clone(), v: frame.v.clone(), width: frame.width, height: frame.height }
}

/// Il decodificatore H.264 dei campioni: la scheda video (VA-API) se c'è, è permessa e il video è adatto,
/// altrimenti OpenH264. Con la scheda i fotogrammi escono in ordine di visualizzazione, a tempo di decodifica
/// dei campioni; con OpenH264 portano il tempo del campione che li ha fatti uscire, come sempre.
struct SampleDecoder {
    soft: Option<Decoder>,
    #[cfg(target_os = "linux")]
    gpu: Option<crate::gpuenc::H264Decoder>,
    /// Durata e tempo dei campioni con un fotogramma, in attesa del loro fotogramma (solo scheda video).
    queue: std::collections::VecDeque<(i64, i64)>,
    /// Tempo e durata dell'ultimo campione (per i fotogrammi che escono alla fine con OpenH264).
    last: (i64, i64),
    /// Il controllo dei primi fotogrammi della scheda video contro OpenH264, finché non è fatto.
    check: Option<GpuCheck>,
}

/// Quanti fotogrammi della scheda si confrontano con OpenH264 prima di fidarsi.
const CHECK_FRAMES: usize = 8;
/// Quanti campioni si tengono per il confronto.
const CHECK_SAMPLES: usize = 40;

/// I primi campioni e i primi fotogrammi della scheda video, per controllarli contro OpenH264.
struct GpuCheck {
    header: Vec<u8>,
    samples: Vec<Vec<u8>>,
    frames: Vec<OwnedFrame>,
}

/// Quanto due piani sono vicini, in dB (99 se uguali).
fn plane_psnr(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let sum: f64 = a.iter().zip(b).map(|(&x, &y)| (f64::from(x) - f64::from(y)).powi(2)).sum();
    if sum == 0.0 {
        return 99.0;
    }
    10.0 * (255.0f64 * 255.0 / (sum / a.len() as f64)).log10()
}

impl GpuCheck {
    /// Decodifica gli stessi campioni con OpenH264 e confronta i fotogrammi: la scheda video di un PC
    /// che sbaglia in silenzio non deve rovinare un video. `whole`: i campioni sono tutto il video.
    fn verify(self, whole: bool) -> Result<(), String> {
        let mut soft = h264_decoder()?;
        if !self.header.is_empty() {
            let _ = soft.decode(&self.header);
        }
        let mut want: Vec<OwnedFrame> = Vec::new();
        for sample in &self.samples {
            if let Ok(Some(frame)) = soft.decode(sample) {
                want.push(owned_frame(&frame)?);
            }
        }
        if whole {
            if let Ok(rest) = soft.flush_remaining() {
                for frame in &rest {
                    want.push(owned_frame(frame)?);
                }
            }
        }
        let n = want.len().min(self.frames.len());
        if n == 0 && !self.frames.is_empty() {
            return Err(format!("{GPU_DECODE_FAILED}: nessun fotogramma da confrontare"));
        }
        for (index, (gpu, soft)) in self.frames.iter().zip(&want).take(n).enumerate() {
            let worst = if (gpu.width, gpu.height) == (soft.width, soft.height) {
                plane_psnr(&gpu.y, &soft.y).min(plane_psnr(&gpu.u, &soft.u)).min(plane_psnr(&gpu.v, &soft.v))
            } else {
                0.0
            };
            if worst < 38.0 {
                return Err(format!("{GPU_DECODE_FAILED}: il fotogramma {index} è diverso da quello di OpenH264 ({worst:.1} dB)"));
            }
        }
        Ok(())
    }
}

impl SampleDecoder {
    /// `header`: SPS e PPS in Annex B (vuoto se viaggiano nei campioni).
    fn open(header: &[u8]) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        // Ancora in prova sulle schede vere: si accende con CEDRO_VA_DECODE=1.
        if crate::sink::shape().gpu && std::env::var_os("CEDRO_VA_DECODE").is_some() && !GPU_DECODE_OFF.with(|off| off.get()) {
            let sets: Vec<&[u8]> = annex_nals(header).into_iter().filter(|nal| matches!(nal.first().map(|b| b & 0x1f), Some(7 | 8))).collect();
            if !sets.is_empty() {
                match crate::gpuenc::H264Decoder::new(&sets) {
                    Ok(gpu) => {
                        eprintln!("decodifica sulla scheda video: {}", gpu.name());
                        let check = GpuCheck { header: header.to_vec(), samples: Vec::new(), frames: Vec::new() };
                        return Ok(SampleDecoder { soft: None, gpu: Some(gpu), queue: Default::default(), last: (0, 333_333), check: Some(check) });
                    }
                    Err(err) => eprintln!("decodifica sulla scheda video non usata: {err}"),
                }
            }
        }
        let mut decoder = h264_decoder()?;
        if !header.is_empty() {
            let _ = decoder.decode(header);
        }
        #[cfg(target_os = "linux")]
        return Ok(SampleDecoder { soft: Some(decoder), gpu: None, queue: Default::default(), last: (0, 333_333), check: None });
        #[cfg(not(target_os = "linux"))]
        Ok(SampleDecoder { soft: Some(decoder), queue: Default::default(), last: (0, 333_333), check: None })
    }

    /// Un campione (Annex B) con la sua durata e il suo tempo (100 ns). I fotogrammi pronti vanno a `emit`
    /// con durata e tempo.
    fn decode(
        &mut self,
        annex: &[u8],
        duration: i64,
        time: i64,
        emit: &mut dyn FnMut(OwnedFrame, i64, i64) -> Result<(), String>,
    ) -> Result<(), String> {
        self.last = (time, duration);
        #[cfg(target_os = "linux")]
        if let Some(gpu) = self.gpu.as_mut() {
            let mut frames = Vec::new();
            let had = gpu
                .decode(annex, &mut |frame| {
                    frames.push(frame);
                    Ok(())
                })
                .map_err(|err| format!("{GPU_DECODE_FAILED}: {err}"))?;
            if had {
                self.queue.push_back((duration, time));
            }
            if let Some(check) = self.check.as_mut() {
                if check.samples.len() < CHECK_SAMPLES {
                    check.samples.push(annex.to_vec());
                }
                for frame in &frames {
                    if check.frames.len() < CHECK_FRAMES {
                        check.frames.push(frame_copy(frame));
                    }
                }
                if check.frames.len() >= CHECK_FRAMES {
                    if let Some(check) = self.check.take() {
                        check.verify(false)?;
                    }
                }
            }
            for frame in frames {
                let (d, t) = self.queue.pop_front().unwrap_or((duration, time));
                emit(OwnedFrame { y: frame.y, u: frame.u, v: frame.v, width: frame.width, height: frame.height }, d, t)?;
            }
            return Ok(());
        }
        let decoder = self.soft.as_mut().ok_or("H.264 non letto")?;
        if let Some(frame) = decoder.decode(annex).map_err(|err| format!("H.264 non letto: {err}"))? {
            emit(owned_frame(&frame)?, duration, time)?;
        }
        Ok(())
    }

    /// Fine dei campioni: escono i fotogrammi che il decodificatore teneva (per i B).
    fn finish(&mut self, emit: &mut dyn FnMut(OwnedFrame, i64, i64) -> Result<(), String>) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        if let Some(gpu) = self.gpu.as_mut() {
            let mut frames = Vec::new();
            gpu.flush(&mut |frame| {
                frames.push(frame);
                Ok(())
            })
            .map_err(|err| format!("{GPU_DECODE_FAILED}: {err}"))?;
            if let Some(mut check) = self.check.take() {
                for frame in &frames {
                    if check.frames.len() < CHECK_FRAMES {
                        check.frames.push(frame_copy(frame));
                    }
                }
                let whole = check.samples.len() < CHECK_SAMPLES;
                check.verify(whole)?;
            }
            for frame in frames {
                let (d, t) = self.queue.pop_front().unwrap_or((self.last.1, self.last.0 + self.last.1));
                emit(OwnedFrame { y: frame.y, u: frame.u, v: frame.v, width: frame.width, height: frame.height }, d, t)?;
            }
            return Ok(());
        }
        let last = self.last;
        if let Some(decoder) = self.soft.as_mut() {
            if let Ok(rest) = decoder.flush_remaining() {
                for (k, frame) in rest.iter().enumerate() {
                    emit(owned_frame(frame)?, last.1, last.0 + last.1 * (k as i64 + 1))?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::is_annex;

    #[test]
    fn length_prefixed_sample_starting_like_a_start_code() {
        // NAL da 256 byte con prefisso a 4 byte: 00 00 01 00 sembra un codice di inizio, ma è una lunghezza.
        let mut sample = vec![0, 0, 1, 0, 0x65];
        sample.extend(std::iter::repeat_n(0x11, 255));
        assert!(!is_annex(&sample));
        assert!(is_annex(&[0, 0, 0, 1, 0x67, 0x42, 0x00, 0x1e]));
    }

    /// Contatori enormi in tabelle piccole (file rovinato): errore, non gigabyte chiesti alla memoria.
    #[test]
    fn damaged_tables_do_not_ask_for_gigabytes() {
        let table = |head: &[u32]| head.iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<u8>>();
        assert!(super::sample_sizes(&table(&[0, 0, u32::MAX, 0])).is_err());
        assert!(super::sample_sizes(&table(&[0, 100, u32::MAX])).is_err());
        assert_eq!(super::sample_sizes(&table(&[0, 0, 2, 7, 9])).unwrap(), vec![7, 9]);
        let huge = table(&[0, u32::MAX, 0, 0]);
        assert!(super::chunk_offsets(None, Some(&huge)).is_err());
        assert!(super::chunk_offsets(Some(&huge), None).is_err());
        assert!(super::samples_per_chunk(&huge, 10).is_err());
        assert!(super::time_runs(&huge).is_err());
        // Voci di `stsc` sovrapposte: vale l'ultima che copre il blocco.
        let stsc = table(&[0, 3, 1, 5, 1, 1, 7, 1, 3, 9, 1]);
        assert_eq!(super::samples_per_chunk(&stsc, 4).unwrap(), vec![7, 7, 9, 9]);
    }

    fn test_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cedro-mux-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// Un fotogramma H.264 vero (SPS, PPS e IDR) di un'immagine grigia.
    fn gray_frame(w: usize, h: usize) -> Vec<u8> {
        let mut encoder = super::open_soft_h264(w as u32, h as u32, 333_333).unwrap();
        let (y, c) = (vec![120u8; w * h], vec![128u8; w * h / 4]);
        let picture = super::YUVSlices::new((&y, &c, &c), (w, h), (w, w / 2, w / 2));
        encoder.encode(&picture).unwrap().to_vec()
    }

    /// A 30 fps (333.333 × 100 ns) un'ora di video dura quanto la somma delle durate: a 1/90000 non si
    /// troncano (2999 per fotogramma perdevano più di un secondo l'ora).
    #[test]
    fn durations_do_not_drift() {
        let mut sink = <super::H264Aac as super::VideoSink>::open(&test_path("deriva.mp4"), 64, 64, 333_333, None).unwrap();
        let frames = 30 * 3600;
        let total: u64 = (0..frames).map(|_| sink.push_duration(333_333) as u64).sum();
        assert_eq!(total, (frames as u64 * 333_333 * 9 + 500) / 1000);
        assert!(total.abs_diff(3600 * 90_000) < 90_000 / 100, "{total}");
    }

    /// L'MP4 scritto mentre si codifica si rilegge: fotogrammi, durate, audio e «Ottimizza per il web».
    #[test]
    fn streamed_mp4_reads_back() {
        // Anche con `mdat` a 64 bit e `co64`, come oltre i 4 GiB (qui forzati su un file piccolo).
        // Con `CEDRO_MP4_PROVA=cartella` i file restano lì, per guardarli con ffprobe.
        for wide in [false, true] {
            streamed_mp4(wide);
        }
    }

    fn streamed_mp4(wide: bool) {
        let name = if wide { "flusso-64.mp4" } else { "flusso.mp4" };
        let path = test_path(name);
        let frame = gray_frame(64, 64);
        let mut sink = <super::H264Aac as super::VideoSink>::open(&path, 64, 64, 333_333, Some((44_100, 2))).unwrap();
        for _ in 0..30 {
            sink.push_annex(&frame, 333_333, true).unwrap();
            if wide {
                sink.mp4.as_mut().unwrap().wide_from = 0;
            }
        }
        let tone: Vec<i16> = (0..44_100 * 2).map(|i| ((i as f32 * 0.05).sin() * 8000.0) as i16).collect();
        for part in tone.chunks(2048) {
            super::VideoSink::write_pcm(&mut sink, part, 2, 44_100).unwrap();
        }
        super::VideoSink::finish(sink).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let has = |kind: &[u8]| bytes.windows(4).any(|w| w == kind);
        assert_eq!((has(b"co64"), has(b"stco"), has(b"free")), (wide, !wide, !wide));
        if let Ok(dir) = std::env::var("CEDRO_MP4_PROVA") {
            std::fs::copy(&path, std::path::Path::new(&dir).join(name)).unwrap();
        }
        for pass in 0..2 {
            let file = std::fs::File::open(&path).unwrap();
            let size = file.metadata().unwrap().len();
            let mut reader = mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size).unwrap();
            let (video, audio) = {
                let tracks = reader.tracks();
                let id = |kind| tracks.values().find(|t| t.media_type().ok() == Some(kind)).map(|t| (t.track_id(), t.sample_count())).unwrap();
                (id(mp4::MediaType::H264), id(mp4::MediaType::AAC))
            };
            assert_eq!(video.1, 30, "passata {pass}");
            assert!(audio.1 >= 43, "{}", audio.1);
            let first = reader.read_sample(video.0, 1).unwrap().unwrap();
            assert_eq!(first.duration, 3000);
            let last = reader.read_sample(video.0, 30).unwrap().unwrap();
            assert_eq!(last.start_time, 29 * 3000);
            assert!(super::annex_nals(&super::annex_sample(&last.bytes)).iter().any(|nal| nal.first().is_some_and(|b| b & 0x1f == 5)));
            assert!(reader.read_sample(audio.0, audio.1).unwrap().is_some());
            crate::mp4fast::make_faststart(&path).unwrap();
        }
        let _ = std::fs::remove_file(&path);
    }

    /// Prova a mano di un MP4 oltre i 4 GiB (`mdat` a 64 bit, `co64`), da guardare con ffprobe:
    /// `CEDRO_MP4_GRANDE=/percorso/grande.mp4 cargo test --lib oltre_4_gib -- --ignored`.
    #[test]
    #[ignore]
    fn oltre_4_gib() {
        let Ok(path) = std::env::var("CEDRO_MP4_GRANDE") else { return };
        let mut frame = gray_frame(64, 64);
        // Un NAL di riempimento (tipo 12) da 1 MiB per fotogramma: i decodificatori lo saltano.
        frame.extend_from_slice(&[0, 0, 0, 1, 0x0C]);
        frame.extend(std::iter::repeat_n(0xFF, 1 << 20));
        frame.push(0x80);
        let frames = 4400;
        let mut sink = <super::H264Aac as super::VideoSink>::open(std::path::Path::new(&path), 64, 64, 333_333, Some((44_100, 1))).unwrap();
        for _ in 0..frames {
            sink.push_annex(&frame, 333_333, true).unwrap();
        }
        let second: Vec<i16> = (0..44_100).map(|i| ((i as f32 * 0.06).sin() * 8000.0) as i16).collect();
        for _ in 0..frames / 30 {
            super::VideoSink::write_pcm(&mut sink, &second, 1, 44_100).unwrap();
        }
        super::VideoSink::finish(sink).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > u32::MAX as u64);
    }
}
