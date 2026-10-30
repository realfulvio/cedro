//! Le scansioni lette per le tabelle: la pagina si raddrizza e se ne trovano le linee di griglia.

use image::{DynamicImage, GrayImage, RgbImage};

/// Allarga i grigi della pagina tra il 2° e il 98° percentile: una foto sfocata o grigia si legge molto meglio.
pub fn enhance(image: &DynamicImage) -> DynamicImage {
    let mut gray = image.to_luma8();
    let mut hist = [0u64; 256];
    for p in gray.pixels() {
        hist[p.0[0] as usize] += 1;
    }
    let total: u64 = hist.iter().sum();
    let at = |frac: f64| -> u8 {
        let target = (total as f64 * frac) as u64;
        let mut acc = 0;
        for (v, n) in hist.iter().enumerate() {
            acc += n;
            if acc >= target {
                return v as u8;
            }
        }
        255
    };
    let (lo, hi) = (at(0.02) as f32, at(0.98) as f32);
    if hi - lo < 30.0 {
        return DynamicImage::ImageLuma8(gray);
    }
    for p in gray.pixels_mut() {
        p.0[0] = (((p.0[0] as f32 - lo) / (hi - lo)).clamp(0.0, 1.0) * 255.0) as u8;
    }
    DynamicImage::ImageLuma8(gray)
}

/// Una linea della griglia: dove sta (`pos`, in pixel) e da dove a dove va.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub pos: f32,
    pub from: f32,
    pub to: f32,
}

/// Le linee orizzontali e verticali trovate nella pagina (coordinate dell'immagine letta).
#[derive(Clone, Debug, Default)]
pub struct Rules {
    pub horizontal: Vec<Rule>,
    pub vertical: Vec<Rule>,
}

/// Sotto questo grigio un pixel conta come inchiostro per il raddrizzamento.
const DARK: u8 = 150;
/// Per le linee di griglia si tollera di più: in una foto sfocata una linea sottile diventa grigia.
const LINE_DARK: u8 = 195;

/// L'angolo (in gradi) da dare alla pagina per rimetterne le righe orizzontali:
/// si cerca dove il profilo delle righe di testo è più «a picchi».
fn skew_angle(gray: &GrayImage) -> f32 {
    let width = 900u32.min(gray.width());
    let small = image::imageops::resize(gray, width, (gray.height() as f32 * width as f32 / gray.width().max(1) as f32).max(1.0) as u32, image::imageops::FilterType::Triangle);
    let dark: Vec<(f32, f32)> = small.enumerate_pixels().filter(|(_, _, p)| p.0[0] < DARK).map(|(x, y, _)| (x as f32, y as f32)).collect();
    if dark.len() < 200 {
        return 0.0;
    }
    let bins = (small.height() + small.width()) as usize + 8;
    let offset = small.width() as f32 * 0.2;
    let score = |deg: f32| -> f64 {
        let (s, c) = deg.to_radians().sin_cos();
        let mut hist = vec![0u32; bins];
        for (x, y) in &dark {
            let v = y * c + x * s + offset;
            if v >= 0.0 && (v as usize) < bins {
                hist[v as usize] += 1;
            }
        }
        hist.iter().map(|n| (*n as f64) * (*n as f64)).sum()
    };
    let best = |from: f32, to: f32, step: f32| -> f32 {
        let mut best = (from, f64::MIN);
        let mut a = from;
        while a <= to + 1e-4 {
            let s = score(a);
            if s > best.1 {
                best = (a, s);
            }
            a += step;
        }
        best.0
    };
    let coarse = best(-6.0, 6.0, 0.3);
    best(coarse - 0.3, coarse + 0.3, 0.05)
}

/// Rimette dritta la pagina se è inclinata di più di mezzo grado; altrimenti la lascia com'è.
pub fn deskew(image: &DynamicImage) -> DynamicImage {
    let gray = image.to_luma8();
    let angle = skew_angle(&gray);
    if angle.abs() < 0.15 {
        return image.clone();
    }
    let rgb = image.to_rgb8();
    let (w, h) = rgb.dimensions();
    let (s, c) = angle.to_radians().sin_cos();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let mut out = RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
    for y in 0..h {
        let dy = y as f32 - cy;
        for x in 0..w {
            let dx = x as f32 - cx;
            let sx = dx * c + dy * s + cx;
            let sy = dy * c - dx * s + cy;
            if sx < 0.0 || sy < 0.0 || sx >= (w - 1) as f32 || sy >= (h - 1) as f32 {
                continue;
            }
            let (x0, y0) = (sx as u32, sy as u32);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let at = |x: u32, y: u32| rgb.get_pixel(x, y).0;
            let (p00, p10, p01, p11) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
            let mut px = [0u8; 3];
            for k in 0..3 {
                let top = p00[k] as f32 * (1.0 - fx) + p10[k] as f32 * fx;
                let bottom = p01[k] as f32 * (1.0 - fx) + p11[k] as f32 * fx;
                px[k] = (top * (1.0 - fy) + bottom * fy) as u8;
            }
            out.put_pixel(x, y, image::Rgb(px));
        }
    }
    DynamicImage::ImageRgb8(out)
}

/// Il tratto scuro continuo più lungo di una fila di pixel (tollera buchi di 2 pixel): (inizio, fine).
fn longest_run(len: u32, dark: impl Fn(u32) -> bool) -> (u32, u32) {
    let (mut best, mut start, mut last, mut open) = ((0, 0), 0, 0, false);
    for i in 0..len {
        if dark(i) {
            if !open {
                start = i;
                open = true;
            }
            last = i;
        } else if open && i - last > 3 {
            if last - start > best.1 - best.0 {
                best = (start, last);
            }
            open = false;
        }
    }
    if open && last - start > best.1 - best.0 {
        best = (start, last);
    }
    best
}

/// Unisce le file o colonne vicine che hanno la stessa linea (una linea è spessa qualche pixel).
fn merge(found: Vec<(u32, u32, u32)>, max_thick: u32) -> Vec<Rule> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < found.len() {
        let mut j = i;
        while j + 1 < found.len() && found[j + 1].0 - found[j].0 <= 3 {
            j += 1;
        }
        let thick = found[j].0 - found[i].0 + 1;
        if thick <= max_thick {
            let group = &found[i..=j];
            out.push(Rule {
                pos: (found[i].0 + found[j].0) as f32 / 2.0,
                from: group.iter().map(|g| g.1).min().unwrap_or(0) as f32,
                to: group.iter().map(|g| g.2).max().unwrap_or(0) as f32,
            });
        }
        i = j + 1;
    }
    out
}

/// Le linee di griglia di una pagina già dritta: tratti scuri sottili lunghi almeno una piccola parte della pagina.
pub fn rules(image: &DynamicImage) -> Rules {
    let gray = image.to_luma8();
    let (w, h) = gray.dimensions();
    let thick = (w.max(h) / 200).max(6);
    let mut rows = Vec::new();
    for y in 0..h {
        let (a, b) = longest_run(w, |x| gray.get_pixel(x, y).0[0] < LINE_DARK);
        if b - a >= w / 8 {
            rows.push((y, a, b));
        }
    }
    let mut cols = Vec::new();
    for x in 0..w {
        let (a, b) = longest_run(h, |y| gray.get_pixel(x, y).0[0] < LINE_DARK);
        if b - a >= h / 30 {
            cols.push((x, a, b));
        }
    }
    Rules { horizontal: merge(rows, thick), vertical: merge(cols, thick) }
}

/// Cancella le linee di griglia (le fa bianche): senza, l'OCR le legge come lettere e perde le righe.
pub fn erase_rules(image: &DynamicImage, rules: &Rules) -> DynamicImage {
    let mut gray = image.to_luma8();
    let (w, h) = gray.dimensions();
    let pad = 4i32;
    for r in &rules.horizontal {
        let (x0, x1) = (r.from.max(0.0) as u32, (r.to as u32).min(w - 1));
        for y in (r.pos as i32 - pad).max(0)..=(r.pos as i32 + pad).min(h as i32 - 1) {
            for x in x0..=x1 {
                gray.put_pixel(x, y as u32, image::Luma([255]));
            }
        }
    }
    for r in &rules.vertical {
        let (y0, y1) = (r.from.max(0.0) as u32, (r.to as u32).min(h - 1));
        for x in (r.pos as i32 - pad).max(0)..=(r.pos as i32 + pad).min(w as i32 - 1) {
            for y in y0..=y1 {
                gray.put_pixel(x as u32, y, image::Luma([255]));
            }
        }
    }
    DynamicImage::ImageLuma8(gray)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_page(angle: f32) -> DynamicImage {
        let (w, h) = (1200u32, 1600u32);
        let mut img = RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
        let (s, c) = angle.to_radians().sin_cos();
        let mut put = |x: f32, y: f32| {
            let (dx, dy) = (x - 600.0, y - 800.0);
            let (px, py) = (dx * c - dy * s + 600.0, dy * c + dx * s + 800.0);
            for k in 0..3 {
                let (ix, iy) = (px as i32, (py + k as f32) as i32);
                if ix >= 0 && iy >= 0 && (ix as u32) < w && (iy as u32) < h {
                    img.put_pixel(ix as u32, iy as u32, image::Rgb([0, 0, 0]));
                }
            }
        };
        for row in 0..6 {
            let y = 300.0 + row as f32 * 100.0;
            for x in 150..1050 {
                put(x as f32, y);
            }
        }
        for col in 0..4 {
            let x = 150.0 + col as f32 * 300.0;
            for y in 300..800 {
                put(x, y as f32);
            }
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn a_tilted_table_is_straightened_and_its_grid_found() {
        for angle in [0.0f32, 0.8, -2.5] {
            let page = deskew(&table_page(angle));
            let r = rules(&page);
            assert_eq!(r.horizontal.len(), 6, "angolo {angle}: {:?}", r.horizontal);
            assert_eq!(r.vertical.len(), 4, "angolo {angle}: {:?}", r.vertical);
            let gap = r.horizontal[1].pos - r.horizontal[0].pos;
            assert!((gap - 100.0).abs() < 3.0, "angolo {angle}: {gap}");
        }
    }
}
