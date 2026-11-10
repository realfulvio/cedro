//! Lettore Matroska: tiene solo il video H.264. L'audio lo legge symphonia.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const EBML: u64 = 0x1A45DFA3;
const SEGMENT: u64 = 0x18538067;
const INFO: u64 = 0x1549A966;
const TIMESTAMP_SCALE: u64 = 0x2AD7B1;
const TRACKS: u64 = 0x1654AE6B;
const TRACK_ENTRY: u64 = 0xAE;
const TRACK_NUMBER: u64 = 0xD7;
const TRACK_TYPE: u64 = 0x83;
const CODEC_ID: u64 = 0x86;
const CODEC_PRIVATE: u64 = 0x63A2;
const DEFAULT_DURATION: u64 = 0x23E383;
const CLUSTER: u64 = 0x1F43B675;
const CLUSTER_TIME: u64 = 0xE7;
const SIMPLE_BLOCK: u64 = 0xA3;
const BLOCK_GROUP: u64 = 0xA0;
const BLOCK: u64 = 0xA1;
const BLOCK_DURATION: u64 = 0x9B;

const H264: &str = "V_MPEG4/ISO/AVC";

pub(crate) struct H264Mkv {
    reader: BufReader<File>,
    clusters: Vec<(u64, u64)>,
    pub private_data: Vec<u8>,
    pub wants_audio: bool,
    pub default_100ns: i64,
    scale: u64,
    video_track: u64,
    /// L'ultimo cluster arriva fino in fondo al file: registrazione interrotta (tagliato, o con la misura
    /// «sconosciuta» di chi scrive a flusso). Lì un blocco a metà chiude la lettura invece di essere un errore.
    open_tail: bool,
}

struct Piece {
    track: u64,
    rel: i16,
    data: Vec<u8>,
}

impl H264Mkv {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|err| format!("il video non è stato letto: {err}"))?;
        let len = file.metadata().map_err(|err| format!("il video non è stato letto: {err}"))?.len();
        let mut reader = BufReader::new(file);
        if read_id(&mut reader)? != EBML {
            return Err("il video non è stato letto".into());
        }
        skip_body(&mut reader)?;
        if read_id(&mut reader)? != SEGMENT {
            return Err("il video non è stato letto".into());
        }
        let (seg_size, unknown) = read_len(&mut reader)?;
        let here = position(&mut reader)?;
        let seg_end = if unknown { len } else { here.saturating_add(seg_size).min(len) };
        let mut mkv = Self {
            reader,
            clusters: Vec::new(),
            private_data: Vec::new(),
            wants_audio: false,
            default_100ns: 0,
            scale: 1_000_000,
            video_track: 0,
            open_tail: false,
        };
        while position(&mut mkv.reader)? + 2 <= seg_end {
            let id = read_id(&mut mkv.reader)?;
            let (size, unknown) = read_len(&mut mkv.reader)?;
            let start = position(&mut mkv.reader)?;
            let end = start.saturating_add(size);
            // Registrazione interrotta (OBS, telefoni) o scritta a flusso: dal cluster tagliato o senza misura
            // si legge fino in fondo quello che c'è, come per gli AVI.
            if id == CLUSTER && (unknown || end > len) {
                mkv.clusters.push((start, seg_end.saturating_sub(start)));
                mkv.open_tail = true;
                break;
            }
            if unknown {
                return Err("il video non è stato letto".into());
            }
            if end > len {
                // Dopo i cluster interi (gli indici in fondo tagliati) i fotogrammi ci sono già tutti.
                if !mkv.clusters.is_empty() {
                    break;
                }
                return Err("il video non è stato letto".into());
            }
            match id {
                INFO => mkv.parse_info(end)?,
                TRACKS => mkv.parse_tracks(end)?,
                CLUSTER => {
                    mkv.clusters.push((start, size));
                    mkv.reader.seek(SeekFrom::Start(end)).map_err(|_| "il video non è stato letto".to_string())?;
                }
                _ => {
                    mkv.reader.seek(SeekFrom::Start(end)).map_err(|_| "il video non è stato letto".to_string())?;
                }
            }
        }
        if mkv.video_track == 0 {
            return Err("il video non è stato letto".into());
        }
        Ok(mkv)
    }

    pub(crate) fn each(&mut self, f: &mut dyn FnMut(&[u8], i64, i64) -> Result<(), String>) -> Result<(), String> {
        let clusters = self.clusters.clone();
        let scale = self.scale.max(1);
        let video = self.video_track;
        let count = clusters.len();
        for (index, (at, size)) in clusters.into_iter().enumerate() {
            let tail = self.open_tail && index + 1 == count;
            let end = at.saturating_add(size);
            self.reader.seek(SeekFrom::Start(at)).map_err(|_| "il video non è stato letto".to_string())?;
            let mut cluster_ts = 0u64;
            while position(&mut self.reader)? + 2 <= end {
                let header = read_id(&mut self.reader).and_then(|id| Ok((id, read_len(&mut self.reader)?)));
                let (id, (child, unknown)) = match header {
                    Ok(header) => header,
                    // Il file finisce in mezzo a un'intestazione.
                    Err(_) if tail => break,
                    Err(err) => return Err(err),
                };
                if tail && id == CLUSTER {
                    // Il cluster dopo, dentro quello senza misura: i suoi elementi seguono qui.
                    cluster_ts = 0;
                    continue;
                }
                if unknown {
                    return Err("il video non è stato letto".into());
                }
                let child_end = position(&mut self.reader)?.saturating_add(child);
                if tail && child_end > end {
                    // L'ultimo blocco è tagliato: si tiene quello che c'era prima.
                    break;
                }
                match id {
                    CLUSTER_TIME => cluster_ts = read_uint(&mut self.reader, child)?,
                    SIMPLE_BLOCK => {
                        // Prima il numero della traccia: i blocchi dell'audio non si leggono né si copiano.
                        let (track, width) = if child > 0 { read_marked(&mut self.reader, true)? } else { (video, 0) };
                        if track == video {
                            self.reader.seek_relative(-(width as i64)).map_err(|_| "il video non è stato letto".to_string())?;
                            let data = read_vec(&mut self.reader, child)?;
                            emit_block(&data, cluster_ts, scale, video, None, f)?;
                        }
                    }
                    BLOCK_GROUP => self.block_group(child_end, cluster_ts, scale, video, f)?,
                    _ => {}
                }
                skip_to(&mut self.reader, child_end)?;
            }
        }
        Ok(())
    }

    fn parse_info(&mut self, end: u64) -> Result<(), String> {
        while position(&mut self.reader)? + 2 <= end {
            let id = read_id(&mut self.reader)?;
            let (size, unknown) = read_len(&mut self.reader)?;
            if unknown {
                return Err("il video non è stato letto".into());
            }
            let child_end = position(&mut self.reader)?.saturating_add(size);
            if id == TIMESTAMP_SCALE {
                self.scale = read_uint(&mut self.reader, size)?.max(1);
            }
            self.reader.seek(SeekFrom::Start(child_end)).map_err(|_| "il video non è stato letto".to_string())?;
        }
        self.reader.seek(SeekFrom::Start(end)).map_err(|_| "il video non è stato letto".to_string())?;
        Ok(())
    }

    fn parse_tracks(&mut self, end: u64) -> Result<(), String> {
        while position(&mut self.reader)? + 2 <= end {
            let id = read_id(&mut self.reader)?;
            let (size, unknown) = read_len(&mut self.reader)?;
            if unknown {
                return Err("il video non è stato letto".into());
            }
            let child_end = position(&mut self.reader)?.saturating_add(size);
            if id == TRACK_ENTRY {
                self.parse_entry(child_end)?;
            }
            self.reader.seek(SeekFrom::Start(child_end)).map_err(|_| "il video non è stato letto".to_string())?;
        }
        self.reader.seek(SeekFrom::Start(end)).map_err(|_| "il video non è stato letto".to_string())?;
        Ok(())
    }

    fn parse_entry(&mut self, end: u64) -> Result<(), String> {
        let mut number = 0u64;
        let mut kind = 0u64;
        let mut codec = String::new();
        let mut private = Vec::new();
        let mut default_ns = 0u64;
        while position(&mut self.reader)? + 2 <= end {
            let id = read_id(&mut self.reader)?;
            let (size, unknown) = read_len(&mut self.reader)?;
            if unknown {
                return Err("il video non è stato letto".into());
            }
            let child_end = position(&mut self.reader)?.saturating_add(size);
            match id {
                TRACK_NUMBER => number = read_uint(&mut self.reader, size)?,
                TRACK_TYPE => kind = read_uint(&mut self.reader, size)?,
                CODEC_ID => codec = read_text(&mut self.reader, size)?,
                CODEC_PRIVATE => private = read_vec(&mut self.reader, size)?,
                DEFAULT_DURATION => default_ns = read_uint(&mut self.reader, size)?,
                _ => {}
            }
            self.reader.seek(SeekFrom::Start(child_end)).map_err(|_| "il video non è stato letto".to_string())?;
        }
        if kind == 1 && codec == H264 && self.video_track == 0 {
            self.video_track = number;
            self.private_data = private;
            self.default_100ns = i64::try_from(default_ns / 100).unwrap_or(i64::MAX);
        }
        if kind == 2 && symphonia_audio(&codec) {
            self.wants_audio = true;
        }
        Ok(())
    }

    fn block_group(
        &mut self,
        end: u64,
        cluster_ts: u64,
        scale: u64,
        video: u64,
        f: &mut dyn FnMut(&[u8], i64, i64) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut block = None;
        let mut duration = None;
        while position(&mut self.reader)? + 2 <= end {
            let id = read_id(&mut self.reader)?;
            let (size, unknown) = read_len(&mut self.reader)?;
            if unknown {
                return Err("il video non è stato letto".into());
            }
            let child_end = position(&mut self.reader)?.saturating_add(size);
            match id {
                BLOCK => block = Some(read_vec(&mut self.reader, size)?),
                BLOCK_DURATION => duration = Some(read_uint(&mut self.reader, size)?),
                _ => {}
            }
            skip_to(&mut self.reader, child_end)?;
        }
        if let Some(data) = block {
            emit_block(&data, cluster_ts, scale, video, duration, f)?;
        }
        Ok(())
    }
}

fn emit_block(
    data: &[u8],
    cluster_ts: u64,
    scale: u64,
    video: u64,
    duration_ticks: Option<u64>,
    f: &mut dyn FnMut(&[u8], i64, i64) -> Result<(), String>,
) -> Result<(), String> {
    let pieces = split_block(data)?;
    if pieces.is_empty() {
        return Ok(());
    }
    let each = duration_ticks
        .map(|ticks| scale_100ns(ticks / pieces.len() as u64, scale))
        .unwrap_or(0);
    for piece in &pieces {
        if piece.track != video {
            continue;
        }
        let abs = if piece.rel < 0 {
            cluster_ts.saturating_sub(piece.rel.unsigned_abs() as u64)
        } else {
            cluster_ts.saturating_add(piece.rel as u64)
        };
        f(&piece.data, scale_100ns(abs, scale), each)?;
    }
    Ok(())
}

fn symphonia_audio(codec: &str) -> bool {
    matches!(
        codec,
        "A_AAC"
            | "A_AAC/MPEG2/MAIN"
            | "A_AAC/MPEG2/LC"
            | "A_AAC/MPEG2/LC/SBR"
            | "A_AAC/MPEG2/SSR"
            | "A_AAC/MPEG4/MAIN"
            | "A_AAC/MPEG4/LC"
            | "A_AAC/MPEG4/LC/SBR"
            | "A_AAC/MPEG4/SSR"
            | "A_AAC/MPEG4/LTP"
            | "A_VORBIS"
            | "A_MPEG/L1"
            | "A_MPEG/L2"
            | "A_MPEG/L3"
            | "A_FLAC"
            | "A_PCM/INT/LIT"
            | "A_PCM/INT/BIG"
            | "A_PCM/FLOAT/IEEE"
    )
}

fn scale_100ns(ticks: u64, scale: u64) -> i64 {
    let value = (ticks as u128).saturating_mul(scale as u128) / 100;
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn position(reader: &mut BufReader<File>) -> Result<u64, String> {
    reader.stream_position().map_err(|_| "il video non è stato letto".to_string())
}

/// Va a `end` restando nel buffer quando si può (un `seek` assoluto lo butterebbe ogni volta).
fn skip_to(reader: &mut BufReader<File>, end: u64) -> Result<(), String> {
    let here = position(reader)?;
    if here != end {
        let delta = i64::try_from(end as i128 - here as i128).map_err(|_| "il video non è stato letto".to_string())?;
        reader.seek_relative(delta).map_err(|_| "il video non è stato letto".to_string())?;
    }
    Ok(())
}

fn skip_body(reader: &mut BufReader<File>) -> Result<(), String> {
    let (size, unknown) = read_len(reader)?;
    if unknown {
        return Err("il video non è stato letto".into());
    }
    let end = position(reader)?.saturating_add(size);
    reader.seek(SeekFrom::Start(end)).map_err(|_| "il video non è stato letto".to_string())?;
    Ok(())
}

fn read_id(reader: &mut impl Read) -> Result<u64, String> {
    let (value, _) = read_marked(reader, false)?;
    Ok(value)
}

fn read_len(reader: &mut impl Read) -> Result<(u64, bool), String> {
    let (value, width) = read_marked(reader, true)?;
    let bits = 7 * width;
    let unknown = bits < 64 && value == (1u64 << bits) - 1;
    Ok((value, unknown))
}

fn read_marked(reader: &mut impl Read, strip: bool) -> Result<(u64, usize), String> {
    let first = read_byte(reader)?;
    let width = vint_width(first)?;
    let mask = if !strip || width >= 8 { 0xFF } else { (1u8 << (8 - width)) - 1 };
    let mut value = (first & mask) as u64;
    if strip && width >= 8 {
        value = 0;
    }
    for _ in 1..width {
        value = (value << 8) | u64::from(read_byte(reader)?);
    }
    Ok((value, width))
}

fn vint_width(first: u8) -> Result<usize, String> {
    let zeros = first.leading_zeros() as usize;
    if zeros > 7 {
        return Err("il video non è stato letto".into());
    }
    Ok(zeros + 1)
}

fn read_byte(reader: &mut impl Read) -> Result<u8, String> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf).map_err(|_| "il video non è stato letto".to_string())?;
    Ok(buf[0])
}

fn read_uint(reader: &mut impl Read, size: u64) -> Result<u64, String> {
    if size > 8 {
        return Err("il video non è stato letto".into());
    }
    let mut value = 0u64;
    for _ in 0..size {
        value = (value << 8) | u64::from(read_byte(reader)?);
    }
    Ok(value)
}

fn read_vec(reader: &mut impl Read, size: u64) -> Result<Vec<u8>, String> {
    if size > 64 * 1024 * 1024 {
        return Err("il video non è stato letto".into());
    }
    let mut buf = vec![0u8; size as usize];
    if size > 0 {
        reader.read_exact(&mut buf).map_err(|_| "il video non è stato letto".to_string())?;
    }
    Ok(buf)
}

fn read_text(reader: &mut impl Read, size: u64) -> Result<String, String> {
    let bytes = read_vec(reader, size)?;
    Ok(String::from_utf8_lossy(&bytes).trim_matches('\0').to_string())
}

fn split_block(data: &[u8]) -> Result<Vec<Piece>, String> {
    let (track, mut at, _) = vint_at(data, 0)?;
    if at + 3 > data.len() {
        return Err("il video non è stato letto".into());
    }
    let rel = i16::from_be_bytes([data[at], data[at + 1]]);
    at += 2;
    let lacing = (data[at] >> 1) & 0b11;
    at += 1;
    let payloads = match lacing {
        0 => vec![data[at..].to_vec()],
        1 => lace_xiph(data, at)?,
        2 => lace_fixed(data, at)?,
        _ => lace_ebml(data, at)?,
    };
    Ok(payloads.into_iter().map(|buf| Piece { track, rel, data: buf }).collect())
}

fn lace_xiph(data: &[u8], mut at: usize) -> Result<Vec<Vec<u8>>, String> {
    if at >= data.len() {
        return Err("il video non è stato letto".into());
    }
    let count = data[at] as usize;
    at += 1;
    let mut sizes = Vec::with_capacity(count);
    for _ in 0..count {
        let mut size = 0usize;
        loop {
            if at >= data.len() {
                return Err("il video non è stato letto".into());
            }
            let byte = data[at];
            at += 1;
            size += byte as usize;
            if byte != 255 {
                break;
            }
        }
        sizes.push(size);
    }
    take_sizes(data, at, &sizes)
}

fn lace_fixed(data: &[u8], mut at: usize) -> Result<Vec<Vec<u8>>, String> {
    if at >= data.len() {
        return Err("il video non è stato letto".into());
    }
    let frames = data[at] as usize + 1;
    at += 1;
    if frames == 0 || !(data.len() - at).is_multiple_of(frames) {
        return Err("il video non è stato letto".into());
    }
    let size = (data.len() - at) / frames;
    let mut out = Vec::with_capacity(frames);
    for _ in 0..frames {
        out.push(data[at..at + size].to_vec());
        at += size;
    }
    Ok(out)
}

fn lace_ebml(data: &[u8], mut at: usize) -> Result<Vec<Vec<u8>>, String> {
    if at >= data.len() {
        return Err("il video non è stato letto".into());
    }
    let count = data[at] as usize;
    at += 1;
    let mut sizes = Vec::with_capacity(count);
    for index in 0..count {
        let (raw, next, width) = vint_at(data, at)?;
        at = next;
        if index == 0 {
            sizes.push(raw as usize);
            continue;
        }
        let delta = raw as i64 - ((1i64 << (7 * width - 1)) - 1);
        let next_size = sizes[index - 1] as i64 + delta;
        if next_size < 0 {
            return Err("il video non è stato letto".into());
        }
        sizes.push(next_size as usize);
    }
    take_sizes(data, at, &sizes)
}

fn take_sizes(data: &[u8], mut at: usize, sizes: &[usize]) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::with_capacity(sizes.len() + 1);
    for size in sizes {
        if at + size > data.len() {
            return Err("il video non è stato letto".into());
        }
        out.push(data[at..at + size].to_vec());
        at += size;
    }
    out.push(data[at..].to_vec());
    Ok(out)
}

fn vint_at(data: &[u8], at: usize) -> Result<(u64, usize, usize), String> {
    if at >= data.len() {
        return Err("il video non è stato letto".into());
    }
    let width = vint_width(data[at])?;
    if at + width > data.len() {
        return Err("il video non è stato letto".into());
    }
    let mask = if width >= 8 { 0 } else { (1u8 << (8 - width)) - 1 };
    let mut value = u64::from(data[at] & mask);
    for byte in &data[at + 1..at + width] {
        value = (value << 8) | u64::from(*byte);
    }
    Ok((value, at + width, width))
}

#[cfg(test)]
mod tests {
    use super::{split_block, H264Mkv};

    fn el(id: &[u8], body: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.push(0x01);
        out.extend_from_slice(&(body.len() as u64).to_be_bytes()[1..]);
        out.extend_from_slice(body);
        out
    }

    /// Una registrazione interrotta: segmento e cluster senza misura, l'ultimo blocco tagliato. I fotogrammi
    /// interi si leggono, quello tagliato no.
    #[test]
    fn interrupted_recording_keeps_the_whole_blocks() {
        let unknown = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let entry = [el(&[0xD7], &[1]), el(&[0x83], &[1]), el(&[0x86], b"V_MPEG4/ISO/AVC")].concat();
        let mut file = el(&[0x1A, 0x45, 0xDF, 0xA3], &el(&[0x42, 0x82], b"matroska"));
        file.extend_from_slice(&[0x18, 0x53, 0x80, 0x67]);
        file.extend_from_slice(&unknown);
        file.extend(el(&[0x16, 0x54, 0xAE, 0x6B], &el(&[0xAE], &entry)));
        for (ts, frames) in [(0u8, [b"uno", b"due"]), (40, [b"tre", b"qua"])] {
            file.extend_from_slice(&[0x1F, 0x43, 0xB6, 0x75]);
            file.extend_from_slice(&unknown);
            file.extend(el(&[0xE7], &[ts]));
            for (n, frame) in frames.iter().enumerate() {
                file.extend(el(&[0xA3], &[&[0x81, 0x00, n as u8 * 20, 0x80][..], &frame[..]].concat()));
            }
        }
        // L'ultimo blocco: l'intestazione dice 100 byte, il file finisce prima.
        file.extend_from_slice(&[0xA3, 0x01, 0, 0, 0, 0, 0, 0, 100, 0x81, 0x00]);
        let dir = std::env::temp_dir().join(format!("cedro-mkv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tagliato.mkv");
        std::fs::write(&path, &file).unwrap();
        let mut mkv = H264Mkv::open(&path).unwrap();
        let mut got = Vec::new();
        mkv.each(&mut |data, time, _| {
            got.push((data.to_vec(), time));
            Ok(())
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let frames: Vec<&[u8]> = got.iter().map(|(d, _)| d.as_slice()).collect();
        assert_eq!(frames, vec![&b"uno"[..], b"due", b"tre", b"qua"]);
        assert_eq!(got[2].1, 40 * 10_000);
    }

    #[test]
    fn block_without_lacing() {
        let frames = split_block(&[0x81, 0x00, 0x00, 0x80, 0x01, 0x02, 0x03]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].track, 1);
        assert_eq!(frames[0].data, vec![1, 2, 3]);
    }

    #[test]
    fn xiph_and_ebml_lacing() {
        let xiph = split_block(&[0x81, 0x00, 0x00, 0x82, 0x01, 0x01, 0xAA, 0xBB, 0xCC]).unwrap();
        assert_eq!(xiph[0].data, vec![0xAA]);
        assert_eq!(xiph[1].data, vec![0xBB, 0xCC]);
        let ebml = split_block(&[0x81, 0x00, 0x00, 0x86, 0x01, 0x81, 0xAA, 0xBB, 0xCC]).unwrap();
        assert_eq!(ebml[0].data, vec![0xAA]);
        assert_eq!(ebml[1].data, vec![0xBB, 0xCC]);
        let fixed = split_block(&[0x81, 0x00, 0x00, 0x84, 0x01, 0xAA, 0xBB]).unwrap();
        assert_eq!(fixed[0].data, vec![0xAA]);
        assert_eq!(fixed[1].data, vec![0xBB]);
    }

    #[test]
    fn signed_ebml_lace_delta() {
        let mut block = vec![0x81, 0x00, 0x00, 0x86, 0x02, 0xC6, 0x80];
        block.extend(std::iter::repeat_n(0x11, 70));
        block.extend(std::iter::repeat_n(0x22, 7));
        block.push(0x33);
        let frames = split_block(&block).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].data.len(), 70);
        assert_eq!(frames[1].data, vec![0x22; 7]);
        assert_eq!(frames[2].data, vec![0x33]);
    }
}
