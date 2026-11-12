//! JPEG sequenziale con Huffman a 8 e 12 bit (processi 1, 2 e 4): serve per i DICOM «JPEG Extended»
//! (1.2.840.10008.1.2.4.51) che hanno grigi a 12 bit, quelli che il decoder JPEG comune non legge.

use super::jpegls::{Decoded, MAX_SAMPLES};

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30,
    37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

#[derive(Clone, Default)]
struct Huffman {
    maxcode: [i32; 18],
    valptr: [i32; 17],
    mincode: [i32; 17],
    vals: Vec<u8>,
}

impl Huffman {
    fn new(counts: &[u8], vals: &[u8]) -> Huffman {
        let mut h = Huffman { vals: vals.to_vec(), ..Default::default() };
        let mut code = 0i32;
        let mut k = 0i32;
        for l in 1..=16 {
            let n = counts[l - 1] as i32;
            if n == 0 {
                h.maxcode[l] = -1;
            } else {
                h.valptr[l] = k;
                h.mincode[l] = code;
                code += n;
                k += n;
                h.maxcode[l] = code - 1;
            }
            code <<= 1;
        }
        h.maxcode[17] = i32::MAX;
        h
    }
}

#[derive(Clone)]
struct Comp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    td: usize,
    ta: usize,
    /// Piano dei campioni (con le misure arrotondate ai blocchi).
    plane: Vec<i32>,
    stride: usize,
    pred: i32,
}

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u32,
    n: u32,
}

impl<'a> Bits<'a> {
    fn bit(&mut self) -> i32 {
        if self.n == 0 {
            let mut b = 0u8;
            if self.pos < self.d.len() {
                b = self.d[self.pos];
                if b == 0xFF {
                    let next = self.d.get(self.pos + 1).copied().unwrap_or(0);
                    if next == 0 {
                        self.pos += 2;
                    } else {
                        // Un marcatore: da qui in poi zeri, il marcatore resta da leggere.
                        b = 0;
                    }
                } else {
                    self.pos += 1;
                }
            }
            self.acc = b as u32;
            self.n = 8;
        }
        self.n -= 1;
        ((self.acc >> self.n) & 1) as i32
    }
    fn receive(&mut self, s: u32) -> i32 {
        let mut v = 0;
        for _ in 0..s {
            v = (v << 1) | self.bit();
        }
        v
    }
    fn decode(&mut self, h: &Huffman) -> Result<u8, String> {
        let mut code = self.bit();
        for l in 1..=16 {
            if h.maxcode[l] >= 0 && code <= h.maxcode[l] {
                let i = (h.valptr[l] + code - h.mincode[l]) as usize;
                return h.vals.get(i).copied().ok_or_else(|| "JPEG: codice di Huffman non valido".to_string());
            }
            code = (code << 1) | self.bit();
        }
        Err("JPEG: codice di Huffman non valido".into())
    }
    /// Dopo un intervallo di restart: si salta il marcatore RSTn.
    fn restart(&mut self) {
        self.n = 0;
        while self.pos + 1 < self.d.len() {
            if self.d[self.pos] == 0xFF && (0xD0..=0xD7).contains(&self.d[self.pos + 1]) {
                self.pos += 2;
                return;
            }
            self.pos += 1;
        }
    }
    fn next_marker(&self) -> usize {
        let mut p = self.pos;
        while p + 1 < self.d.len() {
            if self.d[p] == 0xFF && self.d[p + 1] != 0 && self.d[p + 1] != 0xFF && !(0xD0..=0xD7).contains(&self.d[p + 1]) {
                return p;
            }
            p += 1;
        }
        self.d.len()
    }
}

fn extend(v: i32, s: u32) -> i32 {
    if s == 0 {
        0
    } else if v < (1 << (s - 1)) {
        v - (1 << s) + 1
    } else {
        v
    }
}

/// IDCT 8×8 in virgola mobile (separabile), con lo spostamento di livello e il taglio ai bit.
struct Idct {
    cos: [[f32; 8]; 8],
}

impl Idct {
    fn new() -> Idct {
        let mut cos = [[0f32; 8]; 8];
        for (x, row) in cos.iter_mut().enumerate() {
            for (u, c) in row.iter_mut().enumerate() {
                let cu = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
                *c = cu * (((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI) / 16.0).cos() / 2.0;
            }
        }
        Idct { cos }
    }
    fn run(&self, coef: &[i32; 64], out: &mut [i32], stride: usize, at: usize, shift: i32, max: i32) {
        let mut tmp = [0f32; 64];
        // Righe: per ogni v, da u a x.
        for v in 0..8 {
            for x in 0..8 {
                let mut s = 0.0;
                for u in 0..8 {
                    s += self.cos[x][u] * coef[v * 8 + u] as f32;
                }
                tmp[v * 8 + x] = s;
            }
        }
        for x in 0..8 {
            for y in 0..8 {
                let mut s = 0.0;
                for v in 0..8 {
                    s += self.cos[y][v] * tmp[v * 8 + x];
                }
                out[at + y * stride + x] = (s.round() as i32).saturating_add(shift).clamp(0, max);
            }
        }
    }
}

/// Decodifica un JPEG sequenziale (SOF0/SOF1) a 8 o 12 bit.
pub fn decode(d: &[u8]) -> Result<Decoded, String> {
    if !d.starts_with(&[0xFF, 0xD8]) {
        return Err("JPEG non valido".into());
    }
    let mut qt = [[0i32; 64]; 4];
    let mut dc: [Huffman; 4] = Default::default();
    let mut ac: [Huffman; 4] = Default::default();
    let mut comps: Vec<Comp> = Vec::new();
    let (mut w, mut h, mut prec) = (0usize, 0usize, 8u32);
    let mut ri = 0usize;
    let mut adobe: Option<u8> = None;
    let mut jfif = false;
    let (mut hmax, mut vmax) = (1usize, 1usize);
    let idct = Idct::new();
    let mut pos = 2;
    while pos + 4 <= d.len() {
        if d[pos] != 0xFF {
            pos += 1;
            continue;
        }
        let m = d[pos + 1];
        if m == 0xFF || m == 0x00 || (0xD0..=0xD7).contains(&m) {
            pos += if m == 0xFF { 1 } else { 2 };
            continue;
        }
        if m == 0xD9 {
            break;
        }
        let len = u16::from_be_bytes([d[pos + 2], d[pos + 3]]) as usize;
        let seg = d.get(pos + 4..pos + 2 + len.max(2)).ok_or("JPEG troncato")?;
        match m {
            0xDB => {
                let mut i = 0;
                while i < seg.len() {
                    let (pq, tq) = ((seg[i] >> 4) as usize, (seg[i] & 3) as usize);
                    i += 1;
                    for k in 0..64 {
                        let v = if pq == 0 {
                            seg.get(i + k).copied().unwrap_or(1) as i32
                        } else {
                            seg.get(i + 2 * k..i + 2 * k + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as i32).unwrap_or(1)
                        };
                        qt[tq][ZIGZAG[k]] = v;
                    }
                    i += if pq == 0 { 64 } else { 128 };
                }
            }
            0xC4 => {
                let mut i = 0;
                while i + 17 <= seg.len() {
                    let (tc, th) = (seg[i] >> 4, (seg[i] & 3) as usize);
                    let counts = &seg[i + 1..i + 17];
                    let total: usize = counts.iter().map(|c| *c as usize).sum();
                    let vals = seg.get(i + 17..i + 17 + total).ok_or("JPEG: tabella di Huffman troncata")?;
                    let t = Huffman::new(counts, vals);
                    if tc == 0 {
                        dc[th] = t;
                    } else {
                        ac[th] = t;
                    }
                    i += 17 + total;
                }
            }
            0xC0 | 0xC1 => {
                if seg.len() < 6 {
                    return Err("JPEG troncato".into());
                }
                // Un'immagine sola: un secondo fotogramma aggiungerebbe componenti a quelli che ci sono.
                if !comps.is_empty() {
                    return Err("JPEG con più di un'intestazione di fotogramma".into());
                }
                prec = seg[0] as u32;
                h = u16::from_be_bytes([seg[1], seg[2]]) as usize;
                w = u16::from_be_bytes([seg[3], seg[4]]) as usize;
                let n = seg[5] as usize;
                if w == 0 || h == 0 || n == 0 || n > 4 || !(prec == 8 || prec == 12) || seg.len() < 6 + 3 * n {
                    return Err("JPEG: misure non valide".into());
                }
                if w * h * n > MAX_SAMPLES {
                    return Err("JPEG: immagine troppo grande".into());
                }
                for i in 0..n {
                    let c = &seg[6 + 3 * i..9 + 3 * i];
                    let (hs, vs) = (((c[1] >> 4) as usize).clamp(1, 4), ((c[1] & 15) as usize).clamp(1, 4));
                    comps.push(Comp { id: c[0], h: hs, v: vs, tq: (c[2] & 3) as usize, td: 0, ta: 0, plane: Vec::new(), stride: 0, pred: 0 });
                }
                hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
                vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
                let (mcux, mcuy) = (w.div_ceil(8 * hmax), h.div_ceil(8 * vmax));
                for c in comps.iter_mut() {
                    c.stride = mcux * c.h * 8;
                    c.plane = vec![0; c.stride * mcuy * c.v * 8];
                }
            }
            0xC2 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Err("JPEG progressivo, lossless o aritmetico: non è il tipo previsto".into()),
            0xDD if seg.len() >= 2 => ri = u16::from_be_bytes([seg[0], seg[1]]) as usize,
            0xE0 if seg.starts_with(b"JFIF") => jfif = true,
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => adobe = Some(seg[11]),
            0xDA => {
                if comps.is_empty() {
                    return Err("JPEG senza intestazione".into());
                }
                let ns = seg.first().copied().unwrap_or(0) as usize;
                if ns == 0 || ns > 4 || seg.len() < 1 + 2 * ns {
                    return Err("JPEG: scansione non valida".into());
                }
                let mut sc = Vec::new();
                for i in 0..ns {
                    let id = seg[1 + 2 * i];
                    let k = comps.iter().position(|c| c.id == id).unwrap_or(i.min(comps.len() - 1));
                    comps[k].td = (seg[2 + 2 * i] >> 4) as usize & 3;
                    comps[k].ta = (seg[2 + 2 * i] & 15) as usize & 3;
                    comps[k].pred = 0;
                    sc.push(k);
                }
                let mut bits = Bits { d, pos: pos + 2 + len, acc: 0, n: 0 };
                let shift = 1i32 << (prec - 1);
                let max = (1i32 << prec) - 1;
                let mut coef = [0i32; 64];
                let mut block = |bits: &mut Bits, c: &mut Comp, bx: usize, by: usize| -> Result<(), String> {
                    coef.fill(0);
                    let t = bits.decode(&dc[c.td])? as u32;
                    if t > 16 {
                        return Err("JPEG: dati non validi".into());
                    }
                    // Con dati rovinati i numeri possono uscire di misura: si fa il giro invece di cadere.
                    let diff = extend(bits.receive(t), t);
                    c.pred = c.pred.wrapping_add(diff);
                    let q = &qt[c.tq];
                    coef[0] = c.pred.wrapping_mul(q[0]);
                    let mut k = 1;
                    while k < 64 {
                        let rs = bits.decode(&ac[c.ta])?;
                        let (r, s) = ((rs >> 4) as usize, (rs & 15) as u32);
                        if s == 0 {
                            if r == 15 {
                                k += 16;
                                continue;
                            }
                            break;
                        }
                        k += r;
                        if k > 63 {
                            break;
                        }
                        let z = ZIGZAG[k];
                        coef[z] = extend(bits.receive(s), s).wrapping_mul(q[z]);
                        k += 1;
                    }
                    let at = by * 8 * c.stride + bx * 8;
                    if at + 7 * c.stride + 8 <= c.plane.len() {
                        idct.run(&coef, &mut c.plane, c.stride, at, shift, max);
                    }
                    Ok(())
                };
                let mut count = 0usize;
                if ns == 1 {
                    // Non intercalato: blocchi del solo componente, quanti ne servono a coprirlo.
                    let k = sc[0];
                    let cw = (w * comps[k].h).div_ceil(hmax);
                    let ch = (h * comps[k].v).div_ceil(vmax);
                    for by in 0..ch.div_ceil(8) {
                        for bx in 0..cw.div_ceil(8) {
                            if ri > 0 && count > 0 && count.is_multiple_of(ri) {
                                bits.restart();
                                comps[k].pred = 0;
                            }
                            block(&mut bits, &mut comps[k], bx, by)?;
                            count += 1;
                        }
                    }
                } else {
                    let (mcux, mcuy) = (w.div_ceil(8 * hmax), h.div_ceil(8 * vmax));
                    for my in 0..mcuy {
                        for mx in 0..mcux {
                            if ri > 0 && count > 0 && count.is_multiple_of(ri) {
                                bits.restart();
                                for &k in &sc {
                                    comps[k].pred = 0;
                                }
                            }
                            for &k in &sc {
                                let (ch, cv) = (comps[k].h, comps[k].v);
                                for v in 0..cv {
                                    for u in 0..ch {
                                        block(&mut bits, &mut comps[k], mx * ch + u, my * cv + v)?;
                                    }
                                }
                            }
                            count += 1;
                        }
                    }
                }
                pos = bits.next_marker();
                continue;
            }
            _ => {}
        }
        pos += 2 + len;
    }
    if comps.is_empty() {
        return Err("JPEG senza immagine".into());
    }
    // Ricampionamento al formato pieno e, per tre componenti YCbCr, conversione in RGB.
    let n = comps.len();
    let ycc = n == 3
        && match adobe {
            Some(t) => t == 1,
            None => jfif || comps.iter().map(|c| c.id).collect::<Vec<_>>() != *b"RGB",
        };
    let mut samples = vec![0u16; w * h * n];
    let max = ((1u32 << prec) - 1) as f32;
    let half = (1u32 << (prec - 1)) as f32;
    for y in 0..h {
        for x in 0..w {
            let mut px = [0i32; 4];
            for (i, c) in comps.iter().enumerate() {
                let (sx, sy) = (x * c.h / hmax, y * c.v / vmax);
                px[i] = c.plane.get(sy * c.stride + sx).copied().unwrap_or(0);
            }
            let o = (y * w + x) * n;
            if ycc {
                let (yy, cb, cr) = (px[0] as f32, px[1] as f32 - half, px[2] as f32 - half);
                let cl = |v: f32| v.round().clamp(0.0, max) as u16;
                samples[o] = cl(yy + 1.402 * cr);
                samples[o + 1] = cl(yy - 0.344_136 * cb - 0.714_136 * cr);
                samples[o + 2] = cl(yy + 1.772 * cb);
            } else {
                for i in 0..n {
                    samples[o + i] = px[i] as u16;
                }
            }
        }
    }
    Ok(Decoded { width: w as u32, height: h as u32, components: n as u32, samples })
}

/// Il marcatore dell'intestazione di fotogramma (SOF0…SOF15: dice se è sequenziale, progressivo o
/// lossless) e la precisione dichiarata (8, 12, 16…), se c'è.
pub fn sof(d: &[u8]) -> Option<(u8, u32)> {
    let mut i = 2usize;
    while i + 4 <= d.len() && d[i] == 0xFF {
        let m = d[i + 1];
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        if matches!(m, 0xC0..=0xCF) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
            return d.get(i + 4).map(|p| (m, *p as u32));
        }
        if m == 0xDA {
            return None;
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod prove {
    use super::*;

    /// Un SOF1 a 12 bit, 8×8, con `n` componenti.
    fn sof(n: u8) -> Vec<u8> {
        let mut s = vec![0xFF, 0xC1, 0x00, 8 + 3 * n, 12, 0, 8, 0, 8, n];
        for i in 0..n {
            s.extend_from_slice(&[i + 1, 0x11, 0]);
        }
        s
    }

    #[test]
    fn una_scansione_vuota_non_fa_cadere_il_programma() {
        // FFDA con lunghezza 2: il segmento non ha nemmeno il numero dei componenti.
        let mut d = vec![0xFF, 0xD8];
        d.extend_from_slice(&sof(1));
        d.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0, 0, 0, 0]);
        assert!(decode(&d).is_err());
    }

    #[test]
    fn due_intestazioni_di_fotogramma_non_fanno_cadere_il_programma() {
        // Due SOF a tre componenti: sarebbero sei componenti in un pixel che ne tiene quattro.
        let mut d = vec![0xFF, 0xD8];
        d.extend_from_slice(&sof(3));
        d.extend_from_slice(&sof(3));
        d.extend_from_slice(&[0xFF, 0xD9]);
        assert!(decode(&d).is_err());
    }

    #[test]
    fn il_tipo_di_jpeg_si_legge_dall_intestazione() {
        let mut d = vec![0xFF, 0xD8];
        d.extend_from_slice(&sof(1));
        assert_eq!(super::sof(&d), Some((0xC1, 12)));
        d[3] = 0xC3;
        assert_eq!(super::sof(&d), Some((0xC3, 12)));
    }
}
