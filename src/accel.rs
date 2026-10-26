//! Le accelerazioni: la scheda video (anche quella integrata nel processore) per codificare i video,
//! AVX-512 per i calcoli di OCR e trascrizione.
//!
//! Cedro le cerca nell'hardware a ogni avvio. Un interruttore acceso non basta: se il PC non le ha
//! restano spente, e se la scheda video rifiuta un lavoro il video si rifà col processore.

use std::sync::OnceLock;

/// Quello che Cedro ha trovato su questo PC.
#[derive(Clone, Debug, Default)]
pub struct Hardware {
    /// Il codificatore H.264 della scheda video, per nome; `None` se non c'è.
    pub gpu: Option<String>,
    /// Il processore ha AVX-512 (F, VL, BW e DQ, come lo vogliono i calcoli).
    pub avx512: bool,
}

/// L'hardware di questo PC, letto la prima volta che serve.
pub fn hardware() -> &'static Hardware {
    static FOUND: OnceLock<Hardware> = OnceLock::new();
    FOUND.get_or_init(|| Hardware { gpu: gpu_encoder(), avx512: cpu_avx512() })
}

/// La scheda video codifica i video: c'è e l'interruttore è acceso.
pub fn use_gpu() -> bool {
    crate::read_settings().gpu && hardware().gpu.is_some()
}

/// I calcoli usano AVX-512: il processore lo ha e l'interruttore è acceso.
pub fn use_avx512() -> bool {
    crate::read_settings().avx512 && hardware().avx512
}

/// Applica l'interruttore AVX-512 ai calcoli di OCR (rten). Va chiamata prima di ogni lavoro che li usa.
pub fn apply() {
    rten_simd::allow_avx512(use_avx512());
}

fn cpu_avx512() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512vl")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512dq")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn gpu_encoder() -> Option<String> {
    #[cfg(windows)]
    {
        crate::video::hardware_encoder()
    }
    #[cfg(target_os = "linux")]
    {
        crate::vaapi::probe()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avx512_follows_the_cpu() {
        let found = hardware();
        if !cpu_avx512() {
            assert!(!found.avx512);
        }
        // Spento dall'interruttore, rten non lo usa nemmeno dove c'è.
        rten_simd::allow_avx512(false);
        #[cfg(target_arch = "x86_64")]
        assert!(!rten_simd::is_avx512_supported());
        rten_simd::allow_avx512(true);
        #[cfg(target_arch = "x86_64")]
        assert_eq!(rten_simd::is_avx512_supported(), cpu_avx512());
    }
}
