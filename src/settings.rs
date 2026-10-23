use std::fs;
use std::path::Path;

/// Le impostazioni di Cedro, nel file `config.txt` (righe `chiave=valore`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub quality: u8,
    pub notify: bool,
    pub update_check: bool,
    pub key_shift: bool,
    pub key_ctrl: bool,
    pub key_alt: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { quality: 82, notify: true, update_check: true, key_shift: true, key_ctrl: true, key_alt: true }
    }
}

fn flag(value: &str) -> Option<bool> {
    match value.trim() {
        "1" | "true" | "si" | "sì" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once('=') else { continue };
            match key.trim() {
                "quality" => {
                    if let Ok(n) = value.trim().parse::<u8>() {
                        s.quality = n.clamp(40, 95);
                    }
                }
                "notify" => s.notify = flag(value).unwrap_or(s.notify),
                "update_check" => s.update_check = flag(value).unwrap_or(s.update_check),
                "key_shift" => s.key_shift = flag(value).unwrap_or(s.key_shift),
                "key_ctrl" => s.key_ctrl = flag(value).unwrap_or(s.key_ctrl),
                "key_alt" => s.key_alt = flag(value).unwrap_or(s.key_alt),
                _ => {}
            }
        }
        s
    }

    /// Riscrive le chiavi note e lascia al loro posto le altre righe del file.
    pub fn merge_into(&self, old: &str) -> String {
        let b = |v: bool| if v { "1" } else { "0" };
        let pairs = [
            ("quality", self.quality.clamp(40, 95).to_string()),
            ("notify", b(self.notify).into()),
            ("update_check", b(self.update_check).into()),
            ("key_shift", b(self.key_shift).into()),
            ("key_ctrl", b(self.key_ctrl).into()),
            ("key_alt", b(self.key_alt).into()),
        ];
        let mut out: Vec<String> = old
            .lines()
            .filter(|line| !pairs.iter().any(|(k, _)| line.trim().split_once('=').is_some_and(|(key, _)| key.trim() == *k)))
            .map(str::to_string)
            .collect();
        out.extend(pairs.iter().map(|(k, v)| format!("{k}={v}")));
        out.join("\n") + "\n"
    }

    pub fn load_from(path: &Path) -> Settings {
        fs::read_to_string(path).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let old = fs::read_to_string(path).unwrap_or_default();
        fs::write(path, self.merge_into(&old))
    }
}

pub fn read_settings() -> Settings {
    Settings::load_from(&crate::config_dir().join("config.txt"))
}

pub fn write_settings(s: &Settings) -> std::io::Result<()> {
    s.save_to(&crate::config_dir().join("config.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty() {
        assert_eq!(Settings::parse(""), Settings::default());
    }

    #[test]
    fn reads_values_and_clamps_quality() {
        let s = Settings::parse("quality=99\nnotify=0\nkey_alt = off\nupdate_check=1\n");
        assert_eq!(s.quality, 95);
        assert!(!s.notify && !s.key_alt && s.update_check && s.key_shift);
    }

    #[test]
    fn ignores_garbage() {
        let s = Settings::parse("quality=abc\nnotify=forse\n# nota\n=1\n");
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn merge_keeps_unknown_lines_and_replaces_known() {
        let mut s = Settings::default();
        s.quality = 70;
        s.notify = false;
        let text = s.merge_into("quality=82\nfuturo=x\nnotify=1\n");
        assert!(text.contains("futuro=x\n"));
        assert_eq!(text.matches("quality=").count(), 1);
        assert_eq!(Settings::parse(&text), s);
    }
}
