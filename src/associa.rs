//! Il doppio clic sui file firmati: .p7m, .p7s, .m7m, .tsd, .tsr e .p7e si aprono nel visualizzatore
//! di Cedro (`cedro verifica <file>`). È una scelta di Impostazioni nella «Firma digitale», accesa
//! di serie, e si può spegnere quando si vuole: Cedro scrive (o toglie) le sue voci solo per l'utente.
//! I PDF restano al lettore PDF, a meno che si accenda anche «Anche i PDF firmati»; Cedro c'è comunque
//! nel menu «Apri con».

/// Le estensioni dei file firmati che Cedro apre col doppio clic.
pub const EXTENSIONS: &[&str] = &["p7m", "p7s", "m7m", "tsd", "tsr", "p7e"];

/// Scrive o toglie le associazioni. `on`: i file firmati; `pdf`: anche i PDF.
pub fn apply(on: bool, pdf: bool) -> Result<(), String> {
    imp::apply(on, pdf)
}

#[cfg(windows)]
mod imp {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

    const PROGID: &str = "Cedro.Firmato";
    const CLASSES: &str = r"Software\Classes";

    fn set(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
        let key = HSTRING::from(format!(r"{CLASSES}\{key}"));
        let name = name.map(HSTRING::from);
        let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null()),
                REG_SZ.0,
                Some(data.as_ptr() as *const _),
                (data.len() * 2) as u32,
            )
            .ok()
            .map_err(|e| e.to_string())
        }
    }

    fn get(key: &str, name: Option<&str>) -> Option<String> {
        let key = HSTRING::from(format!(r"{CLASSES}\{key}"));
        let name = name.map(HSTRING::from);
        let pname = name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null());
        let mut buf = vec![0u16; 512];
        let mut len = (buf.len() * 2) as u32;
        unsafe {
            RegGetValueW(HKEY_CURRENT_USER, &key, pname, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut len)).ok().ok()?;
        }
        let n = (len as usize / 2).saturating_sub(1).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }

    fn unset(key: &str, name: Option<&str>) {
        let key = HSTRING::from(format!(r"{CLASSES}\{key}"));
        let name = name.map(HSTRING::from);
        unsafe {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &key, name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null()));
        }
    }

    /// Il tipo di file predefinito di un'estensione diventa Cedro (o smette di esserlo, se lo era).
    fn default_for(ext: &str, on: bool) -> Result<(), String> {
        let key = format!(".{ext}");
        if on {
            set(&format!(r"{key}\OpenWithProgids"), Some(PROGID), "")?;
            set(&key, None, PROGID)
        } else {
            if get(&key, None).as_deref() == Some(PROGID) {
                unset(&key, None);
            }
            Ok(())
        }
    }

    pub fn apply(on: bool, pdf: bool) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.display().to_string();
        if on || pdf {
            set(PROGID, None, "File firmato digitalmente")?;
            set(&format!(r"{PROGID}\DefaultIcon"), None, &format!("\"{exe}\",0"))?;
            set(&format!(r"{PROGID}\shell\open"), None, "Apri e verifica con Cedro")?;
            set(&format!(r"{PROGID}\shell\open\command"), None, &format!("\"{exe}\" verifica \"%1\""))?;
        }
        for ext in super::EXTENSIONS {
            default_for(ext, on)?;
            if !on {
                unset(&format!(r".{ext}\OpenWithProgids"), Some(PROGID));
            }
        }
        // I PDF: Cedro sta sempre in «Apri con»; predefinito solo se lo si chiede.
        if on || pdf {
            set(r".pdf\OpenWithProgids", Some(PROGID), "")?;
        } else {
            unset(r".pdf\OpenWithProgids", Some(PROGID));
        }
        default_for("pdf", pdf)?;
        if !on && !pdf {
            unsafe {
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(format!(r"{CLASSES}\{PROGID}")));
            }
            unset(PROGID, None);
        }
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::{Path, PathBuf};

    const DESKTOP: &str = "dev.cedro.Verifica.desktop";

    /// I tipi dei file firmati, con le estensioni (quelli che il sistema non conosce li definisce Cedro).
    const TYPES: &[(&str, &str, &str)] = &[
        ("application/pkcs7-mime", "p7m", "File firmato (CAdES)"),
        ("application/pkcs7-signature", "p7s", "Firma staccata (CAdES)"),
        ("application/x-cedro-m7m", "m7m", "File firmato con marca temporale (M7M)"),
        ("application/timestamped-data", "tsd", "File con marca temporale (TSD)"),
        ("application/timestamp-reply", "tsr", "Marca temporale (TSR)"),
        ("application/x-cedro-p7e", "p7e", "File firmato (P7E)"),
    ];

    fn data_home() -> Option<PathBuf> {
        match std::env::var_os("XDG_DATA_HOME") {
            Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
            _ => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")),
        }
    }

    fn config_home() -> Option<PathBuf> {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
            _ => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")),
        }
    }

    fn write(path: &Path, body: &str) -> Result<bool, String> {
        if std::fs::read_to_string(path).is_ok_and(|old| old == body) {
            return Ok(false);
        }
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, body).map_err(|e| e.to_string())?;
        Ok(true)
    }

    fn quiet(cmd: &str, args: &[&str]) {
        let _ = std::process::Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    fn quote(exe: &str) -> String {
        if exe.contains([' ', '\t', '"']) {
            format!("\"{}\"", exe.replace('"', "\\\""))
        } else {
            exe.to_string()
        }
    }

    /// `pdf`: dichiara anche i PDF. Spento non li si dichiara, sennò senza un lettore PDF predefinito
    /// il sistema sceglierebbe Cedro.
    pub fn desktop_entry(exe: &str, pdf: bool) -> String {
        let mut mimes: Vec<&str> = TYPES.iter().map(|t| t.0).collect();
        if pdf {
            mimes.push("application/pdf");
        }
        format!(
            "[Desktop Entry]\nType=Application\nName=Cedro · Verifica firma\nName[en]=Cedro · Verify signature\nComment=Apri e verifica i file firmati digitalmente\nComment[en]=Open and verify digitally signed files\nExec={} verifica %f\nIcon=dev.cedro.Cedro\nTerminal=false\nNoDisplay=true\nMimeType={};\nStartupNotify=true\n",
            quote(exe),
            mimes.join(";")
        )
    }

    pub fn mime_package() -> String {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n");
        for (mime, ext, comment) in TYPES {
            out.push_str(&format!("  <mime-type type=\"{mime}\">\n    <comment xml:lang=\"it\">{comment}</comment>\n    <glob pattern=\"*.{ext}\" weight=\"80\"/>\n  </mime-type>\n"));
        }
        out.push_str("</mime-info>\n");
        out
    }

    /// Mette o toglie `type=dev.cedro.Verifica.desktop;` nella sezione [Default Applications].
    pub fn edit_mimeapps(text: &str, types: &[&str], on: bool) -> String {
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let start = lines.iter().position(|l| l.trim() == "[Default Applications]");
        let start = match start {
            Some(s) => s,
            None => {
                if !on {
                    return text.to_string();
                }
                if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    lines.push(String::new());
                }
                lines.push("[Default Applications]".into());
                lines.len() - 1
            }
        };
        let end = lines.iter().enumerate().skip(start + 1).find(|(_, l)| l.trim_start().starts_with('[')).map(|(i, _)| i).unwrap_or(lines.len());
        let mut section: Vec<String> = lines[start + 1..end].to_vec();
        for t in types {
            let prefix = format!("{t}=");
            let pos = section.iter().position(|l| l.starts_with(&prefix));
            if on {
                let line = format!("{t}={DESKTOP};");
                match pos {
                    Some(p) => section[p] = line,
                    None => {
                        let at = section.iter().rposition(|l| !l.trim().is_empty()).map(|p| p + 1).unwrap_or(0);
                        section.insert(at, line);
                    }
                }
            } else if let Some(p) = pos {
                // Si toglie solo Cedro: se c'erano altri programmi dopo, restano loro.
                let rest: Vec<&str> = section[p][prefix.len()..].split(';').filter(|x| !x.is_empty() && *x != DESKTOP).collect();
                if rest.is_empty() {
                    section.remove(p);
                } else {
                    section[p] = format!("{t}={};", rest.join(";"));
                }
            }
        }
        lines.splice(start + 1..end, section);
        let mut out = lines.join("\n");
        out.push('\n');
        out
    }

    pub fn apply(on: bool, pdf: bool) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_str().ok_or("percorso non UTF-8")?.to_string();
        let data = data_home().ok_or("HOME non impostata")?;
        let desktop = data.join("applications").join(DESKTOP);
        let package = data.join("mime/packages/cedro-firma.xml");
        let mut changed = false;
        if on || pdf {
            changed |= write(&desktop, &desktop_entry(&exe, pdf))?;
            changed |= write(&package, &mime_package())?;
        } else {
            changed |= std::fs::remove_file(&desktop).is_ok();
            changed |= std::fs::remove_file(&package).is_ok();
        }
        if let Some(cfg) = config_home() {
            let path = cfg.join("mimeapps.list");
            let old = std::fs::read_to_string(&path).unwrap_or_default();
            let signed: Vec<&str> = TYPES.iter().map(|t| t.0).collect();
            let mut text = edit_mimeapps(&old, &signed, on);
            text = edit_mimeapps(&text, &["application/pdf"], pdf);
            if text != old && !(old.is_empty() && !on && !pdf) {
                changed |= write(&path, &text)?;
            }
        }
        if changed {
            quiet("update-mime-database", &[&data.join("mime").to_string_lossy()]);
            quiet("update-desktop-database", &[&data.join("applications").to_string_lossy()]);
            for tool in ["kbuildsycoca6", "kbuildsycoca5"] {
                if std::process::Command::new(tool).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().is_ok() {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn apply(_: bool, _: bool) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::imp::*;

    #[test]
    fn mimeapps() {
        let old = "[Added Associations]\ntext/plain=kate.desktop;\n\n[Default Applications]\napplication/pdf=okular.desktop;\n";
        let on = edit_mimeapps(old, &["application/pkcs7-mime"], true);
        assert!(on.contains("application/pkcs7-mime=dev.cedro.Verifica.desktop;"));
        assert!(on.contains("application/pdf=okular.desktop;"));
        assert!(on.contains("text/plain=kate.desktop;"));
        let off = edit_mimeapps(&on, &["application/pkcs7-mime"], false);
        assert!(!off.contains("pkcs7"));
        assert!(off.contains("application/pdf=okular.desktop;"));
        let fresh = edit_mimeapps("", &["application/pkcs7-mime"], true);
        assert_eq!(fresh, "[Default Applications]\napplication/pkcs7-mime=dev.cedro.Verifica.desktop;\n");
        assert!(desktop_entry("/usr/bin/cedro", false).contains("Exec=/usr/bin/cedro verifica %f"));
        assert!(!desktop_entry("/usr/bin/cedro", false).contains("application/pdf"));
        assert!(desktop_entry("/usr/bin/cedro", true).contains("application/pdf"));
        assert!(mime_package().contains("*.tsd"));
    }
}
