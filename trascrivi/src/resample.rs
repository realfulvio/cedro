// Packet-based sinc resampling. The frozen buffered version is a test oracle only.
#[cfg(test)]
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    const PHASES: usize = 256;
    let ratio = from as f64 / to as f64;
    let cutoff = (to as f64 / from as f64).min(1.0) * 0.94;
    let half = (8.0 / cutoff).ceil() as isize;
    let taps = (2 * half) as usize;
    let mut table = vec![0f32; PHASES * taps];
    for p in 0..PHASES {
        let frac = p as f64 / PHASES as f64;
        let row = &mut table[p * taps..(p + 1) * taps];
        let mut sum = 0.0;
        for (k, w) in row.iter_mut().enumerate() {
            let t = (k as isize - half + 1) as f64 - frac;
            let x = cutoff * t;
            let sinc = if x.abs() < 1e-9 {
                1.0
            } else {
                (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
            };
            let hann = 0.5 + 0.5 * (std::f64::consts::PI * t / half as f64).cos();
            let v = if t.abs() >= half as f64 {
                0.0
            } else {
                sinc * hann
            };
            *w = v as f32;
            sum += v;
        }
        for w in row.iter_mut() {
            *w /= sum as f32;
        }
    }
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let last = input.len() as isize - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let center = i as f64 * ratio;
        let base = center.floor() as isize;
        let phase = (((center - base as f64) * PHASES as f64) as usize).min(PHASES - 1);
        let row = &table[phase * taps..(phase + 1) * taps];
        let first = base - half + 1;
        let mut acc = 0f32;
        if first >= 0 && first + taps as isize - 1 <= last {
            let window = &input[first as usize..first as usize + taps];
            for (x, w) in window.iter().zip(row) {
                acc += x * w;
            }
        } else {
            for (k, w) in row.iter().enumerate() {
                let at = (first + k as isize).clamp(0, last) as usize;
                acc += input[at] * w;
            }
        }
        out.push(acc);
    }
    out
}

/// Same sinc filter and global sample positions as the reference implementation.
/// Only the filter overlap and the current decoder packet remain at source rate.
pub(crate) struct StreamResampler {
    pub(crate) from: u32,
    to: u32,
    ratio: f64,
    half: isize,
    taps: usize,
    table: Vec<f32>,
    input: Vec<f32>,
    base: usize,
    total: usize,
    output: Vec<f32>,
}
impl StreamResampler {
    pub(crate) fn new(from: u32, to: u32) -> Self {
        const PHASES: usize = 256;
        let ratio = from as f64 / to as f64;
        let cutoff = (to as f64 / from as f64).min(1.0) * 0.94;
        let half = (8.0 / cutoff).ceil() as isize;
        let taps = (2 * half) as usize;
        let mut table = vec![0f32; PHASES * taps];
        for p in 0..PHASES {
            let frac = p as f64 / PHASES as f64;
            let row = &mut table[p * taps..(p + 1) * taps];
            let mut sum = 0.0;
            for (k, w) in row.iter_mut().enumerate() {
                let t = (k as isize - half + 1) as f64 - frac;
                let x = cutoff * t;
                let sinc = if x.abs() < 1e-9 {
                    1.0
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                let hann = 0.5 + 0.5 * (std::f64::consts::PI * t / half as f64).cos();
                let v = if t.abs() >= half as f64 {
                    0.0
                } else {
                    sinc * hann
                };
                *w = v as f32;
                sum += v;
            }
            for w in row.iter_mut() {
                *w /= sum as f32;
            }
        }

        Self {
            from,
            to,
            ratio,
            half,
            taps,
            table,
            input: Vec::new(),
            base: 0,
            total: 0,
            output: Vec::new(),
        }
    }
    pub(crate) fn push(&mut self, chunk: &[f32]) {
        self.total += chunk.len();
        if self.from == self.to {
            self.output.extend_from_slice(chunk);
            return;
        }
        self.input.extend_from_slice(chunk);
        self.drain(false);
    }
    fn drain(&mut self, end: bool) {
        if self.total == 0 {
            return;
        }
        let target = (self.total as f64 / self.ratio).floor() as usize;
        while self.output.len() < target {
            let center = self.output.len() as f64 * self.ratio;
            let base = center.floor() as isize;
            let first = base - self.half + 1;
            if !end && first + self.taps as isize > self.total as isize {
                break;
            }
            let phase = (((center - base as f64) * 256.0) as usize).min(255);
            let row = &self.table[phase * self.taps..(phase + 1) * self.taps];
            let mut acc = 0f32;
            for (k, w) in row.iter().enumerate() {
                let at = (first + k as isize).clamp(0, self.total as isize - 1) as usize;
                acc += self.input[at - self.base] * w;
            }
            self.output.push(acc);
        }
        let next = (self.output.len() as f64 * self.ratio).floor() as isize - self.half + 1;
        let keep = next.max(0) as usize;
        let remove = keep.saturating_sub(self.base).min(self.input.len());
        self.input.drain(..remove);
        self.base += remove;
    }
    pub(crate) fn finish(mut self) -> Vec<f32> {
        if self.from != self.to {
            self.drain(true);
        }
        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RATE: u32 = 16000;
    #[test]
    #[ignore = "manual bounded-memory benchmark"]
    fn resample_memory_benchmark() {
        let count = 48_000 * 600;
        let sample = |i: usize| ((i * 7919 % 60000) as f32 - 30000.0) / 32768.0;
        let start = std::time::Instant::now();
        let out = if std::env::var_os("CEDRO_BENCH_BUFFERED").is_some() {
            let input: Vec<f32> = (0..count).map(sample).collect();
            resample(&input, 48000, RATE)
        } else {
            let mut r = StreamResampler::new(48000, RATE);
            for at in (0..count).step_by(4096) {
                let packet: Vec<f32> = (at..(at + 4096).min(count)).map(sample).collect();
                r.push(&packet);
            }
            r.finish()
        };
        let hash = out.iter().fold(0u64, |a, x| {
            a.wrapping_mul(31).wrapping_add(x.to_bits() as u64)
        });
        println!(
            "samples={}, hash={hash:016x}, elapsed={:?}",
            out.len(),
            start.elapsed()
        );
        assert_eq!(out.len(), 600 * RATE as usize);
    }

    fn tone(freq: f64, rate: u32, secs: f64) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize)
            .map(|i| {
                (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32 * 0.5
            })
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn streaming_is_bit_exact_at_packet_boundaries() {
        for from in [8000, 16000, 22050, 44100, 48000, 96000] {
            for len in [0, 1, 7, 1001, 50000] {
                let input = tone(441.0, from, 1.0)
                    .into_iter()
                    .cycle()
                    .take(len)
                    .collect::<Vec<_>>();
                let expected = resample(&input, from, RATE);
                for size in [1, 17, 1152, 4096] {
                    let mut r = StreamResampler::new(from, RATE);
                    for c in input.chunks(size) {
                        r.push(c);
                        assert!(r.input.len() < 4096 + 512);
                    }
                    assert_eq!(r.finish(), expected, "rate={from}, len={len}, chunk={size}");
                }
            }
        }
    }
    #[test]
    fn speech_band_passes_and_high_tones_are_removed() {
        let low = resample(&tone(1000.0, 48_000, 1.0), 48_000, 16_000);
        assert_eq!(low.len(), 16_000);
        assert!(
            (rms(&low[200..15_800]) - 0.3535).abs() < 0.01,
            "1 kHz deve restare: {}",
            rms(&low)
        );
        // 12 kHz a 16 kHz diventerebbe un'eco a 4 kHz: il filtro lo toglie.
        let high = resample(&tone(12_000.0, 48_000, 1.0), 48_000, 16_000);
        assert!(
            rms(&high[200..15_800]) < 0.01,
            "12 kHz deve sparire: {}",
            rms(&high)
        );
        let odd = resample(&tone(440.0, 44_100, 1.0), 44_100, 16_000);
        assert!((odd.len() as i64 - 16_000).abs() <= 1);
        assert!((rms(&odd[200..15_800]) - 0.3535).abs() < 0.01);
    }
}
