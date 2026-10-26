use std::fs;
use std::path::Path;

/// Dove va la copia convertita.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    /// Accanto al file originale.
    Same,
    /// In una sottocartella «Cedro» accanto al file.
    Sub,
    /// Sempre nella cartella `dest_dir`.
    Fixed,
}

/// Le impostazioni di Cedro, nel file `config.txt` (righe `chiave=valore`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub quality: u8,
    pub notify: bool,
    pub update_check: bool,
    pub key_shift: bool,
    pub key_ctrl: bool,
    pub key_alt: bool,
    /// Apre la ruota anche nelle finestre Apri/Salva.
    pub dialogs: bool,
    /// A lavoro finito mostra il file nuovo in Esplora file.
    pub reveal: bool,
    pub dest: Dest,
    pub dest_dir: String,
    /// Se il file esiste già lo sostituisce, invece di aggiungere un numero.
    pub replace: bool,
    /// Risoluzione di PDF in JPG, in dpi (100, 144, 200 o 300).
    pub pdf_dpi: u32,
    /// Quanto riduce «Comprimi»: 0 poco, 1 medio, 2 molto.
    pub compress: u8,
    /// Lingua dell'OCR: `auto`, `it` o `en`.
    pub ocr_lang: String,
    /// Pagine al massimo per l'OCR di un PDF; 0 vuol dire tutte.
    pub ocr_max: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            quality: 82,
            notify: true,
            update_check: true,
            key_shift: true,
            key_ctrl: true,
            key_alt: true,
            dialogs: true,
            reveal: false,
            dest: Dest::Same,
            dest_dir: String::new(),
            replace: false,
            pdf_dpi: 144,
            compress: 1,
            ocr_lang: "auto".into(),
            ocr_max: 100,
        }
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
    /// Il fattore di scala per disegnare un PDF alla risoluzione scelta (72 dpi sono 1.0).
    pub fn pdf_scale(&self) -> f32 {
        self.pdf_dpi as f32 / 72.0
    }

    /// Il tetto di qualità JPEG di «Comprimi».
    pub fn compress_quality(&self) -> u8 {
        match self.compress {
            0 => 75,
            2 => 45,
            _ => 62,
        }
    }

    /// Le pagine al massimo per l'OCR di un PDF.
    pub fn ocr_pages(&self) -> usize {
        if self.ocr_max == 0 {
            usize::MAX
        } else {
            self.ocr_max as usize
        }
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once('=') else { continue };
            let value = value.trim();
            match key.trim() {
                "quality" => {
                    if let Ok(n) = value.parse::<u8>() {
                        s.quality = n.clamp(40, 95);
                    }
                }
                "notify" => s.notify = flag(value).unwrap_or(s.notify),
                "update_check" => s.update_check = flag(value).unwrap_or(s.update_check),
                "key_shift" => s.key_shift = flag(value).unwrap_or(s.key_shift),
                "key_ctrl" => s.key_ctrl = flag(value).unwrap_or(s.key_ctrl),
                "key_alt" => s.key_alt = flag(value).unwrap_or(s.key_alt),
                "dialogs" => s.dialogs = flag(value).unwrap_or(s.dialogs),
                "reveal" => s.reveal = flag(value).unwrap_or(s.reveal),
                "replace" => s.replace = flag(value).unwrap_or(s.replace),
                "dest" => {
                    s.dest = match value {
                        "sub" => Dest::Sub,
                        "fixed" => Dest::Fixed,
                        "same" => Dest::Same,
                        _ => s.dest,
                    }
                }
                "dest_dir" => s.dest_dir = value.to_string(),
                "pdf_dpi" => {
                    if let Ok(n) = value.parse::<u32>() {
                        s.pdf_dpi = n.clamp(72, 300);
                    }
                }
                "compress" => {
                    if let Ok(n) = value.parse::<u8>() {
                        s.compress = n.min(2);
                    }
                }
                "ocr_lang" if matches!(value, "auto" | "it" | "en") => s.ocr_lang = value.to_string(),
                "ocr_max" => {
                    if let Ok(n) = value.parse::<u32>() {
                        s.ocr_max = n;
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// Riscrive le chiavi note e lascia al loro posto le altre righe del file.
    pub fn merge_into(&self, old: &str) -> String {
        let b = |v: bool| if v { "1" } else { "0" };
        let dest = match self.dest {
            Dest::Same => "same",
            Dest::Sub => "sub",
            Dest::Fixed => "fixed",
        };
        let pairs = [
            ("quality", self.quality.clamp(40, 95).to_string()),
            ("notify", b(self.notify).into()),
            ("update_check", b(self.update_check).into()),
            ("key_shift", b(self.key_shift).into()),
            ("key_ctrl", b(self.key_ctrl).into()),
            ("key_alt", b(self.key_alt).into()),
            ("dialogs", b(self.dialogs).into()),
            ("reveal", b(self.reveal).into()),
            ("replace", b(self.replace).into()),
            ("dest", dest.into()),
            ("dest_dir", self.dest_dir.clone()),
            ("pdf_dpi", self.pdf_dpi.to_string()),
            ("compress", self.compress.to_string()),
            ("ocr_lang", self.ocr_lang.clone()),
            ("ocr_max", self.ocr_max.to_string()),
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
    fn reads_new_options() {
        let s = Settings::parse("dest=fixed\ndest_dir=D:\\Convertiti\nreplace=1\npdf_dpi=999\ncompress=2\nocr_lang=en\nocr_max=0\n");
        assert_eq!(s.dest, Dest::Fixed);
        assert_eq!(s.dest_dir, "D:\\Convertiti");
        assert!(s.replace);
        assert_eq!(s.pdf_dpi, 300);
        assert_eq!(s.compress_quality(), 45);
        assert_eq!(s.ocr_lang, "en");
        assert_eq!(s.ocr_pages(), usize::MAX);
        assert_eq!(Settings::parse(&s.merge_into("")), s);
    }

    #[test]
    fn ocr_lang_ignores_unknown() {
        assert_eq!(Settings::parse("ocr_lang=klingon").ocr_lang, "auto");
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
