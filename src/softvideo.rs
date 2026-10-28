use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Mutex;

use symphonia::core::audio::{Layout, SampleBuffer};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL, CODEC_TYPE_OPUS};
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

/// Quello che il thread di lettura passa al codificatore, nell'ordine del file.
enum Item {
    Video { frame: Frame, time: i64, duration: i64 },
    Audio { samples: Vec<i16>, channels: u16, rate: u32 },
}

/// I fotogrammi decodificati di un gruppo, in ordine (o l'errore che ha fermato la lettura).
type Segment = Receiver<Result<Item, String>>;

/// Un gruppo di fotogrammi da un fotogramma chiave al successivo: si decodifica da solo, con un
/// decodificatore nuovo, mentre altri gruppi si decodificano accanto.
struct Gop {
    entries: Receiver<Entry>,
    out: SyncSender<Result<Item, String>>,
}

/// Quello che il thread di lettura passa a chi decodifica un gruppo, nell'ordine del file.
enum Entry {
    Video { bytes: Vec<u8>, time: i64, duration: i64 },
    Audio(Item),
}

/// Pacchetti (compressi, piccoli) in attesa di chi decodifica il loro gruppo.
const ENTRY_QUEUE: usize = 512;
/// Memoria per i fotogrammi già decodificati in attesa del codificatore, in tutto.
#[cfg(not(target_os = "android"))]
const FRAME_BUDGET: usize = 256 << 20;
#[cfg(target_os = "android")]
const FRAME_BUDGET: usize = 96 << 20;

/// Quanti gruppi si decodificano insieme.
fn decode_workers() -> usize {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    if cfg!(target_os = "android") { cores.clamp(1, 2) } else { cores.clamp(1, 4) }
}

pub(crate) fn transcode<S: VideoSink>(input: &Path, output: &Path) -> Result<(), String> {
    let codec = codec_of(input)?;
    let geometry = first_frame(input, &codec)?;
    let opened = open_media(input)?;
    // Senza durata dei blocchi (i WebM del browser) e senza la seconda misura, 30 fps.
    let fallback = if geometry.duration > 0 { geometry.duration } else { 333_333 };
    let mut encoder = S::open(
        output,
        geometry.width,
        geometry.height,
        fallback,
        opened.audio.as_ref().map(|audio| (audio.rate, audio.channels)),
    )?;
    // La lettura e la decodifica (VP8/VP9 e audio) vanno su altri thread, la codifica resta qui
    // (le opzioni e l'anteprima sono per thread). VP9 decodificato è quasi tutto il tempo: i gruppi
    // fra un fotogramma chiave e l'altro si decodificano insieme, ognuno su un thread.
    let workers = decode_workers();
    let frame_bytes = (geometry.width as usize * geometry.height as usize * 3 / 2).max(1);
    let frame_queue = (FRAME_BUDGET / frame_bytes / workers).clamp(2, 1024);
    // Al massimo `workers` gruppi hanno fotogrammi in attesa (quello che si codifica e quelli in coda):
    // la memoria resta entro `FRAME_BUDGET`.
    let (send, receive) = sync_channel::<Segment>(workers - 1);
    let reader = std::thread::Builder::new()
        .name("cedro-webm".into())
        .spawn(move || read_items(opened, codec, workers, frame_queue, &send))
        .map_err(|err| format!("WebM non letto: {err}"))?;
    let mut shown = 0u32;
    // Il fotogramma si scrive quando arriva il successivo e dura fino al tempo del prossimo: senza
    // durata nel blocco (MediaRecorder) non c'è altro, e la durata di traccia in millisecondi (33 per
    // 30 fps) farebbe perdere 6 secondi ogni 10 minuti. VP8 e VP9 non riordinano i fotogrammi.
    let mut pending: Option<(Frame, i64, i64)> = None;
    let mut last_dur = fallback;
    let mut written = |encoder: &mut S, (frame, time, dur): (Frame, i64, i64), next: Option<i64>| -> Result<(), String> {
        let dur = match next {
            Some(next) if next > time => next - time,
            _ if dur > 0 => dur,
            _ => last_dur,
        };
        last_dur = dur;
        encoder.write_yuv(&frame.y, &frame.u, &frame.v, frame.width, frame.height, time, dur)
    };
    let result = (|| -> Result<(), String> {
        for item in receive.iter().flat_map(|segment| segment.into_iter()) {
            match item? {
                Item::Video { frame, time, duration } => {
                    if let Some(prev) = pending.take() {
                        written(&mut encoder, prev, Some(time))?;
                        shown += 1;
                    }
                    pending = Some((frame, time, duration));
                }
                Item::Audio { samples, channels, rate } => encoder.write_pcm(&samples, channels, rate)?,
            }
        }
        if let Some(prev) = pending.take() {
            written(&mut encoder, prev, None)?;
            shown += 1;
        }
        Ok(())
    })();
    // Se la codifica si è fermata, i gruppi ancora in arrivo si buttano: chi li decodifica trova il
    // canale chiuso ed esce, e così il thread di lettura.
    if result.is_err() {
        receive.iter().for_each(drop);
    }
    drop(receive);
    let read = reader.join();
    result?;
    let (audio_packets, audio_samples) = read.map_err(|_| "il video non è stato letto".to_string())??;
    if shown == 0 {
        return Err("il video non ha fotogrammi".into());
    }
    if audio_packets > 0 && audio_samples == 0 {
        return Err("l'audio del video non è stato letto".into());
    }
    encoder.finish()
}

/// Il thread di lettura: divide i pacchetti video in gruppi (da un fotogramma chiave all'altro), li dà
/// da decodificare a `workers` thread e manda a `send`, in ordine, da dove prendere i fotogrammi di
/// ogni gruppo. L'audio si decodifica qui e viaggia col suo gruppo. Restituisce i pacchetti audio letti
/// e i campioni ottenuti.
fn read_items(mut opened: Opened, codec: Codec, workers: usize, frame_queue: usize, send: &SyncSender<Segment>) -> Result<(u32, usize), String> {
    let fail = |err: String| {
        let (out, segment) = sync_channel(1);
        let _ = out.send(Err(err));
        let _ = send.send(segment);
    };
    // Opus (WebM dei browser, vocali): symphonia dà i pacchetti, libopus li decodifica.
    let mut opus = match opened.audio.as_ref() {
        Some(audio) if audio.params.codec == CODEC_TYPE_OPUS => {
            match crate::audio::OpusStream::new(audio.params.extra_data.as_deref().unwrap_or_default()) {
                Ok(opus) => Some(opus),
                Err(err) => {
                    fail(format!("audio non decodificato: {err}"));
                    return Ok((0, 0));
                }
            }
        }
        _ => None,
    };
    let mut audio_dec = match opened.audio.as_ref() {
        Some(_) if opus.is_some() => None,
        Some(audio) => match symphonia::default::get_codecs().make(&audio.params, &DecoderOptions::default()) {
            Ok(dec) => Some(dec),
            Err(err) => {
                fail(format!("audio non decodificato: {err}"));
                return Ok((0, 0));
            }
        },
        None => None,
    };
    let audio_id = opened.audio.as_ref().map(|audio| audio.id);
    let mut audio_packets = 0u32;
    let mut audio_samples = 0usize;
    let mut buf: Option<(symphonia::core::audio::SignalSpec, SampleBuffer<i16>)> = None;
    let (jobs, queue) = sync_channel::<Gop>(workers);
    let queue = Mutex::new(queue);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (queue, codec) = (&queue, &codec);
            scope.spawn(move || loop {
                let job = queue.lock().map_err(|_| ()).and_then(|q| q.recv().map_err(|_| ()));
                let Ok(job) = job else { break };
                decode_gop(codec, job);
            });
        }
        // Il gruppo in corso: dove vanno i suoi pacchetti, e se ha già del video.
        let mut current: Option<(SyncSender<Entry>, bool)> = None;
        let put = |current: &mut Option<(SyncSender<Entry>, bool)>, entry: Entry, key: bool| -> bool {
            let video = matches!(entry, Entry::Video { .. });
            if current.as_ref().is_none_or(|(_, has_video)| key && *has_video) {
                // Il gruppo prima si chiude subito: chi lo decodifica può finirlo mentre qui si aspetta
                // il posto per il nuovo (con un solo thread, altrimenti, si aspetterebbero a vicenda).
                drop(current.take());
                let (entries, gop_entries) = sync_channel(ENTRY_QUEUE);
                let (out, segment) = sync_channel(frame_queue);
                if send.send(segment).is_err() || jobs.send(Gop { entries: gop_entries, out }).is_err() {
                    return false;
                }
                *current = Some((entries, false));
            }
            let (entries, has_video) = current.as_mut().expect("gruppo");
            *has_video |= video;
            entries.send(entry).is_ok()
        };
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
                let duration = ticks(packet.dur(), opened.numer, opened.denom);
                let key = is_keyframe(&codec, packet.buf());
                if !put(&mut current, Entry::Video { bytes: packet.buf().to_vec(), time, duration }, key) {
                    break;
                }
            } else if audio_id == Some(packet.track_id()) {
                audio_packets += 1;
                if let Some(opus) = opus.as_mut() {
                    let samples = opus.decode(packet.buf());
                    if samples.is_empty() {
                        continue;
                    }
                    audio_samples += samples.len();
                    let item = Item::Audio { samples: samples.to_vec(), channels: opus.channels(), rate: 48_000 };
                    if !put(&mut current, Entry::Audio(item), false) {
                        break;
                    }
                    continue;
                }
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
                    Err(err) => {
                        drop(current.take());
                        fail(format!("audio non decodificato: {err}"));
                        break;
                    }
                };
                let spec = *decoded.spec();
                // Il buffer si riusa finché basta e il formato non cambia.
                let reuse = matches!(&buf, Some((kept, b)) if *kept == spec && b.capacity() >= decoded.capacity() * spec.channels.count());
                if !reuse {
                    buf = Some((spec, SampleBuffer::<i16>::new(decoded.capacity() as u64, spec)));
                }
                let (_, buf) = buf.as_mut().expect("buffer");
                buf.copy_interleaved_ref(decoded);
                let samples = buf.samples();
                if samples.is_empty() {
                    continue;
                }
                audio_samples += samples.len();
                let item = Item::Audio { samples: samples.to_vec(), channels: spec.channels.count() as u16, rate: spec.rate };
                if !put(&mut current, Entry::Audio(item), false) {
                    break;
                }
            }
        }
        // Chiusi i canali, chi decodifica finisce il suo gruppo ed esce.
        drop(current);
        drop(jobs);
    });
    Ok((audio_packets, audio_samples))
}

/// Decodifica un gruppo con un decodificatore nuovo: i fotogrammi (e l'audio del gruppo) vanno a `out`.
fn decode_gop(codec: &Codec, gop: Gop) {
    let mut video = decoder(codec);
    for entry in gop.entries.iter() {
        match entry {
            Entry::Video { bytes, time, duration } => {
                let frames = match shown_frames(&mut video, &bytes) {
                    Ok(frames) => frames,
                    Err(err) => {
                        let _ = gop.out.send(Err(err));
                        return;
                    }
                };
                for frame in frames {
                    if gop.out.send(Ok(Item::Video { frame, time, duration })).is_err() {
                        return;
                    }
                }
            }
            Entry::Audio(item) => {
                if gop.out.send(Ok(item)).is_err() {
                    return;
                }
            }
        }
    }
}

/// Un fotogramma chiave: da lì in poi la decodifica non dipende da nulla di quello che c'era prima
/// (VP9 rinfresca tutti i riferimenti e riparte dalle probabilità iniziali). Si controlla anche il
/// codice di sincronia, così un pacchetto strano non apre un gruppo a metà.
fn is_keyframe(codec: &Codec, bytes: &[u8]) -> bool {
    match codec {
        // VP8: bit 0 a zero e il codice 9d 01 2a dopo i 3 byte dell'intestazione.
        Codec::Vp8 => bytes.len() >= 6 && bytes[0] & 1 == 0 && bytes[3..6] == [0x9d, 0x01, 0x2a],
        Codec::Vp9 => {
            let Some(first) = split_superframe(bytes).into_iter().next() else { return false };
            let mut bits = BitReader { data: first, at: 0 };
            let Some(marker) = bits.read(2) else { return false };
            let (Some(low), Some(high)) = (bits.read(1), bits.read(1)) else { return false };
            if marker != 2 || ((high << 1) | low == 3 && bits.read(1) != Some(0)) {
                return false;
            }
            // show_existing_frame, frame_type (0 = chiave), show_frame, error_resilient_mode, codice di sincronia.
            bits.read(1) == Some(0) && bits.read(1) == Some(0) && bits.read(2).is_some() && bits.read(24) == Some(0x49_83_42)
        }
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    at: usize,
}

impl BitReader<'_> {
    fn read(&mut self, n: usize) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..n {
            let byte = *self.data.get(self.at / 8)?;
            value = (value << 1) | u32::from((byte >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Some(value)
    }
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
        let mut duration = ticks(packet.dur(), opened.numer, opened.denom);
        if let Some(frame) = shown_frames(&mut video, packet.buf())?.into_iter().next() {
            if frame.width < 2 || frame.height < 2 {
                return Err("il video non ha una dimensione".into());
            }
            // Senza durata nel blocco: la distanza dal pacchetto video dopo.
            if duration <= 0 {
                let time = ticks(packet.ts(), opened.numer, opened.denom);
                while let Ok(next) = opened.format.next_packet() {
                    if next.track_id() == opened.video_id {
                        duration = (ticks(next.ts(), opened.numer, opened.denom) - time).max(0);
                        break;
                    }
                }
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
        let opus = track.codec_params.codec == CODEC_TYPE_OPUS;
        if !opus
            && symphonia::default::get_codecs()
                .make(&track.codec_params, &DecoderOptions::default())
                .is_err()
        {
            return None;
        }
        Some(AudioIn {
            id: track.id,
            rate: if opus { 48_000 } else { track.codec_params.sample_rate.unwrap_or(44100) },
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
    let value = ((ts as u128).saturating_mul(numer as u128).saturating_mul(10_000_000) + denom as u128 / 2) / denom as u128;
    i64::try_from(value).unwrap_or(i64::MAX)
}

// ---- Fotogrammi per l'anteprima ----

/// Un fotogramma RGBA (larghezza, altezza, pixel).
pub(crate) type Rgba = (u32, u32, Vec<u8>);

/// Lato lungo dei fotogrammi dell'anteprima.
pub(crate) const PEEK_SIDE: u32 = 960;
/// Oltre questo tempo l'anteprima si accontenta dei fotogrammi che ha.
pub(crate) const PEEK_BUDGET: std::time::Duration = std::time::Duration::from_secs(12);

/// I fotogrammi presi per l'anteprima e la misura vera del video.
pub(crate) struct Peek {
    pub frames: Vec<Rgba>,
    pub size: (u32, u32),
}

/// Da YUV 4:2:0 (BT.601) a RGBA, rimpicciolito a `PEEK_SIDE`.
pub(crate) fn i420_rgba(y: &[u8], u: &[u8], v: &[u8], width: u32, height: u32) -> Option<Rgba> {
    let (w, h) = (width as usize, height as usize);
    let cw = w.div_ceil(2);
    if w == 0 || h == 0 || y.len() < w * h || u.len() < cw * h.div_ceil(2) || v.len() < cw * h.div_ceil(2) {
        return None;
    }
    let mut out = vec![255u8; w * h * 4];
    for row in 0..h {
        for col in 0..w {
            let c = (row / 2) * cw + col / 2;
            let yy = (y[row * w + col] as i32 - 16).max(0) * 298;
            let (uu, vv) = (u[c] as i32 - 128, v[c] as i32 - 128);
            let i = (row * w + col) * 4;
            out[i] = ((yy + 409 * vv + 128) >> 8).clamp(0, 255) as u8;
            out[i + 1] = ((yy - 100 * uu - 208 * vv + 128) >> 8).clamp(0, 255) as u8;
            out[i + 2] = ((yy + 516 * uu + 128) >> 8).clamp(0, 255) as u8;
        }
    }
    shrink(width, height, out)
}

/// Un fotogramma RGBA portato al massimo a `PEEK_SIDE` di lato lungo.
pub(crate) fn shrink(width: u32, height: u32, rgba: Vec<u8>) -> Option<Rgba> {
    let img = image::RgbaImage::from_raw(width, height, rgba)?;
    let side = width.max(height);
    let img = if side > PEEK_SIDE { image::imageops::thumbnail(&img, (width * PEEK_SIDE / side).max(1), (height * PEEK_SIDE / side).max(1)) } else { img };
    Some((img.width(), img.height(), img.into_raw()))
}

/// WebM VP8 e VP9: per ogni secondo di `at` salta al fotogramma chiave più vicino e lo decodifica.
pub(crate) fn peek(input: &Path, at: &[f64]) -> Result<Peek, String> {
    use symphonia::core::formats::{SeekMode, SeekTo};
    use symphonia::core::units::Time;
    let codec = codec_of(input)?;
    let mut opened = open_media(input)?;
    let start = std::time::Instant::now();
    let mut out = Peek { frames: Vec::new(), size: (0, 0) };
    for &t in at {
        if start.elapsed() > PEEK_BUDGET {
            break;
        }
        let target = (t.max(0.0) * 10_000_000.0) as i64;
        let jumped = opened
            .format
            .seek(SeekMode::Coarse, SeekTo::Time { time: Time::from(t.max(0.0)), track_id: Some(opened.video_id) })
            .is_ok();
        let mut video = decoder(&codec);
        for _ in 0..600 {
            let Ok(packet) = opened.format.next_packet() else { break };
            if packet.track_id() != opened.video_id {
                continue;
            }
            let time = ticks(packet.ts(), opened.numer, opened.denom);
            // Senza salto si decodifica tutto, ma si tiene solo il fotogramma del punto chiesto.
            let Ok(frames) = shown_frames(&mut video, packet.buf()) else { continue };
            let Some(frame) = frames.into_iter().next() else { continue };
            if !jumped && time + 5_000_000 < target {
                continue;
            }
            out.size = (frame.width, frame.height);
            out.frames.extend(i420_rgba(&frame.y, &frame.u, &frame.v, frame.width, frame.height));
            break;
        }
    }
    Ok(out)
}

/// Da YUV 4:2:0 a RGBA già piccolo (al massimo `side` di lato lungo), prendendo un punto ogni tanti:
/// è svelto, per i fotogrammi del tasto Play.
pub(crate) fn i420_small(y: &[u8], u: &[u8], v: &[u8], width: u32, height: u32, side: u32) -> Option<Rgba> {
    let (w, h) = (width as usize, height as usize);
    let cw = w.div_ceil(2);
    if w == 0 || h == 0 || y.len() < w * h || u.len() < cw * h.div_ceil(2) || v.len() < cw * h.div_ceil(2) {
        return None;
    }
    let step = (width.max(height).div_ceil(side.max(1))).max(1) as usize;
    let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
    let mut out = vec![255u8; ow * oh * 4];
    for oy in 0..oh {
        let row = oy * step;
        for ox in 0..ow {
            let col = ox * step;
            let c = (row / 2) * cw + col / 2;
            let yy = (y[row * w + col] as i32 - 16).max(0) * 298;
            let (uu, vv) = (u[c] as i32 - 128, v[c] as i32 - 128);
            let i = (oy * ow + ox) * 4;
            out[i] = ((yy + 409 * vv + 128) >> 8).clamp(0, 255) as u8;
            out[i + 1] = ((yy - 100 * uu - 208 * vv + 128) >> 8).clamp(0, 255) as u8;
            out[i + 2] = ((yy + 516 * uu + 128) >> 8).clamp(0, 255) as u8;
        }
    }
    Some((ow as u32, oh as u32, out))
}

/// WebM VP8 e VP9 dal secondo `from`: ogni fotogramma (YUV 4:2:0, tempo in 100 ns) va a `tap`
/// finché non risponde `false`. Per il tasto Play dell'anteprima.
#[allow(dead_code)]
pub(crate) fn play(input: &Path, from: f64, mut tap: impl FnMut(&[u8], &[u8], &[u8], u32, u32, i64) -> bool) -> Result<(), String> {
    use symphonia::core::formats::{SeekMode, SeekTo};
    use symphonia::core::units::Time;
    let codec = codec_of(input)?;
    let mut opened = open_media(input)?;
    if from > 0.0 {
        let _ = opened.format.seek(SeekMode::Coarse, SeekTo::Time { time: Time::from(from), track_id: Some(opened.video_id) });
    }
    let mut video = decoder(&codec);
    loop {
        let Ok(packet) = opened.format.next_packet() else { break };
        if packet.track_id() != opened.video_id {
            continue;
        }
        let time = ticks(packet.ts(), opened.numer, opened.denom);
        let Ok(frames) = shown_frames(&mut video, packet.buf()) else { continue };
        for frame in frames {
            if !tap(&frame.y, &frame.u, &frame.v, frame.width, frame.height, time) {
                return Ok(());
            }
        }
    }
    Ok(())
}
