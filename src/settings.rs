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
    /// La ruota al clic destro (Shift, Ctrl o Alt). Spenta di default dalla 0.36: si accende in Impostazioni > Ruota.
    pub wheel: bool,
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
    /// Laboratorio: i micron di un pixel della fotocamera del microscopio con l'obiettivo 10x, come
    /// scritto (per esempio «0,65»); vuoto = non misurato. Gli altri obiettivi si ricavano da questo.
    pub calibrazione_10x: String,
    /// Trascrizione: i termini dei referti veterinari (anatomia patologica) suggeriti al motore.
    pub glossario_vet: bool,
    /// Trascrizione: altre parole da suggerire al motore, separate da virgole (nomi, farmaci, sigle).
    pub glossario_termini: String,
    /// Radiografie per il proprietario: il nome della clinica in cima alla pagina.
    pub nome_clinica: String,
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
    /// Lingua dell'interfaccia: `auto` (quella del sistema), `it` o `en`.
    pub language: String,
    /// Firma digitale: il doppio clic sui file firmati (.p7m, .p7s, .m7m, .tsd, .tsr, .p7e) apre il visualizzatore.
    pub firma_assoc: bool,
    /// Anche i PDF (se no restano al lettore PDF e si aprono con «Verifica con Cedro»).
    pub firma_pdf: bool,
    /// Controlla online la revoca dei certificati (OCSP e CRL).
    pub firma_online: bool,
    /// Il driver della chiavetta scelto a mano (vuoto = lo cerco da solo).
    pub firma_driver: String,
    /// Formato: 0 automatico (PDF in PDF firmato, gli altri .p7m), 1 tutto .p7m, 2 tutto PDF firmato.
    pub firma_format: u8,
    /// Cosa aggiungere al nome dei PDF firmati («_firmato»).
    pub firma_suffix: String,
    /// File già firmati: 0 firma accanto (parallela), 1 sopra (controfirma), 2 busta nuova.
    pub firma_existing: u8,
    /// Dove salvare i file firmati (vuoto = accanto agli originali).
    pub firma_dir: String,
    /// Firma visibile nei PDF.
    pub firma_visible: bool,
    /// L'immagine del riquadro (stemma o firma scansionata), vuoto = niente.
    pub firma_image: String,
    /// Dove va il riquadro se il PDF non ha lo spazio: 0 ultima pagina in basso a destra, 1 ultima in basso
    /// a sinistra, 2 prima pagina in basso a destra.
    pub firma_corner: u8,
    pub firma_reason: String,
    pub firma_location: String,
    /// Tenere il PIN per tutta la coda (sempre: lo si dimentica alla fine), o chiederlo a ogni file.
    pub firma_keep_pin: bool,
    /// Avvisare 3 mesi, 1 mese e 1 settimana prima che scada il certificato.
    pub firma_remind: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            quality: 82,
            notify: true,
            update_check: true,
            wheel: false,
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
            calibrazione_10x: String::new(),
            glossario_vet: false,
            glossario_termini: String::new(),
            nome_clinica: String::new(),
            replace: false,
            pdf_dpi: 144,
            compress: 1,
            ocr_lang: "auto".into(),
            ocr_max: 100,
            ui_theme: String::new(),
            language: "auto".into(),
            firma_assoc: true,
            firma_pdf: false,
            firma_online: true,
            firma_driver: String::new(),
            firma_format: 0,
            firma_suffix: "_firmato".into(),
            firma_existing: 0,
            firma_dir: String::new(),
            firma_visible: true,
            firma_image: String::new(),
            firma_corner: 0,
            firma_reason: String::new(),
            firma_location: String::new(),
            firma_keep_pin: true,
            firma_remind: true,
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

    /// Il glossario per il motore della trascrizione: «veterinaria», le parole scritte, o tutti e due.
    pub fn glossario(&self) -> Option<String> {
        let mut g = Vec::new();
        if self.glossario_vet {
            g.push("veterinaria".to_string());
        }
        if !self.glossario_termini.trim().is_empty() {
            g.push(self.glossario_termini.trim().to_string());
        }
        (!g.is_empty()).then(|| g.join(", "))
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
                "wheel" => s.wheel = flag(value).unwrap_or(s.wheel),
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
                "calibrazione_10x" => s.calibrazione_10x = value.trim().to_string(),
                "glossario_vet" => s.glossario_vet = flag(value).unwrap_or(s.glossario_vet),
                "glossario_termini" => s.glossario_termini = value.trim().to_string(),
                "nome_clinica" => s.nome_clinica = value.trim().to_string(),
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
                "firma_assoc" => s.firma_assoc = flag(value).unwrap_or(s.firma_assoc),
                "firma_pdf" => s.firma_pdf = flag(value).unwrap_or(s.firma_pdf),
                "firma_online" => s.firma_online = flag(value).unwrap_or(s.firma_online),
                "firma_driver" => s.firma_driver = value.to_string(),
                "firma_format" => s.firma_format = value.parse::<u8>().unwrap_or(0).min(2),
                "firma_suffix" => s.firma_suffix = value.to_string(),
                "firma_existing" => s.firma_existing = value.parse::<u8>().unwrap_or(0).min(2),
                "firma_dir" => s.firma_dir = value.to_string(),
                "firma_visible" => s.firma_visible = flag(value).unwrap_or(s.firma_visible),
                "firma_image" => s.firma_image = value.to_string(),
                "firma_corner" => s.firma_corner = value.parse::<u8>().unwrap_or(0).min(2),
                "firma_reason" => s.firma_reason = value.to_string(),
                "firma_location" => s.firma_location = value.to_string(),
                "firma_keep_pin" => s.firma_keep_pin = flag(value).unwrap_or(s.firma_keep_pin),
                "firma_remind" => s.firma_remind = flag(value).unwrap_or(s.firma_remind),
                "ui_theme" => s.ui_theme = value.to_string(),
                "language" if matches!(value, "auto" | "it" | "en") => s.language = value.to_string(),
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
            ("wheel", b(self.wheel).into()),
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
            ("calibrazione_10x", self.calibrazione_10x.clone()),
            ("glossario_vet", b(self.glossario_vet).into()),
            ("glossario_termini", self.glossario_termini.clone()),
            ("nome_clinica", self.nome_clinica.clone()),
            ("pdf_dpi", self.pdf_dpi.to_string()),
            ("compress", self.compress.to_string()),
            ("ocr_lang", self.ocr_lang.clone()),
            ("ocr_max", self.ocr_max.to_string()),
            ("ui_theme", self.ui_theme.clone()),
            ("language", self.language.clone()),
            ("firma_assoc", b(self.firma_assoc).into()),
            ("firma_pdf", b(self.firma_pdf).into()),
            ("firma_online", b(self.firma_online).into()),
            ("firma_driver", self.firma_driver.clone()),
            ("firma_format", self.firma_format.to_string()),
            ("firma_suffix", self.firma_suffix.clone()),
            ("firma_existing", self.firma_existing.to_string()),
            ("firma_dir", self.firma_dir.clone()),
            ("firma_visible", b(self.firma_visible).into()),
            ("firma_image", self.firma_image.clone()),
            ("firma_corner", self.firma_corner.to_string()),
            ("firma_reason", self.firma_reason.clone()),
            ("firma_location", self.firma_location.clone()),
            ("firma_keep_pin", b(self.firma_keep_pin).into()),
            ("firma_remind", b(self.firma_remind).into()),
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
        read_config(path).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let old = read_config(path).unwrap_or_default();
        // Prima un file a parte, scritto davvero sul disco, poi lo scambio: se il PC si spegne a metà,
        // le impostazioni non vanno perse.
        // Un nome per ogni salvataggio: due Cedro (la ruota e le Impostazioni) non scrivono nello stesso file.
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temp = path.with_extension(format!("txt.{}-{n}.tmp", std::process::id()));
        let written = fs::File::create(&temp).and_then(|mut file| {
            file.write_all(self.merge_into(&old).as_bytes())?;
            file.sync_all()
        });
        written.and_then(|()| fs::rename(&temp, path)).inspect_err(|_| {
            let _ = fs::remove_file(&temp);
        })
    }
}

/// Il config.txt come testo, anche se salvato col Blocco note in ANSI o in UTF-16: con `read_to_string`
/// un file non UTF-8 riportava tutto ai valori di partenza, e il salvataggio dopo cancellava il resto.
fn read_config(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|bytes| crate::engine::decode_text(&bytes))
}

/// Chi aveva la vecchia edizione Comune di Bondeno e passa a questa: l'installatore (o il pacchetto per Linux)
/// lascia questo file nella cartella delle impostazioni, e al primo avvio il tema Bondeno si accende da solo.
pub const DA_BONDENO: &str = "da-bondeno";

/// Chi nell'installatore sceglie «Cedro 4 Labs» (o lo installa con `/TEMA=labs`, `./install.sh --labs`):
/// stesso meccanismo, il tema Cedro 4 Labs si accende al primo avvio.
pub const DA_LABS: &str = "da-labs";

/// Se c'è il file `marker` in `dir`: sceglie il tema `chiave` nelle impostazioni e toglie il file.
fn passa_a_tema(dir: &Path, marker: &str, chiave: &str) {
    let marker = dir.join(marker);
    if !marker.is_file() {
        return;
    }
    let path = dir.join("config.txt");
    let mut s = Settings::load_from(&path);
    s.ui_theme = chiave.to_string();
    // Il file si toglie solo se la scelta è stata scritta: altrimenti ci si riprova al prossimo avvio.
    if s.save_to(&path).is_ok() {
        let _ = fs::remove_file(&marker);
    }
}

/// I file lasciati dall'installatore: Bondeno, poi Cedro 4 Labs (se ci sono tutti e due vale l'ultimo).
fn passa_a_bondeno(dir: &Path) {
    passa_a_tema(dir, DA_BONDENO, crate::edizione::CHIAVE_TEMA);
    passa_a_tema(dir, DA_LABS, crate::ui::theme::ThemeId::CedroLabs.key());
}

pub fn read_settings() -> Settings {
    static AL_PRIMO_AVVIO: std::sync::Once = std::sync::Once::new();
    let dir = crate::config_dir();
    AL_PRIMO_AVVIO.call_once(|| passa_a_bondeno(&dir));
    let s = Settings::load_from(&dir.join("config.txt"));
    crate::edizione::segui_tema(&s.ui_theme);
    crate::i18n::segui(&s.language);
    s
}

pub fn write_settings(s: &Settings) -> std::io::Result<()> {
    crate::edizione::segui_tema(&s.ui_theme);
    crate::i18n::segui(&s.language);
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
    fn the_wheel_is_off_unless_turned_on() {
        // Anche chi aggiorna da una versione che non scriveva la chiave la trova spenta.
        assert!(!Settings::parse("key_shift=1\nkey_ctrl=1\n").wheel);
        let on = Settings::parse("wheel=1\n");
        assert!(on.wheel);
        assert!(Settings::parse(&on.merge_into("")).wheel);
    }

    #[test]
    fn a_file_saved_with_bom_still_reads_its_first_key() {
        assert!(!Settings::parse("\u{feff}progress=0\nnotify=0\n").progress);
    }

    #[test]
    fn a_file_saved_as_ansi_or_utf16_keeps_its_values() {
        let dir = std::env::temp_dir().join(format!("cedro-settings-{}-{:?}", std::process::id(), std::thread::current().id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.txt");
        // ANSI (Windows-1252): «ò» è un byte solo, non UTF-8.
        let mut ansi = b"quality=60\nfuturo=si\ndest_dir=C:\\Users\\Niccol".to_vec();
        ansi.extend_from_slice(&[0xF2]);
        ansi.extend_from_slice(b"\\Convertiti\n");
        fs::write(&path, &ansi).unwrap();
        let s = Settings::load_from(&path);
        assert_eq!((s.quality, s.dest_dir.as_str()), (60, "C:\\Users\\Niccolò\\Convertiti"));
        s.save_to(&path).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("futuro=si") && saved.contains("quality=60"), "{saved}");
        // UTF-16 con BOM («Unicode» nel Blocco note).
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend("quality=70\r\n".encode_utf16().flat_map(u16::to_le_bytes));
        fs::write(&path, &utf16).unwrap();
        assert_eq!(Settings::load_from(&path).quality, 70);
        let _ = fs::remove_dir_all(&dir);
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

    #[test]
    fn chi_sceglie_cedro_4_labs_nell_installatore_trova_il_tema_acceso() {
        let dir = std::env::temp_dir().join(format!("cedro-da-labs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.txt"), "quality=70\nui_theme=windows-essenziale\n").unwrap();
        passa_a_bondeno(&dir);
        assert_eq!(Settings::load_from(&dir.join("config.txt")).ui_theme, "windows-essenziale");
        fs::write(dir.join(DA_LABS), "").unwrap();
        passa_a_bondeno(&dir);
        let s = Settings::load_from(&dir.join("config.txt"));
        assert_eq!((s.ui_theme.as_str(), s.quality), ("cedro-labs", 70));
        assert!(!dir.join(DA_LABS).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
