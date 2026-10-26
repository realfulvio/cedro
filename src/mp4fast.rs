//! "Ottimizza per il web": sposta il blocco `moov` di un MP4 davanti ai dati (`mdat`),
//! così il video parte prima che sia scaricato tutto. Gli offset dei dati (`stco`, `co64`) si aggiornano.

use std::fs;
use std::path::Path;

struct TopBox {
    start: usize,
    end: usize,
    kind: [u8; 4],
}

fn top_boxes(data: &[u8]) -> Result<Vec<TopBox>, String> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= data.len() {
        let size32 = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as u64;
        let kind = [data[at + 4], data[at + 5], data[at + 6], data[at + 7]];
        let size = match size32 {
            0 => (data.len() - at) as u64,
            1 => {
                if at + 16 > data.len() {
                    return Err("MP4 troncato".into());
                }
                u64::from_be_bytes(data[at + 8..at + 16].try_into().unwrap())
            }
            n => n,
        };
        if size < 8 || at as u64 + size > data.len() as u64 {
            return Err("MP4 con un blocco fuori misura".into());
        }
        out.push(TopBox { start: at, end: at + size as usize, kind });
        at += size as usize;
    }
    Ok(out)
}

/// Somma `shift` a tutti gli offset dei blocchi `stco` e `co64` dentro `moov`.
fn shift_offsets(moov: &mut [u8], shift: u64) -> Result<(), String> {
    let mut at = 8usize;
    while at + 8 <= moov.len() {
        let size = u32::from_be_bytes(moov[at..at + 4].try_into().unwrap()) as usize;
        if size < 8 || at + size > moov.len() {
            return Err("MP4 con un blocco fuori misura".into());
        }
        shift_in(moov, at, size, shift)?;
        at += size;
    }
    Ok(())
}

fn shift_in(moov: &mut [u8], at: usize, size: usize, shift: u64) -> Result<(), String> {
    let kind: [u8; 4] = moov[at + 4..at + 8].try_into().unwrap();
    match &kind {
        b"stco" => {
            let count = table_len(moov, at, size, 4)?;
            for i in 0..count {
                let p = at + 16 + i * 4;
                let value = u32::from_be_bytes(moov[p..p + 4].try_into().unwrap()) as u64 + shift;
                let value = u32::try_from(value).map_err(|_| "il video è troppo grande per spostare il moov".to_string())?;
                moov[p..p + 4].copy_from_slice(&value.to_be_bytes());
            }
        }
        b"co64" => {
            let count = table_len(moov, at, size, 8)?;
            for i in 0..count {
                let p = at + 16 + i * 8;
                let value = u64::from_be_bytes(moov[p..p + 8].try_into().unwrap()).checked_add(shift).ok_or("MP4 con un offset fuori misura")?;
                moov[p..p + 8].copy_from_slice(&value.to_be_bytes());
            }
        }
        b"trak" | b"mdia" | b"minf" | b"stbl" => {
            let end = at + size;
            let mut inner = at + 8;
            while inner + 8 <= end {
                let s = u32::from_be_bytes(moov[inner..inner + 4].try_into().unwrap()) as usize;
                if s < 8 || inner + s > end {
                    return Err("MP4 con un blocco fuori misura".into());
                }
                shift_in(moov, inner, s, shift)?;
                inner += s;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Quante voci ha la tabella `stco`/`co64` che parte in `at`, solo se stanno tutte dentro il suo blocco:
/// un file rovinato non deve far leggere (o riscrivere) i blocchi accanto.
fn table_len(moov: &[u8], at: usize, size: usize, entry: usize) -> Result<usize, String> {
    if size < 16 {
        return Err("MP4 con una tabella troppo corta".into());
    }
    let count = u32::from_be_bytes(moov[at + 12..at + 16].try_into().unwrap()) as usize;
    if count.checked_mul(entry).is_none_or(|bytes| bytes > size - 16) {
        return Err("MP4 con una tabella fuori misura".into());
    }
    Ok(count)
}

/// Il file MP4 con il `moov` davanti ai dati. Se c'è già, non lo tocca.
pub fn make_faststart(path: &Path) -> Result<(), String> {
    let data = fs::read(path).map_err(|e| format!("MP4 non letto: {e}"))?;
    let boxes = top_boxes(&data)?;
    let moov_i = boxes.iter().position(|b| &b.kind == b"moov").ok_or("MP4 senza moov")?;
    let mdat_i = boxes.iter().position(|b| &b.kind == b"mdat").ok_or("MP4 senza dati")?;
    if moov_i < mdat_i {
        return Ok(());
    }
    let moov = &boxes[moov_i];
    let mut new_moov = data[moov.start..moov.end].to_vec();
    let shift = new_moov.len() as u64;
    shift_offsets(&mut new_moov, shift)?;
    let mut out = Vec::with_capacity(data.len());
    for b in boxes.iter().take(mdat_i) {
        out.extend_from_slice(&data[b.start..b.end]);
    }
    out.extend_from_slice(&new_moov);
    for (i, b) in boxes.iter().enumerate().skip(mdat_i) {
        if i != moov_i {
            out.extend_from_slice(&data[b.start..b.end]);
        }
    }
    let tmp = path.with_extension("mp4.tmp");
    fs::write(&tmp, &out).map_err(|e| format!("MP4 non scritto: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("MP4 non sostituito: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn moov_moves_in_front_and_offsets_follow() {
        let payload = b"FOTOGRAMMI-DEL-VIDEO";
        let ftyp = make(b"ftyp", b"isom0000isom");
        let mdat = make(b"mdat", payload);
        let chunk_at = (ftyp.len() + 8) as u32;
        let mut stco_body = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stco_body.extend_from_slice(&chunk_at.to_be_bytes());
        let stbl = make(b"stbl", &make(b"stco", &stco_body));
        let moov = make(b"moov", &make(b"trak", &make(b"mdia", &make(b"minf", &stbl))));
        let mut file = ftyp.clone();
        file.extend_from_slice(&mdat);
        file.extend_from_slice(&moov);
        let dir = std::env::temp_dir().join(format!("cedro-fast-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.mp4");
        fs::write(&path, &file).unwrap();

        make_faststart(&path).unwrap();
        let out = fs::read(&path).unwrap();
        assert_eq!(out.len(), file.len());
        let kinds: Vec<[u8; 4]> = top_boxes(&out).unwrap().into_iter().map(|b| b.kind).collect();
        assert_eq!(kinds, vec![*b"ftyp", *b"moov", *b"mdat"]);
        let at = out.windows(4).position(|w| w == b"stco").unwrap();
        let moved = u32::from_be_bytes(out[at + 12..at + 16].try_into().unwrap()) as usize;
        assert_eq!(&out[moved..moved + payload.len()], payload);

        // Già in ordine: il file resta com'è.
        make_faststart(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), out);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Una tabella che dice di avere più voci di quante ne stanno nel suo blocco: errore, non un crash
    /// e non una scrittura sui blocchi accanto.
    #[test]
    fn a_damaged_offset_table_is_refused() {
        for (kind, count) in [(b"stco", 1000u32), (b"co64", 1000), (b"stco", u32::MAX), (b"co64", u32::MAX)] {
            let mut body = vec![0, 0, 0, 0];
            body.extend_from_slice(&count.to_be_bytes());
            body.extend_from_slice(&[0; 8]);
            let table = make(kind, &body);
            let mut copy = make(b"moov", &make(b"trak", &make(b"mdia", &make(b"minf", &make(b"stbl", &table)))));
            assert!(shift_offsets(&mut copy, 100).is_err(), "{}", String::from_utf8_lossy(kind));
        }
        // Una tabella più corta della sua intestazione.
        let mut copy = make(b"moov", &make(b"stco", &[0, 0, 0, 0]));
        assert!(shift_offsets(&mut copy, 100).is_err());
    }
}
