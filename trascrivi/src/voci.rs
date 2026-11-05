//! Chi parla e quando (diarizzazione), tutto in Rust con rten. Il procedimento è quello di sherpa-onnx
//! (pyannote): segmentazione pyannote-segmentation-3.0 e impronte della voce CAM++ di 3D-Speaker
//! (cinese e inglese, 200 mila voci: separa bene anche le voci italiane).
//!
//! 1. Finestre di 10 s: il modello di segmentazione dice, fotogramma per fotogramma (17 ms), quali delle
//!    (al massimo) 3 voci della finestra parlano.
//! 2. Per ogni voce di ogni finestra si prende il suo parlato (fino a 4 s) e se ne calcola l'impronta.
//! 3. Le impronte si raggruppano (agglomerativo a legame completo, distanza coseno): ogni gruppo è un
//!    interlocutore. Con il numero dato si taglia a quel numero; con «Automatico» si taglia alla
//!    distanza che separa due persone diverse. I gruppi minuscoli (voci accavallate, rumori) non contano.
//! 4. Le finestre si rimettono insieme: per ogni fotogramma parlano le voci con più consenso.

use std::sync::atomic::{AtomicBool, Ordering};

use kaldi_native_fbank::fbank::{FbankComputer, FbankOptions};
use kaldi_native_fbank::online::{FeatureComputer, OnlineFeature};
use std::sync::Arc;

// Su Android questo file si compila dentro Cedro (src/whisper_android), che per l'OCR usa ancora rten 0.26:
// lì la 0.27 delle voci ha un altro nome.
#[cfg(target_os = "android")]
use {rten_voci as rten, rten_tensor_voci as rten_tensor};
use rten::{Model, RunOptions, ThreadPool};
use rten_tensor::prelude::*;
use rten_tensor::NdTensor;

pub const RATE: usize = 16_000;
const WINDOW: usize = 160_000;
/// Passo del modello di segmentazione: un fotogramma ogni 270 campioni, largo 991.
const FIELD_SHIFT: usize = 270;
const FIELD_SIZE: usize = 991;
const LOCAL: usize = 3;
/// Non più di tante finestre: oltre, il passo si allunga (fino a metà finestra), così un'ora di
/// riunione resta sotto il minuto anche su un portatile.
const MAX_CHUNKS: usize = 400;
/// L'impronta di una voce si calcola su al massimo tanti secondi del suo parlato nella finestra.
const EMB_MAX: usize = 4 * RATE;
pub const MAX_SPEAKERS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    /// Inizio e fine in centesimi di secondo.
    pub t0: i64,
    pub t1: i64,
    /// 0, 1, 2… in ordine di prima apparizione.
    pub speaker: usize,
}

pub struct Diarizer {
    seg: Model,
    emb: Model,
}

pub enum Speakers {
    Auto,
    Exactly(usize),
}

impl Diarizer {
    pub fn load(segmentation: &std::path::Path, embedding: &std::path::Path) -> Result<Diarizer, String> {
        let seg = Model::load_file(segmentation).map_err(|e| format!("modello delle voci (segmentazione) non valido: {e}"))?;
        let emb = Model::load_file(embedding).map_err(|e| format!("modello delle voci (impronte) non valido: {e}"))?;
        Ok(Diarizer { seg, emb })
    }

    /// I turni di parola di `audio` (16 kHz mono). `progress` riceve 0..=100.
    pub fn run(&self, audio: &[f32], speakers: Speakers, threads: usize, progress: &dyn Fn(u8), cancel: &AtomicBool) -> Result<Vec<Turn>, String> {
        if audio.len() < RATE / 2 {
            return Ok(Vec::new());
        }
        let step = step_for(audio.len());
        // I thread scelti da Cedro (uno resta libero per il resto del PC).
        let mut opts = RunOptions::default();
        opts.thread_pool = Some(Arc::new(ThreadPool::with_num_threads(threads.max(1))));
        // --- 1. segmentazione
        let full = if audio.len() >= WINDOW { (audio.len() - WINDOW) / step + 1 } else { 0 };
        let has_last = audio.len() < WINDOW || (audio.len() - WINDOW) % step > 0;
        let chunks = full + usize::from(has_last);
        let mut labels: Vec<[Vec<u8>; LOCAL]> = Vec::with_capacity(chunks);
        let mut frames_per_chunk = 0;
        const BATCH: usize = 16;
        let mut i = 0;
        while i < chunks {
            if cancel.load(Ordering::Relaxed) {
                return Err("Annullato".into());
            }
            let n = BATCH.min(chunks - i);
            let x = NdTensor::<f32, 3>::from_fn([n, 1, WINDOW], |[b, _, k]| {
                let at = (i + b) * step + k;
                audio.get(at).copied().unwrap_or(0.0)
            });
            let y: NdTensor<f32, 3> = self.seg.run_one(x.view().into(), Some(opts.clone())).map_err(|e| format!("segmentazione: {e}"))?.try_into().map_err(|_| "segmentazione: uscita inattesa".to_string())?;
            frames_per_chunk = y.size(1);
            for b in 0..n {
                let mut l: [Vec<u8>; LOCAL] = Default::default();
                for s in &mut l {
                    s.resize(frames_per_chunk, 0);
                }
                for f in 0..frames_per_chunk {
                    let mut best = 0;
                    for c in 1..y.size(2) {
                        if y[[b, f, c]] > y[[b, f, best]] {
                            best = c;
                        }
                    }
                    for s in POWERSET[best] {
                        l[*s][f] = 1;
                    }
                }
                labels.push(l);
            }
            i += n;
            progress((i * 30 / chunks) as u8);
        }
        let total_frames = (WINDOW + (chunks - 1) * step) / FIELD_SHIFT + 1;
        let chunk_start = |c: usize| ((c * step) as f64 / FIELD_SHIFT as f64 + 0.5) as usize;
        // Quante voci insieme per fotogramma (media delle finestre che lo coprono).
        let mut sum = vec![0f32; total_frames];
        let mut cover = vec![0f32; total_frames];
        for (c, l) in labels.iter().enumerate() {
            let s = chunk_start(c);
            for f in 0..frames_per_chunk {
                if s + f >= total_frames {
                    break;
                }
                sum[s + f] += l.iter().map(|v| v[f] as f32).sum::<f32>();
                cover[s + f] += 1.0;
            }
        }
        let count: Vec<usize> = sum.iter().zip(&cover).map(|(s, c)| (s / c.max(1e-12) + 0.5) as usize).collect();
        if count.iter().all(|c| *c == 0) {
            return Ok(Vec::new());
        }

        // --- 2. impronte
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for (c, l) in labels.iter().enumerate() {
            for s in 0..LOCAL {
                if l[s].iter().map(|v| *v as usize).sum::<usize>() >= 10 {
                    pairs.push((c, s));
                }
            }
        }
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(pairs.len());
        for (k, (c, sp)) in pairs.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Annullato".into());
            }
            let speech = voice_audio(audio, &labels[*c][*sp], c * step, frames_per_chunk);
            embeddings.push(self.embed(&speech, &opts)?);
            progress((30 + (k + 1) * 65 / pairs.len()) as u8);
        }

        // --- 3. raggruppamento
        let clusters = cluster(&embeddings, speakers);
        let k = clusters.iter().copied().max().unwrap_or(0) + 1;

        // --- 4. ricomposizione
        let mut votes = vec![vec![0f32; k]; total_frames];
        for (p, (c, s)) in pairs.iter().enumerate() {
            let cl = clusters[p];
            let start = chunk_start(*c);
            for f in 0..frames_per_chunk {
                if start + f < total_frames && labels[*c][*s][f] == 1 {
                    votes[start + f][cl] += 1.0;
                }
            }
        }
        let stop = if has_last { (audio.len() / FIELD_SHIFT).min(total_frames) } else { total_frames };
        let mut active = vec![vec![false; stop]; k];
        for f in 0..stop {
            let mut order: Vec<usize> = (0..k).collect();
            order.sort_by(|a, b| votes[f][*b].partial_cmp(&votes[f][*a]).unwrap());
            for o in order.iter().take(count[f].min(k)) {
                if votes[f][*o] > 0.0 {
                    active[*o][f] = true;
                }
            }
        }
        let scale = FIELD_SHIFT as f64 / RATE as f64;
        let offset = FIELD_SIZE as f64 / RATE as f64 * 0.5;
        let mut turns: Vec<Turn> = Vec::new();
        for (spk, frames) in active.iter().enumerate() {
            let mut list: Vec<(f64, f64)> = Vec::new();
            let mut start: Option<usize> = None;
            for (f, on) in frames.iter().enumerate() {
                match (on, start) {
                    (true, None) => start = Some(f),
                    (false, Some(s)) => {
                        list.push((s as f64 * scale + offset, f as f64 * scale + offset));
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = start {
                list.push((s as f64 * scale + offset, (frames.len() - 1) as f64 * scale + offset));
            }
            // Pause brevi (meno di mezzo secondo) dentro lo stesso turno si chiudono; i pezzetti sotto 0,3 s si tolgono.
            let mut merged: Vec<(f64, f64)> = Vec::new();
            for (a, b) in list {
                match merged.last_mut() {
                    Some(last) if a - last.1 <= 0.5 => last.1 = b,
                    _ => merged.push((a, b)),
                }
            }
            for (a, b) in merged {
                if b - a >= 0.3 {
                    turns.push(Turn { t0: (a * 100.0).round() as i64, t1: (b * 100.0).round() as i64, speaker: spk });
                }
            }
        }
        turns.sort_by_key(|t| (t.t0, t.t1));
        // Interlocutore 1 è chi parla per primo.
        let mut order: Vec<usize> = Vec::new();
        for t in &turns {
            if !order.contains(&t.speaker) {
                order.push(t.speaker);
            }
        }
        for t in &mut turns {
            t.speaker = order.iter().position(|s| *s == t.speaker).unwrap_or(0);
        }
        progress(100);
        Ok(turns)
    }

    fn embed(&self, speech: &[f32], opts: &RunOptions) -> Result<Vec<f32>, String> {
        let mut o = FbankOptions::default();
        o.frame_opts.dither = 0.0;
        o.frame_opts.snip_edges = false;
        o.mel_opts.num_bins = 80;
        o.mel_opts.low_freq = 20.0;
        o.mel_opts.high_freq = -400.0;
        o.use_energy = false;
        let mut on = OnlineFeature::new(FeatureComputer::Fbank(FbankComputer::new(o)?));
        on.accept_waveform(RATE as f32, speech);
        on.input_finished();
        let n = on.num_frames_ready();
        let mut feats = NdTensor::<f32, 3>::zeros([1, n, 80]);
        for i in 0..n {
            if let Some(f) = on.get_frame(i) {
                for (j, v) in f.iter().take(80).enumerate() {
                    feats[[0, i, j]] = *v;
                }
            }
        }
        // Le impronte di 3D-Speaker vogliono le fbank senza la media di ogni banda (global-mean).
        for j in 0..80 {
            let m = (0..n).map(|i| feats[[0, i, j]]).sum::<f32>() / n.max(1) as f32;
            for i in 0..n {
                feats[[0, i, j]] -= m;
            }
        }
        let e: NdTensor<f32, 2> = self.emb.run_one(feats.view().into(), Some(opts.clone())).map_err(|e| format!("impronta della voce: {e}"))?.try_into().map_err(|_| "impronta della voce: uscita inattesa".to_string())?;
        let mut v: Vec<f32> = e.iter().copied().collect();
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
        v.iter_mut().for_each(|x| *x /= norm);
        Ok(v)
    }
}

/// Le classi del modello (powerset): nessuno, A, B, C, A+B, A+C, B+C.
const POWERSET: [&[usize]; 7] = [&[], &[0], &[1], &[2], &[0, 1], &[0, 2], &[1, 2]];

/// Il passo tra le finestre: 1 s di solito, più lungo per audio molto lunghi.
fn step_for(len: usize) -> usize {
    let base = RATE;
    if len <= WINDOW {
        return base;
    }
    let need = (len - WINDOW).div_ceil(MAX_CHUNKS - 1);
    need.clamp(base, WINDOW / 2)
}

/// Il parlato di una voce dentro una finestra (fino a `EMB_MAX` campioni, presi dal centro).
fn voice_audio(audio: &[f32], frames: &[u8], offset: usize, n: usize) -> Vec<f32> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let at = |f: usize| (f as f64 / n as f64 * WINDOW as f64) as usize + offset;
    for f in 0..=n {
        let on = f < n && frames[f] != 0;
        match (on, start) {
            (true, None) => start = Some(f),
            (false, Some(s)) => {
                let (a, b) = (at(s).min(audio.len()), at(f.min(n - 1)).min(audio.len()));
                out.extend_from_slice(&audio[a..b]);
                start = None;
            }
            _ => {}
        }
    }
    if out.len() > EMB_MAX {
        let cut = (out.len() - EMB_MAX) / 2;
        out = out[cut..cut + EMB_MAX].to_vec();
    }
    out
}

/// Distanza coseno oltre la quale due gruppi sono voci diverse (legame completo, impronte CAM++).
const AUTO_THRESHOLD: f32 = 0.7;

/// Raggruppa le impronte (già normalizzate). Ritorna il gruppo di ognuna.
///
/// Un gruppo con pochissime impronte di solito non è una persona ma un pezzo confuso (due voci
/// insieme, un rumore): non conta come interlocutore e le sue impronte vanno al gruppo più vicino.
pub fn cluster(emb: &[Vec<f32>], speakers: Speakers) -> Vec<usize> {
    let n = emb.len();
    if n == 0 {
        return Vec::new();
    }
    if matches!(speakers, Speakers::Exactly(1)) || n == 1 {
        return vec![0; n];
    }
    let tree = Tree::build(emb);
    let min_size = if n < 10 { 1 } else { ((n as f32 * 0.08).ceil() as usize).clamp(2, 12) };
    let big = |labels: &[usize]| {
        let k = labels.iter().copied().max().unwrap_or(0) + 1;
        let mut sizes = vec![0usize; k];
        for l in labels {
            sizes[*l] += 1;
        }
        sizes.iter().filter(|s| **s >= min_size).count()
    };
    let want = match speakers {
        Speakers::Exactly(k) => k.clamp(1, n),
        Speakers::Auto => {
            let merged = tree.merges.iter().filter(|m| m.2 <= AUTO_THRESHOLD).count();
            big(&tree.cut(n - merged)).clamp(1, MAX_SPEAKERS)
        }
    };
    if want == 1 {
        return vec![0; n];
    }
    // Il taglio più fine che dà esattamente `want` gruppi «veri».
    let labels = (want..=n).map(|c| tree.cut(c)).find(|l| big(l) == want).unwrap_or_else(|| tree.cut(want));
    reassign_small(emb, labels, min_size)
}

/// Le impronte dei gruppi piccoli vanno al gruppo grande col centro più vicino; poi si rinumera 0, 1, 2…
fn reassign_small(emb: &[Vec<f32>], labels: Vec<usize>, min_size: usize) -> Vec<usize> {
    let k = labels.iter().copied().max().unwrap_or(0) + 1;
    let dim = emb[0].len();
    let mut sizes = vec![0usize; k];
    let mut centers = vec![vec![0f32; dim]; k];
    for (e, l) in emb.iter().zip(&labels) {
        sizes[*l] += 1;
        for (c, v) in centers[*l].iter_mut().zip(e) {
            *c += v;
        }
    }
    for c in &mut centers {
        let norm = c.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
        c.iter_mut().for_each(|x| *x /= norm);
    }
    let bigs: Vec<usize> = (0..k).filter(|c| sizes[*c] >= min_size).collect();
    if bigs.is_empty() {
        return vec![0; emb.len()];
    }
    let out: Vec<usize> = emb
        .iter()
        .zip(&labels)
        .map(|(e, l)| {
            if sizes[*l] >= min_size {
                return *l;
            }
            *bigs
                .iter()
                .max_by(|a, b| {
                    let da: f32 = centers[**a].iter().zip(e).map(|(x, y)| x * y).sum();
                    let db: f32 = centers[**b].iter().zip(e).map(|(x, y)| x * y).sum();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap()
        })
        .collect();
    let mut seen: Vec<usize> = Vec::new();
    out.iter()
        .map(|l| match seen.iter().position(|s| s == l) {
            Some(p) => p,
            None => {
                seen.push(*l);
                seen.len() - 1
            }
        })
        .collect()
}

/// L'albero dell'agglomerativo (legame completo sulla distanza coseno, come sherpa-onnx).
pub struct Tree {
    n: usize,
    /// Le fusioni in ordine: (a, b, distanza), con gli indici dei gruppi come in scipy (n + i per il nuovo).
    pub merges: Vec<(usize, usize, f32)>,
}

impl Tree {
    pub fn build(emb: &[Vec<f32>]) -> Tree {
        let n = emb.len();
        // Matrice delle distanze (triangolare) e legame medio con la catena dei vicini più vicini.
        let mut d = vec![0f32; n * n];
        for i in 0..n {
            for j in i + 1..n {
                let dot: f32 = emb[i].iter().zip(&emb[j]).map(|(a, b)| a * b).sum();
                let v = 1.0 - dot;
                d[i * n + j] = v;
                d[j * n + i] = v;
            }
        }
        let mut size = vec![1usize; n];
        let mut alive = vec![true; n];
        let mut id: Vec<usize> = (0..n).collect();
        let mut merges = Vec::with_capacity(n.saturating_sub(1));
        let mut chain: Vec<usize> = Vec::new();
        let mut next_id = n;
        for _ in 0..n.saturating_sub(1) {
            if chain.is_empty() {
                chain.push((0..n).find(|i| alive[*i]).unwrap());
            }
            loop {
                let a = *chain.last().unwrap();
                let prev = if chain.len() > 1 { Some(chain[chain.len() - 2]) } else { None };
                let mut best = None::<(usize, f32)>;
                for b in 0..n {
                    if b == a || !alive[b] {
                        continue;
                    }
                    let v = d[a * n + b];
                    match best {
                        Some((_, bv)) if v > bv || (v == bv && Some(b) != prev) => {}
                        _ => best = Some((b, v)),
                    }
                }
                let (b, v) = best.unwrap();
                if Some(b) == prev {
                    chain.pop();
                    chain.pop();
                    let (x, y) = (a.min(b), a.max(b));
                    merges.push((id[x].min(id[y]), id[x].max(id[y]), v));
                    // y confluisce in x
                    for z in 0..n {
                        if alive[z] && z != x && z != y {
                            let nv = d[x * n + z].max(d[y * n + z]);
                            d[x * n + z] = nv;
                            d[z * n + x] = nv;
                        }
                    }
                    size[x] += size[y];
                    alive[y] = false;
                    id[x] = next_id;
                    next_id += 1;
                    break;
                }
                chain.push(b);
            }
        }
        // In ordine di distanza, come lo vuole il taglio.
        let mut order: Vec<usize> = (0..merges.len()).collect();
        order.sort_by(|a, b| merges[*a].2.partial_cmp(&merges[*b].2).unwrap());
        // Rinumerazione degli id dopo il riordino.
        let mut remap = std::collections::HashMap::new();
        let mut sorted = Vec::with_capacity(merges.len());
        for (k, o) in order.iter().enumerate() {
            let (a, b, v) = merges[*o];
            let a = *remap.get(&a).unwrap_or(&a);
            let b = *remap.get(&b).unwrap_or(&b);
            remap.insert(n + *o, n + k);
            sorted.push((a.min(b), a.max(b), v));
        }
        Tree { n, merges: sorted }
    }

    /// Le etichette con `k` gruppi: si applicano le prime n-k fusioni.
    pub fn cut(&self, k: usize) -> Vec<usize> {
        let n = self.n;
        let mut parent: Vec<usize> = (0..2 * n).collect();
        fn root(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        for (i, (a, b, _)) in self.merges.iter().take(n.saturating_sub(k)).enumerate() {
            let new = n + i;
            let ra = root(&mut parent, *a);
            let rb = root(&mut parent, *b);
            parent[ra] = new;
            parent[rb] = new;
        }
        let mut labels = vec![0; n];
        let mut seen: Vec<usize> = Vec::new();
        for i in 0..n {
            let r = root(&mut parent, i);
            let l = match seen.iter().position(|s| *s == r) {
                Some(p) => p,
                None => {
                    seen.push(r);
                    seen.len() - 1
                }
            };
            labels[i] = l;
        }
        labels
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    /// Impronte finte: `k` voci, ognuna una direzione diversa con un po' di rumore.
    fn voices(k: usize, each: usize) -> (Vec<Vec<f32>>, Vec<usize>) {
        let dim = 32;
        let mut seed = 12345u32;
        let mut rnd = move || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ((seed >> 8) as f32 / (1u32 << 24) as f32) - 0.5
        };
        let mut emb = Vec::new();
        let mut truth = Vec::new();
        for i in 0..k * each {
            let v = i % k;
            let mut e: Vec<f32> = (0..dim).map(|_| rnd() * 0.35).collect();
            e[v] += 1.0;
            let n = e.iter().map(|x| x * x).sum::<f32>().sqrt();
            emb.push(e.iter().map(|x| x / n).collect());
            truth.push(v);
        }
        (emb, truth)
    }

    fn same_partition(a: &[usize], b: &[usize]) -> bool {
        a.iter().zip(b).all(|(x, y)| a.iter().zip(b).all(|(x2, y2)| (x == x2) == (y == y2)))
    }

    #[test]
    fn automatic_counts_the_voices() {
        for k in 1..=5 {
            let (emb, truth) = voices(k, 30);
            let labels = cluster(&emb, Speakers::Auto);
            assert_eq!(labels.iter().max().unwrap() + 1, k, "voci {k}");
            assert!(same_partition(&labels, &truth));
        }
    }

    #[test]
    fn a_given_number_is_respected_and_tiny_groups_do_not_count() {
        let (mut emb, truth) = voices(3, 30);
        // Due impronte strane (voci accavallate): non devono diventare una quarta persona.
        let odd: Vec<f32> = (0..32).map(|i| if i == 20 { 1.0 } else { 0.0 }).collect();
        emb.push(odd.clone());
        emb.push(odd);
        let labels = cluster(&emb, Speakers::Auto);
        assert_eq!(labels.iter().max().unwrap() + 1, 3);
        assert!(same_partition(&labels[..90], &truth));
        let two = cluster(&emb, Speakers::Exactly(2));
        assert_eq!(two.iter().max().unwrap() + 1, 2);
        assert_eq!(cluster(&emb, Speakers::Exactly(1)), vec![0; emb.len()]);
    }

    #[test]
    fn long_audio_uses_a_longer_step() {
        assert_eq!(step_for(30 * RATE), RATE);
        assert!(step_for(3600 * RATE) > RATE);
        assert!(step_for(3600 * RATE) <= WINDOW / 2);
        assert_eq!(step_for(3600 * RATE), WINDOW / 2);
        let len = 20 * 60 * RATE;
        assert!((len - WINDOW) / step_for(len) < MAX_CHUNKS);
    }

    /// Con `CEDRO_VOCI_DIR` (i due modelli, `segmentazione.onnx` e `impronte.onnx`, e `quattro-voci.f32`, campioni
    /// f32 a 16 kHz mono) prova il riconoscimento vero. Altrove non fa niente.
    #[test]
    fn real_models_find_the_voices() {
        let Some(dir) = std::env::var_os("CEDRO_VOCI_DIR").map(std::path::PathBuf::from) else { return };
        let d = Diarizer::load(&dir.join("segmentazione.onnx"), &dir.join("impronte.onnx")).unwrap();
        let bytes = std::fs::read(dir.join("quattro-voci.f32")).unwrap();
        let audio: Vec<f32> = bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let want: usize = std::env::var("CEDRO_VOCI_N").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
        let count = |s| {
            let turns = d.run(&audio, s, 4, &|_| {}, &AtomicBool::new(false)).unwrap();
            turns.iter().map(|t| t.speaker + 1).max().unwrap_or(0)
        };
        assert_eq!(count(Speakers::Exactly(want)), want);
        // Da sola può sbagliare di una voce (anche sherpa-onnx trova un gruppo in più su questo audio).
        assert!(count(Speakers::Auto).abs_diff(want) <= 1);
    }
}
