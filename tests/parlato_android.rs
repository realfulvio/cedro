//! Lo stesso sorgente usato dal motore desktop e da whisper Android, senza caricare Whisper.
#[path = "../trascrivi/src/parlato.rs"]
mod parlato;

mod decode {
    // Solo per il test ignorato opt-in di voce reale: la fixture dichiarata è PCM mono 16 kHz.
    pub fn to_16k_mono(path: &std::path::Path) -> Result<Vec<f32>,String> {
        let mut wav=hound::WavReader::open(path).map_err(|e|e.to_string())?;
        let spec=wav.spec();
        if spec.sample_rate!=16000||spec.channels!=1||spec.bits_per_sample!=16{return Err("Serve PCM mono 16 kHz a 16 bit".into());}
        wav.samples::<i16>().map(|s|s.map(|v|v as f32/32768.0).map_err(|e|e.to_string())).collect()
    }
}
