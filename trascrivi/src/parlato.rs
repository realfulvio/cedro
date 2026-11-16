//! C'è del parlato in quest'audio? Whisper su silenzio, ronzii, soffi e toni inventa frasi (e con la lingua
//! automatica sceglie lingue a caso: gallese, georgiano…): meglio non darglieli.
//!
//! Il parlato non è mai stazionario: tra una sillaba e l'altra l'energia sale e scende di molti decibel. Un ronzio,
//! un soffio, un tono hanno invece energia quasi costante. Qui si misura, a finestre di un secondo, quanto
//! l'energia (a frammenti di 20 ms) si muove tra i frammenti più bassi e più alti: dove si muove abbastanza c'è
//! parlato (o comunque qualcosa che vale la pena trascrivere), altrove no. Il controllo è volutamente largo:
//! meglio passare del rumore a whisper (poi lo ripulisce `rifinitura`) che tagliare un parlato sommesso.

use crate::decode::RATE;

/// Durata di un frammento (20 ms).
const FRAME: usize = (RATE as usize) / 50;
/// Frammenti per finestra: un secondo.
const WINDOW: usize = 50;
/// Sotto questo livello (dBFS) un frammento è silenzio digitale e non conta.
const FLOOR_DB: f32 = -75.0;
/// Quanti decibel deve muoversi l'energia in una finestra perché ci sia parlato.
const MIN_RANGE_DB: f32 = 12.0;
/// Quante finestre con parlato servono per dire che l'audio ne ha (un «sì» breve dura più di un secondo di finestra).
const MIN_WINDOWS: usize = 1;

/// L'energia dei frammenti di 20 ms, in dBFS.
fn frame_levels(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks_exact(FRAME)
        .map(|f| {
            let power = f.iter().map(|s| s * s).sum::<f32>() / FRAME as f32;
            (10.0 * power.max(1e-12).log10()).max(-120.0)
        })
        .collect()
}

/// Il percentile `p` (0..1) di una finestra già ordinata.
fn percentile(sorted: &[f32], p: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

/// La finestra col movimento d'energia più grande, in decibel (per le prove).
pub fn best_range(samples: &[f32]) -> f32 {
    let levels = frame_levels(samples);
    (0..levels.len().saturating_sub(WINDOW) + 1)
        .step_by(WINDOW / 2)
        .map(|s| {
            let mut w: Vec<f32> = levels[s..(s + WINDOW).min(levels.len())].to_vec();
            w.sort_by(|a, b| a.partial_cmp(b).unwrap());
            percentile(&w, 0.95) - percentile(&w, 0.1)
        })
        .fold(0.0, f32::max)
}

/// Quante finestre di un secondo (spostate di mezzo secondo) hanno l'energia che si muove come nel parlato.
pub fn moving_windows(samples: &[f32]) -> usize {
    let levels = frame_levels(samples);
    if levels.len() < WINDOW / 2 {
        return 0;
    }
    let step = WINDOW / 2;
    let mut starts: Vec<usize> = (0..levels.len().saturating_sub(WINDOW) + 1).step_by(step).collect();
    if starts.is_empty() {
        starts.push(0);
    }
    starts
        .into_iter()
        .filter(|&s| {
            let mut w: Vec<f32> = levels[s..(s + WINDOW).min(levels.len())].to_vec();
            w.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let (low, high) = (percentile(&w, 0.1), percentile(&w, 0.95));
            high > FLOOR_DB && high - low >= MIN_RANGE_DB
        })
        .count()
}

/// L'audio ha del parlato da trascrivere?
pub fn has_speech(samples: &[f32]) -> bool {
    moving_windows(samples) >= MIN_WINDOWS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: &mut u64) -> f32 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*seed >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
    }

    fn secs(n: usize) -> usize {
        n * RATE as usize
    }

    #[test]
    fn silence_hum_tone_and_noise_are_not_speech() {
        assert!(!has_speech(&vec![0.0; secs(5)]));
        let hum: Vec<f32> = (0..secs(5)).map(|i| 0.05 * (i as f32 * 2.0 * std::f32::consts::PI * 50.0 / RATE as f32).sin()).collect();
        assert!(!has_speech(&hum));
        let tone: Vec<f32> = (0..secs(5)).map(|i| 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / RATE as f32).sin()).collect();
        assert!(!has_speech(&tone));
        let mut seed = 7;
        let noise: Vec<f32> = (0..secs(5)).map(|_| 0.1 * lcg(&mut seed)).collect();
        assert!(!has_speech(&noise));
    }

    /// Un parlato finto: raffiche di rumore a «sillabe» di 0,2 s con pause di 0,1 s e volume che cambia.
    fn syllables(seconds: usize, gain: f32) -> Vec<f32> {
        let mut seed = 3;
        (0..secs(seconds))
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let phase = (t / 0.3).fract();
                let env = if phase < 0.66 { (phase / 0.66 * std::f32::consts::PI).sin() } else { 0.01 };
                gain * env * lcg(&mut seed)
            })
            .collect()
    }

    #[test]
    fn syllabic_audio_is_speech_even_when_quiet() {
        assert!(has_speech(&syllables(4, 0.3)));
        assert!(has_speech(&syllables(4, 0.003)));
    }

    #[test]
    fn a_short_word_in_silence_counts() {
        let mut a = vec![0.0f32; secs(3)];
        a.extend(syllables(1, 0.2));
        a.extend(vec![0.0f32; secs(3)]);
        assert!(has_speech(&a));
    }

    #[test]
    fn too_short_audio_is_not_speech() {
        assert!(!has_speech(&vec![0.5; 100]));
    }
}
