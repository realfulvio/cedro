//! Le accelerazioni: la scheda video (anche quella integrata nel processore) per codificare i video,
//! AVX-512 (x86) o i8mm (ARM) per i calcoli di OCR e trascrizione.
//!
//! Cedro le cerca nell'hardware a ogni avvio. Un interruttore acceso non basta: se il PC non le ha
//! restano spente, e se la scheda video rifiuta un lavoro il video si rifà col processore.

use std::sync::OnceLock;

/// Quello che Cedro ha trovato su questo PC.
#[derive(Clone, Debug, Default)]
pub struct Hardware {
    /// Il codificatore H.264 della scheda video, per nome; `None` se non c'è.
    pub gpu: Option<String>,
    /// Il processore ha le istruzioni avanzate dei calcoli: AVX-512 (F, VL, BW e DQ) su x86, i8mm su ARM.
    /// Il campo si chiama come l'impostazione («avx512»), che vale per entrambe le famiglie.
    pub avx512: bool,
    /// Solo ARM: il processore ha le istruzioni dotprod.
    pub dotprod: bool,
}

/// L'hardware di questo PC, letto la prima volta che serve.
pub fn hardware() -> &'static Hardware {
    static FOUND: OnceLock<Hardware> = OnceLock::new();
    FOUND.get_or_init(|| Hardware { gpu: gpu_encoder(), avx512: cpu_boost(), dotprod: cpu_dotprod() })
}

/// La scheda video codifica i video: c'è e l'interruttore è acceso.
pub fn use_gpu() -> bool {
    use_gpu_with(&crate::read_settings())
}

/// Come `use_gpu`, con le impostazioni già lette (una conversione non rilegge il file).
pub fn use_gpu_with(settings: &crate::settings::Settings) -> bool {
    settings.gpu && hardware().gpu.is_some()
}

/// I calcoli usano AVX-512: il processore lo ha e l'interruttore è acceso.
pub fn use_avx512() -> bool {
    crate::read_settings().avx512 && hardware().avx512
}

/// Applica l'interruttore AVX-512 ai calcoli di OCR (rten). Va chiamata prima di ogni lavoro che li usa.
pub fn apply() {
    rten_simd::allow_avx512(use_avx512());
    set_in_use(false, cpu_isa());
}

/// Cosa sta accelerando il lavoro di ora, per la spia nella finestra: scheda video o istruzioni del processore.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InUse {
    /// Vero se è la scheda video, falso se è il processore.
    pub gpu: bool,
    /// Il nome preciso: «AVX-512 · 8 thread», «Vulkan · NVIDIA RTX 4000», «NVIDIA NVENC».
    pub detail: String,
}

static IN_USE: std::sync::Mutex<Option<InUse>> = std::sync::Mutex::new(None);

/// Lo dicono i motori di lavoro quando partono: la spia mostra solo quello che è davvero in uso.
pub fn set_in_use(gpu: bool, detail: impl Into<String>) {
    if let Ok(mut g) = IN_USE.lock() {
        *g = Some(InUse { gpu, detail: detail.into() });
    }
}

/// Fine del lavoro (o del file): nessuna accelerazione in uso.
pub fn clear_in_use() {
    if let Ok(mut g) = IN_USE.lock() {
        *g = None;
    }
}

pub fn in_use() -> Option<InUse> {
    IN_USE.lock().ok().and_then(|g| g.clone())
}

/// Le istruzioni del processore che i calcoli usano adesso, col loro nome.
pub fn cpu_isa() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        let hw = hardware();
        if use_avx512() && hw.dotprod {
            return "i8mm";
        }
        if hw.dotprod {
            return "dotprod";
        }
        "NEON"
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        if use_avx512() {
            return "AVX-512";
        }
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") {
            return "AVX2";
        }
        "SSE"
    }
}

/// Il nome delle istruzioni avanzate di questo processore, come lo legge una persona.
pub const BOOST_NAME: &str = if cfg!(target_arch = "aarch64") { "i8mm" } else { "AVX-512" };

/// Le istruzioni avanzate che il processore ha, per l'etichetta «Trovato: …».
pub const BOOST_FOUND: &str = if cfg!(target_arch = "aarch64") { "i8mm (moltiplicazione di matrici a 8 bit)" } else { "AVX-512 F, VL, BW, DQ" };

fn cpu_boost() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        arm_features().1
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        cpu_avx512()
    }
}

fn cpu_dotprod() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        arm_features().0
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false
    }
}

/// (dotprod, i8mm) del processore ARM.
#[cfg(target_arch = "aarch64")]
fn arm_features() -> (bool, bool) {
    static FOUND: OnceLock<(bool, bool)> = OnceLock::new();
    *FOUND.get_or_init(|| {
        #[cfg(windows)]
        {
            // Windows non dice «i8mm» a IsProcessorFeaturePresent: lo si legge dal registro di ID_AA64ISAR0/1_EL1.
            let isar = |name: &str| registry_qword(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", name);
            let (dp, i8) = (isar("CP 4030").map(|v| arm_isar0_dotprod(v)), isar("CP 4031").map(|v| arm_isar1_i8mm(v)));
            (dp.unwrap_or_else(|| std::arch::is_aarch64_feature_detected!("dotprod")), i8.unwrap_or(false))
        }
        #[cfg(not(windows))]
        {
            (std::arch::is_aarch64_feature_detected!("dotprod"), std::arch::is_aarch64_feature_detected!("i8mm"))
        }
    })
}

/// Un valore QWORD di HKEY_LOCAL_MACHINE (Windows ARM tiene lì i registri ID_AA64*_EL1 come «CP 4030…»).
#[cfg(all(windows, target_arch = "aarch64"))]
fn registry_qword(key: &str, name: &str) -> Option<u64> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_QWORD};
    let wide = |t: &str| t.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (key, name) = (wide(key), wide(name));
    let mut value = 0u64;
    let mut size = std::mem::size_of::<u64>() as u32;
    let status = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()), RRF_RT_REG_QWORD, None, Some(&mut value as *mut u64 as *mut _), Some(&mut size)) };
    status.is_ok().then_some(value)
}

/// ID_AA64ISAR0_EL1.DP (bit 47:44): 1 = dotprod.
#[allow(dead_code)]
pub fn arm_isar0_dotprod(value: u64) -> bool {
    (value >> 44) & 0xf >= 1
}

/// ID_AA64ISAR1_EL1.I8MM (bit 55:52): 1 = i8mm.
#[allow(dead_code)]
pub fn arm_isar1_i8mm(value: u64) -> bool {
    (value >> 52) & 0xf >= 1
}

#[allow(dead_code)]
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
    #[cfg(target_os = "android")]
    {
        crate::mediacodec::probe()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
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
        if !cpu_boost() {
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

    #[test]
    fn arm_registers_are_decoded() {
        // Neoverse N2 (Cobalt 100): DP = 1 in ISAR0, I8MM = 1 in ISAR1.
        assert!(arm_isar0_dotprod(1 << 44));
        assert!(!arm_isar0_dotprod(0));
        assert!(arm_isar1_i8mm(1 << 52));
        assert!(!arm_isar1_i8mm(0xf << 48));
    }
}
