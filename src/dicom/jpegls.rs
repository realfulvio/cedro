//! Decoder JPEG-LS (ITU-T T.87), senza perdita e «quasi senza perdita»: è una delle compressioni dei
//! DICOM. Componenti separati (ILV 0), a righe (ILV 1) e a campioni (ILV 2), da 2 a 16 bit.

/// Oltre questo numero di campioni (1 GB a 16 bit) il file è considerato rovinato.
pub const MAX_SAMPLES: usize = 1 << 29;

pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub components: u32,
    /// Campioni interleaved (RGBRGB… per tre componenti).
    pub samples: Vec<u16>,
}

const J: [u32; 32] = [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 9, 10, 11, 12, 13, 14, 15];

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u64,
    n: u32,
    /// Vero dopo un 0xFF: il byte dopo ha solo 7 bit buoni.
    after_ff: bool,
    end: bool,
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8], pos: usize) -> Bits<'a> {
        Bits { d, pos, acc: 0, n: 0, after_ff: false, end: false }
    }
    fn fill(&mut self) {
        while self.n <= 56 {
            if self.end || self.pos >= self.d.len() {
                // Oltre la fine: zeri.
                self.acc |= 0;
                self.n += 8;
                self.end = true;
                continue;
            }
            let b = self.d[self.pos];
            if self.after_ff && b >= 0x80 {
                // Un marcatore: fine dei dati.
                self.end = true;
                continue;
            }
            self.pos += 1;
            if self.after_ff {
                self.acc |= (b as u64) << (57 - self.n);
                self.n += 7;
            } else {
                self.acc |= (b as u64) << (56 - self.n);
                self.n += 8;
            }
            self.after_ff = b == 0xFF;
        }
    }
    fn bit(&mut self) -> u32 {
        if self.n == 0 {
            self.fill();
        }
        let v = (self.acc >> 63) as u32;
        self.acc <<= 1;
        self.n -= 1;
        v
    }
    fn read(&mut self, k: u32) -> u32 {
        let mut v = 0;
        for _ in 0..k {
            v = (v << 1) | self.bit();
        }
        v
    }
    fn high_bits(&mut self, max: u32) -> u32 {
        let mut c = 0;
        while self.bit() == 0 {
            c += 1;
            if c > max + 64 {
                break;
            }
        }
        c
    }
    /// Dove riprendono i byte dopo i dati della scansione (al prossimo marcatore).
    fn next_marker(&self) -> usize {
        let mut p = self.pos.saturating_sub(2);
        while p + 1 < self.d.len() {
            if self.d[p] == 0xFF && self.d[p + 1] >= 0x80 {
                return p;
            }
            p += 1;
        }
        self.d.len()
    }
}

#[derive(Clone, Copy)]
struct Params {
    maxval: i32,
    t1: i32,
    t2: i32,
    t3: i32,
    reset: i32,
}

fn clamp_t(i: i32, j: i32, maxval: i32) -> i32 {
    if i > maxval || i < j {
        j
    } else {
        i
    }
}

fn defaults(maxval: i32, near: i32) -> (i32, i32, i32) {
    if maxval >= 128 {
        let f = (maxval.min(4095) + 128) / 256;
        let t1 = clamp_t(f + 2 + 3 * near, near + 1, maxval);
        let t2 = clamp_t(f * 4 + 3 + 5 * near, t1, maxval);
        let t3 = clamp_t(f * 17 + 4 + 7 * near, t2, maxval);
        (t1, t2, t3)
    } else {
        let f = 256 / (maxval + 1);
        let t1 = clamp_t((3 / f + 3 * near).max(2), near + 1, maxval);
        let t2 = clamp_t((7 / f + 5 * near).max(3), t1, maxval);
        let t3 = clamp_t((21 / f + 7 * near).max(4), t2, maxval);
        (t1, t2, t3)
    }
}

struct Coder {
    p: Params,
    near: i32,
    range: i32,
    qbpp: u32,
    limit: u32,
    a: Vec<i32>,
    b: Vec<i32>,
    c: Vec<i32>,
    n: Vec<i32>,
    nn: [i32; 2],
}

fn ceil_log2(v: i32) -> u32 {
    let mut k = 0;
    while (1i64 << k) < v as i64 {
        k += 1;
    }
    k
}

impl Coder {
    fn new(p: Params, near: i32, bits: u32) -> Coder {
        let range = (p.maxval + 2 * near) / (2 * near + 1) + 1;
        let qbpp = ceil_log2(range);
        let bpp = ceil_log2(p.maxval + 1).max(2).max(bits.min(16));
        let bpp = bpp.min(16).max(ceil_log2(p.maxval + 1).max(2));
        let limit = 2 * (bpp + bpp.max(8));
        let a0 = 2.max((range + 32) / 64);
        Coder { p, near, range, qbpp, limit, a: vec![a0; 367], b: vec![0; 367], c: vec![0; 367], n: vec![1; 367], nn: [0; 2] }
    }

    fn quant(&self, d: i32) -> i32 {
        let (t1, t2, t3, near) = (self.p.t1, self.p.t2, self.p.t3, self.near);
        if d <= -t3 {
            -4
        } else if d <= -t2 {
            -3
        } else if d <= -t1 {
            -2
        } else if d < -near {
            -1
        } else if d <= near {
            0
        } else if d < t1 {
            1
        } else if d < t2 {
            2
        } else if d < t3 {
            3
        } else {
            4
        }
    }

    fn decode_value(&self, bits: &mut Bits, k: u32, limit: u32) -> i32 {
        let high = bits.high_bits(limit);
        if high >= limit.saturating_sub(self.qbpp + 1) {
            return bits.read(self.qbpp) as i32 + 1;
        }
        if k == 0 {
            return high as i32;
        }
        // Tetto: con dati rovinati i calcoli che seguono non devono uscire dagli i32.
        ((high << k) + bits.read(k)).min(1 << 30) as i32
    }

    fn fix(&self, px: i32, err: i32) -> i32 {
        let mut rx = px.wrapping_add(err.wrapping_mul(2 * self.near + 1));
        if rx < -self.near {
            rx += self.range * (2 * self.near + 1);
        } else if rx > self.p.maxval + self.near {
            rx -= self.range * (2 * self.near + 1);
        }
        rx.clamp(0, self.p.maxval)
    }

    fn regular(&mut self, bits: &mut Bits, q: i32, ra: i32, rb: i32, rc: i32) -> i32 {
        let (q, sign) = if q < 0 { ((-q) as usize, -1) } else { (q as usize, 1) };
        let mut px = if rc >= ra.max(rb) {
            ra.min(rb)
        } else if rc <= ra.min(rb) {
            ra.max(rb)
        } else {
            ra + rb - rc
        };
        px = (px + sign * self.c[q]).clamp(0, self.p.maxval);
        // In i64: con RESET grande (dal file) N << k non sta in un i32.
        let mut k = 0;
        while ((self.n[q] as i64) << k) < self.a[q] as i64 && k < 24 {
            k += 1;
        }
        let m = self.decode_value(bits, k, self.limit);
        let special = self.near == 0 && k == 0 && 2 * self.b[q] <= -self.n[q];
        let mut err = if special {
            if m & 1 == 1 {
                (m - 1) / 2
            } else {
                -(m / 2) - 1
            }
        } else if m & 1 == 0 {
            m / 2
        } else {
            -(m + 1) / 2
        };
        // Aggiornamento del contesto (A.6).
        // Con dati rovinati i contatori possono uscire di misura: si fa il giro invece di cadere.
        self.b[q] = self.b[q].wrapping_add(err.wrapping_mul(2 * self.near + 1));
        self.a[q] = self.a[q].saturating_add(err.saturating_abs());
        if self.n[q] == self.p.reset {
            self.a[q] >>= 1;
            self.b[q] = if self.b[q] >= 0 { self.b[q] >> 1 } else { -((1 - self.b[q]) >> 1) };
            self.n[q] >>= 1;
        }
        self.n[q] += 1;
        if self.b[q] <= -self.n[q] {
            self.b[q] += self.n[q];
            if self.c[q] > -128 {
                self.c[q] -= 1;
            }
            if self.b[q] <= -self.n[q] {
                self.b[q] = -self.n[q] + 1;
            }
        } else if self.b[q] > 0 {
            self.b[q] -= self.n[q];
            if self.c[q] < 127 {
                self.c[q] += 1;
            }
            if self.b[q] > 0 {
                self.b[q] = 0;
            }
        }
        err *= sign;
        self.fix(px, err)
    }

    /// `own_type` falso nei campioni intercalati, dove si usa sempre il contesto di tipo 0.
    fn interruption(&mut self, bits: &mut Bits, ra: i32, rb: i32, run_index: usize, own_type: bool) -> i32 {
        let ri = (own_type && (ra - rb).abs() <= self.near) as usize;
        let q = 365 + ri;
        let temp = if ri == 1 { self.a[q].saturating_add(self.n[q] >> 1) } else { self.a[q] };
        let mut k = 0;
        while ((self.n[q] as i64) << k) < temp as i64 && k < 24 {
            k += 1;
        }
        let limit = self.limit.saturating_sub(J[run_index] + 1);
        let em = self.decode_value(bits, k, limit);
        let t = em + ri as i32;
        let map = t & 1;
        let abs = (t + map) / 2;
        let neg = (k != 0 || 2 * self.nn[ri] >= self.n[q]) == (map == 1);
        let err = if neg { -abs } else { abs };
        if err < 0 {
            self.nn[ri] += 1;
        }
        self.a[q] = self.a[q].saturating_add(em.wrapping_add(1 - ri as i32) >> 1);
        if self.n[q] == self.p.reset {
            self.a[q] >>= 1;
            self.n[q] >>= 1;
            self.nn[ri] >>= 1;
        }
        self.n[q] += 1;
        if ri == 1 {
            self.fix(ra, err)
        } else {
            let s = if rb < ra { -1 } else { 1 };
            self.fix(rb, err * s)
        }
    }
}

/// Decodifica una riga di un componente. `prev` e `cur` hanno un posto in più a sinistra e a destra.
fn line(cd: &mut Coder, bits: &mut Bits, prev: &[i32], cur: &mut [i32], w: usize, run_index: &mut usize) {
    let mut x = 1;
    while x <= w {
        let ra = cur[x - 1];
        let rb = prev[x];
        let rc = prev[x - 1];
        let rd = prev[x + 1];
        let (d1, d2, d3) = (rd - rb, rb - rc, rc - ra);
        let (q1, q2, q3) = (cd.quant(d1), cd.quant(d2), cd.quant(d3));
        if q1 == 0 && q2 == 0 && q3 == 0 {
            // Modo «serie»: lo stesso valore ripetuto.
            let left = w - x + 1;
            let count = run_length(bits, left, run_index);
            for i in 0..count {
                cur[x + i] = ra;
            }
            x += count;
            if x > w {
                break;
            }
            let rb = prev[x];
            cur[x] = cd.interruption(bits, ra, rb, *run_index, true);
            *run_index = run_index.saturating_sub(1);
            x += 1;
            continue;
        }
        let q = 81 * q1 + 9 * q2 + q3;
        cur[x] = cd.regular(bits, q, ra, rb, rc);
        x += 1;
    }
}

/// Il numero di pixel di una «serie» (modo run), da `left` pixel rimasti.
fn run_length(bits: &mut Bits, left: usize, run_index: &mut usize) -> usize {
    let mut count = 0usize;
    while bits.bit() == 1 {
        let step = 1usize << J[*run_index];
        let c = step.min(left - count);
        count += c;
        if c == step && *run_index < 31 {
            *run_index += 1;
        }
        if count == left || (bits.end && bits.n == 0) {
            break;
        }
    }
    if count != left {
        let extra = if J[*run_index] > 0 { bits.read(J[*run_index]) as usize } else { 0 };
        count = (count + extra).min(left);
    }
    count
}

/// Una riga a campioni intercalati (ILV 2): tutti i componenti di un pixel uno dopo l'altro, la
/// «serie» solo se tutti i componenti sono piatti.
fn line_samples(cd: &mut Coder, bits: &mut Bits, prev: &[Vec<i32>], cur: &mut [Vec<i32>], w: usize, run_index: &mut usize) {
    let ns = prev.len();
    let mut x = 1;
    let mut qs = [0i32; 4];
    while x <= w {
        let mut flat = true;
        for c in 0..ns {
            let (ra, rb, rc, rd) = (cur[c][x - 1], prev[c][x], prev[c][x - 1], prev[c][x + 1]);
            let q = 81 * cd.quant(rd - rb) + 9 * cd.quant(rb - rc) + cd.quant(rc - ra);
            qs[c] = q;
            flat &= q == 0;
        }
        if flat {
            let left = w - x + 1;
            let count = run_length(bits, left, run_index);
            for line in cur.iter_mut().take(ns) {
                let ra = line[x - 1];
                line[x..x + count].fill(ra);
            }
            x += count;
            if x > w {
                break;
            }
            for c in 0..ns {
                let (ra, rb) = (cur[c][x - 1], prev[c][x]);
                cur[c][x] = cd.interruption(bits, ra, rb, *run_index, false);
            }
            *run_index = run_index.saturating_sub(1);
            x += 1;
            continue;
        }
        for c in 0..ns {
            let (ra, rb, rc) = (cur[c][x - 1], prev[c][x], prev[c][x - 1]);
            cur[c][x] = cd.regular(bits, qs[c], ra, rb, rc);
        }
        x += 1;
    }
}

pub fn decode(d: &[u8]) -> Result<Decoded, String> {
    if !d.starts_with(&[0xFF, 0xD8]) {
        return Err("JPEG-LS non valido".into());
    }
    let mut pos = 2;
    let (mut w, mut h, mut bits_p, mut comps) = (0usize, 0usize, 8u32, 0usize);
    let mut ids: Vec<u8> = Vec::new();
    let mut preset: Option<Params> = None;
    let mut planes: Vec<Vec<u16>> = Vec::new();
    let mut seen_frame = false;
    while pos + 4 <= d.len() {
        if d[pos] != 0xFF {
            pos += 1;
            continue;
        }
        let m = d[pos + 1];
        if m == 0xFF {
            pos += 1;
            continue;
        }
        if m == 0xD9 {
            break;
        }
        let len = u16::from_be_bytes([d[pos + 2], d[pos + 3]]) as usize;
        let seg = d.get(pos + 4..pos + 2 + len).ok_or("JPEG-LS troncato")?;
        match m {
            0xF7 => {
                if seg.len() < 6 {
                    return Err("JPEG-LS troncato".into());
                }
                bits_p = seg[0] as u32;
                h = u16::from_be_bytes([seg[1], seg[2]]) as usize;
                w = u16::from_be_bytes([seg[3], seg[4]]) as usize;
                comps = seg[5] as usize;
                if w == 0 || h == 0 || comps == 0 || comps > 4 || !(2..=16).contains(&bits_p) || seg.len() < 6 + comps * 3 {
                    return Err("JPEG-LS: misure non valide".into());
                }
                // Un file rovinato non deve far chiedere gigabyte di memoria.
                if w * h * comps > MAX_SAMPLES {
                    return Err("JPEG-LS: immagine troppo grande".into());
                }
                ids = (0..comps).map(|i| seg[6 + i * 3]).collect();
                planes = vec![vec![0u16; w * h]; comps];
                seen_frame = true;
            }
            0xF8 => {
                if seg.first() == Some(&1) && seg.len() >= 11 {
                    let v = |i: usize| u16::from_be_bytes([seg[i], seg[i + 1]]) as i32;
                    preset = Some(Params { maxval: v(1), t1: v(3), t2: v(5), t3: v(7), reset: v(9) });
                } else if seg.first() != Some(&1) {
                    return Err("JPEG-LS con tabelle di mappatura: non letto".into());
                }
            }
            0xDA => {
                if !seen_frame {
                    return Err("JPEG-LS senza intestazione".into());
                }
                let ns = seg.first().copied().unwrap_or(0) as usize;
                if ns == 0 || ns > 4 || seg.len() < 3 + ns * 2 {
                    return Err("JPEG-LS: scansione non valida".into());
                }
                let scan_comps: Vec<usize> = (0..ns).map(|i| ids.iter().position(|id| *id == seg[1 + i * 2]).unwrap_or(i)).collect();
                let near = seg[1 + ns * 2] as i32;
                let ilv = seg[2 + ns * 2];
                let maxval_default = ((1u32 << bits_p) - 1) as i32;
                let mut p = preset.unwrap_or(Params { maxval: 0, t1: 0, t2: 0, t3: 0, reset: 0 });
                if p.maxval == 0 {
                    p.maxval = maxval_default;
                }
                let (t1, t2, t3) = defaults(p.maxval, near);
                if p.t1 == 0 {
                    p.t1 = t1;
                }
                if p.t2 == 0 {
                    p.t2 = t2;
                }
                if p.t3 == 0 {
                    p.t3 = t3;
                }
                if p.reset == 0 {
                    p.reset = 64;
                }
                let mut cd = Coder::new(p, near, bits_p);
                let mut bits = Bits::new(d, pos + 2 + len);
                let mut prev = vec![vec![0i32; w + 2]; ns];
                let mut cur = vec![vec![0i32; w + 2]; ns];
                let mut runs = vec![0usize; ns];
                if ilv == 2 && ns > 1 {
                    for y in 0..h {
                        for c in 0..ns {
                            prev[c][w + 1] = prev[c][w];
                            cur[c][0] = prev[c][1];
                        }
                        line_samples(&mut cd, &mut bits, &prev, &mut cur, w, &mut runs[0]);
                        for c in 0..ns {
                            let plane = &mut planes[scan_comps[c].min(comps - 1)];
                            for x in 0..w {
                                plane[y * w + x] = cur[c][x + 1] as u16;
                            }
                        }
                        std::mem::swap(&mut prev, &mut cur);
                    }
                    pos = bits.next_marker();
                    continue;
                }
                for y in 0..h {
                    for c in 0..ns {
                        let pr = &mut prev[c];
                        pr[w + 1] = pr[w];
                        let cu = &mut cur[c];
                        cu[0] = pr[1];
                        line(&mut cd, &mut bits, &prev[c], &mut cur[c], w, &mut runs[c]);
                        let plane = &mut planes[scan_comps[c].min(comps - 1)];
                        for x in 0..w {
                            plane[y * w + x] = cur[c][x + 1] as u16;
                        }
                        std::mem::swap(&mut prev[c], &mut cur[c]);
                    }
                }
                pos = bits.next_marker();
                continue;
            }
            _ => {}
        }
        pos += 2 + len;
    }
    if !seen_frame {
        return Err("JPEG-LS senza immagine".into());
    }
    let mut samples = Vec::with_capacity(w * h * comps);
    if comps == 1 {
        samples = std::mem::take(&mut planes[0]);
    } else {
        for i in 0..w * h {
            for p in &planes {
                samples.push(p[i]);
            }
        }
    }
    Ok(Decoded { width: w as u32, height: h as u32, components: comps as u32, samples })
}

#[cfg(test)]
mod prove {
    use super::*;

    /// L'intestazione di un JPEG-LS 8×8 a 8 bit in grigio.
    fn testa() -> Vec<u8> {
        let mut d = vec![0xFF, 0xD8];
        d.extend_from_slice(&[0xFF, 0xF7, 0x00, 0x0B, 8, 0, 8, 0, 8, 1, 1, 0x11, 0]);
        d
    }

    #[test]
    fn una_scansione_troppo_corta_non_fa_cadere_il_programma() {
        // Ns = 4 ma il segmento ha un solo byte: prima si leggeva oltre la fine.
        let mut d = testa();
        d.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x03, 4, 0xFF, 0xD9]);
        assert!(decode(&d).is_err());
        let mut d = testa();
        d.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0xFF, 0xD9]);
        assert!(decode(&d).is_err());
    }

    #[test]
    fn dati_a_caso_con_reset_grande_non_fanno_cadere_il_programma() {
        // RESET = 65535 dal file: N << k usciva dagli i32 (prova utile con i controlli di overflow).
        for seed in 0..32u32 {
            let mut d = testa();
            d.extend_from_slice(&[0xFF, 0xF8, 0x00, 0x0D, 1, 0, 255, 0, 3, 0, 7, 0, 21, 0xFF, 0xFF]);
            d.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 0, 0]);
            let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
            for _ in 0..200 {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                let b = (x >> 24) as u8;
                d.push(if b == 0xFF { 0x7F } else { b });
            }
            d.extend_from_slice(&[0xFF, 0xD9]);
            let _ = decode(&d);
        }
    }
}
