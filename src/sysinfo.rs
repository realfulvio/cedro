//! Che PC è questo: processore, memoria, scheda video, sistema e istruzioni del processore.
//! Serve alla scheda «Sistema» delle Impostazioni e a «Copia informazioni» per le segnalazioni.

use std::sync::OnceLock;

/// Quello che Cedro sa di questo PC. Ogni testo è già pronto da mostrare.
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub cpu: String,
    /// Sotto il processore: core, thread e frequenza.
    pub cpu_detail: String,
    pub ram: String,
    pub gpu: String,
    /// Sotto la scheda video: che codifica fa.
    pub gpu_detail: String,
    pub os: String,
    /// Le istruzioni del processore e se ci sono.
    pub isa: Vec<(&'static str, bool)>,
}

/// L'hardware letto la prima volta che serve.
pub fn info() -> &'static Info {
    static FOUND: OnceLock<Info> = OnceLock::new();
    FOUND.get_or_init(read)
}

fn read() -> Info {
    let hw = crate::accel::hardware();
    let mut info = platform();
    info.isa = isa();
    info.gpu_detail = match &hw.gpu {
        Some(encoder) => format!("Codifica H.264 con {}", encoder.rsplit(" · ").next().unwrap_or(encoder)),
        None => "Nessuna codifica video hardware".to_string(),
    };
    info
}

/// Le istruzioni che interessano ai calcoli di Cedro.
fn isa() -> Vec<(&'static str, bool)> {
    #[cfg(target_arch = "x86_64")]
    {
        vec![
            ("SSE4.2", std::is_x86_feature_detected!("sse4.2")),
            ("AVX", std::is_x86_feature_detected!("avx")),
            ("AVX2", std::is_x86_feature_detected!("avx2")),
            ("FMA", std::is_x86_feature_detected!("fma")),
            ("AVX-VNNI", std::is_x86_feature_detected!("avxvnni")),
            ("AVX-512", crate::accel::hardware().avx512),
        ]
    }
    #[cfg(target_arch = "aarch64")]
    {
        let hw = crate::accel::hardware();
        vec![("NEON", true), ("dotprod", hw.dotprod), ("i8mm", hw.avx512)]
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        Vec::new()
    }
}

/// Gigabyte a numero tondo: 15,4 GiB utilizzabili sono «16 GB» sull'etichetta.
fn gigabytes(bytes: u64) -> String {
    let gb = bytes as f64 / (1u64 << 30) as f64;
    format!("{} GB", gb.round().max(1.0) as u64)
}

fn ghz(mhz: u32) -> String {
    format!("{:.1}", mhz as f64 / 1000.0).replace('.', ",")
}

/// Testo da incollare in una segnalazione.
pub fn report() -> String {
    let i = info();
    let hw = crate::accel::hardware();
    let mut out = format!("Cedro {}\n", crate::update::CURRENT);
    out += &format!("Processore: {}", i.cpu);
    if !i.cpu_detail.is_empty() {
        out += &format!(" ({})", i.cpu_detail);
    }
    out += &format!("\nMemoria: {}\nScheda video: {} ({})\nSistema: {}\n", i.ram, i.gpu, i.gpu_detail, i.os);
    let chips: Vec<String> = i.isa.iter().map(|(n, y)| format!("{n} {}", if *y { "sì" } else { "no" })).collect();
    out += &format!("Istruzioni: {}\n", chips.join(", "));
    let s = crate::read_settings();
    out += &format!(
        "Accelerazione GPU: {}\nAccelerazione {}: {}\n",
        state(s.gpu, hw.gpu.is_some()),
        crate::accel::BOOST_NAME,
        state(s.avx512, hw.avx512)
    );
    out
}

fn state(on: bool, available: bool) -> &'static str {
    match (available, on) {
        (false, _) => "non disponibile",
        (true, true) => "accesa",
        (true, false) => "spenta",
    }
}

#[cfg(target_os = "linux")]
fn platform() -> Info {
    use std::collections::BTreeSet;
    use std::fs;

    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu = cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| clean(v))
        .unwrap_or_else(|| "Processore sconosciuto".into());

    // Core fisici: coppie (pacchetto, core) distinte; P ed E dai due insiemi dei processori ibridi.
    let cpu_ids = |list: &str| -> BTreeSet<u32> {
        let mut set = BTreeSet::new();
        for part in list.trim().split(',') {
            let (a, b) = part.split_once('-').unwrap_or((part, part));
            if let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                set.extend(a..=b);
            }
        }
        set
    };
    let all: BTreeSet<u32> = std::thread::available_parallelism().map(|n| (0..n.get() as u32).collect()).unwrap_or_default();
    let cores_of = |ids: &BTreeSet<u32>| -> usize {
        ids.iter()
            .filter_map(|n| {
                let base = format!("/sys/devices/system/cpu/cpu{n}/topology");
                let core = fs::read_to_string(format!("{base}/core_id")).ok()?;
                let package = fs::read_to_string(format!("{base}/physical_package_id")).unwrap_or_default();
                Some((package.trim().to_string(), core.trim().to_string()))
            })
            .collect::<BTreeSet<_>>()
            .len()
    };
    let p_ids = fs::read_to_string("/sys/devices/cpu_core/cpus").map(|l| cpu_ids(&l)).unwrap_or_default();
    let e_ids = fs::read_to_string("/sys/devices/cpu_atom/cpus").map(|l| cpu_ids(&l)).unwrap_or_default();
    let threads = all.len();
    let cores = cores_of(&all).max(1);
    let mut detail = if !p_ids.is_empty() && !e_ids.is_empty() {
        format!("{cores} core ({} P + {} E) · {threads} thread", cores_of(&p_ids), cores_of(&e_ids))
    } else {
        format!("{cores} core · {threads} thread")
    };
    let freq = |name: &str| -> Option<u32> {
        let khz: u32 = fs::read_to_string(format!("/sys/devices/system/cpu/cpu0/cpufreq/{name}")).ok()?.trim().parse().ok()?;
        Some(khz / 1000)
    };
    match (freq("cpuinfo_min_freq"), freq("cpuinfo_max_freq")) {
        (Some(min), Some(max)) if max > min => detail += &format!(" · {}–{} GHz", ghz(min), ghz(max)),
        (_, Some(max)) => detail += &format!(" · {} GHz", ghz(max)),
        _ => {}
    }

    let ram = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| m.lines().find_map(|l| l.strip_prefix("MemTotal:").map(str::to_string)))
        .and_then(|l| l.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
        .map(|kb| gigabytes(kb * 1024))
        .unwrap_or_default();

    let release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |key: &str| {
        release.lines().find_map(|l| l.strip_prefix(&format!("{key}="))).map(|v| v.trim_matches('"').to_string())
    };
    let distro = field("NAME").or_else(|| field("PRETTY_NAME")).unwrap_or_else(|| "Linux".into());
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let desktop = match desktop.split(':').next_back().unwrap_or("") {
        "KDE" => "KDE Plasma".to_string(),
        "GNOME" => "GNOME".to_string(),
        "" => String::new(),
        other => other.to_string(),
    };
    let session = match std::env::var("XDG_SESSION_TYPE").as_deref() {
        Ok("wayland") => " (Wayland)",
        Ok("x11") => " (X11)",
        _ => "",
    };
    let os = if desktop.is_empty() { distro } else { format!("{distro} · {desktop}{session}") };

    let gpu = match &crate::accel::hardware().gpu {
        Some(encoder) => {
            let name = encoder.split(" · ").next().unwrap_or(encoder);
            let integrated = fs::read_dir("/sys/class/drm")
                .map(|dir| dir.flatten().any(|e| e.file_name().to_string_lossy().starts_with("card")))
                .unwrap_or(false);
            if integrated && is_integrated(name) { format!("{name} (integrata)") } else { name.to_string() }
        }
        None => drm_gpu().unwrap_or_else(|| "Scheda video sconosciuta".into()),
    };
    Info { cpu, cpu_detail: detail, ram, gpu, gpu_detail: String::new(), os, isa: Vec::new() }
}

/// Il nome del driver è l'unico indizio: «AMD Radeon Graphics» e «Intel Gen Graphics» sono grafiche integrate.
#[cfg(target_os = "linux")]
fn is_integrated(name: &str) -> bool {
    let n = name.to_lowercase();
    n == "amd radeon graphics" || n.contains("intel") && !n.contains("arc")
}

/// Senza VA-API: il costruttore dal bus PCI (`/sys/class/drm/card*/device/vendor`).
#[cfg(target_os = "linux")]
fn drm_gpu() -> Option<String> {
    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let vendor = std::fs::read_to_string(entry.path().join("device/vendor")).ok()?;
        let label = match vendor.trim() {
            "0x1002" => "AMD",
            "0x10de" => "NVIDIA",
            "0x8086" => "Intel",
            "0x1af4" => "Red Hat Virtio",
            _ => continue,
        };
        return Some(format!("{label} (scheda video)"));
    }
    None
}

#[cfg(target_os = "linux")]
fn clean(text: &str) -> String {
    text.replace("(R)", "").replace("(TM)", "").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(windows)]
fn platform() -> Info {
    use windows::core::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::Registry::*;
    use windows::Win32::System::SystemInformation::*;

    fn text_value(key: HKEY, sub: PCWSTR, name: PCWSTR) -> Option<String> {
        let mut buf = [0u16; 256];
        let mut size = std::mem::size_of_val(&buf) as u32;
        let status = unsafe { RegGetValueW(key, sub, name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
    fn number_value(key: HKEY, sub: PCWSTR, name: PCWSTR) -> Option<u32> {
        let mut value = 0u32;
        let mut size = 4u32;
        let status = unsafe { RegGetValueW(key, sub, name, RRF_RT_REG_DWORD, None, Some(&mut value as *mut u32 as *mut _), Some(&mut size)) };
        (status == ERROR_SUCCESS).then_some(value)
    }

    let cpu_key = w!("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0");
    let cpu = text_value(HKEY_LOCAL_MACHINE, cpu_key, w!("ProcessorNameString"))
        .map(|s| clean(&s))
        .unwrap_or_else(|| "Processore sconosciuto".into());
    let mhz = number_value(HKEY_LOCAL_MACHINE, cpu_key, w!("~MHz"));

    // Core fisici, e per i processori ibridi quali sono P (classe più alta) ed E.
    let (mut cores, mut threads, mut classes) = (0u32, 0u32, std::collections::BTreeMap::<u8, u32>::new());
    unsafe {
        let mut len = 0u32;
        let _ = GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len);
        if len > 0 {
            let mut buf = vec![0u8; len as usize];
            let first = buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX;
            if GetLogicalProcessorInformationEx(RelationProcessorCore, Some(first), &mut len).is_ok() {
                let mut at = 0usize;
                while at + 8 <= len as usize {
                    let entry = &*(buf.as_ptr().add(at) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX);
                    let core = &entry.Anonymous.Processor;
                    cores += 1;
                    *classes.entry(core.EfficiencyClass).or_insert(0) += 1;
                    let masks = std::slice::from_raw_parts(core.GroupMask.as_ptr(), core.GroupCount as usize);
                    threads += masks.iter().map(|m| m.Mask.count_ones()).sum::<u32>();
                    at += entry.Size as usize;
                    if entry.Size == 0 {
                        break;
                    }
                }
            }
        }
    }
    if cores == 0 {
        cores = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
        threads = cores;
    }
    let mut detail = if classes.len() > 1 {
        let efficient = classes.values().next().copied().unwrap_or(0);
        format!("{cores} core ({} P + {efficient} E) · {threads} thread", cores - efficient)
    } else {
        format!("{cores} core · {threads} thread")
    };
    if let Some(mhz) = mhz {
        detail += &format!(" · {} GHz", ghz(mhz));
    }

    let ram = unsafe {
        let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        if GlobalMemoryStatusEx(&mut status).is_ok() { gigabytes(status.ullTotalPhys) } else { String::new() }
    };

    // Windows 11 dichiara ancora «Windows 10» nel nome: si distingue dal numero di build.
    let version = w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
    let build: u32 = text_value(HKEY_LOCAL_MACHINE, version, w!("CurrentBuildNumber")).and_then(|b| b.parse().ok()).unwrap_or(0);
    let mut name = text_value(HKEY_LOCAL_MACHINE, version, w!("ProductName")).unwrap_or_else(|| "Windows".into());
    if build >= 22000 {
        name = name.replace("Windows 10", "Windows 11");
    }
    let os = match text_value(HKEY_LOCAL_MACHINE, version, w!("DisplayVersion")) {
        Some(v) => format!("{name} {v} · 64 bit"),
        None => format!("{name} · 64 bit"),
    };

    // La prima scheda video che non sia quella di base di Microsoft.
    let mut gpu = String::new();
    unsafe {
        let mut n = 0;
        loop {
            let mut device = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
            if !EnumDisplayDevicesW(None, n, &mut device, 0).as_bool() {
                break;
            }
            n += 1;
            let len = device.DeviceString.iter().position(|c| *c == 0).unwrap_or(device.DeviceString.len());
            let found = String::from_utf16_lossy(&device.DeviceString[..len]);
            if found.is_empty() {
                continue;
            }
            if gpu.is_empty() || gpu.contains("Basic") {
                gpu = clean(&found);
            }
        }
    }
    if gpu.is_empty() {
        gpu = "Scheda video sconosciuta".into();
    }
    Info { cpu, cpu_detail: detail, ram, gpu, gpu_detail: String::new(), os, isa: Vec::new() }
}

#[cfg(windows)]
fn clean(text: &str) -> String {
    text.replace("(R)", "").replace("(TM)", "").replace("®", "").replace("™", "").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(not(any(windows, target_os = "linux")))]
fn platform() -> Info {
    Info { cpu: "Processore sconosciuto".into(), gpu: "Scheda video sconosciuta".into(), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gigabytes_round_to_the_label() {
        assert_eq!(gigabytes(15_400_000_000), "14 GB");
        assert_eq!(gigabytes(16 * (1u64 << 30)), "16 GB");
        assert_eq!(gigabytes(15 * (1u64 << 30) + (1u64 << 29) + 1), "16 GB");
    }

    #[test]
    fn frequencies_use_the_italian_comma() {
        assert_eq!(ghz(2100), "2,1");
        assert_eq!(ghz(4900), "4,9");
    }

    #[test]
    fn the_report_names_everything() {
        let text = report();
        for word in ["Processore:", "Memoria:", "Scheda video:", "Sistema:", "Accelerazione GPU:"] {
            assert!(text.contains(word), "{word} manca in:\n{text}");
        }
        assert!(text.contains(&format!("Accelerazione {}:", crate::accel::BOOST_NAME)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_pc_has_a_cpu_and_memory() {
        let i = info();
        assert!(!i.cpu.is_empty() && i.cpu != "Processore sconosciuto", "{i:?}");
        assert!(i.ram.ends_with("GB"), "{i:?}");
        assert!(i.cpu_detail.contains("thread"), "{i:?}");
    }
}
