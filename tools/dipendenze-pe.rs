// Elenca le DLL che un programma Windows (.exe o .dll) importa e fallisce se dipende dal runtime di Visual C++
// (vcruntime140, msvcp140, vcomp140, concrt140, msvcr1xx…): sono le DLL che mancano su un Windows appena installato
// e che fanno comparire «VCRUNTIME140.dll non è stato trovato». Cedro e i suoi motori linkano il runtime dentro
// il file (+crt-static), quindi qui non deve comparire nessuna di queste DLL.
//
// Solo libreria standard, un file solo: si compila con `rustc -O tools/dipendenze-pe.rs -o dipendenze.exe`.
// Uso:  dipendenze file1.exe [file2.exe ...]
// Esce con 1 se almeno un file dipende dal runtime di Visual C++ o non è un eseguibile PE valido.
// Le DLL del runtime universale (api-ms-win-crt-*, ucrtbase) fanno solo avvisare: in Windows 10 e 11 ci sono già.
use std::process::ExitCode;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Sezioni del file: (indirizzo virtuale, dimensione virtuale, posizione nel file, dimensione nel file).
type Sezioni = Vec<(u32, u32, u32, u32)>;

fn rva_a_file(sezioni: &Sezioni, rva: u32) -> Option<usize> {
    for &(va, vsize, ptr, raw) in sezioni {
        let len = vsize.max(raw);
        if rva >= va && rva < va + len {
            return Some((rva - va + ptr) as usize);
        }
    }
    None
}

fn stringa_a(b: &[u8], off: usize) -> Option<String> {
    let resto = b.get(off..)?;
    let fine = resto.iter().position(|&c| c == 0)?;
    Some(String::from_utf8_lossy(&resto[..fine]).into_owned())
}

/// Le DLL importate (normali e a caricamento ritardato), nell'ordine in cui compaiono.
fn importate(b: &[u8]) -> Result<Vec<String>, String> {
    if b.get(0..2) != Some(b"MZ") {
        return Err("non è un eseguibile Windows (manca MZ)".into());
    }
    let pe = u32_at(b, 0x3c).ok_or("intestazione DOS troncata")? as usize;
    if b.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err("manca la firma PE".into());
    }
    let n_sezioni = u16_at(b, pe + 6).ok_or("intestazione troncata")? as usize;
    let dim_opt = u16_at(b, pe + 20).ok_or("intestazione troncata")? as usize;
    let opt = pe + 24;
    let (dir_off, n_dir_off) = match u16_at(b, opt) {
        Some(0x10b) => (opt + 96, opt + 92),
        Some(0x20b) => (opt + 112, opt + 108),
        _ => return Err("intestazione opzionale sconosciuta".into()),
    };
    let n_dir = u32_at(b, n_dir_off).ok_or("intestazione troncata")? as usize;
    let mut sezioni: Sezioni = Vec::new();
    for i in 0..n_sezioni {
        let s = opt + dim_opt + i * 40;
        sezioni.push((
            u32_at(b, s + 12).ok_or("tabella sezioni troncata")?,
            u32_at(b, s + 8).ok_or("tabella sezioni troncata")?,
            u32_at(b, s + 20).ok_or("tabella sezioni troncata")?,
            u32_at(b, s + 16).ok_or("tabella sezioni troncata")?,
        ));
    }
    let mut dll = Vec::new();
    // Directory 1: importazioni normali (20 byte l'una, fino a una tutta a zero); il nome è al byte 12.
    if n_dir > 1 {
        let rva = u32_at(b, dir_off + 8).unwrap_or(0);
        if rva != 0 {
            let mut o = rva_a_file(&sezioni, rva).ok_or("tabella delle importazioni fuori dal file")?;
            loop {
                let d = b.get(o..o + 20).ok_or("tabella delle importazioni troncata")?;
                if d.iter().all(|&x| x == 0) {
                    break;
                }
                let nome = u32_at(b, o + 12).unwrap_or(0);
                let f = rva_a_file(&sezioni, nome).ok_or("nome di una DLL fuori dal file")?;
                dll.push(stringa_a(b, f).ok_or("nome di una DLL senza fine")?);
                o += 20;
            }
        }
    }
    // Directory 13: importazioni a caricamento ritardato (32 byte l'una); il nome è al byte 4 (RVA se il bit 0 è acceso).
    if n_dir > 13 {
        let rva = u32_at(b, dir_off + 13 * 8).unwrap_or(0);
        if rva != 0 {
            let mut o = rva_a_file(&sezioni, rva).ok_or("importazioni ritardate fuori dal file")?;
            loop {
                let d = b.get(o..o + 32).ok_or("importazioni ritardate troncate")?;
                if d.iter().all(|&x| x == 0) {
                    break;
                }
                let attr = u32_at(b, o).unwrap_or(0);
                let nome = u32_at(b, o + 4).unwrap_or(0);
                if attr & 1 != 0 {
                    let f = rva_a_file(&sezioni, nome).ok_or("nome di una DLL ritardata fuori dal file")?;
                    dll.push(format!("{} (a caricamento ritardato)", stringa_a(b, f).ok_or("nome senza fine")?));
                }
                o += 32;
            }
        }
    }
    Ok(dll)
}

/// Il runtime di Visual C++ che non c'è su un Windows pulito.
fn e_vcruntime(nome: &str) -> bool {
    let n = nome.to_ascii_lowercase();
    let n = n.split(' ').next().unwrap_or("");
    ["vcruntime", "msvcp", "vcomp", "concrt", "vccorlib", "mfc"].iter().any(|p| n.starts_with(p))
        // msvcr100.dll, msvcr120.dll…: sì; msvcrt.dll è di Windows: no.
        || (n.starts_with("msvcr") && n.as_bytes().get(5).is_some_and(|c| c.is_ascii_digit()))
}

/// Il runtime C universale: in Windows 10 e 11 c'è, in Windows 7 e 8 no.
fn e_ucrt(nome: &str) -> bool {
    let n = nome.to_ascii_lowercase();
    n.starts_with("api-ms-win-crt-") || n.starts_with("ucrtbase")
}

fn main() -> ExitCode {
    let file: Vec<String> = std::env::args().skip(1).collect();
    if file.is_empty() {
        eprintln!("uso: dipendenze file.exe [altro.exe ...]");
        return ExitCode::from(2);
    }
    let mut male = false;
    for f in &file {
        let b = match std::fs::read(f) {
            Ok(b) => b,
            Err(e) => {
                println!("ERRORE {f}: {e}");
                male = true;
                continue;
            }
        };
        match importate(&b) {
            Err(e) => {
                println!("ERRORE {f}: {e}");
                male = true;
            }
            Ok(dll) => {
                let vc: Vec<&String> = dll.iter().filter(|d| e_vcruntime(d)).collect();
                let ucrt = dll.iter().filter(|d| e_ucrt(d)).count();
                let stato = if !vc.is_empty() { "DIPENDE DA VISUAL C++" } else { "ok" };
                println!("{stato}  {f}");
                println!("    importa: {}", dll.join(", "));
                if !vc.is_empty() {
                    male = true;
                    println!("    ✗ mancano su un Windows pulito: {}", vc.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
                }
                if ucrt > 0 {
                    println!("    avviso: {ucrt} DLL del runtime C universale (ci sono in Windows 10 e 11, non in Windows 7 e 8)");
                }
            }
        }
    }
    if male {
        println!("\nNo: almeno un file dipende dal runtime di Visual C++.");
        ExitCode::from(1)
    } else {
        println!("\nSì: nessun file dipende dal runtime di Visual C++.");
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn riconosce_il_runtime_di_visual_cpp() {
        for d in ["VCRUNTIME140.dll", "VCRUNTIME140_1.dll", "MSVCP140.dll", "vcomp140.dll", "concrt140.dll", "MSVCR120.dll", "VCRUNTIME140.dll (a caricamento ritardato)"] {
            assert!(e_vcruntime(d), "{d}");
        }
        for d in ["KERNEL32.dll", "msvcrt.dll", "vulkan-1.dll", "api-ms-win-crt-runtime-l1-1-0.dll", "WINMM.dll"] {
            assert!(!e_vcruntime(d), "{d}");
        }
        assert!(e_ucrt("api-ms-win-crt-stdio-l1-1-0.dll") && e_ucrt("ucrtbase.dll") && !e_ucrt("KERNEL32.dll"));
    }

    #[test]
    fn legge_una_tabella_di_importazioni_minima() {
        // PE32+ minimo: una sezione .idata a 0x1000 (file 0x200) con KERNEL32.dll e VCRUNTIME140.dll.
        let mut b = vec![0u8; 0x400];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        b[0x40..0x44].copy_from_slice(b"PE\0\0");
        b[0x40 + 6..0x40 + 8].copy_from_slice(&1u16.to_le_bytes()); // sezioni
        b[0x40 + 20..0x40 + 22].copy_from_slice(&240u16.to_le_bytes()); // dimensione intestazione opzionale
        let opt = 0x40 + 24;
        b[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        b[opt + 108..opt + 112].copy_from_slice(&16u32.to_le_bytes());
        // directory 1: importazioni a RVA 0x1000
        b[opt + 112 + 8..opt + 112 + 12].copy_from_slice(&0x1000u32.to_le_bytes());
        let sez = opt + 240;
        b[sez + 8..sez + 12].copy_from_slice(&0x200u32.to_le_bytes()); // dimensione virtuale
        b[sez + 12..sez + 16].copy_from_slice(&0x1000u32.to_le_bytes()); // indirizzo virtuale
        b[sez + 16..sez + 20].copy_from_slice(&0x200u32.to_le_bytes()); // dimensione nel file
        b[sez + 20..sez + 24].copy_from_slice(&0x200u32.to_le_bytes()); // posizione nel file
        let base = 0x200;
        b[base + 12..base + 16].copy_from_slice(&0x1080u32.to_le_bytes()); // nome della prima DLL
        b[base + 20 + 12..base + 20 + 16].copy_from_slice(&0x1090u32.to_le_bytes()); // della seconda
        b[base + 0x80..base + 0x80 + 13].copy_from_slice(b"KERNEL32.dll\0");
        b[base + 0x90..base + 0x90 + 17].copy_from_slice(b"VCRUNTIME140.dll\0");
        assert_eq!(importate(&b).unwrap(), vec!["KERNEL32.dll".to_string(), "VCRUNTIME140.dll".to_string()]);
    }
}
