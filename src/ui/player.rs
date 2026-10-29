//! Il tasto Play dell'anteprima: l'audio esce dalle casse (cpal: ALSA su Linux, WASAPI su Windows) e il
//! video si guarda nella scheda, sincronizzato con l'audio. Un thread per l'audio (che tiene lo stream
//! del sistema), uno per i fotogrammi; tutti si fermano quando il lettore viene chiuso.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::gfx::Image;

/// Lato lungo dei fotogrammi mostrati durante la riproduzione.
#[cfg(any(windows, target_os = "linux"))]
const SIDE: u32 = 960;
/// La posizione dell'audio in fotogrammi del file, con 10 bit di frazione (per ricampionare).
const FRAC: u32 = 10;

struct Shared {
    stop: AtomicBool,
    paused: AtomicBool,
    ended: AtomicBool,
    /// L'audio si sta ancora decodificando: l'orologio è fermo.
    loading: AtomicBool,
    has_audio: AtomicBool,
    audio_pos: AtomicU64,
    audio_len: AtomicU64,
    audio_rate: AtomicU32,
    /// Cambia a ogni salto: il thread dei fotogrammi vecchio si ferma.
    generation: AtomicU64,
    /// Orologio senza audio: secondi già passati e da quando corre.
    clock: Mutex<(f64, Option<Instant>)>,
    frame: Mutex<Option<Image>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn position(&self) -> f64 {
        if self.has_audio.load(Ordering::Relaxed) {
            let rate = self.audio_rate.load(Ordering::Relaxed).max(1) as f64;
            return (self.audio_pos.load(Ordering::Relaxed) >> FRAC) as f64 / rate;
        }
        let c = self.clock.lock().unwrap_or_else(|e| e.into_inner());
        c.0 + c.1.map_or(0.0, |t| t.elapsed().as_secs_f64())
    }

    fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        let mut c = self.clock.lock().unwrap_or_else(|e| e.into_inner());
        if paused {
            if let Some(t) = c.1.take() {
                c.0 += t.elapsed().as_secs_f64();
            }
        } else if c.1.is_none() && !self.loading.load(Ordering::Relaxed) {
            c.1 = Some(Instant::now());
        }
    }

    fn jump(&self, secs: f64) {
        let rate = self.audio_rate.load(Ordering::Relaxed) as f64;
        self.audio_pos.store(((secs * rate) as u64) << FRAC, Ordering::Relaxed);
        let mut c = self.clock.lock().unwrap_or_else(|e| e.into_inner());
        c.0 = secs;
        if c.1.is_some() {
            c.1 = Some(Instant::now());
        }
    }

    fn finish(&self) {
        self.ended.store(true, Ordering::Relaxed);
        self.set_paused(true);
        (self.wake)();
    }
}

/// Un file in riproduzione (o in pausa) nell'anteprima.
pub struct Player {
    pub path: PathBuf,
    pub duration: Option<f64>,
    video: bool,
    sh: Arc<Shared>,
}

impl Player {
    /// Parte subito dall'inizio. `video` mostra anche i fotogrammi.
    pub fn start(path: &Path, video: bool, duration: Option<f64>, wake: Arc<dyn Fn() + Send + Sync>) -> Player {
        let sh = Arc::new(Shared {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            loading: AtomicBool::new(true),
            has_audio: AtomicBool::new(false),
            audio_pos: AtomicU64::new(0),
            audio_len: AtomicU64::new(0),
            audio_rate: AtomicU32::new(44_100),
            generation: AtomicU64::new(0),
            clock: Mutex::new((0.0, None)),
            frame: Mutex::new(None),
            wake,
        });
        let p = Player { path: path.to_path_buf(), duration, video, sh };
        p.spawn_audio();
        if video {
            p.spawn_video(0.0);
        }
        p
    }

    pub fn position(&self) -> f64 {
        let p = self.sh.position();
        self.duration.map_or(p, |d| p.min(d))
    }

    /// Vero mentre suona (anche quando sta ancora preparando l'audio).
    pub fn playing(&self) -> bool {
        !self.sh.paused.load(Ordering::Relaxed) && !self.sh.ended.load(Ordering::Relaxed)
    }

    pub fn loading(&self) -> bool {
        self.sh.loading.load(Ordering::Relaxed)
    }

    /// L'ultimo fotogramma del video da mostrare.
    pub fn frame(&self) -> Option<Image> {
        self.sh.frame.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Play o pausa; finito il file, ricomincia dall'inizio.
    pub fn toggle(&mut self) {
        if self.sh.ended.swap(false, Ordering::Relaxed) {
            self.seek(0.0);
            self.sh.set_paused(false);
            return;
        }
        let paused = self.sh.paused.load(Ordering::Relaxed);
        self.sh.set_paused(!paused);
    }

    /// Salta al secondo `secs`.
    pub fn seek(&mut self, secs: f64) {
        let secs = self.duration.map_or(secs, |d| secs.min((d - 0.05).max(0.0))).max(0.0);
        self.sh.ended.store(false, Ordering::Relaxed);
        self.sh.jump(secs);
        if self.video {
            self.spawn_video(secs);
        }
        (self.sh.wake)();
    }

    fn spawn_audio(&self) {
        let sh = self.sh.clone();
        let path = self.path.clone();
        std::thread::spawn(move || audio_thread(&path, sh));
    }

    #[allow(unused_variables)]
    fn spawn_video(&self, from: f64) {
        let generation = self.sh.generation.fetch_add(1, Ordering::Relaxed) + 1;
        #[cfg(any(windows, target_os = "linux"))]
        {
            let sh = self.sh.clone();
            let path = self.path.clone();
            std::thread::spawn(move || video_thread(&path, from, generation, sh));
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.sh.stop.store(true, Ordering::Relaxed);
    }
}

// ---- Audio ----

fn audio_thread(path: &Path, sh: Arc<Shared>) {
    let pcm = crate::audio::decode(path).ok().filter(|p| !p.samples.is_empty() && p.channels > 0);
    #[cfg(any(windows, target_os = "linux"))]
    if let Some(pcm) = pcm {
        let pcm = Arc::new(pcm);
        let frames = (pcm.samples.len() / pcm.channels as usize) as u64;
        sh.audio_rate.store(pcm.rate.max(1), Ordering::Relaxed);
        sh.audio_len.store(frames << FRAC, Ordering::Relaxed);
        // Un salto fatto mentre si decodificava vale ancora: l'orologio senza audio sa dov'era.
        let start = sh.clock.lock().unwrap_or_else(|e| e.into_inner()).0;
        sh.audio_pos.store(((start * pcm.rate as f64) as u64) << FRAC, Ordering::Relaxed);
        if let Ok(stream) = open_stream(pcm, sh.clone()) {
            use cpal::traits::StreamTrait;
            sh.has_audio.store(true, Ordering::Relaxed);
            sh.loading.store(false, Ordering::Relaxed);
            let _ = stream.play();
            (sh.wake)();
            let mut last = Instant::now();
            while !sh.stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(30));
                if sh.audio_pos.load(Ordering::Relaxed) >= sh.audio_len.load(Ordering::Relaxed) && !sh.ended.load(Ordering::Relaxed) {
                    sh.finish();
                }
                // Il tempo scritto accanto alla barra va aggiornato anche senza fotogrammi.
                if last.elapsed() > Duration::from_millis(250) && !sh.paused.load(Ordering::Relaxed) {
                    last = Instant::now();
                    (sh.wake)();
                }
            }
            return;
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = pcm;
    // Niente audio (o niente casse): corre l'orologio, così il video si vede lo stesso.
    sh.loading.store(false, Ordering::Relaxed);
    if !sh.paused.load(Ordering::Relaxed) {
        let mut c = sh.clock.lock().unwrap_or_else(|e| e.into_inner());
        c.1 = Some(Instant::now());
    }
    (sh.wake)();
}

#[cfg(any(windows, target_os = "linux"))]
fn open_stream(pcm: Arc<crate::audio::Pcm>, sh: Arc<Shared>) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    use cpal::SampleFormat;
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("nessuna uscita audio")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    // cpal 0.17+ riporta anche i 24 bit (I24) e F64: con il ripiego su f32 quei dispositivi resterebbero muti.
    match format {
        SampleFormat::I16 => build::<i16>(&device, &config, pcm, sh),
        SampleFormat::U16 => build::<u16>(&device, &config, pcm, sh),
        SampleFormat::I24 => build::<cpal::I24>(&device, &config, pcm, sh),
        SampleFormat::I32 => build::<i32>(&device, &config, pcm, sh),
        SampleFormat::F64 => build::<f64>(&device, &config, pcm, sh),
        _ => build::<f32>(&device, &config, pcm, sh),
    }
}

#[cfg(any(windows, target_os = "linux"))]
fn build<T>(device: &cpal::Device, config: &cpal::StreamConfig, pcm: Arc<crate::audio::Pcm>, sh: Arc<Shared>) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    use cpal::traits::DeviceTrait;
    let out_ch = config.channels.max(1) as usize;
    let in_ch = pcm.channels.max(1) as usize;
    let frames = (pcm.samples.len() / in_ch) as u64;
    // Passo in fotogrammi del file per ogni fotogramma delle casse (ricampionamento semplice).
    let step = ((pcm.rate as f64 / config.sample_rate.max(1) as f64) * (1u64 << FRAC) as f64) as u64;
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                let silent = T::from_sample(0.0f32);
                if sh.paused.load(Ordering::Relaxed) || sh.stop.load(Ordering::Relaxed) {
                    data.iter_mut().for_each(|s| *s = silent);
                    return;
                }
                let mut pos = sh.audio_pos.load(Ordering::Relaxed);
                for frame in data.chunks_mut(out_ch) {
                    let i = (pos >> FRAC) as usize;
                    if i as u64 >= frames {
                        frame.iter_mut().for_each(|s| *s = silent);
                        continue;
                    }
                    for (c, s) in frame.iter_mut().enumerate() {
                        // Mono su tutte le casse; più canali delle casse: si prendono i primi.
                        let v = pcm.samples[i * in_ch + c.min(in_ch - 1)];
                        *s = T::from_sample(v as f32 / 32768.0);
                    }
                    pos += step;
                }
                sh.audio_pos.store(pos, Ordering::Relaxed);
            },
            |err| eprintln!("audio dell'anteprima: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}

// ---- Video ----

#[cfg(any(windows, target_os = "linux"))]
fn alive(sh: &Shared, generation: u64) -> bool {
    !sh.stop.load(Ordering::Relaxed) && sh.generation.load(Ordering::Relaxed) == generation
}

#[cfg(any(windows, target_os = "linux"))]
fn video_thread(path: &Path, from: f64, generation: u64, sh: Arc<Shared>) {
    let shared = sh.clone();
    let mut first = true;
    let mut on_frame = move |t100: i64, make: &mut dyn FnMut() -> Option<crate::softvideo::Rgba>| -> bool {
        let sh = &shared;
        if !alive(sh, generation) {
            return false;
        }
        let t = t100 as f64 / 10_000_000.0;
        // Su Linux si legge dall'inizio: fino al punto del salto si va avanti senza mostrare.
        if t + 0.02 < from {
            return true;
        }
        // Il primo fotogramma dopo un salto si mostra subito, anche in pausa.
        if !first {
            loop {
                if !alive(sh, generation) {
                    return false;
                }
                let waiting = sh.loading.load(Ordering::Relaxed) || sh.paused.load(Ordering::Relaxed);
                let now = sh.position();
                if !waiting && now + 0.005 >= t {
                    // In ritardo di troppo: questo fotogramma si salta.
                    if now - t > 0.12 {
                        return true;
                    }
                    break;
                }
                let wait = if waiting { 0.015 } else { (t - now).clamp(0.002, 0.02) };
                std::thread::sleep(Duration::from_secs_f64(wait));
            }
        }
        first = false;
        if let Some((w, h, rgba)) = make() {
            if let Some(img) = Image::from_rgba(w, h, &rgba) {
                *sh.frame.lock().unwrap_or_else(|e| e.into_inner()) = Some(img);
                (sh.wake)();
            }
        }
        true
    };
    #[cfg(windows)]
    let _ = crate::video::play_frames(path, from, SIDE, &mut on_frame);
    #[cfg(target_os = "linux")]
    let _ = crate::linux_video::play_frames(path, move |y, u, v, w, h, t| on_frame(t, &mut || crate::softvideo::i420_small(y, u, v, w, h, SIDE)));
    // Senza audio la fine del video è la fine del file.
    if alive(&sh, generation) && !sh.has_audio.load(Ordering::Relaxed) && !sh.loading.load(Ordering::Relaxed) {
        sh.finish();
    }
}
