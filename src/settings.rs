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
    /// Mostra la sottile barra di avanzamento in basso a destra mentre converte.
    pub progress: bool,
    /// Modalità Semplice: i pannelli mostrano solo l'essenziale, le altre opzioni restano a un clic.
    pub simple: bool,
    /// La scelta tra vista Semplice e Completa è già stata proposta (installatore / primo avvio).
    pub view_asked: bool,
    /// Codifica i video con la scheda video, se il PC ne ha una adatta (vedi `accel`).
    pub gpu: bool,
    /// Calcoli con le istruzioni avanzate (AVX-512 su x86, i8mm su ARM), se il processore le ha (vedi `accel`).
    pub avx512: bool,
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
    /// Il tema della versione standard (`ui::theme::ThemeId::key`); sconosciuto = Windows essenziale.
    pub ui_theme: String,
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
            progress: true,
            simple: false,
            view_asked: false,
            gpu: true,
            avx512: true,
            dest: Dest::Same,
            dest_dir: String::new(),
            replace: false,
            pdf_dpi: 144,
            compress: 1,
            ocr_lang: "auto".into(),
            ocr_max: 100,
            ui_theme: String::new(),
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
        // Il Blocco note salva con il BOM: senza toglierlo la prima chiave non verrebbe riconosciuta.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
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
                "progress" => s.progress = flag(value).unwrap_or(s.progress),
                "simple" => s.simple = flag(value).unwrap_or(s.simple),
                "view_asked" => s.view_asked = flag(value).unwrap_or(s.view_asked),
                "gpu" => s.gpu = flag(value).unwrap_or(s.gpu),
                "avx512" => s.avx512 = flag(value).unwrap_or(s.avx512),
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
                "ui_theme" => s.ui_theme = value.to_string(),
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
            ("progress", b(self.progress).into()),
            ("simple", b(self.simple).into()),
            ("view_asked", b(self.view_asked).into()),
            ("gpu", b(self.gpu).into()),
            ("avx512", b(self.avx512).into()),
            ("replace", b(self.replace).into()),
            ("dest", dest.into()),
            ("dest_dir", self.dest_dir.clone()),
            ("pdf_dpi", self.pdf_dpi.to_string()),
            ("compress", self.compress.to_string()),
            ("ocr_lang", self.ocr_lang.clone()),
            ("ocr_max", self.ocr_max.to_string()),
            ("ui_theme", self.ui_theme.clone()),
            ("ui_schema", "1".into()),
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
        // Prima un file a parte, poi lo scambio: se il PC si spegne a metà, le impostazioni non vanno perse.
        let temp = path.with_extension("txt.tmp");
        fs::write(&temp, self.merge_into(&old))?;
        fs::rename(&temp, path).inspect_err(|_| {
            let _ = fs::remove_file(&temp);
        })
    }
}

/// Chi aveva la vecchia edizione Comune di Bondeno e passa a questa: l'installatore (o il pacchetto per Linux)
/// lascia questo file nella cartella delle impostazioni, e al primo avvio il tema Bondeno si accende da solo.
pub const DA_BONDENO: &str = "da-bondeno";

/// Se c'è il file `da-bondeno` in `dir`: sceglie il tema Bondeno nelle impostazioni e toglie il file.
fn passa_a_bondeno(dir: &Path) {
    let marker = dir.join(DA_BONDENO);
    if !marker.is_file() {
        return;
    }
    let path = dir.join("config.txt");
    let mut s = Settings::load_from(&path);
    s.ui_theme = crate::edizione::CHIAVE_TEMA.to_string();
    // Il file si toglie solo se la scelta è stata scritta: altrimenti ci si riprova al prossimo avvio.
    if s.save_to(&path).is_ok() {
        let _ = fs::remove_file(&marker);
    }
}

pub fn read_settings() -> Settings {
    static AL_PRIMO_AVVIO: std::sync::Once = std::sync::Once::new();
    let dir = crate::config_dir();
    AL_PRIMO_AVVIO.call_once(|| passa_a_bondeno(&dir));
    let s = Settings::load_from(&dir.join("config.txt"));
    crate::edizione::segui_tema(&s.ui_theme);
    s
}

pub fn write_settings(s: &Settings) -> std::io::Result<()> {
    crate::edizione::segui_tema(&s.ui_theme);
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
    fn a_file_saved_with_bom_still_reads_its_first_key() {
        assert!(!Settings::parse("\u{feff}progress=0\nnotify=0\n").progress);
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
    fn simple_mode_is_off_by_default_and_round_trips() {
        assert!(!Settings::default().simple);
        let mut s = Settings::default();
        s.simple = true;
        assert!(Settings::parse(&s.merge_into("")).simple);
        assert!(!Settings::default().view_asked);
        s.view_asked = true;
        assert!(Settings::parse(&s.merge_into("")).view_asked);
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

#[cfg(test)]
mod theme_tests {
    use super::*;

    #[test]
    fn ui_theme_round_trips_and_keeps_other_lines() {
        let old = "\u{feff}simple=1\nmia_chiave=resta\nui_theme=banco-di-lavoro\n";
        let mut s = Settings::parse(old);
        assert!(s.simple);
        assert_eq!(s.ui_theme, "banco-di-lavoro");
        s.ui_theme = "percorso-guidato".into();
        let text = s.merge_into(old);
        assert!(text.contains("mia_chiave=resta"));
        assert_eq!(text.matches("ui_theme=").count(), 1);
        assert!(text.contains("ui_schema=1"));
        assert_eq!(Settings::parse(&text).ui_theme, "percorso-guidato");
        // Un file vecchio, senza le chiavi nuove, resta com'era.
        assert_eq!(Settings::parse("simple=0\n").ui_theme, "");
    }
}

#[cfg(test)]
mod bondeno_tests {
    use super::*;

    #[test]
    fn chi_arriva_dalla_vecchia_edizione_bondeno_trova_il_tema_acceso() {
        let dir = std::env::temp_dir().join(format!("cedro-da-bondeno-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.txt"), "quality=70\nui_theme=banco-di-lavoro\n").unwrap();
        // Senza il file, niente cambia.
        passa_a_bondeno(&dir);
        assert_eq!(Settings::load_from(&dir.join("config.txt")).ui_theme, "banco-di-lavoro");
        // Con il file: il tema è Bondeno, il resto resta, e il file sparisce (una volta sola).
        fs::write(dir.join(DA_BONDENO), "").unwrap();
        passa_a_bondeno(&dir);
        let s = Settings::load_from(&dir.join("config.txt"));
        assert_eq!((s.ui_theme.as_str(), s.quality), ("bondeno", 70));
        assert!(!dir.join(DA_BONDENO).exists());
        // Senza impostazioni ancora scritte vale lo stesso.
        let nuova = dir.join("nuova");
        fs::create_dir_all(&nuova).unwrap();
        fs::write(nuova.join(DA_BONDENO), "").unwrap();
        passa_a_bondeno(&nuova);
        assert_eq!(Settings::load_from(&nuova.join("config.txt")).ui_theme, "bondeno");
        let _ = fs::remove_dir_all(&dir);
    }
}
