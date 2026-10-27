//! Utilità per i flussi H.264: codici di inizio (Annex B) e prefissi di lunghezza.

/// True se `data` è davvero Annex B. Un campione con prefisso di lunghezza (MP4/MKV) può iniziare con `00 00 01 xx`
/// (NAL da 256 a 511 byte) e sembrare un codice di inizio: se i prefissi coprono il campione esatto, è a lunghezze.
pub(crate) fn is_annex(data: &[u8]) -> bool {
    if annex_nals(data).is_empty() {
        return false;
    }
    !(1..=4usize).any(|nal_len| length_tiles(data, nal_len))
}

pub(crate) fn length_tiles(sample: &[u8], nal_len: usize) -> bool {
    let mut i = 0;
    let mut count = 0;
    while i + nal_len <= sample.len() {
        let mut len = 0usize;
        for byte in &sample[i..i + nal_len] {
            len = (len << 8) | *byte as usize;
        }
        i += nal_len;
        if len == 0 || i + len > sample.len() {
            return false;
        }
        i += len;
        count += 1;
    }
    i == sample.len() && count > 0
}

pub(crate) fn annex_nals(data: &[u8]) -> Vec<&[u8]> {
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

pub(crate) fn avcc_prefix(extra: Option<&[u8]>) -> (usize, Vec<u8>) {
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

pub(crate) fn take_nal(extra: &[u8], i: &mut usize, annex: &mut Vec<u8>) -> bool {
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

pub(crate) fn length_to_annex(sample: &[u8], nal_len: usize) -> Vec<u8> {
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
