//! Il lettore DOC (Word 97–2003, formato binario [MS-DOC]): il testo dalla tabella dei pezzi, le
//! proprietà di carattere e di paragrafo dalle pagine FKP e dagli stili, sezioni, intestazioni e piè,
//! note, campi, tabelle (anche annidate), elenchi e immagini; il risultato è lo stesso `Documento`
//! di docx.rs, così l'impaginazione è una sola.

use super::documento::*;
use super::pagina::{Dash, Rgb, Target};
use std::collections::HashMap;
use std::io::Read;

fn u16_at(b: &[u8], i: usize) -> u16 {
    b.get(i..i + 2).map_or(0, |x| u16::from_le_bytes([x[0], x[1]]))
}
fn i16_at(b: &[u8], i: usize) -> i16 {
    u16_at(b, i) as i16
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4).map_or(0, |x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}
fn i32_at(b: &[u8], i: usize) -> i32 {
    u32_at(b, i) as i32
}
fn tw(v: i32) -> f32 {
    v as f32 / 20.0
}

const ICO: [Rgb; 17] = [
    (0, 0, 0),
    (0, 0, 0),
    (0, 0, 255),
    (0, 255, 255),
    (0, 255, 0),
    (255, 0, 255),
    (255, 0, 0),
    (255, 255, 0),
    (255, 255, 255),
    (0, 0, 128),
    (0, 128, 128),
    (0, 128, 0),
    (128, 0, 128),
    (128, 0, 0),
    (128, 128, 0),
    (128, 128, 128),
    (192, 192, 192),
];

fn ico(i: u8) -> Option<Rgb> {
    (i > 0 && (i as usize) < ICO.len()).then(|| ICO[i as usize])
}

/// COLORREF: rosso, verde, blu e il byte «automatico».
fn cv(b: &[u8], i: usize) -> Option<Rgb> {
    let x = b.get(i..i + 4)?;
    (x[3] != 0xFF).then_some((x[0], x[1], x[2]))
}

fn linea_brc(width8: u8, tipo: u8, colore: Option<Rgb>, space: u8) -> Option<Linea> {
    if tipo == 0 || tipo == 0xFF {
        return None;
    }
    let w = if tipo == 5 { 0.25 } else { (width8 as f32 / 8.0).max(0.25) };
    Some(Linea {
        width: w,
        color: colore.unwrap_or((0, 0, 0)),
        dash: match tipo {
            6 | 20 => Dash::Dotted,
            7 | 8 | 9 | 22 | 23 => Dash::Dashed,
            _ => Dash::Solid,
        },
        double: matches!(tipo, 3 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 21),
        space: (space & 0x1F) as f32,
    })
}

fn brc80(b: &[u8]) -> Option<Linea> {
    if b.len() < 4 || b == [0xFF, 0xFF, 0xFF, 0xFF] {
        return None;
    }
    linea_brc(b[0], b[1], ico(b[2]), b[3])
}

fn brc(b: &[u8]) -> Option<Linea> {
    if b.len() < 8 {
        return None;
    }
    linea_brc(b[4], b[5], cv(b, 0), b[6])
}

fn mescola(a: Rgb, b: Rgb, f: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 * f + y as f32 * (1.0 - f)).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

fn ombra(fore: Option<Rgb>, back: Option<Rgb>, ipat: u16) -> Option<Rgb> {
    let pct = match ipat {
        0 => return back,
        1 => return fore.or(Some((0, 0, 0))),
        2 => 0.05,
        3 => 0.1,
        4 => 0.2,
        5 => 0.25,
        6 => 0.3,
        7 => 0.4,
        8 => 0.5,
        9 => 0.6,
        10 => 0.7,
        11 => 0.75,
        12 => 0.8,
        13 => 0.9,
        _ => return back,
    };
    Some(mescola(fore.unwrap_or((0, 0, 0)), back.unwrap_or((255, 255, 255)), pct))
}

fn shd80(v: u16) -> Option<Rgb> {
    ombra(ico((v & 0x1F) as u8), ico(((v >> 5) & 0x1F) as u8), v >> 10)
}

fn shd(b: &[u8]) -> Option<Rgb> {
    if b.len() < 10 {
        return None;
    }
    ombra(cv(b, 0), cv(b, 4), u16_at(b, 8))
}

// ---------------------------------------------------------------- sprm

/// Le sprm di un grpprl: (codice, operando).
fn sprms(g: &[u8]) -> Vec<(u16, &[u8])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 2 <= g.len() {
        let op = u16_at(g, i);
        i += 2;
        let spra = op >> 13;
        let len = match spra {
            0 | 1 => 1,
            2 | 4 | 5 => 2,
            3 => 4,
            7 => 3,
            _ => {
                if op == 0xD608 || op == 0xD606 {
                    // sprmTDefTable: la misura è un u16 (meno uno).
                    let n = u16_at(g, i) as usize;
                    i += 2;
                    n.saturating_sub(1)
                } else if op == 0xC615 {
                    let n = *g.get(i).unwrap_or(&0) as usize;
                    i += 1;
                    if n == 255 {
                        // sprmPChgTabs lungo: si conta a mano.
                        let del = *g.get(i).unwrap_or(&0) as usize;
                        let add = *g.get(i + 1 + del * 4).unwrap_or(&0) as usize;
                        1 + del * 4 + 1 + add * 3
                    } else {
                        n
                    }
                } else {
                    let n = *g.get(i).unwrap_or(&0) as usize;
                    i += 1;
                    n
                }
            }
        };
        let end = (i + len).min(g.len());
        out.push((op, &g[i..end]));
        i += len;
    }
    out
}

// ---------------------------------------------------------------- Word 6/95

/// La misura dell'operando di uno sprm di Word 6/95 (un byte di codice): i byte fissi, 255 se davanti
/// c'è un byte di lunghezza, 254 se ce ne sono due (sprmTDefTable). `None`: sprm sconosciuto.
fn misura_w6(op: u8) -> Option<usize> {
    Some(match op {
        2 => 2,
        3 | 12 | 15 | 23 | 64 | 68 | 74 | 77 | 79 | 81 | 82 | 103 | 105 | 106 | 108 | 113 | 115 | 116 | 120 | 133 | 188 | 191 | 207 => 255,
        4..=11 | 13 | 14 | 24 | 25 | 29 | 37 | 44 | 50 | 51 => 1,
        16..=19 | 21 | 22 | 26..=28 | 30..=36 | 38..=43 | 45..=49 => 2,
        20 => 4,
        52 | 83 | 163 => 0,
        65..=67 | 71 | 75 | 85..=92 | 94 | 98 | 100 | 102 | 104 | 117..=119 => 1,
        69 | 72 | 80 | 93 | 96 | 97 | 99 | 101 | 107 | 109..=112 | 121..=124 => 2,
        70 => 4,
        73 | 95 => 3,
        131 | 132 | 138 | 139 | 142 | 143 | 146 | 147 | 150..=153 | 158 | 159 | 162 => 1,
        136 | 137 => 3,
        140 | 141 | 144 | 145 | 148 | 149 | 154..=157 | 160 | 161 | 164..=171 => 2,
        182..=184 | 189 | 195 | 197 | 198 => 2,
        185 | 186 => 1,
        187 => 12,
        190 => 254,
        192 | 194 | 196 | 200 => 4,
        193 | 199 => 5,
        _ => return None,
    })
}

/// Un bordo di Word 6 (due byte) come BRC80 di Word 97.
fn brc_w6(v: u16) -> [u8; 4] {
    let (largo, tipo) = ((v & 7) as u8, ((v >> 3) & 3) as u8);
    if v == 0 || tipo == 0 {
        return [0; 4];
    }
    let (w8, t) = match largo {
        6 => (6, 6),
        7 => (6, 22),
        n => (n.max(1) * 6, [1, 1, 2, 3][tipo as usize]),
    };
    [w8, t, ((v >> 6) & 0x1F) as u8, ((v >> 11) & 0x1F) as u8 | (((v >> 5) & 1) as u8) << 5]
}

/// Le proprietà di Word 6/95 riscritte con gli sprm di Word 97 che il resto del lettore conosce.
/// Si tengono quelle che contano per la pagina; le altre (revisioni, numerazione automatica) restano fuori.
fn da_w6(g: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    let fisso = |out: &mut Vec<u8>, code: u16, v: &[u8]| {
        out.extend_from_slice(&code.to_le_bytes());
        out.extend_from_slice(v);
    };
    let var = |out: &mut Vec<u8>, code: u16, v: &[u8]| {
        out.extend_from_slice(&code.to_le_bytes());
        out.push(v.len().min(255) as u8);
        out.extend_from_slice(&v[..v.len().min(255)]);
    };
    while i < g.len() {
        let op = g[i];
        i += 1;
        let Some(m) = misura_w6(op) else { break };
        let (v, dopo): (&[u8], usize) = match m {
            255 => {
                let n = *g.get(i).unwrap_or(&0) as usize;
                (g.get(i + 1..i + 1 + n).unwrap_or(&[]), i + 1 + n)
            }
            254 => {
                // Come in Word 97, la misura conta un byte in più dei dati.
                let n = (u16_at(g, i) as usize).saturating_sub(1);
                (g.get(i + 2..i + 2 + n).unwrap_or(&[]), i + 2 + n)
            }
            n => (g.get(i..i + n).unwrap_or(&[]), i + n),
        };
        i = dopo;
        let uguale: Option<u16> = match op {
            2 => Some(0x4600),
            5 => Some(0x2403),
            7 => Some(0x2405),
            8 => Some(0x2406),
            9 => Some(0x2407),
            16 => Some(0x840E),
            17 => Some(0x840F),
            19 => Some(0x8411),
            20 => Some(0x6412),
            21 => Some(0xA413),
            22 => Some(0xA414),
            24 => Some(0x2416),
            25 => Some(0x2417),
            47 => Some(0x442D),
            51 => Some(0x2431),
            71 => Some(0x0806),
            75 => Some(0x080A),
            80 => Some(0x4A30),
            85 => Some(0x0835),
            86 => Some(0x0836),
            87 => Some(0x0837),
            90 => Some(0x083A),
            91 => Some(0x083B),
            92 => Some(0x083C),
            93 => Some(0x4A4F),
            94 => Some(0x2A3E),
            96 => Some(0x8840),
            98 => Some(0x2A42),
            99 => Some(0x4A43),
            101 => Some(0x4845),
            104 => Some(0x2A48),
            117 => Some(0x0855),
            142 => Some(0x3009),
            143 => Some(0x300A),
            144 => Some(0x500B),
            145 => Some(0x900C),
            150 => Some(0x3011),
            153 => Some(SPRM_IHDT_W6),
            156 => Some(0xB017),
            157 => Some(0xB018),
            161 => Some(0x501C),
            162 => Some(0x301D),
            164 => Some(0xB01F),
            165 => Some(0xB020),
            166 => Some(0xB021),
            167 => Some(0xB022),
            168 => Some(0x9023),
            169 => Some(0x9024),
            182 => Some(0x5400),
            183 => Some(0x9601),
            184 => Some(0x9602),
            185 => Some(0x3403),
            186 => Some(0x3404),
            189 => Some(0x9407),
            _ => None,
        };
        if let Some(code) = uguale {
            if v.len() == m {
                fisso(&mut out, code, v);
            }
            continue;
        }
        match op {
            15 => var(&mut out, 0xC60D, v),
            23 => var(&mut out, 0xC615, v),
            38..=42 => fisso(&mut out, [0x6424, 0x6425, 0x6426, 0x6427, 0x6428][(op - 38) as usize], &brc_w6(u16_at(v, 0))),
            68 if v.len() >= 4 => fisso(&mut out, 0x6A03, &v[..4]),
            187 => {
                let b: Vec<u8> = (0..6).flat_map(|k| brc_w6(u16_at(v, k * 2))).collect();
                var(&mut out, 0xD605, &b);
            }
            190 if !v.is_empty() => {
                // sprmTDefTable: i centri restano, le celle (10 byte) diventano TC80 (20 byte).
                let n = v[0] as usize;
                let mut d = vec![v[0]];
                d.extend_from_slice(v.get(1..1 + (n + 1) * 2).unwrap_or(&[]));
                let base = 1 + (n + 1) * 2;
                for k in 0..n {
                    let tc = v.get(base + k * 10..base + k * 10 + 10).unwrap_or(&[0; 10]);
                    d.extend_from_slice(&tc[..2]);
                    d.extend_from_slice(&[0, 0]);
                    for b in 0..4 {
                        d.extend_from_slice(&brc_w6(u16_at(tc, 2 + b * 2)));
                    }
                }
                out.extend_from_slice(&0xD608u16.to_le_bytes());
                out.extend_from_slice(&((d.len() + 1) as u16).to_le_bytes());
                out.extend_from_slice(&d);
            }
            191 => var(&mut out, 0xD609, v),
            193 if v.len() >= 5 => {
                let mut d = v[..3].to_vec();
                d.extend_from_slice(&brc_w6(u16_at(v, 3)));
                var(&mut out, 0xD620, &d);
            }
            _ => {}
        }
    }
    out
}

/// Il segno, per le sezioni di Word 6, di quali intestazioni e piè ci sono (sprmSGprfIhdt).
const SPRM_IHDT_W6: u16 = 0x32FF;

/// La tabella dei caratteri a un byte di un documento di Word 6/95: dal set di caratteri del font
/// (Cirillico, Europa centrale…) o, per i font «ANSI», dalla lingua del documento.
fn caratteri_w6(lid: u16, chs: u8) -> Vec<char> {
    let enc = match chs {
        2 => return (0..=255u32).map(|b| char::from_u32(if b < 0x20 { b } else { 0xF000 + b }).unwrap_or(' ')).collect(),
        77 => encoding_rs::MACINTOSH,
        161 => encoding_rs::WINDOWS_1253,
        162 => encoding_rs::WINDOWS_1254,
        163 => encoding_rs::WINDOWS_1258,
        177 => encoding_rs::WINDOWS_1255,
        178 => encoding_rs::WINDOWS_1256,
        186 => encoding_rs::WINDOWS_1257,
        204 => encoding_rs::WINDOWS_1251,
        222 => encoding_rs::WINDOWS_874,
        238 => encoding_rs::WINDOWS_1250,
        _ => match lid & 0x3FF {
        0x05 | 0x0E | 0x15 | 0x18 | 0x1A | 0x1B | 0x1C | 0x24 => encoding_rs::WINDOWS_1250,
        0x02 | 0x19 | 0x22 | 0x23 | 0x2F => encoding_rs::WINDOWS_1251,
        0x08 => encoding_rs::WINDOWS_1253,
        0x1F => encoding_rs::WINDOWS_1254,
        0x0D => encoding_rs::WINDOWS_1255,
        0x01 => encoding_rs::WINDOWS_1256,
        0x25..=0x27 => encoding_rs::WINDOWS_1257,
        _ => encoding_rs::WINDOWS_1252,
        },
    };
    (0..=255u8).map(|b| enc.decode_without_bom_handling(&[b]).0.chars().next().unwrap_or(' ')).collect()
}

// ---------------------------------------------------------------- proprietà

#[derive(Clone, Debug, Default)]
struct Chp {
    ftc: u16,
    hps: u16,
    bold: bool,
    italic: bool,
    ul: u8,
    strike: bool,
    caps: bool,
    scaps: bool,
    hidden: bool,
    color: Option<Rgb>,
    highlight: Option<Rgb>,
    iss: u8,
    spacing: i16,
    deleted: bool,
    spec: bool,
    pic: Option<u32>,
    ole: bool,
    data: bool,
    symbol: Option<(u16, u16)>,
    istd: u16,
}

#[derive(Clone, Debug, Default)]
struct Tc {
    first: bool,
    merged: bool,
    vrestart: bool,
    vmerged: bool,
    valign: u8,
    bordi: [Option<Linea>; 4],
    sfondo: Option<Rgb>,
    pad: [Option<f32>; 4],
}

#[derive(Clone, Debug, Default)]
struct Tap {
    centri: Vec<i16>,
    tc: Vec<Tc>,
    gaph: i16,
    altezza: i16,
    intestazione: bool,
    non_dividere: bool,
    jc: u8,
    bordi: [Option<Linea>; 6],
    pad: [Option<f32>; 4],
}

#[derive(Clone, Debug, Default)]
struct Pap {
    istd: u16,
    jc: u8,
    sx: i32,
    dx: i32,
    prima: i32,
    sb: i32,
    sa: i32,
    sb_auto: bool,
    sa_auto: bool,
    linea: (i16, i16),
    keep: bool,
    keep_next: bool,
    salto: bool,
    vedove: bool,
    tabs: Vec<(i16, u8)>,
    in_tab: bool,
    ttp: bool,
    itap: i32,
    inner_cell: bool,
    inner_ttp: bool,
    ilfo: u16,
    ilvl: u8,
    livello: u8,
    contestuale: bool,
    bordi: [Option<Linea>; 5],
    sfondo: Option<Rgb>,
    tap: Option<Tap>,
}

fn toggle(op: &[u8], base: bool) -> bool {
    match op.first().copied().unwrap_or(0) {
        0 => false,
        1 => true,
        0x80 => base,
        0x81 => !base,
        _ => base,
    }
}

fn applica_chp(c: &mut Chp, g: &[u8], base: &Chp) {
    for (op, v) in sprms(g) {
        let b0 = v.first().copied().unwrap_or(0);
        match op {
            0x0835 => c.bold = toggle(v, base.bold),
            0x0836 => c.italic = toggle(v, base.italic),
            0x0837 | 0x2A53 => c.strike = toggle(v, base.strike),
            0x083A => c.scaps = toggle(v, base.scaps),
            0x083B => c.caps = toggle(v, base.caps),
            0x083C => c.hidden = toggle(v, base.hidden),
            0x0800 => c.deleted = b0 != 0,
            0x2A3E => c.ul = b0,
            0x4A43 => c.hps = u16_at(v, 0).max(2),
            0x2A48 => c.iss = b0,
            0x4845 => {
                let p = i16_at(v, 0);
                c.iss = if p > 0 { 1 } else if p < 0 { 2 } else { c.iss };
            }
            0x4A4F | 0x4A3D => c.ftc = u16_at(v, 0),
            0x2A42 => c.color = ico(b0),
            0x6870 => c.color = cv(v, 0),
            0x2A0C => c.highlight = ico(b0),
            0x8840 => c.spacing = i16_at(v, 0),
            0x0855 => c.spec = b0 != 0,
            0x6A03 => c.pic = Some(u32_at(v, 0)),
            0x080A => c.ole = b0 != 0,
            0x0806 => c.data = b0 != 0,
            0x6A09 => c.symbol = Some((u16_at(v, 0), u16_at(v, 2))),
            0x4A30 => c.istd = u16_at(v, 0),
            _ => {}
        }
    }
}

fn applica_pap(p: &mut Pap, g: &[u8], data: &[u8]) {
    for (op, v) in sprms(g) {
        let b0 = v.first().copied().unwrap_or(0);
        match op {
            0x4600 => p.istd = u16_at(v, 0),
            0x2403 | 0x2461 => p.jc = b0,
            0x2405 => p.keep = b0 != 0,
            0x2406 => p.keep_next = b0 != 0,
            0x2407 => p.salto = b0 != 0,
            0x260A => p.ilvl = b0.min(8),
            0x460B => p.ilfo = u16_at(v, 0),
            0x2416 => p.in_tab = b0 != 0,
            0x2417 => p.ttp = b0 != 0,
            0x6649 => p.itap = i32_at(v, 0).clamp(0, 64),
            0x244B => p.inner_cell = b0 != 0,
            0x244C => p.inner_ttp = b0 != 0,
            0x840F | 0x845E => p.sx = i16_at(v, 0) as i32,
            0x840E | 0x845D => p.dx = i16_at(v, 0) as i32,
            0x8411 | 0x8460 => p.prima = i16_at(v, 0) as i32,
            0x6412 => p.linea = (i16_at(v, 0), i16_at(v, 2)),
            0xA413 => p.sb = u16_at(v, 0) as i32,
            0xA414 => p.sa = u16_at(v, 0) as i32,
            0x245B => p.sb_auto = b0 != 0,
            0x245C => p.sa_auto = b0 != 0,
            0x2431 => p.vedove = b0 != 0,
            0x2640 => p.livello = b0,
            0x246D => p.contestuale = b0 != 0,
            0xC60D | 0xC615 => {
                let del = *v.first().unwrap_or(&0) as usize;
                let mut i = 1;
                let mut tolti = Vec::new();
                for _ in 0..del {
                    tolti.push(i16_at(v, i));
                    i += 2;
                }
                if op == 0xC615 {
                    // Con le tolleranze: non servono.
                    i += del * 2;
                }
                p.tabs.retain(|(x, _)| !tolti.iter().any(|t| (t - x).abs() <= 1));
                let add = *v.get(i).unwrap_or(&0) as usize;
                i += 1;
                for k in 0..add {
                    let x = i16_at(v, i + k * 2);
                    let tbd = *v.get(i + add * 2 + k).unwrap_or(&0);
                    p.tabs.retain(|(y, _)| *y != x);
                    p.tabs.push((x, tbd));
                }
                p.tabs.sort_by_key(|t| t.0);
            }
            0x6424 => p.bordi[0] = brc80(v),
            0x6425 => p.bordi[2] = brc80(v),
            0x6426 => p.bordi[1] = brc80(v),
            0x6427 => p.bordi[3] = brc80(v),
            0x6428 => p.bordi[4] = brc80(v),
            0xC64E => p.bordi[0] = brc(v),
            0xC64F => p.bordi[2] = brc(v),
            0xC650 => p.bordi[1] = brc(v),
            0xC651 => p.bordi[3] = brc(v),
            0xC652 => p.bordi[4] = brc(v),
            0x442D => p.sfondo = shd80(u16_at(v, 0)),
            0xC64D => p.sfondo = shd(v),
            0x6646 => {
                // sprmPHugePapx: le proprietà stanno nello stream Data.
                let at = u32_at(v, 0) as usize;
                let n = u16_at(data, at) as usize;
                if let Some(g2) = data.get(at + 2..at + 2 + n) {
                    applica_pap(p, g2, &[]);
                }
            }
            _ => {
                if op >> 10 & 7 == 5 {
                    applica_tap(p.tap.get_or_insert_with(Tap::default), op, v);
                }
            }
        }
    }
}

fn applica_tap(t: &mut Tap, op: u16, v: &[u8]) {
    let b0 = v.first().copied().unwrap_or(0);
    match op {
        0xD608 | 0xD606 => {
            let n = b0 as usize;
            t.centri = (0..=n).map(|k| i16_at(v, 1 + k * 2)).collect();
            let base = 1 + (n + 1) * 2;
            let size = if v.len() >= base + n * 20 { 20 } else { 10 };
            t.tc = (0..n)
                .map(|k| {
                    let at = base + k * size;
                    let g = u16_at(v, at);
                    let mut tc = Tc { first: g & 1 != 0, merged: g & 2 != 0, vmerged: g & 0x20 != 0, vrestart: g & 0x40 != 0, valign: ((g >> 7) & 3) as u8, ..Default::default() };
                    if size == 20 {
                        let b = |o: usize| v.get(at + o..at + o + 4).and_then(brc80);
                        tc.bordi = [b(4), b(12), b(8), b(16)];
                    }
                    tc
                })
                .collect();
        }
        0x9602 => t.gaph = i16_at(v, 0),
        0x9601 => {
            // sprmTDxaLeft: sposta tutti i bordi.
            let d = i16_at(v, 0) - t.centri.first().copied().unwrap_or(0) - t.gaph;
            t.centri.iter_mut().for_each(|c| *c = c.saturating_add(d));
        }
        0x9407 => t.altezza = i16_at(v, 0),
        0x3404 => t.intestazione = b0 != 0,
        0x3403 | 0x3466 => t.non_dividere = b0 != 0,
        0x5400 | 0x548A => t.jc = u16_at(v, 0).min(2) as u8,
        0xD605 => {
            for k in 0..6 {
                t.bordi[k] = v.get(k * 4..k * 4 + 4).and_then(brc80);
            }
        }
        0xD613 => {
            for k in 0..6 {
                t.bordi[k] = v.get(k * 8..k * 8 + 8).and_then(brc);
            }
        }
        0xD612 | 0xD616 | 0xD60C => {
            // Sfondo per cella (a pezzi di 22 celle, il secondo e il terzo pezzo continuano).
            let off = match op {
                0xD616 => 22,
                0xD60C => 44,
                _ => 0,
            };
            for (k, c) in v.chunks_exact(10).enumerate() {
                if let Some(tc) = t.tc.get_mut(off + k) {
                    tc.sfondo = shd(c);
                }
            }
        }
        0xD609 => {
            for (k, c) in v.chunks_exact(2).enumerate() {
                if let Some(tc) = t.tc.get_mut(k) {
                    tc.sfondo = shd80(u16_at(c, 0));
                }
            }
        }
        0xD62F | 0xD620 => {
            // sprmTSetBrc: celle da..a, quali lati, il bordo.
            let (a, z, lati) = (b0 as usize, *v.get(1).unwrap_or(&0) as usize, *v.get(2).unwrap_or(&0));
            let l = if op == 0xD62F { v.get(3..11).and_then(brc) } else { v.get(3..7).and_then(brc80) };
            for k in a..z.min(t.tc.len()) {
                let tc = &mut t.tc[k];
                if lati & 1 != 0 {
                    tc.bordi[0] = l;
                }
                if lati & 2 != 0 {
                    tc.bordi[2] = l;
                }
                if lati & 4 != 0 {
                    tc.bordi[1] = l;
                }
                if lati & 8 != 0 {
                    tc.bordi[3] = l;
                }
            }
        }
        0x7629 => {
            if let Some(tc) = t.tc.get_mut(b0 as usize) {
                let f = *v.get(1).unwrap_or(&0);
                tc.vmerged = f != 0;
                tc.vrestart = f == 3;
            }
        }
        0xD62C => {
            let (a, z, va) = (b0 as usize, *v.get(1).unwrap_or(&0) as usize, *v.get(2).unwrap_or(&0));
            for k in a..z.min(t.tc.len()) {
                t.tc[k].valign = va.min(2);
            }
        }
        0xD634 | 0xD632 | 0xD633 => {
            // CSSA: celle, lati, unità (3 = twip), misura.
            let (a, z, lati, fts, w) = (b0 as usize, *v.get(1).unwrap_or(&0) as usize, *v.get(2).unwrap_or(&0), *v.get(3).unwrap_or(&0), u16_at(v, 4));
            if fts != 3 {
                return;
            }
            let x = Some(tw(w as i32));
            let set = |pad: &mut [Option<f32>; 4]| {
                if lati & 1 != 0 {
                    pad[2] = x;
                }
                if lati & 2 != 0 {
                    pad[0] = x;
                }
                if lati & 4 != 0 {
                    pad[3] = x;
                }
                if lati & 8 != 0 {
                    pad[1] = x;
                }
            };
            if op == 0xD634 {
                set(&mut t.pad);
            } else {
                for k in a..z.min(t.tc.len()) {
                    set(&mut t.tc[k].pad);
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- stili, font, elenchi

#[derive(Clone, Default)]
struct Stile {
    nome: String,
    base: Option<u16>,
    papx: Vec<u8>,
    chpx: Vec<u8>,
}

#[derive(Clone, Default)]
struct Lvl {
    start: i32,
    nfc: u8,
    jc: u8,
    testo: Vec<u16>,
    segue: u8,
    papx: Vec<u8>,
    chpx: Vec<u8>,
}

struct Lista {
    id: i32,
    livelli: Vec<Lvl>,
}

struct Lfo {
    lsid: i32,
    sostituti: Vec<(u8, Option<i32>, Option<Lvl>)>,
}

/// Una forma mobile (FSPA) con il suo testo o la sua immagine.
/// Le proprietà di una forma OfficeArt che servono qui.
#[derive(Clone, Copy)]
struct FormaP {
    txid: Option<u32>,
    fill: Option<Rgb>,
    filled: bool,
    line: Option<Rgb>,
    line_w: f32,
    lined: bool,
    /// Posizione (OfficeArt): riferimento e allineamento orizzontale e verticale.
    posrelh: Option<u32>,
    posrelv: Option<u32>,
    posh: u32,
    posv: u32,
}

impl Default for FormaP {
    fn default() -> Self {
        // Bordo nero sottile come in OfficeArt; lo sfondo solo se il colore è scritto (come LibreOffice).
        FormaP { txid: None, fill: None, filled: true, line: Some((0, 0, 0)), line_w: 0.75, lined: true, posrelh: None, posrelv: None, posh: 0, posv: 0 }
    }
}

#[derive(Clone)]
struct Fspa {
    spid: i32,
    r: [i32; 4],
    bx: u8,
    by: u8,
    wr: u8,
    dietro: bool,
}

// ---------------------------------------------------------------- uscita (come RTF)

#[derive(Default)]
struct Livello {
    cella: Vec<Blocco>,
    celle: Vec<Vec<Blocco>>,
    righe: Vec<(Tap, Vec<Vec<Blocco>>)>,
}

#[derive(Default)]
struct Contesto {
    corpo: Vec<Blocco>,
    livelli: Vec<Livello>,
    runs: Vec<Run>,
    ancorati: Vec<Mobile>,
}

struct Campo {
    istr: String,
    risultato: bool,
    nascosto: bool,
    link_prima: Option<Option<Target>>,
    /// Casella di spunta di un modulo (FORMCHECKBOX): spuntata o no.
    casella: Option<bool>,
    /// La voce scelta di un elenco a tendina (FORMDROPDOWN).
    scelta: Option<String>,
}

struct Lettore<'a> {
    word: &'a [u8],
    table: &'a [u8],
    data: &'a [u8],
    fib: Vec<(u32, u32)>,
    ccp: [u32; 8],
    testo: Vec<u16>,
    /// cp, cp finale, fc del primo carattere, compresso, prm
    pezzi: Vec<(u32, u32, u32, bool, u16)>,
    prc: Vec<Vec<u8>>,
    chpx: Vec<(u32, u32, Vec<u8>)>,
    papx: Vec<(u32, u32, Vec<u8>)>,
    stili: Vec<Option<Stile>>,
    chp0: Chp,
    fonts: Vec<(String, u8)>,
    liste: Vec<Lista>,
    lfo: Vec<Lfo>,
    contatori: HashMap<i32, [i32; 9]>,
    spa: [Vec<(u32, Fspa)>; 2],
    blip: HashMap<i32, usize>,
    forme: HashMap<i32, FormaP>,
    doc: Documento,
    ctx: Contesto,
    link: Option<Target>,
    campi: Vec<Campo>,
    nota_numero: Option<String>,
    note_rif: [Vec<u32>; 2],
    cache_pap: HashMap<usize, Pap>,
    profondita: usize,
    /// Word 6/95: tutto nello stream WordDocument, testo a un byte, sprm di un byte.
    w6: bool,
    cp8: Vec<char>,
    /// Word 6: la tabella dei caratteri di ogni font (dal suo set di caratteri).
    cp8_font: Vec<Vec<char>>,
    /// Word 6: quante pagine FKP di caratteri e di paragrafi ci sono (anche oltre la tabella salvata).
    cpn_bte: [usize; 2],
    /// Word 6: le intestazioni già lette (ci sono solo quelle che le sezioni dichiarano).
    testate_w6: usize,
}

impl<'a> Lettore<'a> {
    fn fc(&self, k: usize) -> (usize, usize) {
        self.fib.get(k).map_or((0, 0), |(f, l)| (*f as usize, *l as usize))
    }

    fn tab(&self, k: usize) -> &'a [u8] {
        let (f, l) = self.fc(k);
        if l == 0 {
            return &[];
        }
        self.table.get(f..f.saturating_add(l)).unwrap_or(&[])
    }

    /// Una PLC: n+1 posizioni e n dati grandi `cb`.
    fn plc(b: &[u8], cb: usize) -> (Vec<u32>, Vec<&[u8]>) {
        if b.len() < 4 {
            return (Vec::new(), Vec::new());
        }
        let n = (b.len() - 4) / (4 + cb);
        let cps = (0..=n).map(|i| u32_at(b, i * 4)).collect();
        let dati = (0..n).map(|i| &b[4 * (n + 1) + i * cb..4 * (n + 1) + (i + 1) * cb]).collect();
        (cps, dati)
    }

    fn leggi_pezzi(&mut self) -> Result<(), String> {
        if self.w6 && (u16_at(self.word, 0x0A) & 0x0004 == 0 || self.fc(33).1 == 0) {
            // Word 6 non «complesso»: il testo è tutto di seguito, un byte per carattere.
            let fc_min = u32_at(self.word, 0x18);
            let n: u32 = self.ccp.iter().sum::<u32>() + 1;
            let limite = self.word.len() as u32;
            for j in 0..n {
                // Il byte resta com'è: la sua lettera dipende dal font del tratto (vedi `lettera_w6`).
                let b = self.word.get((fc_min + j).min(limite) as usize).copied().unwrap_or(b' ');
                self.testo.push(b as u16);
            }
            self.pezzi.push((0, n, fc_min, true, 0));
            return Ok(());
        }
        let clx = self.tab(33);
        let mut i = 0;
        while i < clx.len() && clx[i] == 0x01 {
            let cb = u16_at(clx, i + 1) as usize;
            self.prc.push(clx.get(i + 3..i + 3 + cb).unwrap_or(&[]).to_vec());
            i += 3 + cb;
        }
        if clx.get(i) != Some(&0x02) {
            return Err("DOC senza tabella dei pezzi".into());
        }
        let lcb = u32_at(clx, i + 1) as usize;
        let plc = clx.get(i + 5..i + 5 + lcb).ok_or("DOC con tabella dei pezzi rovinata")?;
        let (cps, pcd) = Self::plc(plc, 8);
        let limite = self.word.len().saturating_mul(2).max(1 << 20);
        for (k, d) in pcd.iter().enumerate() {
            let (a, z) = (cps[k], cps[k + 1]);
            if z < a || self.testo.len() > limite {
                break;
            }
            let raw = u32_at(d, 2);
            // In Word 6 i pezzi sono sempre a un byte e l'fc è già in byte.
            let comp = self.w6 || raw & (1 << 30) != 0;
            let fc = if self.w6 { raw.wrapping_mul(2) } else { raw & 0x3FFF_FFFF };
            let prm = u16_at(d, 6);
            let n = (z - a) as usize;
            let mut letti = 0;
            if comp {
                let off = (fc / 2) as usize;
                for j in 0..n {
                    let Some(&b) = self.word.get(off + j) else { break };
                    self.testo.push(if self.w6 { b as u16 } else { crate::doctext::cp1252(b) as u32 as u16 });
                    letti += 1;
                }
            } else {
                for j in 0..n {
                    let at = fc as usize + j * 2;
                    if at + 2 > self.word.len() {
                        break;
                    }
                    self.testo.push(u16_at(self.word, at));
                    letti += 1;
                }
            }
            // Pezzi corti (file troncato): il testo resta allineato ai CP.
            for _ in letti..n {
                self.testo.push(b' ' as u16);
            }
            self.pezzi.push((a, z, if comp { fc / 2 } else { fc }, comp, prm));
        }
        Ok(())
    }

    fn fc_di(&self, cp: u32) -> Option<u32> {
        let k = self.pezzi.partition_point(|p| p.1 <= cp);
        let p = self.pezzi.get(k)?;
        (cp >= p.0).then(|| p.2 + (cp - p.0) * if p.3 { 1 } else { 2 })
    }

    fn prm_di(&self, cp: u32) -> Option<&[u8]> {
        let k = self.pezzi.partition_point(|p| p.1 <= cp);
        let prm = self.pezzi.get(k)?.4;
        if prm & 1 == 1 {
            self.prc.get((prm >> 1) as usize).map(|v| v.as_slice())
        } else {
            None
        }
    }

    /// Le pagine FKP (caratteri o paragrafi) dalla loro tabella: in Word 6 i numeri sono a due byte,
    /// e se il file è stato salvato in fretta le ultime pagine seguono la tabella.
    fn pagine_fkp(&self, k: usize) -> Vec<usize> {
        if !self.w6 {
            let (_, dati) = Self::plc(self.tab(k), 4);
            return dati.iter().map(|d| (u32_at(d, 0) & 0x3F_FFFF) as usize).collect();
        }
        let (_, dati) = Self::plc(self.tab(k), 2);
        let mut pn: Vec<usize> = dati.iter().map(|d| u16_at(d, 0) as usize).collect();
        let quante = self.cpn_bte[if k == 12 { 0 } else { 1 }].min(4096);
        while pn.len() < quante {
            let dopo = pn.last().map_or(0, |p| p + 1);
            pn.push(dopo);
        }
        pn
    }

    fn leggi_fkp(&mut self) {
        let word = self.word;
        let w6 = self.w6;
        // Caratteri.
        for pn in self.pagine_fkp(12) {
            let Some(page) = word.get(pn * 512..pn * 512 + 512) else { continue };
            let n = page[511] as usize;
            for k in 0..n {
                let (a, z) = (u32_at(page, k * 4), u32_at(page, (k + 1) * 4));
                let off = page.get(4 * (n + 1) + k).copied().unwrap_or(0) as usize * 2;
                let mut g = if off == 0 { Vec::new() } else { let cb = page[off] as usize; page.get(off + 1..off + 1 + cb).unwrap_or(&[]).to_vec() };
                if w6 {
                    g = da_w6(&g);
                }
                self.chpx.push((a, z, g));
            }
        }
        self.chpx.sort_by_key(|c| c.0);
        // Paragrafi (in Word 6 ogni voce dopo gli fc è lunga 7 byte, non 13).
        let bx = if w6 { 7 } else { 13 };
        for pn in self.pagine_fkp(13) {
            let Some(page) = word.get(pn * 512..pn * 512 + 512) else { continue };
            let n = page[511] as usize;
            for k in 0..n {
                let (a, z) = (u32_at(page, k * 4), u32_at(page, (k + 1) * 4));
                let off = page.get(4 * (n + 1) + k * bx).copied().unwrap_or(0) as usize * 2;
                if off == 0 {
                    self.papx.push((a, z, vec![0, 0]));
                    continue;
                }
                let cb = page[off] as usize;
                let (start, len) = if w6 {
                    (off + 1, cb * 2)
                } else if cb == 0 {
                    (off + 2, page.get(off + 1).copied().unwrap_or(0) as usize * 2)
                } else {
                    (off + 1, cb * 2 - 1)
                };
                let mut g = page.get(start..start + len).unwrap_or(&[0, 0]).to_vec();
                if w6 && g.len() >= 2 {
                    let resto = da_w6(&g[2..]);
                    g.truncate(2);
                    g.extend(resto);
                }
                self.papx.push((a, z, g));
            }
        }
        self.papx.sort_by_key(|c| c.0);
    }

    fn leggi_stili(&mut self) {
        let s = self.tab(1);
        if s.len() < 4 {
            return;
        }
        let cb_shi = u16_at(s, 0) as usize;
        let shi = &s[2..(2 + cb_shi).min(s.len())];
        let cstd = u16_at(shi, 0) as usize;
        let cb_base = u16_at(shi, 2) as usize;
        self.chp0 = Chp { ftc: u16_at(shi, 12), hps: 20, ..Default::default() };
        let mut at = 2 + cb_shi;
        for _ in 0..cstd.min(4096) {
            let cb = u16_at(s, at) as usize;
            at += 2;
            if cb == 0 {
                self.stili.push(None);
                continue;
            }
            let Some(std) = s.get(at..at + cb) else { break };
            at += cb;
            let stk = (u16_at(std, 2) & 0xF) as u8;
            let base = u16_at(std, 2) >> 4;
            let cupx = (u16_at(std, 4) & 0xF) as usize;
            let (nome, mut i) = if self.w6 {
                // Word 6: la base è di 8 byte e il nome è a un byte per carattere.
                let i = cb_base.max(8);
                let cch = std.get(i).copied().unwrap_or(0) as usize;
                let nome: String = std.get(i + 1..i + 1 + cch).unwrap_or(&[]).iter().map(|b| self.cp8[*b as usize]).collect();
                (nome, i + 1 + cch + 1)
            } else {
                let i = cb_base.max(10);
                let cch = u16_at(std, i) as usize;
                let nome: String = (0..cch).map(|k| u16_at(std, i + 2 + k * 2)).map(|u| char::from_u32(u as u32).unwrap_or(' ')).collect();
                (nome, i + 2 + cch * 2 + 2)
            };
            let mut upx: Vec<Vec<u8>> = Vec::new();
            for _ in 0..cupx {
                if i % 2 == 1 {
                    i += 1;
                }
                let n = u16_at(std, i) as usize;
                upx.push(std.get(i + 2..i + 2 + n).unwrap_or(&[]).to_vec());
                i += 2 + n;
            }
            if self.w6 {
                for (k, u) in upx.iter_mut().enumerate() {
                    // Nel primo UPX di uno stile di paragrafo, davanti alle proprietà c'è l'istd.
                    let salta = if stk == 1 && k == 0 { 2.min(u.len()) } else { 0 };
                    let mut nuovo = u[..salta].to_vec();
                    nuovo.extend(da_w6(&u[salta..]));
                    *u = nuovo;
                }
            }
            let para = stk == 1;
            let (papx, chpx) = if para { (upx.first().map(|u| u.get(2..).unwrap_or(&[]).to_vec()).unwrap_or_default(), upx.get(1).cloned().unwrap_or_default()) } else if stk == 2 { (Vec::new(), upx.first().cloned().unwrap_or_default()) } else { (Vec::new(), Vec::new()) };
            self.stili.push(Some(Stile { nome, base: (base != 0xFFF).then_some(base), papx, chpx }));
        }
    }

    fn leggi_font(&mut self) {
        let s = self.tab(15);
        if s.len() < 4 {
            return;
        }
        if self.w6 {
            // Word 6: in testa la misura in byte, poi le FFN con il nome a un byte per carattere.
            let fine = (u16_at(s, 0) as usize).min(s.len());
            let mut at = 2;
            while at < fine && self.fonts.len() < 4096 {
                let cb = s[at] as usize;
                let f = &s[at + 1..(at + 1 + cb).min(s.len())];
                let chs = f.get(3).copied().unwrap_or(0);
                let nome: String = f.get(5..).unwrap_or(&[]).iter().take_while(|b| **b != 0).map(|b| self.cp8[*b as usize]).collect();
                self.fonts.push((nome, chs));
                at += 1 + cb;
            }
            return;
        }
        let mut n = u16_at(s, 0) as usize;
        let mut at = 4;
        if n == 0xFFFF {
            n = u16_at(s, 2) as usize;
            at = 6;
        }
        for _ in 0..n.min(4096) {
            let Some(&cb) = s.get(at) else { break };
            let f = &s[at + 1..(at + 1 + cb as usize).min(s.len())];
            let chs = f.get(3).copied().unwrap_or(0);
            let mut nome = String::new();
            let mut i = 39;
            while i + 1 < f.len() {
                let u = u16_at(f, i);
                if u == 0 {
                    break;
                }
                nome.push(char::from_u32(u as u32).unwrap_or(' '));
                i += 2;
            }
            self.fonts.push((nome, chs));
            at += 1 + cb as usize;
        }
    }

    fn leggi_lvl(b: &[u8], at: &mut usize) -> Lvl {
        let h = b.get(*at..*at + 28).unwrap_or(&[0; 28]);
        let (cbc, cbp) = (h[24] as usize, h[25] as usize);
        let mut i = *at + 28;
        let papx = b.get(i..i + cbp).unwrap_or(&[]).to_vec();
        i += cbp;
        let chpx = b.get(i..i + cbc).unwrap_or(&[]).to_vec();
        i += cbc;
        let n = u16_at(b, i) as usize;
        let testo = (0..n.min(255)).map(|k| u16_at(b, i + 2 + k * 2)).collect();
        i += 2 + n * 2;
        *at = i;
        Lvl { start: i32_at(h, 0), nfc: h[4], jc: h[5] & 3, testo, segue: h[15], papx, chpx }
    }

    /// I dati di una voce del FIB da `fc` fino in fondo: le PlfLst hanno i LVL accodati fuori da `lcb`.
    fn tab_coda(&self, k: usize) -> &'a [u8] {
        let (f, l) = self.fc(k);
        if l == 0 {
            return &[];
        }
        self.table.get(f..).unwrap_or(&[])
    }

    fn leggi_liste(&mut self) {
        let s = self.tab_coda(73);
        if s.len() >= 2 {
            let n = i16_at(s, 0).max(0) as usize;
            let mut at = 2 + n * 28;
            for k in 0..n {
                let h = &s[(2 + k * 28).min(s.len())..(2 + k * 28 + 28).min(s.len())];
                if h.len() < 28 {
                    break;
                }
                let semplice = h[26] & 1 != 0;
                let livelli = (0..if semplice { 1 } else { 9 }).map(|_| Self::leggi_lvl(s, &mut at)).collect();
                self.liste.push(Lista { id: i32_at(h, 0), livelli });
            }
        }
        let s = self.tab_coda(74);
        if s.len() >= 4 {
            let n = i32_at(s, 0).clamp(0, 4096) as usize;
            let mut at = 4 + n * 16;
            for k in 0..n {
                let h = s.get(4 + k * 16..4 + k * 16 + 16).unwrap_or(&[0; 16]);
                let cl = h[12] as usize;
                let mut sost = Vec::new();
                // Ogni LFO ha il suo LFOData (cp e poi i livelli sostituiti), anche senza livelli.
                at += 4;
                {
                    for _ in 0..cl.min(9) {
                        let st = i32_at(s, at);
                        let fl = s.get(at + 4).copied().unwrap_or(0);
                        at += 8;
                        let lvl = (fl & 0x20 != 0).then(|| Self::leggi_lvl(s, &mut at));
                        sost.push((fl & 0xF, (fl & 0x10 != 0).then_some(st), lvl));
                    }
                }
                self.lfo.push(Lfo { lsid: i32_at(h, 0), sostituti: sost });
            }
        }
    }

    // ---------------------------------------------------------------- forme e immagini

    /// Il dato di un BLIP (PNG, JPEG, DIB) dentro un record OfficeArt.
    fn blip_in(b: &[u8]) -> Option<Vec<u8>> {
        // Si cerca la firma del formato: più semplice e robusto delle varianti con uno o due UID.
        let png = b.windows(8).position(|w| w == b"\x89PNG\r\n\x1a\n");
        let jpg = b.windows(3).position(|w| w == [0xFF, 0xD8, 0xFF]);
        match (png, jpg) {
            (Some(p), Some(j)) if j < p => Some(b[j..].to_vec()),
            (Some(p), _) => Some(b[p..].to_vec()),
            (None, Some(j)) => Some(b[j..].to_vec()),
            _ => None,
        }
    }

    fn immagine_cp(&mut self, chp: &Chp) -> Option<(usize, f32, f32)> {
        let at = chp.pic? as usize;
        let d = self.data;
        let lcb = u32_at(d, at) as usize;
        let cbh = u16_at(d, at + 4) as usize;
        if lcb < cbh || cbh < 0x44 {
            return None;
        }
        let mm = i16_at(d, at + 6);
        let (gx, gy) = (i16_at(d, at + 0x1C) as f32, i16_at(d, at + 0x1E) as f32);
        let (mx, my) = (u16_at(d, at + 0x20) as f32, u16_at(d, at + 0x22) as f32);
        let (cl, ct, cr, cb) = (i16_at(d, at + 0x24) as f32, i16_at(d, at + 0x26) as f32, i16_at(d, at + 0x28) as f32, i16_at(d, at + 0x2A) as f32);
        let mut i = at + cbh;
        if mm == 0x66 {
            i += 1 + d.get(i).copied().unwrap_or(0) as usize;
        }
        let corpo = d.get(i..(at + lcb).min(d.len()))?;
        let bytes = Self::blip_in(corpo)?;
        let w = (gx - cl - cr) * if mx > 0.0 { mx } else { 1000.0 } / 1000.0 / 20.0;
        let h = (gy - ct - cb) * if my > 0.0 { my } else { 1000.0 } / 1000.0 / 20.0;
        let img = crate::pdfdoc::DocImage::from_bytes(&bytes, (w > 1.0 && h > 1.0).then_some((w, h)), 0)?;
        let (w, h) = if w > 1.0 && h > 1.0 { (w, h) } else { (img.px.0 as f32 * 0.75, img.px.1 as f32 * 0.75) };
        self.doc.immagini.push(img);
        Some((self.doc.immagini.len() - 1, w, h))
    }

    /// I disegni (OfficeArt): per ogni forma, l'immagine che mostra.
    fn leggi_disegni(&mut self) {
        let dgg = self.tab(50);
        if dgg.is_empty() {
            return;
        }
        // Il magazzino delle immagini (BStore) e le forme con la loro proprietà pib.
        let mut bse: Vec<Option<Vec<u8>>> = Vec::new();
        let mut pib: Vec<(i32, usize)> = Vec::new();
        let mut forme: HashMap<i32, FormaP> = HashMap::new();
        #[allow(clippy::too_many_arguments)]
        fn giro(b: &[u8], word: &[u8], bse: &mut Vec<Option<Vec<u8>>>, pib: &mut Vec<(i32, usize)>, forme: &mut HashMap<i32, FormaP>, spid: &mut i32, prof: usize) {
            let mut i = 0;
            while i + 8 <= b.len() && prof < 12 {
                let ver = u16_at(b, i) & 0xF;
                let tipo = u16_at(b, i + 2);
                let len = u32_at(b, i + 4) as usize;
                let corpo = b.get(i + 8..(i + 8 + len).min(b.len())).unwrap_or(&[]);
                match tipo {
                    0xF007 => {
                        // FBSE: l'immagine dentro, o nello stream WordDocument a foDelay.
                        let size = u32_at(corpo, 20) as usize;
                        let delay = u32_at(corpo, 28) as usize;
                        let cbn = corpo.get(33).copied().unwrap_or(0) as usize;
                        let dentro = corpo.get(36 + cbn..).filter(|r| r.len() > 8);
                        let dato = dentro.and_then(Lettore::blip_in).or_else(|| word.get(delay..delay.saturating_add(size).min(word.len())).and_then(Lettore::blip_in));
                        bse.push(dato);
                    }
                    0xF00A => *spid = i32_at(corpo, 0),
                    0xF00B | 0xF121 | 0xF122 => {
                        // FOPT: proprietà; pib (0x4104) è l'indice nel BStore, da 1.
                        let n = (u16_at(b, i) >> 4) as usize;
                        let colore = |v: u32| -> Option<Rgb> { (v >> 24 == 0).then_some((v as u8, (v >> 8) as u8, (v >> 16) as u8)) };
                        for k in 0..n {
                            let id = u16_at(corpo, k * 6) & 0x3FFF;
                            let v = u32_at(corpo, k * 6 + 2);
                            let f = forme.entry(*spid).or_default();
                            match id {
                                0x0104 => pib.push((*spid, v as usize)),
                                0x0080 => f.txid = Some(v),
                                0x0181 => f.fill = colore(v),
                                0x01BF if v & 0x0010_0000 != 0 => f.filled = v & 0x10 != 0,
                                0x01C0 => f.line = colore(v).or(f.line),
                                0x01CB => f.line_w = v as f32 / 12700.0,
                                0x01FF if v & 0x0008_0000 != 0 => f.lined = v & 0x8 != 0,
                                0x038F => f.posh = v,
                                0x0390 => f.posrelh = Some(v),
                                0x0391 => f.posv = v,
                                0x0392 => f.posrelv = Some(v),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                if ver == 0xF {
                    giro(corpo, word, bse, pib, forme, spid, prof + 1);
                }
                i += 8 + len;
            }
        }
        let mut spid = 0;
        // OfficeArtContent: il DggContainer, poi i disegni, ognuno preceduto da un byte (dgglbl).
        let lunghezza = |at: usize| 8 + u32_at(dgg, at + 4) as usize;
        let mut at = 0;
        if dgg.len() >= 8 && u16_at(dgg, 2) == 0xF000 {
            let n = lunghezza(0).min(dgg.len());
            giro(&dgg[..n], self.word, &mut bse, &mut pib, &mut forme, &mut spid, 0);
            at = n;
        }
        while at + 9 <= dgg.len() {
            let n = lunghezza(at + 1);
            let fine = (at + 1 + n).min(dgg.len());
            giro(&dgg[at + 1..fine], self.word, &mut bse, &mut pib, &mut forme, &mut spid, 0);
            at = fine;
        }
        self.forme = forme;
        for (s, k) in pib {
            if let Some(Some(dato)) = bse.get(k.wrapping_sub(1)) {
                if let Some(img) = crate::pdfdoc::DocImage::from_bytes(dato, None, 0) {
                    self.doc.immagini.push(img);
                    self.blip.insert(s, self.doc.immagini.len() - 1);
                }
            }
        }
        for (slot, k) in [(0usize, 40usize), (1, 41)] {
            let (cps, dati) = Self::plc(self.tab(k), 26);
            for (j, d) in dati.iter().enumerate() {
                let fl = u16_at(d, 20);
                self.spa[slot].push((
                    cps[j],
                    Fspa { spid: i32_at(d, 0), r: [i32_at(d, 4), i32_at(d, 8), i32_at(d, 12), i32_at(d, 16)], bx: ((fl >> 1) & 3) as u8, by: ((fl >> 3) & 3) as u8, wr: ((fl >> 5) & 0xF) as u8, dietro: fl & 0x4000 != 0 },
                ));
            }
        }
    }

    fn forma(&mut self, cp: u32, testata: bool) {
        let slot = testata as usize;
        let base = if testata { self.ccp[0] + self.ccp[1] } else { 0 };
        let Some((_, f)) = self.spa[slot].iter().find(|(c, _)| *c + base == cp).cloned() else { return };
        let contenuto = if let Some(&img) = self.blip.get(&f.spid) {
            MobileContenuto::Immagine(img)
        } else if let Some(fp) = self.forme.get(&f.spid).copied().filter(|p| p.txid.is_some()) {
            let Some(blocchi) = self.casella(f.spid, fp.txid.unwrap_or(0), testata) else { return };
            let fill = fp.filled.then_some(fp.fill).flatten();
            let line = fp.lined.then_some(fp.line).flatten().map(|c| Linea { width: fp.line_w.max(0.25), color: c, dash: Dash::Solid, double: false, space: 0.0 });
            MobileContenuto::Testo(blocchi, fill, line)
        } else {
            return;
        };
        // Le proprietà della forma, se ci sono, valgono più di quelle dell'ancora (FSPA).
        let fp = self.forme.get(&f.spid).copied().unwrap_or_default();
        let bx = match fp.posrelh.unwrap_or(f.bx as u32) {
            0 => Base::Margine,
            1 => Base::Pagina,
            _ => Base::Colonna,
        };
        let by = match fp.posrelv.unwrap_or(f.by as u32) {
            0 => Base::Margine,
            1 => Base::Pagina,
            _ => Base::Paragrafo,
        };
        let x = match fp.posh {
            1 | 4 => Riferimento::Allineato(bx, 0),
            2 => Riferimento::Allineato(bx, 1),
            3 | 5 => Riferimento::Allineato(bx, 2),
            _ => Riferimento::Spostato(bx, tw(f.r[0])),
        };
        let y = match fp.posv {
            1 => Riferimento::Allineato(by, 0),
            2 => Riferimento::Allineato(by, 1),
            3 => Riferimento::Allineato(by, 2),
            _ => Riferimento::Spostato(by, tw(f.r[1])),
        };
        let avvolge = match f.wr {
            1 => Avvolge::SopraSotto,
            3 => Avvolge::Nessuno,
            _ => Avvolge::Lati,
        };
        self.ctx.ancorati.push(Mobile {
            contenuto,
            w: tw(f.r[2] - f.r[0]).max(1.0),
            h: tw(f.r[3] - f.r[1]).max(1.0),
            x,
            y,
            avvolge,
            dietro: f.dietro,
        });
    }

    /// Il testo di una casella: un tratto della storia delle caselle (del corpo o delle intestazioni).
    fn casella(&mut self, spid: i32, txid: u32, testata: bool) -> Option<Vec<Blocco>> {
        let c = &self.ccp;
        let mut base = c[0] + c[1] + c[2] + c[3] + c[4] + c[5];
        if testata {
            base += c[6];
        }
        let (cps, dati) = Self::plc(self.tab(if testata { 58 } else { 56 }), 22);
        // Si cerca la casella con l'id della forma; altrimenti vale il numero nel txid (da 1).
        let k = dati.iter().position(|d| i32_at(d, 14) == spid).or_else(|| ((txid >> 16) as usize).checked_sub(1).filter(|&k| k + 1 < cps.len()))?;
        let (a, z) = (*cps.get(k)?, *cps.get(k + 1)?);
        if z <= a {
            return None;
        }
        let salvato = std::mem::take(&mut self.ctx);
        let campi = std::mem::take(&mut self.campi);
        let link = self.link.take();
        self.storia(base + a, base + z, testata, None, &mut |_, _| {});
        self.chiudi_livelli(0);
        let mut fatto = std::mem::replace(&mut self.ctx, salvato);
        self.campi = campi;
        self.link = link;
        // Le immagini ancorate dentro la casella restano nella casella.
        for m in std::mem::take(&mut fatto.ancorati) {
            fatto.corpo.push(Blocco::Mobile(m));
        }
        let mut blocchi = fatto.corpo;
        if let Some(Blocco::Par(p)) = blocchi.last() {
            if p.runs.is_empty() && blocchi.len() > 1 {
                blocchi.pop();
            }
        }
        Some(blocchi)
    }

    // ---------------------------------------------------------------- proprietà per CP

    fn catena_stile(&self, istd: u16, out: &mut Vec<u16>) {
        let mut cur = Some(istd);
        while let Some(i) = cur {
            if out.len() > 16 || out.contains(&i) {
                break;
            }
            out.push(i);
            cur = self.stili.get(i as usize).and_then(|s| s.as_ref()).and_then(|s| s.base);
        }
        out.reverse();
    }

    fn pap_di(&mut self, cp: u32) -> Pap {
        let fc = self.fc_di(cp).unwrap_or(0);
        let k = self.papx.partition_point(|p| p.1 <= fc);
        let key = if self.papx.get(k).is_some_and(|p| p.0 <= fc) { k } else { usize::MAX };
        let prm = self.prm_di(cp).map(|v| v.to_vec());
        if prm.is_none() {
            if let Some(p) = self.cache_pap.get(&key) {
                return p.clone();
            }
        }
        let grpprl: &[u8] = self.papx.get(key).map_or(&[0, 0], |p| p.2.as_slice());
        let istd = u16_at(grpprl, 0);
        let mut p = Pap { istd, ..Default::default() };
        let mut cat = Vec::new();
        self.catena_stile(istd, &mut cat);
        for i in &cat {
            if let Some(Some(s)) = self.stili.get(*i as usize) {
                applica_pap(&mut p, &s.papx, self.data);
            }
        }
        // Prima l'elenco (dallo stile o dal paragrafo), poi le proprietà dirette che vincono.
        let mut dir = p.clone();
        applica_pap(&mut dir, grpprl.get(2..).unwrap_or(&[]), self.data);
        if let Some(g) = &prm {
            applica_pap(&mut dir, g, self.data);
        }
        if dir.ilfo > 0 && dir.ilfo < 0xF000 {
            if let Some(lvl) = self.lvl(dir.ilfo, dir.ilvl) {
                applica_pap(&mut p, &lvl.papx, self.data);
                applica_pap(&mut p, grpprl.get(2..).unwrap_or(&[]), self.data);
                if let Some(g) = &prm {
                    applica_pap(&mut p, g, self.data);
                }
                p.ilfo = dir.ilfo;
                p.ilvl = dir.ilvl;
            } else {
                p = dir;
            }
        } else {
            p = dir;
        }
        p.istd = istd;
        if prm.is_none() {
            self.cache_pap.insert(key, p.clone());
        }
        p
    }

    fn chp_stile(&self, istd: u16, chp: &mut Chp) {
        let mut cat = Vec::new();
        self.catena_stile(istd, &mut cat);
        for i in cat {
            if let Some(Some(s)) = self.stili.get(i as usize) {
                let b = chp.clone();
                applica_chp(chp, &s.chpx, &b);
            }
        }
    }

    /// Le proprietà di carattere di un CP, dato lo stile del suo paragrafo.
    fn chp_di(&self, cp: u32, istd_par: u16) -> Chp {
        let mut base = self.chp0.clone();
        self.chp_stile(istd_par, &mut base);
        let fc = self.fc_di(cp).unwrap_or(0);
        let k = self.chpx.partition_point(|c| c.1 <= fc);
        let g: &[u8] = self.chpx.get(k).filter(|c| c.0 <= fc).map_or(&[], |c| c.2.as_slice());
        // Lo stile di carattere sta sopra a quello del paragrafo.
        let mut c = base.clone();
        let mut tmp = Chp::default();
        applica_chp(&mut tmp, g, &base);
        if tmp.istd > 0 && tmp.istd != 10 {
            self.chp_stile(tmp.istd, &mut c);
        }
        let b2 = c.clone();
        applica_chp(&mut c, g, &b2);
        if let Some(prm) = self.prm_di(cp) {
            let b3 = c.clone();
            applica_chp(&mut c, prm, &b3);
        }
        c
    }

    fn font(&self, c: &Chp) -> Font {
        Font {
            family: self.fonts.get(c.ftc as usize).map(|f| f.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Times New Roman".into()),
            size: (c.hps.max(2) as f32 / 2.0).min(1638.0),
            bold: c.bold,
            italic: c.italic,
            underline: c.ul != 0 && c.ul != 3,
            double_underline: c.ul == 3,
            strike: c.strike,
            color: c.color.unwrap_or((0, 0, 0)),
            highlight: c.highlight,
            vert: match c.iss {
                1 => 1,
                2 => -1,
                _ => 0,
            },
            caps: c.caps,
            small_caps: c.scaps,
            spacing: tw(c.spacing as i32),
        }
    }

    fn lvl(&self, ilfo: u16, ilvl: u8) -> Option<Lvl> {
        let lfo = self.lfo.get(ilfo as usize - 1)?;
        if let Some((_, _, Some(l))) = lfo.sostituti.iter().find(|(k, _, l)| *k == ilvl && l.is_some()) {
            return Some(l.clone());
        }
        let lista = self.liste.iter().find(|l| l.id == lfo.lsid)?;
        lista.livelli.get((ilvl as usize).min(lista.livelli.len().saturating_sub(1))).cloned()
    }

    fn etichetta(&mut self, p: &Pap, fine: &Chp) -> Option<(String, Font, bool, u8)> {
        if p.ilfo == 0 || p.ilfo >= 0xF000 {
            return None;
        }
        let lfo = self.lfo.get(p.ilfo as usize - 1)?;
        let lsid = lfo.lsid;
        let partenze: Vec<(u8, i32)> = lfo.sostituti.iter().filter_map(|(k, s, _)| s.map(|s| (*k, s))).collect();
        let lista_livelli: Vec<Lvl> = (0..9).filter_map(|k| self.lvl(p.ilfo, k)).collect();
        let lv = lista_livelli.get(p.ilvl as usize)?.clone();
        let cont = self.contatori.entry(lsid).or_insert([i32::MIN; 9]);
        let l = p.ilvl as usize;
        cont[l] = if cont[l] == i32::MIN { partenze.iter().find(|(k, _)| *k as usize == l).map_or(lv.start, |x| x.1) } else { cont[l] + 1 };
        for c in cont.iter_mut().skip(l + 1) {
            *c = i32::MIN;
        }
        let cont = *cont;
        let mut testo = String::new();
        for u in &lv.testo {
            if (*u as usize) < 9 {
                let k = *u as usize;
                let v = if cont[k] == i32::MIN { lista_livelli.get(k).map_or(1, |x| x.start) } else { cont[k] };
                let nfc = lista_livelli.get(k).map_or(0, |x| x.nfc);
                let fmt = match nfc {
                    1 => "upperRoman",
                    2 => "lowerRoman",
                    3 => "upperLetter",
                    4 => "lowerLetter",
                    5 => "ordinal",
                    22 => "decimalZero",
                    _ => "decimal",
                };
                testo.push_str(&super::docx::number_text(v.max(0) as u32, fmt));
            } else if let Some(c) = char::from_u32(*u as u32) {
                testo.push(super::docx::bullet_char(c));
            }
        }
        if lv.nfc == 255 && testo.is_empty() {
            return None;
        }
        let mut c = fine.clone();
        c.ul = 0;
        let b = c.clone();
        applica_chp(&mut c, &lv.chpx, &b);
        let font = self.font(&c);
        let testo: String = testo.chars().map(super::docx::bullet_char).collect();
        Some((testo, font, lv.segue == 0, lv.jc))
    }

    fn paragrafo(&mut self, p: &Pap, fine: &Chp) -> Paragrafo {
        let runs = std::mem::take(&mut self.ctx.runs);
        let et = self.etichetta(p, fine);
        let interlinea = match p.linea {
            (0, _) => Interlinea::Multipla(1.0),
            (v, 1) => Interlinea::Multipla(v as f32 / 240.0),
            (v, _) if v < 0 => Interlinea::Esatta(tw(-(v as i32))),
            (v, _) => Interlinea::Minima(tw(v as i32)),
        };
        let nome = self.stili.get(p.istd as usize).and_then(|s| s.as_ref()).map(|s| s.nome.clone()).unwrap_or_default();
        let low = nome.to_lowercase();
        let titolo = (p.livello < 9).then_some(p.livello + 1).or_else(|| low.strip_prefix("heading ").and_then(|r| r.trim().parse::<u8>().ok()));
        Paragrafo {
            runs,
            allinea: match p.jc {
                1 => Allinea::Centro,
                2 => Allinea::Destra,
                3 | 4 | 5 | 6 | 7 | 8 | 9 => Allinea::Giustifica,
                _ => Allinea::Sinistra,
            },
            rientro_sx: tw(p.sx),
            rientro_dx: tw(p.dx),
            prima_riga: tw(p.prima),
            prima: if p.sb_auto { 14.0 } else { tw(p.sb) },
            dopo: if p.sa_auto { 14.0 } else { tw(p.sa) },
            interlinea,
            insieme_al_seguente: p.keep_next,
            righe_insieme: p.keep,
            salto_prima: p.salto,
            vedove: p.vedove,
            tabs: p
                .tabs
                .iter()
                .filter(|(_, t)| t & 7 != 4)
                .map(|(x, t)| Tab {
                    pos: tw(*x as i32),
                    tipo: match t & 7 {
                        1 => TipoTab::Centro,
                        2 => TipoTab::Destra,
                        3 => TipoTab::Decimale,
                        _ => TipoTab::Sinistra,
                    },
                    leader: match (t >> 3) & 7 {
                        1 | 5 => Some('.'),
                        2 => Some('-'),
                        3 | 4 => Some('_'),
                        _ => None,
                    },
                })
                .collect(),
            etichetta_tab: et.as_ref().is_some_and(|e| e.2),
            etichetta_allinea: match et.as_ref().map_or(0, |e| e.3) {
                1 => Allinea::Centro,
                2 => Allinea::Destra,
                _ => Allinea::Sinistra,
            },
            etichetta: et.map(|(t, f, _, _)| (t, f)),
            titolo: titolo.filter(|l| (1..=9).contains(l)),
            sfondo: p.sfondo,
            bordi: p.bordi,
            font_fine: self.font(fine),
            contestuale: p.contestuale,
            stile: nome,
            segnalibri: Vec::new(),
            auto_prima: p.sb_auto,
            auto_dopo: p.sa_auto,
            interlinea_testo: false,
        }
    }

    // ---------------------------------------------------------------- tabelle (come in rtf.rs)

    fn chiudi_livelli(&mut self, n: usize) {
        while self.ctx.livelli.len() > n {
            let lv = self.ctx.livelli.pop().unwrap();
            let mut resto = lv.cella;
            let tab = tabella(lv.righe);
            let dest = match self.ctx.livelli.last_mut() {
                Some(up) => &mut up.cella,
                None => &mut self.ctx.corpo,
            };
            if let Some(t) = tab {
                dest.push(Blocco::Tab(t));
            }
            dest.append(&mut resto);
        }
    }

    fn metti(&mut self, b: Blocco, livello: usize) {
        if livello < self.ctx.livelli.len() {
            self.chiudi_livelli(livello);
        }
        while self.ctx.livelli.len() < livello {
            self.ctx.livelli.push(Livello::default());
        }
        for m in std::mem::take(&mut self.ctx.ancorati) {
            match self.ctx.livelli.last_mut() {
                Some(lv) if livello > 0 => lv.cella.push(Blocco::Mobile(m)),
                _ => self.ctx.corpo.push(Blocco::Mobile(m)),
            }
        }
        match self.ctx.livelli.last_mut() {
            Some(lv) if livello > 0 => lv.cella.push(b),
            _ => self.ctx.corpo.push(b),
        }
    }

    fn fine_cella(&mut self, livello: usize) {
        if self.ctx.livelli.len() > livello {
            self.chiudi_livelli(livello);
        }
        if let Some(lv) = self.ctx.livelli.get_mut(livello - 1) {
            let c = std::mem::take(&mut lv.cella);
            lv.celle.push(c);
        }
    }

    fn fine_riga(&mut self, livello: usize, tap: Tap) {
        if self.ctx.livelli.len() > livello {
            self.chiudi_livelli(livello);
        }
        while self.ctx.livelli.len() < livello {
            self.ctx.livelli.push(Livello::default());
        }
        let lv = &mut self.ctx.livelli[livello - 1];
        // Testo dopo l'ultima cella (non dovrebbe esserci): va nell'ultima cella.
        if !lv.cella.is_empty() {
            let c = std::mem::take(&mut lv.cella);
            lv.celle.push(c);
        }
        let celle = std::mem::take(&mut lv.celle);
        if !celle.is_empty() {
            lv.righe.push((tap, celle));
        }
    }

    // ---------------------------------------------------------------- il testo

    fn livello(p: &Pap) -> usize {
        if p.itap > 0 {
            p.itap as usize
        } else if p.in_tab {
            1
        } else {
            0
        }
    }

    fn scrivi(&mut self, s: &str, c: &Chp) {
        if s.is_empty() || !self.visibile() {
            return;
        }
        let font = self.font(c);
        let link = self.link.clone();
        if let Some(Run { pezzo: Pezzo::Testo(t), font: f, link: l }) = self.ctx.runs.last_mut() {
            if *f == font && *l == link {
                t.push_str(s);
                return;
            }
        }
        self.ctx.runs.push(Run { pezzo: Pezzo::Testo(s.to_string()), font, link });
    }

    fn pezzo(&mut self, p: Pezzo, c: &Chp) {
        if !self.visibile() {
            return;
        }
        let font = self.font(c);
        self.ctx.runs.push(Run { pezzo: p, font, link: self.link.clone() });
    }

    fn visibile(&self) -> bool {
        self.campi.iter().all(|c| c.risultato && !c.nascosto)
    }

    /// Un tratto di testo (una storia: corpo, intestazione, nota) nel contesto corrente.
    fn storia(&mut self, da: u32, a: u32, testata: bool, sezioni: Option<&[u32]>, fine_sezione: &mut dyn FnMut(&mut Self, u32)) {
        self.profondita += 1;
        if self.profondita > 8 {
            self.profondita -= 1;
            return;
        }
        let a = a.min(self.testo.len() as u32);
        let mut cp = da;
        let mut inizio = da;
        while cp < a {
            let ch = self.testo[cp as usize];
            let fine_par = ch == 0x0D || ch == 0x07 || (ch == 0x0C && sezioni.is_some_and(|s| s.contains(&(cp + 1))));
            if !fine_par {
                cp += 1;
                continue;
            }
            // Un paragrafo: [inizio, cp] con il segno in cp.
            let p = self.pap_di(cp);
            self.caratteri(inizio, cp, &p, testata);
            let fine = self.chp_di(cp, p.istd);
            let liv = Self::livello(&p);
            match ch {
                0x07 if p.ttp || (p.inner_ttp && liv > 1) => {
                    self.ctx.runs.clear();
                    let tap = p.tap.clone().unwrap_or_default();
                    self.fine_riga(liv.max(1), tap);
                }
                0x07 => {
                    let par = self.paragrafo(&p, &fine);
                    self.metti(Blocco::Par(par), liv.max(1));
                    self.fine_cella(liv.max(1));
                }
                _ if p.inner_ttp && liv > 1 => {
                    self.ctx.runs.clear();
                    let tap = p.tap.clone().unwrap_or_default();
                    self.fine_riga(liv, tap);
                }
                _ if p.inner_cell && liv > 1 => {
                    let par = self.paragrafo(&p, &fine);
                    self.metti(Blocco::Par(par), liv);
                    self.fine_cella(liv);
                }
                _ => {
                    let par = self.paragrafo(&p, &fine);
                    self.metti(Blocco::Par(par), liv);
                }
            }
            cp += 1;
            inizio = cp;
            if ch == 0x0C || sezioni.is_some_and(|s| s.contains(&cp)) {
                fine_sezione(self, cp);
            }
        }
        if inizio < a {
            let p = self.pap_di(a.saturating_sub(1));
            self.caratteri(inizio, a, &p, testata);
            if !self.ctx.runs.is_empty() {
                let fine = self.chp_di(a.saturating_sub(1), p.istd);
                let par = self.paragrafo(&p, &fine);
                self.metti(Blocco::Par(par), Self::livello(&p));
            }
        }
        self.profondita -= 1;
    }

    /// Word 6: il byte del testo come lettera, con la tabella del font del tratto.
    fn lettera_w6(&self, ch: u16, c: &Chp) -> u16 {
        if !self.w6 || ch < 0x20 || ch > 0xFF {
            return ch;
        }
        let t = self.cp8_font.get(c.ftc as usize).unwrap_or(&self.cp8);
        t.get(ch as usize).map_or(ch, |u| *u as u32 as u16)
    }

    fn caratteri(&mut self, da: u32, a: u32, p: &Pap, testata: bool) {
        let mut cp = da;
        while cp < a {
            // Un tratto con le stesse proprietà di carattere.
            let fc = self.fc_di(cp).unwrap_or(0);
            let k = self.chpx.partition_point(|c| c.1 <= fc);
            let fc_fine = self.chpx.get(k).filter(|c| c.0 <= fc).map_or(u32::MAX, |c| c.1);
            let pz = self.pezzi.partition_point(|x| x.1 <= cp);
            let pz_fine = self.pezzi.get(pz).map_or(a, |x| x.1);
            let c = self.chp_di(cp, p.istd);
            let mut fine = cp + 1;
            while fine < a && fine < pz_fine && self.fc_di(fine).is_some_and(|f| f < fc_fine) {
                fine += 1;
            }
            if c.deleted || (c.hidden && !c.spec) {
                // Il testo nascosto o cancellato non si vede, ma i campi vanno seguiti lo stesso.
                for x in cp..fine {
                    let ch = self.testo[x as usize];
                    if matches!(ch, 0x13..=0x15) {
                        self.campo(ch, &c);
                    }
                }
                cp = fine;
                continue;
            }
            let mut buf = String::new();
            for x in cp..fine {
                let ch = self.lettera_w6(self.testo[x as usize], &c);
                let speciale = matches!(ch, 0x00..=0x1F) || (c.spec && matches!(ch, 0x01 | 0x02 | 0x05 | 0x08 | 0x28));
                if !speciale {
                    if self.campi.last().is_some_and(|f| !f.risultato) {
                        if let Some(f) = self.campi.last_mut() {
                            f.istr.push(char::from_u32(ch as u32).unwrap_or(' '));
                        }
                        continue;
                    }
                    if let Some(u) = char::from_u32(ch as u32) {
                        buf.push(u);
                    }
                    continue;
                }
                if !buf.is_empty() {
                    self.scrivi(&std::mem::take(&mut buf), &c);
                }
                match ch {
                    0x09 => self.pezzo(Pezzo::Tab, &c),
                    0x0B => self.pezzo(Pezzo::ACapo, &c),
                    0x0C => self.pezzo(Pezzo::SaltoPagina, &c),
                    0x0E => self.pezzo(Pezzo::SaltoColonna, &c),
                    0x1E => self.scrivi("\u{2011}", &c),
                    0x1F => self.scrivi("\u{ad}", &c),
                    0x13..=0x15 => self.campo(ch, &c),
                    0x01 if c.spec && self.campi.last().is_some_and(|f| !f.risultato) => {
                        // I dati del campo modulo (FFData): per le caselle di spunta, lo stato.
                        let modulo = self.dati_modulo(&c);
                        if let Some(f) = self.campi.last_mut() {
                            match modulo {
                                Some(Ok(v)) => f.casella = Some(v),
                                Some(Err(t)) => f.scelta = Some(t),
                                None => {}
                            }
                        }
                    }
                    0x01 if c.spec && self.visibile() => {
                        if let Some((img, w, h)) = self.immagine_cp(&c) {
                            self.pezzo(Pezzo::Immagine { img, w, h }, &c);
                        }
                    }
                    0x08 if c.spec && self.visibile() => self.forma(x, testata),
                    0x02 if c.spec => self.nota(x, &c),
                    0x28 if c.spec => {
                        if let Some((_, sym)) = c.symbol {
                            let s = char::from_u32(sym as u32).map(super::docx::bullet_char).unwrap_or('•');
                            self.scrivi(&s.to_string(), &c);
                        } else {
                            self.scrivi("(", &c);
                        }
                    }
                    0x28 => self.scrivi("(", &c),
                    _ => {}
                }
            }
            if !buf.is_empty() {
                self.scrivi(&buf, &c);
            }
            cp = fine;
        }
    }

    /// I dati di un campo modulo (FFData, dopo l'intestazione NilPICF di 0x44 byte): lo stato di una
    /// casella di spunta (Ok) o la voce scelta di un elenco a tendina (Err).
    fn dati_modulo(&self, c: &Chp) -> Option<Result<bool, String>> {
        let at = c.pic? as usize + 0x44;
        let d = self.data;
        if u32_at(d, at) != 0xFFFF_FFFF {
            return None;
        }
        let bits = u16_at(d, at + 4);
        let tipo = bits & 3;
        if tipo == 0 {
            return None;
        }
        let ires = ((bits >> 2) & 0x1F) as usize;
        // Xstz: lunghezza, caratteri e lo zero finale.
        let salta = |i: usize| i + 2 + u16_at(d, i) as usize * 2 + 2;
        let mut i = salta(at + 10); // dopo il nome
        let wdef = u16_at(d, i) as usize;
        i += 2;
        if tipo == 1 {
            return Some(Ok(if ires == 25 { wdef != 0 } else { ires == 1 }));
        }
        for _ in 0..5 {
            i = salta(i); // formato, aiuto, stato, macro di entrata e di uscita
        }
        // L'elenco delle voci (STTB): 0xFFFF, quante, byte in più.
        if u16_at(d, i) != 0xFFFF {
            return None;
        }
        let n = u16_at(d, i + 2) as usize;
        let extra = u16_at(d, i + 4) as usize;
        let mut j = i + 6;
        let scelta = if ires == 25 { wdef } else { ires };
        for k in 0..n.min(1000) {
            let cch = u16_at(d, j) as usize;
            if k == scelta {
                let u: Vec<u16> = (0..cch).map(|q| u16_at(d, j + 2 + q * 2)).collect();
                return Some(Err(String::from_utf16_lossy(&u)));
            }
            j += 2 + cch * 2 + extra;
        }
        None
    }

    fn campo(&mut self, ch: u16, c: &Chp) {
        match ch {
            0x13 => self.campi.push(Campo { istr: String::new(), risultato: false, nascosto: false, link_prima: None, casella: None, scelta: None }),
            0x14 => {
                let fuori_vis = self.campi.len() <= 1 || self.campi[..self.campi.len() - 1].iter().all(|f| f.risultato && !f.nascosto);
                let Some(f) = self.campi.last_mut() else { return };
                f.risultato = true;
                let istr = f.istr.trim().to_string();
                let parola = istr.split_whitespace().next().unwrap_or("").to_uppercase();
                match parola.as_str() {
                    "PAGE" | "NUMPAGES" | "SECTIONPAGES" => {
                        f.nascosto = true;
                        if fuori_vis {
                            let p = if parola == "PAGE" { Pezzo::NumPagina } else { Pezzo::NumPagine };
                            let font = self.font(c);
                            self.ctx.runs.push(Run { pezzo: p, font, link: self.link.clone() });
                        }
                    }
                    "HYPERLINK" => {
                        let mut parti = istr.split('"').skip(1).step_by(2);
                        let url = parti.next().unwrap_or("").to_string();
                        let target = if let Some(i) = istr.find("\\l") {
                            let a = istr[i + 2..].trim().trim_matches('"').split('"').next().unwrap_or("").to_string();
                            if istr.split_whitespace().nth(1).is_some_and(|t| t.starts_with('\\')) { Target::Anchor(a) } else { Target::Uri(url) }
                        } else {
                            Target::Uri(url)
                        };
                        f.link_prima = Some(self.link.replace(target));
                    }
                    _ => {}
                }
            }
            _ => {
                if let Some(f) = self.campi.pop() {
                    if let Some(prima) = f.link_prima {
                        self.link = prima;
                    }
                    // Un campo senza risultato (solo istruzione): PAGE scritto senza separatore.
                    if !f.risultato {
                        let parola = f.istr.split_whitespace().next().unwrap_or("").to_uppercase();
                        if parola == "PAGE" && self.visibile() {
                            self.pezzo(Pezzo::NumPagina, c);
                        }
                    }
                    let tipo = f.istr.trim_start().to_uppercase();
                    if tipo.starts_with("FORMCHECKBOX") && self.visibile() {
                        self.scrivi(if f.casella == Some(true) { "\u{2612}" } else { "\u{2610}" }, c);
                    }
                    if tipo.starts_with("FORMDROPDOWN") && self.visibile() {
                        if let Some(t) = &f.scelta {
                            self.scrivi(t, c);
                        }
                    }
                }
            }
        }
    }

    fn nota(&mut self, cp: u32, c: &Chp) {
        // Dentro una nota: il suo numero.
        if let Some(n) = self.nota_numero.clone() {
            self.scrivi(&n, c);
            return;
        }
        let (fine, i) = if let Some(i) = self.note_rif[0].iter().position(|r| *r == cp) {
            (false, i)
        } else if let Some(i) = self.note_rif[1].iter().position(|r| *r == cp) {
            (true, i)
        } else {
            return;
        };
        let (txt, base) = if fine { (self.tab(47), self.ccp[0] + self.ccp[1] + self.ccp[2] + self.ccp[3] + self.ccp[4]) } else { (self.tab(3), self.ccp[0]) };
        let (cps, _) = Self::plc(txt, 0);
        let (Some(&a), Some(&z)) = (cps.get(i), cps.get(i + 1)) else { return };
        let n = if fine { self.doc.note_fine.len() + 1 } else { self.doc.note.len() + 1 };
        let numero = if fine { super::docx::number_text(n as u32, "lowerRoman") } else { n.to_string() };
        let mut cc = c.clone();
        cc.iss = 1;
        let font = self.font(&cc);
        if fine {
            self.doc.note_fine.push((numero.clone(), Vec::new()));
        } else {
            self.doc.note.push((numero.clone(), Vec::new()));
        }
        self.ctx.runs.push(Run { pezzo: if fine { Pezzo::NotaFine(n - 1) } else { Pezzo::Nota(n - 1) }, font, link: None });
        let salvato = std::mem::take(&mut self.ctx);
        let campi = std::mem::take(&mut self.campi);
        let link = self.link.take();
        self.nota_numero = Some(numero);
        self.storia(base + a, base + z, false, None, &mut |_, _| {});
        self.chiudi_livelli(0);
        self.nota_numero = None;
        let fatto = std::mem::replace(&mut self.ctx, salvato);
        self.campi = campi;
        self.link = link;
        let mut blocchi = fatto.corpo;
        // L'ultimo paragrafo vuoto della nota (il segno di fine storia) non serve.
        if let Some(Blocco::Par(p)) = blocchi.last() {
            if p.runs.is_empty() && blocchi.len() > 1 {
                blocchi.pop();
            }
        }
        if fine {
            self.doc.note_fine[n - 1].1 = blocchi;
        } else {
            self.doc.note[n - 1].1 = blocchi;
        }
    }

    /// Un'intestazione o un piè di pagina (una storia della PlcfHdd).
    fn testata(&mut self, i: usize) -> Option<Vec<Blocco>> {
        let (cps, _) = Self::plc(self.tab(11), 0);
        let (a, z) = (*cps.get(i)?, *cps.get(i + 1)?);
        if z <= a {
            return None;
        }
        let base = self.ccp[0] + self.ccp[1];
        let salvato = std::mem::take(&mut self.ctx);
        let campi = std::mem::take(&mut self.campi);
        self.storia(base + a, base + z, true, None, &mut |_, _| {});
        self.chiudi_livelli(0);
        self.campi = campi;
        let mut fatto = std::mem::replace(&mut self.ctx, salvato).corpo;
        // La storia finisce con un segno di paragrafo in più.
        if fatto.len() > 1 {
            if let Some(Blocco::Par(p)) = fatto.last() {
                if p.runs.is_empty() {
                    fatto.pop();
                }
            }
        }
        Some(fatto)
    }
}

/// Dalle righe (con il loro TAP) alla tabella sulla griglia comune.
fn tabella(righe: Vec<(Tap, Vec<Vec<Blocco>>)>) -> Option<Tabella> {
    if righe.is_empty() {
        return None;
    }
    let mut x: Vec<i32> = righe.iter().flat_map(|(t, _)| t.centri.iter().map(|c| *c as i32)).collect();
    x.sort_unstable();
    x.dedup_by(|a, b| (*a - *b).abs() <= 2);
    if x.len() < 2 {
        let n = righe.iter().map(|r| r.1.len()).max().unwrap_or(1).max(1) as i32;
        x = (0..=n).map(|k| k * 1440).collect();
    }
    let idx = |v: i32| x.iter().position(|g| (g - v).abs() <= 2).unwrap_or_else(|| x.iter().rposition(|g| *g <= v).unwrap_or(0));
    let griglia: Vec<f32> = x.windows(2).map(|w| tw(w[1] - w[0]).max(1.0)).collect();
    let ncols = griglia.len();
    let t0 = righe[0].0.clone();
    let mut out = Vec::new();
    for (t, celle) in righe {
        let mut row: Vec<Cella> = Vec::new();
        let mut col = 0usize;
        for (k, mut blocchi) in celle.into_iter().enumerate() {
            let tc = t.tc.get(k).cloned().unwrap_or_default();
            let (a, b) = match (t.centri.get(k), t.centri.get(k + 1)) {
                (Some(a), Some(b)) => (idx(*a as i32), idx(*b as i32)),
                _ => (col, col + 1),
            };
            let span = if k == 0 { b.max(1) } else { b.saturating_sub(a.max(col)).max(1) }.min(ncols.saturating_sub(col).max(1));
            if tc.merged && !tc.first {
                if let Some(prev) = row.last_mut() {
                    prev.span += span;
                    col += span;
                    continue;
                }
            }
            if blocchi.is_empty() {
                blocchi.push(Blocco::Par(Paragrafo::default()));
            }
            let g = tw(t.gaph as i32);
            let pad = [tc.pad[0].or(t.pad[0]).unwrap_or(g), tc.pad[1].or(t.pad[1]).unwrap_or(g), tc.pad[2].or(t.pad[2]).unwrap_or(0.0), tc.pad[3].or(t.pad[3]).unwrap_or(0.0)];
            let rb = &t.bordi;
            // Bordi del TAP: sopra, sinistra, sotto, destra, dentro orizzontale, dentro verticale.
            let bordi = [tc.bordi[0].or(rb[0]).or(rb[4]), tc.bordi[1].or(rb[2]).or(rb[4]), tc.bordi[2].or(if col == 0 { rb[1] } else { rb[5] }), tc.bordi[3].or(rb[3]).or(rb[5])];
            row.push(Cella { blocchi, span, vmerge: if tc.vrestart { Some(true) } else if tc.vmerged { Some(false) } else { None }, sfondo: tc.sfondo, bordi, valign: tc.valign, margini: pad, larghezza: None });
            col += span;
        }
        out.push(Riga { celle: row, altezza: tw(t.altezza as i32), intestazione: t.intestazione, non_dividere: t.non_dividere });
    }
    let first_pad = t0.tc.first().and_then(|c| c.pad[0]).or(t0.pad[0]).unwrap_or(tw(t0.gaph as i32));
    Some(Tabella { griglia, righe: out, allinea: t0.jc, rientro: tw(x[0]) + first_pad, relativa: None })
}

pub fn leggi_doc(bytes: &[u8]) -> Result<Documento, String> {
    let mut cf = super::apri_ole(bytes).map_err(|e| format!("DOC non letto: {e}"))?;
    let mut leggi = |nome: &str| -> Option<Vec<u8>> {
        let mut v = Vec::new();
        cf.open_stream(nome).ok()?.read_to_end(&mut v).ok()?;
        Some(v)
    };
    let word = leggi("/WordDocument").ok_or("DOC senza testo")?;
    let ident = u16_at(&word, 0);
    let w6 = ident == 0xA5DC && word.len() >= 0x200;
    if word.len() < 0x200 || (ident != 0xA5EC && !w6) {
        return Err(if ident == 0xA59B || ident == 0xA59C { "documento di Word 2 per Windows: formato troppo vecchio".into() } else { "non è un documento di Word".into() });
    }
    let n_fib = u16_at(&word, 2);
    let flags = u16_at(&word, 0x0A);
    if flags & 0x0100 != 0 {
        return Err("il documento è protetto da password".into());
    }
    if !w6 && n_fib < 101 {
        return Err("documento di Word troppo vecchio".into());
    }
    let (table, data, ccp, fib, cpn_bte) = if w6 {
        // Word 6/95: FIB fisso; le coppie fc/lcb da 0x58, con 10 byte di altri campi dopo la 38ª.
        let ccp = [0, 1, 2, 3, 4, 5, 6, 7].map(|k| u32_at(&word, 0x34 + k * 4));
        let fib: Vec<(u32, u32)> = (0..64)
            .map(|k| {
                let at = if k < 38 { 0x58 + k * 8 } else { 0x192 + (k - 38) * 8 };
                if at + 8 <= 0x2A0.min(word.len()) {
                    (u32_at(&word, at), u32_at(&word, at + 4))
                } else {
                    (0, 0)
                }
            })
            .collect();
        (word.clone(), word.clone(), ccp, fib, [u16_at(&word, 0x18E) as usize, u16_at(&word, 0x190) as usize])
    } else {
        let table = leggi(if flags & 0x0200 != 0 { "/1Table" } else { "/0Table" }).ok_or("DOC senza tabella")?;
        let data = leggi("/Data").unwrap_or_default();
        // FIB: le parti a misura variabile.
        let mut at = 32usize;
        let csw = u16_at(&word, at) as usize;
        at += 2 + csw * 2;
        let cslw = u16_at(&word, at) as usize;
        let lw = at + 2;
        at += 2 + cslw * 4;
        let ccp = [3, 4, 5, 6, 7, 8, 9, 10].map(|k| if k < cslw { u32_at(&word, lw + k * 4) } else { 0 });
        let npair = u16_at(&word, at) as usize;
        at += 2;
        let fib = (0..npair).map(|k| (u32_at(&word, at + k * 8), u32_at(&word, at + k * 8 + 4))).collect();
        (table, data, ccp, fib, [0, 0])
    };
    let mut l = Lettore {
        word: &word,
        table: &table,
        data: &data,
        fib,
        ccp,
        testo: Vec::new(),
        pezzi: Vec::new(),
        prc: Vec::new(),
        chpx: Vec::new(),
        papx: Vec::new(),
        stili: Vec::new(),
        chp0: Chp { hps: 20, ..Default::default() },
        fonts: Vec::new(),
        liste: Vec::new(),
        lfo: Vec::new(),
        contatori: HashMap::new(),
        spa: [Vec::new(), Vec::new()],
        blip: HashMap::new(),
        forme: HashMap::new(),
        doc: Documento::default(),
        ctx: Contesto::default(),
        link: None,
        campi: Vec::new(),
        nota_numero: None,
        note_rif: [Vec::new(), Vec::new()],
        cache_pap: HashMap::new(),
        profondita: 0,
        w6,
        cp8: if w6 { caratteri_w6(u16_at(&word, 6), 0) } else { Vec::new() },
        cp8_font: Vec::new(),
        cpn_bte,
        testate_w6: 0,
    };
    l.leggi_pezzi()?;
    l.leggi_fkp();
    l.leggi_stili();
    l.leggi_font();
    if w6 {
        let lid = u16_at(&word, 6);
        l.cp8_font = l.fonts.iter().map(|f| caratteri_w6(lid, f.1)).collect();
    }
    if !w6 {
        l.leggi_liste();
        l.leggi_disegni();
    } else {
        // Le intestazioni cominciano dopo i separatori delle note che il documento ha.
        l.testate_w6 = l.tab(31).get(1).map_or(0, |b| b.count_ones() as usize);
    }
    for (slot, k) in [(0usize, 2usize), (1, 46)] {
        let (cps, _) = Lettore::plc(l.tab(k), 2);
        l.note_rif[slot] = cps.iter().take(cps.len().saturating_sub(1)).copied().collect();
    }
    let dop = l.tab(31);
    l.doc.pari_dispari = dop.first().is_some_and(|b| b & 1 != 0);
    l.doc.tab_predefinita = if dop.len() >= 12 { tw(u16_at(dop, 10) as i32).max(1.0) } else { 36.0 };
    // Le sezioni: dove finiscono e le loro proprietà (SEPX).
    let (sed_cp, sed) = Lettore::plc(l.tab(6), 12);
    let sed: Vec<Vec<u8>> = sed.iter().map(|d| d.to_vec()).collect();
    let fine_sez: Vec<u32> = sed_cp.iter().skip(1).copied().collect();
    let mut sezioni: Vec<Sezione> = Vec::new();
    let mut testate_prec: [Option<Vec<Blocco>>; 6] = Default::default();
    let chiudi = |l: &mut Lettore, k: usize, sezioni: &mut Vec<Sezione>, testate_prec: &mut [Option<Vec<Blocco>>; 6]| {
        l.chiudi_livelli(0);
        for m in std::mem::take(&mut l.ctx.ancorati) {
            l.ctx.corpo.push(Blocco::Mobile(m));
        }
        let mut s = Sezione { w: 612.0, h: 792.0, margini: [90.0, 90.0, 72.0, 72.0], dist_testa: 36.0, dist_piede: 36.0, ..Default::default() };
        let mut salto = 2;
        let mut orizzontale = false;
        let mut ihdt = 0u8;
        if let Some(d) = sed.get(k) {
            let fc = i32_at(d, 2);
            if fc >= 0 {
                let cb = u16_at(l.word, fc as usize) as usize;
                let g = l.word.get(fc as usize + 2..fc as usize + 2 + cb).unwrap_or(&[]);
                let g6;
                let g = if l.w6 {
                    g6 = da_w6(g);
                    g6.as_slice()
                } else {
                    g
                };
                for (op, v) in sprms(g) {
                    let b0 = v.first().copied().unwrap_or(0);
                    match op {
                        0xB01F => s.w = tw(u16_at(v, 0) as i32),
                        0xB020 => s.h = tw(u16_at(v, 0) as i32),
                        0xB021 => s.margini[0] = tw(u16_at(v, 0) as i32),
                        0xB022 => s.margini[1] = tw(u16_at(v, 0) as i32),
                        0x9023 => s.margini[2] = tw((i16_at(v, 0) as i32).abs()),
                        0x9024 => s.margini[3] = tw((i16_at(v, 0) as i32).abs()),
                        0xB017 => s.dist_testa = tw(u16_at(v, 0) as i32),
                        0xB018 => s.dist_piede = tw(u16_at(v, 0) as i32),
                        0x500B => s.colonne = (u16_at(v, 0) + 1).min(20) as u8,
                        0x900C => s.spazio_colonne = tw(u16_at(v, 0) as i32),
                        0x300A => s.prima_diversa = b0 != 0,
                        0x3009 => salto = b0,
                        0x501C => s.primo_numero = Some(u16_at(v, 0) as u32),
                        0x3011 => {
                            if b0 == 0 {
                                s.primo_numero = None;
                            }
                        }
                        0x301D => orizzontale = b0 == 2,
                        SPRM_IHDT_W6 => ihdt = b0,
                        _ => {}
                    }
                }
            }
        }
        if orizzontale && s.w < s.h {
            std::mem::swap(&mut s.w, &mut s.h);
        }
        s.pagina_nuova = salto != 0 || sezioni.is_empty();
        s.parita = match salto {
            3 => 2,
            4 => 1,
            _ => 0,
        };
        // Intestazioni e piè: 6 per sezione dopo i 6 separatori delle note; vuote = come la sezione prima.
        let mut t: [Option<Vec<Blocco>>; 6] = Default::default();
        for (j, slot) in t.iter_mut().enumerate() {
            let storia = if l.w6 {
                // Word 6: ci sono solo le intestazioni che la sezione dichiara, una dopo l'altra.
                if ihdt & (1 << j) == 0 {
                    None
                } else {
                    let n = l.testate_w6;
                    l.testate_w6 += 1;
                    l.testata(n)
                }
            } else {
                l.testata(6 + k * 6 + j)
            };
            *slot = storia.or_else(|| testate_prec[j].clone());
            testate_prec[j] = slot.clone();
        }
        let [hp, hd, fp, fd, hf, ff] = t;
        s.testa = Intestazioni { normale: hd, prima: if s.prima_diversa { hf } else { None }, pari: if l.doc.pari_dispari { hp } else { None } };
        s.piede = Intestazioni { normale: fd, prima: if s.prima_diversa { ff } else { None }, pari: if l.doc.pari_dispari { fp } else { None } };
        s.blocchi = std::mem::take(&mut l.ctx.corpo);
        sezioni.push(s);
    };
    // Sezione per sezione: le proprietà (carta, margini, intestazioni) cambiano a ogni tratto.
    let fine_testo = l.ccp[0];
    let mut da = 0;
    for (k, &a) in fine_sez.iter().enumerate() {
        let a = a.min(fine_testo);
        if a <= da && k + 1 < fine_sez.len() {
            continue;
        }
        l.storia(da, a, false, Some(&fine_sez), &mut |_, _| {});
        chiudi(&mut l, k, &mut sezioni, &mut testate_prec);
        da = a;
    }
    if da < fine_testo {
        l.storia(da, fine_testo, false, None, &mut |_, _| {});
        let k = fine_sez.len().saturating_sub(1);
        chiudi(&mut l, k, &mut sezioni, &mut testate_prec);
    }
    if sezioni.is_empty() {
        sezioni.push(Sezione::default());
    }
    let mut doc = std::mem::take(&mut l.doc);
    doc.sezioni = sezioni;
    Ok(doc)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn titolo_colori_tabella_e_nota() {
        // Scritto da LibreOffice come Word 97-2003.
        let doc = leggi_doc(include_bytes!("../../tests/fixtures/documento.doc")).unwrap();
        let sez = &doc.sezioni[0];
        assert!((sez.w - 595.3).abs() < 0.5, "{}", sez.w);
        let pars: Vec<&Paragrafo> = sez.blocchi.iter().filter_map(|b| if let Blocco::Par(p) = b { Some(p) } else { None }).collect();
        assert_eq!(pars[0].testo(), "Titolo di prova");
        assert_eq!(pars[0].allinea, Allinea::Centro);
        assert!(pars[0].runs[0].font.bold);
        assert!(pars[1].testo().starts_with("Testo rosso e corsivo con nota"), "{}", pars[1].testo());
        assert!(pars[1].runs.iter().any(|r| r.font.color == (200, 0, 0)));
        assert!(pars[1].runs.iter().any(|r| r.font.italic));
        let tab = sez.blocchi.iter().find_map(|b| if let Blocco::Tab(t) = b { Some(t) } else { None }).expect("tabella");
        assert_eq!(tab.righe.len(), 2);
        assert_eq!(tab.righe[1].celle.len(), 2);
        let Blocco::Par(c) = &tab.righe[1].celle[1].blocchi[0] else { panic!() };
        assert_eq!(c.testo(), "B2");
        assert_eq!(doc.note.len(), 1);
        assert!(pars.iter().any(|p| p.testo() == "Fine."));
    }

    #[test]
    fn legge_word6() {
        // Dai file di prova di Apache POI (Word 6.0, testo a 8 bit, licenza Apache 2.0).
        let doc = leggi_doc(include_bytes!("../../tests/fixtures/word6.doc")).unwrap();
        let testi: Vec<String> = doc.sezioni[0].blocchi.iter().filter_map(|b| if let Blocco::Par(p) = b { Some(p.testo()) } else { None }).collect();
        assert!(testi.iter().any(|t| t == "The quick brown fox jumps over the lazy dog"), "{testi:?}");
    }

    #[test]
    fn word6_tabella_con_ombra() {
        // sprmTDefTable (190) con una cella: la misura conta un byte in più; poi sprmTDefTableShd (191).
        let mut g = vec![190, 16, 0, 1, 0x10, 0, 0x20, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        g.extend_from_slice(&[191, 2, 0x00, 0x15]);
        let w97 = da_w6(&g);
        let ops: Vec<u16> = sprms(&w97).iter().map(|s| s.0).collect();
        assert_eq!(ops, vec![0xD608, 0xD609]);
        let shd = sprms(&w97).into_iter().find(|s| s.0 == 0xD609).unwrap().1;
        assert_eq!(shd80(u16_at(shd, 0)), Some((191, 191, 191)));
    }

    #[test]
    fn rifiuta_file_non_word() {
        assert!(leggi_doc(b"non sono un doc").is_err());
    }
}
