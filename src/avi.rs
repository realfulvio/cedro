//! Lettura dei contenitori AVI: flussi, pacchetti e audio.

use std::path::Path;

use crate::nal::avcc_prefix;

pub(crate) struct AviStream {
    pub video: bool,
    pub scale: u32,
    pub rate: u32,
    pub width: u32,
    pub height: u32,
    /// Altezza con segno di `strf`: positiva se le righe vanno dal basso.
    pub signed_height: i32,
    pub bits: u16,
    pub handler: [u8; 4],
    pub compression: [u8; 4],
    pub channels: u16,
    pub sample_rate: u32,
    pub pcm: bool,
    /// Codice del formato audio (WAVEFORMATEX): 1 PCM, 0x55 MP3, 0xFF AAC.
    pub format: u16,
    /// Dati dopo l'intestazione di `strf`: avcC dell'H.264, configurazione dell'AAC.
    pub extra: Vec<u8>,
    /// Le intestazioni così come sono nel file, per riscrivere l'AVI.
    pub strh: Vec<u8>,
    pub strf: Vec<u8>,
}

pub(crate) struct AviChunk {
    pub stream: usize,
    pub video: bool,
    pub data: Vec<u8>,
}

/// Lunghezza dei prefissi e parametri (SPS, PPS) dell'avcC dentro `strf`, se c'è.
pub(crate) fn avc_config(extra: &[u8]) -> (usize, Vec<u8>) {
    if extra.first() == Some(&1) {
        avcc_prefix(Some(extra))
    } else {
        (4, Vec::new())
    }
}

/// L'audio di un AVI come campioni a 16 bit: PCM così com'è, MP3 e AAC passando da un file temporaneo.
pub(crate) fn avi_audio_pcm(stream: &AviStream, chunks: &[AviChunk], index: usize) -> Result<Option<(Vec<i16>, u32, u16)>, String> {
    let data: Vec<&[u8]> = chunks.iter().filter(|c| c.stream == index && !c.video).map(|c| c.data.as_slice()).collect();
    if data.is_empty() {
        return Ok(None);
    }
    match stream.format {
        1 if stream.pcm => {
            let pcm: Vec<i16> = data.iter().flat_map(|d| pcm16(d)).collect();
            Ok(Some((pcm, stream.sample_rate, stream.channels.max(1))))
        }
        0x55 | 0x50 => decode_raw_audio(&data.concat(), "mp3"),
        0xFF | 0x1600 | 0x706D => {
            // AAC grezzo: ogni pacchetto prende un'intestazione ADTS ricavata dalla configurazione.
            let asc = stream.extra.as_slice();
            let (profile, freq, chan) = if asc.len() >= 2 {
                (((asc[0] >> 3) as u32).saturating_sub(1) & 3, (((asc[0] & 7) << 1) | (asc[1] >> 7)) as u32, ((asc[1] >> 3) & 15) as u32)
            } else {
                (1, adts_rate_index(stream.sample_rate), stream.channels as u32)
            };
            let mut adts = Vec::new();
            for packet in &data {
                let len = packet.len() as u32 + 7;
                adts.extend_from_slice(&[
                    0xFF,
                    0xF1,
                    ((profile << 6) | (freq << 2) | (chan >> 2)) as u8,
                    (((chan & 3) << 6) | (len >> 11)) as u8,
                    ((len >> 3) & 0xFF) as u8,
                    (((len & 7) << 5) | 0x1F) as u8,
                    0xFC,
                ]);
                adts.extend_from_slice(packet);
            }
            decode_raw_audio(&adts, "aac")
        }
        _ => Err("l'audio del video ha un formato che Cedro non legge".into()),
    }
}

pub(crate) fn adts_rate_index(rate: u32) -> u32 {
    [96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000]
        .iter()
        .position(|r| *r == rate)
        .unwrap_or(4) as u32
}

pub(crate) fn decode_raw_audio(bytes: &[u8], ext: &str) -> Result<Option<(Vec<i16>, u32, u16)>, String> {
    let dir = crate::engine::private_temp_dir("cedro-avi-audio")?;
    let file = dir.join(format!("audio.{ext}"));
    let result = std::fs::write(&file, bytes)
        .map_err(|err| format!("audio non scritto: {err}"))
        .and_then(|()| crate::audio::decode(&file));
    let _ = std::fs::remove_dir_all(&dir);
    let pcm = result?;
    Ok(Some((pcm.samples, pcm.rate, pcm.channels)))
}

/// Solo l'audio di un AVI (per la conversione in MP3 o WAV).
pub(crate) fn avi_audio(path: &Path) -> Result<crate::audio::Pcm, String> {
    let (streams, chunks) = read(path)?;
    let index = streams.iter().position(|s| !s.video && s.channels > 0).ok_or_else(|| "il video non ha audio".to_string())?;
    let (samples, rate, channels) = avi_audio_pcm(&streams[index], &chunks, index)?.ok_or_else(|| "il video non ha audio".to_string())?;
    Ok(crate::audio::Pcm::new(rate, channels, samples))
}

pub(crate) fn walk_riff(data: &[u8], streams: &mut Vec<AviStream>, chunks: &mut Vec<AviChunk>) -> Result<(), String> {
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

pub(crate) fn stream_of(id: [u8; 4]) -> Option<usize> {
    if id[0].is_ascii_digit() && id[1].is_ascii_digit() {
        Some(((id[0] - b'0') * 10 + (id[1] - b'0')) as usize)
    } else {
        None
    }
}

pub(crate) fn parse_strl(data: &[u8]) -> Result<AviStream, String> {
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
    let handler = [strh[4], strh[5], strh[6], strh[7]];
    let strf = strf.unwrap_or_default();
    let mut stream = AviStream {
        video,
        scale,
        rate,
        width: 0,
        height: 0,
        signed_height: 0,
        bits: 0,
        handler,
        compression: [0; 4],
        channels: 0,
        sample_rate: 0,
        pcm: false,
        format: 0,
        extra: Vec::new(),
        strh,
        strf: Vec::new(),
    };
    if video {
        if strf.len() < 16 {
            return Err("AVI non letto".into());
        }
        stream.width = u32::from_le_bytes([strf[4], strf[5], strf[6], strf[7]]);
        stream.signed_height = i32::from_le_bytes([strf[8], strf[9], strf[10], strf[11]]);
        stream.height = stream.signed_height.unsigned_abs();
        stream.bits = u16::from_le_bytes([strf[14], strf[15]]);
        if strf.len() >= 20 {
            stream.compression = [strf[16], strf[17], strf[18], strf[19]];
        }
        stream.extra = strf.get(40..).map(<[u8]>::to_vec).unwrap_or_default();
    } else {
        stream.format = if strf.len() >= 2 { u16::from_le_bytes([strf[0], strf[1]]) } else { 0 };
        stream.channels = if strf.len() >= 4 { u16::from_le_bytes([strf[2], strf[3]]) } else { 0 };
        stream.sample_rate = if strf.len() >= 8 { u32::from_le_bytes([strf[4], strf[5], strf[6], strf[7]]) } else { 0 };
        stream.bits = if strf.len() >= 16 { u16::from_le_bytes([strf[14], strf[15]]) } else { 0 };
        stream.pcm = stream.format == 1 && stream.bits == 16 && stream.channels > 0;
        if strf.len() >= 18 {
            let cb = u16::from_le_bytes([strf[16], strf[17]]) as usize;
            stream.extra = strf.get(18..18 + cb).map(<[u8]>::to_vec).unwrap_or_default();
        }
    }
    stream.strf = strf;
    Ok(stream)
}

/// Legge il file: i flussi e i pacchetti nell'ordine in cui stanno.
pub(crate) fn read(path: &Path) -> Result<(Vec<AviStream>, Vec<AviChunk>), String> {
    let bytes = std::fs::read(path).map_err(|err| format!("AVI non letto: {err}"))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"AVI " {
        return Err("AVI non letto".into());
    }
    let mut streams = Vec::new();
    let mut chunks = Vec::new();
    walk_riff(&bytes[12..], &mut streams, &mut chunks)?;
    Ok((streams, chunks))
}

pub(crate) fn is_h264(stream: &AviStream) -> bool {
    stream.video && [b"H264", b"X264", b"AVC1"].iter().any(|n| same_cc(stream.compression, n) || same_cc(stream.handler, n))
}

pub(crate) fn same_cc(value: [u8; 4], name: &[u8; 4]) -> bool {
    value.iter().zip(name.iter()).all(|(a, b)| a.to_ascii_uppercase() == b.to_ascii_uppercase())
}

/// Per Media Foundation (Windows): un AVI con H.264 a lunghezze (avcC, «avc1») si riscrive con i codici di inizio
/// e i parametri davanti a ogni punto di ripartenza, e con la sigla `H264`. Restituisce il file nuovo, se serviva.
pub(crate) fn annex_b_copy(path: &Path, dir: &Path) -> Result<Option<std::path::PathBuf>, String> {
    let (streams, chunks) = read(path)?;
    let Some(vi) = streams.iter().position(is_h264) else {
        return Ok(None);
    };
    let (nal_len, header) = avc_config(&streams[vi].extra);
    let needs = header.len() > 0
        || chunks.iter().find(|c| c.stream == vi && c.video).is_some_and(|c| !crate::nal::is_annex(&c.data));
    if !needs {
        return Ok(None);
    }
    let le32 = |n: u32| n.to_le_bytes().to_vec();
    let chunk = |id: &[u8; 4], data: &[u8]| {
        let mut out = id.to_vec();
        out.extend(le32(data.len() as u32));
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    };
    let list = |kind: &[u8; 4], body: Vec<u8>| {
        let mut out = b"LIST".to_vec();
        out.extend(le32(body.len() as u32 + 4));
        out.extend_from_slice(kind);
        out.extend(body);
        out
    };
    let mut hdrl = Vec::new();
    let mut avih = vec![0u8; 56];
    avih[0..4].copy_from_slice(&le32(avi_frame_us(&streams[vi])));
    avih[16..20].copy_from_slice(&le32(chunks.iter().filter(|c| c.stream == vi && c.video).count() as u32));
    avih[24..28].copy_from_slice(&le32(streams.len() as u32));
    avih[32..36].copy_from_slice(&le32(streams[vi].width));
    avih[36..40].copy_from_slice(&le32(streams[vi].height));
    hdrl.extend(chunk(b"avih", &avih));
    for (index, stream) in streams.iter().enumerate() {
        let (mut strh, mut strf) = (stream.strh.clone(), stream.strf.clone());
        if index == vi {
            strh[4..8].copy_from_slice(b"H264");
            strf.truncate(40);
            if strf.len() >= 20 {
                strf[16..20].copy_from_slice(b"H264");
            }
            strf[0..4].copy_from_slice(&le32(40));
        }
        let mut body = chunk(b"strh", &strh);
        body.extend(chunk(b"strf", &strf));
        hdrl.extend(list(b"strl", body));
    }
    let mut movi = Vec::new();
    let mut index = Vec::new();
    let mut keys = 0u32;
    for c in &chunks {
        let tag = if c.video { b"dc" } else { b"wb" };
        let id = [b'0' + (c.stream / 10) as u8, b'0' + (c.stream % 10) as u8, tag[0], tag[1]];
        let mut data = c.data.clone();
        let mut key = true;
        if c.stream == vi && c.video {
            let annex = if crate::nal::is_annex(&data) { data } else { crate::nal::length_to_annex(&data, nal_len) };
            key = crate::nal::annex_nals(&annex).iter().any(|n| n.first().is_some_and(|b| b & 0x1f == 5));
            data = if key && !header.is_empty() { [header.as_slice(), annex.as_slice()].concat() } else { annex };
            keys += key as u32;
        }
        index.extend_from_slice(&id);
        index.extend(le32(if key { 0x10 } else { 0 }));
        index.extend(le32(movi.len() as u32 + 4));
        index.extend(le32(data.len() as u32));
        movi.extend(chunk(&id, &data));
    }
    let _ = keys;
    let mut body = b"AVI ".to_vec();
    body.extend(list(b"hdrl", hdrl));
    body.extend(list(b"movi", movi));
    body.extend(chunk(b"idx1", &index));
    let mut file = b"RIFF".to_vec();
    file.extend(le32(body.len() as u32));
    file.extend(body);
    let out = dir.join("video.avi");
    std::fs::write(&out, file).map_err(|err| format!("AVI non scritto: {err}"))?;
    Ok(Some(out))
}

fn avi_frame_us(stream: &AviStream) -> u32 {
    ((stream.scale.max(1) as u64) * 1_000_000 / stream.rate.max(1) as u64) as u32
}

pub(crate) fn pcm16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aac_config_becomes_adts_header_fields() {
        assert_eq!(adts_rate_index(44_100), 4);
        assert_eq!(adts_rate_index(48_000), 3);
    }
}
