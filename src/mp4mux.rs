//! MP4 minimo: un video H.264 (avc1) e, se c'è, un audio AAC (mp4a).
//! I tempi del video sono in 1/90000, quelli del filmato in millisecondi.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

/// Un campione già scritto nel file: i suoi dati stanno in `mdat`, qui restano solo le misure.
pub(crate) struct Sample {
    pub size: u32,
    pub duration: u32,
    pub sync: bool,
}

pub(crate) struct Movie {
    pub width: u32,
    pub height: u32,
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
    pub video: Vec<Sample>,
    pub audio_rate: u32,
    pub audio_channels: u16,
    pub audio_asc: Vec<u8>,
    pub audio: Vec<Sample>,
    /// Campioni di avvio del codificatore AAC in testa all'audio: si saltano con `elst`.
    pub audio_delay: u32,
}

const MP4_ERR: &str = "MP4 non scritto";

/// L'MP4 scritto mentre si codifica: i fotogrammi vanno subito nel file (in memoria restano solo le
/// misure); l'audio AAC, piccolo, si accoda alla fine e poi viene il `moov`. Oltre i 4 GiB `mdat`
/// prende la misura a 64 bit e gli offset passano a `co64`.
pub(crate) struct Mp4Writer {
    out: BufWriter<File>,
    /// Dove comincia l'intestazione di `mdat` (16 byte riservati: `free` + `mdat`, o `mdat` a 64 bit).
    mdat_at: u64,
    video_bytes: u64,
    /// Oltre questa misura si passa a 64 bit: 4 GiB (più bassa solo nelle prove).
    pub(crate) wide_from: u64,
}

impl Mp4Writer {
    pub(crate) fn create(path: &Path) -> Result<Self, String> {
        let file = File::create(path).map_err(|err| format!("{MP4_ERR}: {err}"))?;
        let mut out = BufWriter::with_capacity(1 << 20, file);
        let ftyp = ftyp_box();
        out.write_all(&ftyp).map_err(|err| format!("{MP4_ERR}: {err}"))?;
        // Posto per l'intestazione di `mdat`, riscritta in `finish` quando la misura è nota.
        out.write_all(&[0u8; 16]).map_err(|err| format!("{MP4_ERR}: {err}"))?;
        Ok(Mp4Writer { out, mdat_at: ftyp.len() as u64, video_bytes: 0, wide_from: u32::MAX as u64 })
    }

    pub(crate) fn write_video(&mut self, data: &[u8]) -> Result<(), String> {
        self.out.write_all(data).map_err(|err| format!("{MP4_ERR}: {err}"))?;
        self.video_bytes += data.len() as u64;
        Ok(())
    }

    /// Chiude il file: l'audio dopo i fotogrammi, il `moov` in fondo, la misura di `mdat`.
    pub(crate) fn finish(mut self, movie: &Movie, audio: &[Vec<u8>]) -> Result<(), String> {
        if movie.video.is_empty() {
            return Err("il video non ha fotogrammi".into());
        }
        if movie.sps.len() < 4 || movie.pps.is_empty() {
            return Err("H.264 senza parametri".into());
        }
        let mut audio_bytes = 0u64;
        for packet in audio {
            self.out.write_all(packet).map_err(|err| format!("{MP4_ERR}: {err}"))?;
            audio_bytes += packet.len() as u64;
        }
        let video_offset = self.mdat_at + 16;
        let audio_offset = video_offset + self.video_bytes;
        let mdat_end = audio_offset + audio_bytes;
        // Con `stco` gli offset devono stare in 32 bit anche dopo «Ottimizza per il web», che mette
        // il `moov` davanti e li sposta della sua misura.
        let mut moov = moov_box(movie, video_offset, audio_offset, false);
        if audio_offset.saturating_add(moov.len() as u64) > self.wide_from {
            moov = moov_box(movie, video_offset, audio_offset, true);
        }
        self.out.write_all(&moov).map_err(|err| format!("{MP4_ERR}: {err}"))?;
        self.out.seek(SeekFrom::Start(self.mdat_at)).map_err(|err| format!("{MP4_ERR}: {err}"))?;
        self.out.write_all(&mdat_header(mdat_end - self.mdat_at, self.wide_from)).map_err(|err| format!("{MP4_ERR}: {err}"))?;
        self.out.flush().map_err(|err| format!("{MP4_ERR}: {err}"))?;
        Ok(())
    }
}

/// I 16 byte davanti ai dati, per un `mdat` di `span` byte (intestazione compresa) che parte da lì:
/// `free` vuoto e `mdat` a 32 bit se ci sta, altrimenti `mdat` con la misura a 64 bit.
fn mdat_header(span: u64, wide_from: u64) -> [u8; 16] {
    let mut head = [0u8; 16];
    let inner = span - 8;
    if inner <= wide_from.min(u32::MAX as u64) {
        head[0..4].copy_from_slice(&8u32.to_be_bytes());
        head[4..8].copy_from_slice(b"free");
        head[8..12].copy_from_slice(&(inner as u32).to_be_bytes());
        head[12..16].copy_from_slice(b"mdat");
    } else {
        head[0..4].copy_from_slice(&1u32.to_be_bytes());
        head[4..8].copy_from_slice(b"mdat");
        head[8..16].copy_from_slice(&span.to_be_bytes());
    }
    head
}

fn ftyp_box() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"isom");
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(b"isom");
    body.extend_from_slice(b"iso2");
    body.extend_from_slice(b"avc1");
    body.extend_from_slice(b"mp41");
    box_of(b"ftyp", &body)
}

fn moov_box(movie: &Movie, video_offset: u64, audio_offset: u64, wide: bool) -> Vec<u8> {
    let video_media = sum_duration(&movie.video);
    let audio_media = sum_duration(&movie.audio);
    let video_movie = scale_duration(video_media, 90_000, 1000);
    let audio_movie = if movie.audio.is_empty() {
        0
    } else {
        scale_duration(audio_media.saturating_sub(audio_skip(movie)), movie.audio_rate, 1000)
    };
    let movie_duration = video_movie.max(audio_movie).max(1);
    let tracks = if movie.audio.is_empty() { 2 } else { 3 };
    let mut body = mvhd(movie_duration, tracks);
    body.extend(video_trak(movie, video_movie, video_offset, wide));
    if !movie.audio.is_empty() {
        body.extend(audio_trak(movie, audio_movie, audio_offset, wide));
    }
    box_of(b"moov", &body)
}

fn video_trak(movie: &Movie, movie_duration: u32, offset: u64, wide: bool) -> Vec<u8> {
    let media = sum_duration(&movie.video).max(1);
    let mut body = tkhd(1, movie_duration, movie.width, movie.height, false);
    body.extend(mdia(b"vide", b"VideoHandler", 90_000, media, video_minf(movie, offset, wide)));
    box_of(b"trak", &body)
}

/// I campioni d'avvio da saltare, se l'audio è più lungo di loro.
fn audio_skip(movie: &Movie) -> u32 {
    if movie.audio_delay < sum_duration(&movie.audio) { movie.audio_delay } else { 0 }
}

fn audio_trak(movie: &Movie, movie_duration: u32, offset: u64, wide: bool) -> Vec<u8> {
    let media = sum_duration(&movie.audio).max(1);
    let mut body = tkhd(2, movie_duration, 0, 0, true);
    let skip = audio_skip(movie);
    if skip > 0 {
        // L'audio comincia dopo i campioni d'avvio del codificatore (circa 45 ms): così resta a tempo col video.
        let mut elst = full_box(0, 0);
        elst.extend_from_slice(&1u32.to_be_bytes());
        elst.extend_from_slice(&movie_duration.to_be_bytes());
        elst.extend_from_slice(&skip.to_be_bytes());
        elst.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        body.extend(box_of(b"edts", &box_of(b"elst", &elst)));
    }
    body.extend(mdia(
        b"soun",
        b"SoundHandler",
        movie.audio_rate,
        media,
        audio_minf(movie, offset, wide),
    ));
    box_of(b"trak", &body)
}

fn video_minf(movie: &Movie, offset: u64, wide: bool) -> Vec<u8> {
    let mut vmhd = full_box(0, 1);
    vmhd.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
    let mut body = box_of(b"vmhd", &vmhd);
    body.extend(dinf());
    body.extend(stbl_video(movie, offset, wide));
    box_of(b"minf", &body)
}

fn audio_minf(movie: &Movie, offset: u64, wide: bool) -> Vec<u8> {
    let mut smhd = full_box(0, 0);
    smhd.extend_from_slice(&0u16.to_be_bytes());
    smhd.extend_from_slice(&0u16.to_be_bytes());
    let mut body = box_of(b"smhd", &smhd);
    body.extend(dinf());
    body.extend(stbl_audio(movie, offset, wide));
    box_of(b"minf", &body)
}

fn stbl_video(movie: &Movie, offset: u64, wide: bool) -> Vec<u8> {
    let mut body = stsd_video(movie);
    body.extend(stts(&movie.video));
    body.extend(stss(&movie.video));
    body.extend(stsc(movie.video.len() as u32));
    body.extend(stsz(&movie.video));
    body.extend(stco(offset, wide));
    box_of(b"stbl", &body)
}

fn stbl_audio(movie: &Movie, offset: u64, wide: bool) -> Vec<u8> {
    let mut body = stsd_audio(movie);
    body.extend(stts(&movie.audio));
    body.extend(stsc(movie.audio.len() as u32));
    body.extend(stsz(&movie.audio));
    body.extend(stco(offset, wide));
    box_of(b"stbl", &body)
}

fn stsd_video(movie: &Movie) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.extend_from_slice(&[0; 6]);
    entry.extend_from_slice(&1u16.to_be_bytes());
    entry.extend_from_slice(&0u16.to_be_bytes());
    entry.extend_from_slice(&0u16.to_be_bytes());
    entry.extend_from_slice(&[0; 12]);
    entry.extend_from_slice(&(movie.width as u16).to_be_bytes());
    entry.extend_from_slice(&(movie.height as u16).to_be_bytes());
    entry.extend_from_slice(&0x0048_0000u32.to_be_bytes());
    entry.extend_from_slice(&0x0048_0000u32.to_be_bytes());
    entry.extend_from_slice(&0u32.to_be_bytes());
    entry.extend_from_slice(&1u16.to_be_bytes());
    entry.extend_from_slice(&[0; 32]);
    entry.extend_from_slice(&0x0018u16.to_be_bytes());
    entry.extend_from_slice(&(-1i16).to_be_bytes());
    entry.extend(box_of(b"avcC", &avcc(&movie.sps, &movie.pps)));
    let sample = box_of(b"avc1", &entry);
    let mut body = full_box(0, 0);
    body.extend_from_slice(&1u32.to_be_bytes());
    body.extend(sample);
    box_of(b"stsd", &body)
}

fn avcc(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut out = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
    out.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    out.extend_from_slice(sps);
    out.push(1);
    out.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    out.extend_from_slice(pps);
    out
}

fn stsd_audio(movie: &Movie) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.extend_from_slice(&[0; 6]);
    entry.extend_from_slice(&1u16.to_be_bytes());
    entry.extend_from_slice(&0u16.to_be_bytes());
    entry.extend_from_slice(&0u16.to_be_bytes());
    entry.extend_from_slice(&0u32.to_be_bytes());
    entry.extend_from_slice(&movie.audio_channels.to_be_bytes());
    entry.extend_from_slice(&16u16.to_be_bytes());
    entry.extend_from_slice(&0u16.to_be_bytes());
    entry.extend_from_slice(&0u16.to_be_bytes());
    entry.extend_from_slice(&(movie.audio_rate << 16).to_be_bytes());
    entry.extend(esds_box(&movie.audio_asc));
    let sample = box_of(b"mp4a", &entry);
    let mut body = full_box(0, 0);
    body.extend_from_slice(&1u32.to_be_bytes());
    body.extend(sample);
    box_of(b"stsd", &body)
}

fn esds_box(asc: &[u8]) -> Vec<u8> {
    let dsi = descriptor(0x05, asc);
    let mut decoder = vec![0x40, 0x15, 0, 0, 0];
    decoder.extend_from_slice(&128_000u32.to_be_bytes());
    decoder.extend_from_slice(&128_000u32.to_be_bytes());
    decoder.extend(dsi);
    let decoder = descriptor(0x04, &decoder);
    let sl = descriptor(0x06, &[0x02]);
    let mut es = Vec::new();
    es.extend_from_slice(&0u16.to_be_bytes());
    es.push(0);
    es.extend(decoder);
    es.extend(sl);
    let mut body = full_box(0, 0);
    body.extend(descriptor(0x03, &es));
    box_of(b"esds", &body)
}

fn descriptor(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let mut size = body.len();
    let mut bytes = Vec::new();
    loop {
        let mut part = (size & 0x7f) as u8;
        size >>= 7;
        if !bytes.is_empty() {
            part |= 0x80;
        }
        bytes.push(part);
        if size == 0 {
            break;
        }
    }
    bytes.reverse();
    if bytes.is_empty() {
        bytes.push(0);
    }
    out.extend(bytes);
    out.extend_from_slice(body);
    out
}

fn stts(samples: &[Sample]) -> Vec<u8> {
    let mut entries: Vec<(u32, u32)> = Vec::new();
    for sample in samples {
        let duration = sample.duration.max(1);
        if let Some(last) = entries.last_mut() {
            if last.1 == duration {
                last.0 += 1;
                continue;
            }
        }
        entries.push((1, duration));
    }
    let mut body = full_box(0, 0);
    body.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for (count, duration) in entries {
        body.extend_from_slice(&count.to_be_bytes());
        body.extend_from_slice(&duration.to_be_bytes());
    }
    box_of(b"stts", &body)
}

fn stss(samples: &[Sample]) -> Vec<u8> {
    let mut ids: Vec<u32> = samples
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.sync)
        .map(|(index, _)| index as u32 + 1)
        .collect();
    if ids.is_empty() {
        ids.push(1);
    }
    let mut body = full_box(0, 0);
    body.extend_from_slice(&(ids.len() as u32).to_be_bytes());
    for id in ids {
        body.extend_from_slice(&id.to_be_bytes());
    }
    box_of(b"stss", &body)
}

fn stsc(count: u32) -> Vec<u8> {
    let mut body = full_box(0, 0);
    body.extend_from_slice(&1u32.to_be_bytes());
    body.extend_from_slice(&1u32.to_be_bytes());
    body.extend_from_slice(&count.max(1).to_be_bytes());
    body.extend_from_slice(&1u32.to_be_bytes());
    box_of(b"stsc", &body)
}

fn stsz(samples: &[Sample]) -> Vec<u8> {
    let mut body = full_box(0, 0);
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&(samples.len() as u32).to_be_bytes());
    for sample in samples {
        body.extend_from_slice(&sample.size.to_be_bytes());
    }
    box_of(b"stsz", &body)
}

/// Un solo blocco di campioni per traccia: il suo offset, a 32 bit (`stco`) o a 64 (`co64`).
fn stco(offset: u64, wide: bool) -> Vec<u8> {
    let mut body = full_box(0, 0);
    body.extend_from_slice(&1u32.to_be_bytes());
    if wide {
        body.extend_from_slice(&offset.to_be_bytes());
        return box_of(b"co64", &body);
    }
    body.extend_from_slice(&(offset as u32).to_be_bytes());
    box_of(b"stco", &body)
}

fn mdia(handler: &[u8; 4], name: &[u8], timescale: u32, duration: u32, minf: Vec<u8>) -> Vec<u8> {
    let mut body = mdhd(timescale, duration);
    body.extend(hdlr(handler, name));
    body.extend(minf);
    box_of(b"mdia", &body)
}

fn mdhd(timescale: u32, duration: u32) -> Vec<u8> {
    let mut body = full_box(0, 0);
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&timescale.to_be_bytes());
    body.extend_from_slice(&duration.to_be_bytes());
    body.extend_from_slice(&0x55c4u16.to_be_bytes());
    body.extend_from_slice(&0u16.to_be_bytes());
    box_of(b"mdhd", &body)
}

fn hdlr(handler: &[u8; 4], name: &[u8]) -> Vec<u8> {
    let mut body = full_box(0, 0);
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(handler);
    body.extend_from_slice(&[0; 12]);
    body.extend_from_slice(name);
    body.push(0);
    box_of(b"hdlr", &body)
}

fn dinf() -> Vec<u8> {
    let url = full_box(0, 1);
    let url = box_of(b"url ", &url);
    let mut dref = full_box(0, 0);
    dref.extend_from_slice(&1u32.to_be_bytes());
    dref.extend(url);
    box_of(b"dinf", &box_of(b"dref", &dref))
}

fn tkhd(track_id: u32, duration: u32, width: u32, height: u32, audio: bool) -> Vec<u8> {
    let mut body = full_box(0, 7);
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&track_id.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&duration.to_be_bytes());
    body.extend_from_slice(&[0; 8]);
    body.extend_from_slice(&0u16.to_be_bytes());
    body.extend_from_slice(&(if audio { 1u16 } else { 0 }).to_be_bytes());
    body.extend_from_slice(&(if audio { 0x0100u16 } else { 0 }).to_be_bytes());
    body.extend_from_slice(&0u16.to_be_bytes());
    body.extend(matrix());
    body.extend_from_slice(&(width << 16).to_be_bytes());
    body.extend_from_slice(&(height << 16).to_be_bytes());
    box_of(b"tkhd", &body)
}

fn mvhd(duration: u32, next_track: u32) -> Vec<u8> {
    let mut body = full_box(0, 0);
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&1000u32.to_be_bytes());
    body.extend_from_slice(&duration.to_be_bytes());
    body.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    body.extend_from_slice(&0x0100u16.to_be_bytes());
    body.extend_from_slice(&[0; 10]);
    body.extend(matrix());
    body.extend_from_slice(&[0; 24]);
    body.extend_from_slice(&next_track.to_be_bytes());
    box_of(b"mvhd", &body)
}

fn matrix() -> Vec<u8> {
    let mut out = Vec::with_capacity(36);
    for value in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out
}

fn full_box(version: u8, flags: u32) -> Vec<u8> {
    vec![version, (flags >> 16) as u8, (flags >> 8) as u8, flags as u8]
}

fn box_of(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = 8 + body.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(size as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

fn sum_duration(samples: &[Sample]) -> u32 {
    samples.iter().fold(0u32, |acc, sample| acc.saturating_add(sample.duration.max(1)))
}

fn scale_duration(duration: u32, from: u32, to: u32) -> u32 {
    if from == 0 {
        return 1;
    }
    let value = (duration as u64).saturating_mul(to as u64) / from as u64;
    u32::try_from(value).unwrap_or(u32::MAX).max(1)
}
