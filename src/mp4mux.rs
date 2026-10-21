//! MP4 minimo: un video H.264 (avc1) e, se c'è, un audio AAC (mp4a).
//! I tempi del video sono in 1/90000, quelli del filmato in millisecondi.

use std::fs::File;
use std::io::Write;
use std::path::Path;

pub(crate) struct Sample {
    pub data: Vec<u8>,
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
}

pub(crate) fn write_mp4(path: &Path, movie: &Movie) -> Result<(), String> {
    if movie.video.is_empty() {
        return Err("il video non ha fotogrammi".into());
    }
    if movie.sps.len() < 4 || movie.pps.is_empty() {
        return Err("H.264 senza parametri".into());
    }
    let video_bytes = movie.video.iter().map(|sample| sample.data.len()).sum::<usize>();
    let audio_bytes = movie.audio.iter().map(|sample| sample.data.len()).sum::<usize>();
    let mdat_payload = video_bytes + audio_bytes;
    let ftyp = ftyp_box();
    let mdat_header = 8u64;
    let video_offset = ftyp.len() as u64 + mdat_header;
    let audio_offset = video_offset + video_bytes as u64;
    let moov = moov_box(movie, video_offset, audio_offset);
    let mut file = File::create(path).map_err(|err| format!("MP4 non scritto: {err}"))?;
    file.write_all(&ftyp).map_err(|err| format!("MP4 non scritto: {err}"))?;
    let mdat_size = mdat_header + mdat_payload as u64;
    if mdat_size > u32::MAX as u64 {
        return Err("il video è troppo lungo per questo MP4".into());
    }
    file.write_all(&(mdat_size as u32).to_be_bytes()).map_err(|err| format!("MP4 non scritto: {err}"))?;
    file.write_all(b"mdat").map_err(|err| format!("MP4 non scritto: {err}"))?;
    for sample in &movie.video {
        file.write_all(&sample.data).map_err(|err| format!("MP4 non scritto: {err}"))?;
    }
    for sample in &movie.audio {
        file.write_all(&sample.data).map_err(|err| format!("MP4 non scritto: {err}"))?;
    }
    file.write_all(&moov).map_err(|err| format!("MP4 non scritto: {err}"))?;
    Ok(())
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

fn moov_box(movie: &Movie, video_offset: u64, audio_offset: u64) -> Vec<u8> {
    let video_media = sum_duration(&movie.video);
    let audio_media = sum_duration(&movie.audio);
    let video_movie = scale_duration(video_media, 90_000, 1000);
    let audio_movie = if movie.audio.is_empty() {
        0
    } else {
        scale_duration(audio_media, movie.audio_rate, 1000)
    };
    let movie_duration = video_movie.max(audio_movie).max(1);
    let tracks = if movie.audio.is_empty() { 2 } else { 3 };
    let mut body = mvhd(movie_duration, tracks);
    body.extend(video_trak(movie, video_movie, video_offset));
    if !movie.audio.is_empty() {
        body.extend(audio_trak(movie, audio_movie, audio_offset));
    }
    box_of(b"moov", &body)
}

fn video_trak(movie: &Movie, movie_duration: u32, offset: u64) -> Vec<u8> {
    let media = sum_duration(&movie.video).max(1);
    let mut body = tkhd(1, movie_duration, movie.width, movie.height, false);
    body.extend(mdia(b"vide", b"VideoHandler", 90_000, media, video_minf(movie, offset)));
    box_of(b"trak", &body)
}

fn audio_trak(movie: &Movie, movie_duration: u32, offset: u64) -> Vec<u8> {
    let media = sum_duration(&movie.audio).max(1);
    let mut body = tkhd(2, movie_duration, 0, 0, true);
    body.extend(mdia(
        b"soun",
        b"SoundHandler",
        movie.audio_rate,
        media,
        audio_minf(movie, offset),
    ));
    box_of(b"trak", &body)
}

fn video_minf(movie: &Movie, offset: u64) -> Vec<u8> {
    let mut vmhd = full_box(0, 1);
    vmhd.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
    let mut body = box_of(b"vmhd", &vmhd);
    body.extend(dinf());
    body.extend(stbl_video(movie, offset));
    box_of(b"minf", &body)
}

fn audio_minf(movie: &Movie, offset: u64) -> Vec<u8> {
    let mut smhd = full_box(0, 0);
    smhd.extend_from_slice(&0u16.to_be_bytes());
    smhd.extend_from_slice(&0u16.to_be_bytes());
    let mut body = box_of(b"smhd", &smhd);
    body.extend(dinf());
    body.extend(stbl_audio(movie, offset));
    box_of(b"minf", &body)
}

fn stbl_video(movie: &Movie, offset: u64) -> Vec<u8> {
    let mut body = stsd_video(movie);
    body.extend(stts(&movie.video));
    body.extend(stss(&movie.video));
    body.extend(stsc(movie.video.len() as u32));
    body.extend(stsz(&movie.video));
    body.extend(stco(offset));
    box_of(b"stbl", &body)
}

fn stbl_audio(movie: &Movie, offset: u64) -> Vec<u8> {
    let mut body = stsd_audio(movie);
    body.extend(stts(&movie.audio));
    body.extend(stsc(movie.audio.len() as u32));
    body.extend(stsz(&movie.audio));
    body.extend(stco(offset));
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
        body.extend_from_slice(&(sample.data.len() as u32).to_be_bytes());
    }
    box_of(b"stsz", &body)
}

fn stco(offset: u64) -> Vec<u8> {
    let mut body = full_box(0, 0);
    body.extend_from_slice(&1u32.to_be_bytes());
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
