use std::io::Write;

pub(crate) fn win_ansi(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let c = ch as u32;
        let b = if (32..=126).contains(&c) || (160..=255).contains(&c) {
            c as u8
        } else {
            match c {
                0x20ac => 0x80,
                0x2026 => 0x85,
                0x2018 => 0x91,
                0x2019 => 0x92,
                0x201c => 0x93,
                0x201d => 0x94,
                0x2013 => 0x96,
                0x2014 => 0x97,
                _ => b'?',
            }
        };
        out.push(b);
    }
    out
}

fn literal(bytes: &[u8]) -> String {
    let mut s = String::from("(");
    for &b in bytes {
        match b {
            b'\\' | b'(' | b')' => {
                s.push('\\');
                s.push(b as char);
            }
            32..=126 => s.push(b as char),
            _ => s.push_str(&format!("\\{b:03o}")),
        }
    }
    s.push(')');
    s
}

#[cfg(test)]
fn wrap_lines(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.replace("\r\n", "\n").split('\n') {
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut rest = raw;
        while rest.len() > width {
            let end = rest.floor_char_boundary(width);
            let cut = rest[..end].rfind(' ').filter(|p| *p > width / 2).unwrap_or(end);
            let cut = rest.floor_char_boundary(cut.max(1));
            lines.push(rest[..cut].trim_end().to_string());
            rest = rest[cut..].trim_start();
            if cut == 0 {
                break;
            }
        }
        lines.push(rest.to_string());
    }
    lines
}

fn finish(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xFF\xFF\xFF\xFF\n");
    let mut offsets = vec![0usize];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n", i + 1);
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for off in offsets.iter().skip(1) {
        let _ = write!(out, "{off:010} 00000 n \n");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF",
        objects.len() + 1
    );
    out
}

#[cfg(test)]
pub fn pdf_from_text(text: &str) -> Vec<u8> {
    pdf_from_text_sized(text, crate::engine::PageSize::A4)
}

/// PDF di testo con la pagina scelta (A4, A3 o Lettera): le righe si adattano alla larghezza.
#[cfg(test)]
pub fn pdf_from_text_sized(text: &str, size: crate::engine::PageSize) -> Vec<u8> {
    use crate::engine::PageSize;
    let (width, height) = match size {
        PageSize::A4 => (595, 842),
        PageSize::A3 => (842, 1191),
        PageSize::Letter => (612, 792),
    };
    let lines = wrap_lines(text, ((width - 96) as f32 / 5.65) as usize);
    let per_page = ((height - 70) / 14) as usize;
    let mut pages: Vec<&[String]> = Vec::new();
    if lines.is_empty() {
        pages.push(&[]);
    } else {
        let mut start = 0;
        while start < lines.len() {
            let end = (start + per_page).min(lines.len());
            pages.push(&lines[start..end]);
            start = end;
        }
    }
    let font_id = 3 + pages.len() * 2;
    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids = (0..pages.len())
        .map(|i| format!("{} 0 R", 3 + i * 2))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push(format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).into_bytes());
    for page in &pages {
        let page_id = objects.len() + 1;
        let content_id = page_id + 1;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents {content_id} 0 R /Resources << /Font << /F1 {font_id} 0 R >> >> >>"
            )
            .into_bytes(),
        );
        let mut cmds = String::from("BT\n/F1 11 Tf\n14 TL\n48 800 Td\n");
        for (li, line) in page.iter().enumerate() {
            if li > 0 {
                cmds.push_str("T*\n");
            }
            cmds.push_str(&literal(&win_ansi(line)));
            cmds.push_str(" Tj\n");
        }
        cmds.push_str("ET");
        let mut stream = format!("<< /Length {} >>\nstream\n", cmds.len()).into_bytes();
        stream.extend_from_slice(cmds.as_bytes());
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    finish(&objects)
}

/// Una pagina con la foto, dritta o in orizzontale come la foto. Le righe dell'OCR,
/// se ci sono, diventano testo invisibile sopra l'immagine: si seleziona e si cerca.
pub fn pdf_from_jpeg(jpeg: &[u8], px_w: u32, px_h: u32, lines: &[crate::ocr::Line]) -> Vec<u8> {
    let (page_w, page_h) = if px_w > px_h { (842.0, 595.0) } else { (595.0, 842.0) };
    let scale = ((page_w - 72.0) / px_w as f64).min((page_h - 72.0) / px_h as f64);
    let dw = px_w as f64 * scale;
    let dh = px_h as f64 * scale;
    let x = (page_w - dw) / 2.0;
    let y = (page_h - dh) / 2.0;
    let mut content = format!("q {dw:.2} 0 0 {dh:.2} {x:.2} {y:.2} cm /Im Do Q\n");
    if !lines.is_empty() {
        content.push_str("BT 3 Tr\n");
        for line in lines {
            let text = win_ansi(line.text.trim());
            if text.is_empty() {
                continue;
            }
            let size = (line.h as f64 * scale * 0.85).max(1.0);
            let lx = x + line.x as f64 * scale;
            let ly = y + dh - (line.y + line.h) as f64 * scale + line.h as f64 * scale * 0.2;
            // Helvetica è larga in media mezzo corpo: Tz allarga la riga fino al riquadro letto.
            let natural = 0.5 * size * text.len() as f64;
            let stretch = (line.w as f64 * scale / natural * 100.0).clamp(10.0, 1000.0);
            content.push_str(&format!(
                "/F1 {size:.2} Tf {stretch:.1} Tz 1 0 0 1 {lx:.2} {ly:.2} Tm {} Tj\n",
                literal(&text)
            ));
        }
        content.push_str("ET\n");
    }
    let mut content_obj = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    content_obj.extend_from_slice(content.as_bytes());
    content_obj.extend_from_slice(b"endstream");
    let mut image_obj = format!(
        "<< /Type /XObject /Subtype /Image /Width {px_w} /Height {px_h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        jpeg.len()
    )
    .into_bytes();
    image_obj.extend_from_slice(jpeg);
    image_obj.extend_from_slice(b"\nendstream");
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w} {page_h}] /Contents 4 0 R /Resources << /XObject << /Im 5 0 R >> /Font << /F1 6 0 R >> >> >>").into_bytes(),
        content_obj,
        image_obj,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec(),
    ];
    finish(&objects)
}

/// Il PDF protetto con una password (AES a 128 bit): chiede la password per aprirlo.
pub fn protect(pdf: &[u8], password: &str) -> Result<Vec<u8>, String> {
    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
    use lopdf::{Document, Object};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let mut doc = Document::load_mem(pdf).map_err(|e| format!("PDF non letto: {e}"))?;
    let id = {
        let mut seed = pdf.len() as u64 ^ 0x9E37_79B9_7F4A_7C15;
        (0..16)
            .map(|i| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407 + i);
                (seed >> 33) as u8
            })
            .collect::<Vec<u8>>()
    };
    doc.trailer.set("ID", Object::Array(vec![Object::string_literal(id.clone()), Object::string_literal(id)]));
    let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
    filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));
    let state = EncryptionState::try_from(EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: filters,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: password,
        user_password: password,
        permissions: Permissions::all(),
    })
    .map_err(|e| format!("password non applicata: {e}"))?;
    doc.encrypt(&state).map_err(|e| format!("PDF non protetto: {e}"))?;
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
