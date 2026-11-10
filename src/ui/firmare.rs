//! Lo stato di «Firma documenti» (i tre passi: file, come firmare, PIN e firma), di «Chiavetta e
//! certificati» e delle «Opzioni della firma». La firma vera gira in un thread: la chiavetta è lenta
//! e la finestra non deve fermarsi.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::firma::pades_firma::{self, EmptyField, Look, Place};
use crate::firma::pkcs11::{self, Device};
use crate::firma::{self, firmare};
use crate::settings::Settings;

/// Quale pagina della sezione si vede.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Home,
    Sign,
    Devices,
    Options,
}

/// Cosa succede a un file.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Un PDF diventa un PDF firmato.
    Pdf,
    /// Un file non PDF diventa PDF e poi PDF firmato.
    ConvertPdf,
    /// Busta nuova .p7m (anche intorno a un .p7m: busta dentro la busta).
    P7m,
    /// Seconda firma accanto a quelle della busta.
    Parallel,
    /// Controfirma sopra quelle della busta.
    Counter,
    /// Un'altra firma nel PDF già firmato.
    PdfAgain,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Wait,
    Run,
    Done { at: i64, signers: Vec<String> },
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct SignFile {
    pub path: PathBuf,
    pub action: Action,
    pub out: PathBuf,
    /// Chi l'ha già firmato.
    pub signers: Vec<String>,
    /// I PDF: gli spazi per la firma vuoti e le pagine.
    pub fields: Vec<EmptyField>,
    pub pages: Vec<[f32; 4]>,
    pub state: State,
}

impl SignFile {
    pub fn is_pdf_out(&self) -> bool {
        matches!(self.action, Action::Pdf | Action::ConvertPdf | Action::PdfAgain)
    }
}

/// La finestrella per PIN e PUK in «Chiavetta e certificati».
#[derive(Clone, Debug, Default)]
pub struct PinDialog {
    /// 0 cambia il PIN, 1 sblocca con il PUK, 2 cambia il PUK.
    pub kind: u8,
    pub old: String,
    pub new: String,
    pub again: String,
    pub error: Option<String>,
    pub busy: bool,
}

enum Msg {
    Scanned(Vec<Device>, Vec<String>),
    Start(usize),
    Done(usize, Result<(i64, Vec<String>), String>),
    /// La firma è finita; con un errore se si è fermata prima (PIN sbagliato, chiavetta tolta).
    Finished(Option<String>),
    Dialog(Result<String, String>),
    Info(String, bool),
}

pub struct SignUi {
    pub page: Page,
    /// 1 file, 2 come firmare, 3 PIN e firma, 4 fatto.
    pub step: u8,
    pub files: Vec<SignFile>,
    pub devices: Vec<Device>,
    pub scan_errors: Vec<String>,
    pub scanning: bool,
    pub scanned_once: bool,
    pub device: usize,
    pub s: Settings,
    /// Dove va la firma nel PDF: 0 nello spazio che c'è, 1 dove la metto io, 2 non si vede.
    pub place: u8,
    /// Il riquadro trascinato: pagina (da 0) e centro in frazioni della pagina (y dall'alto).
    pub box_page: usize,
    pub box_center: (f32, f32),
    pub more: bool,
    pub pin: String,
    pub error: Option<String>,
    pub running: bool,
    pub dialog: Option<PinDialog>,
    /// Un messaggio da mostrare (testo, errore).
    pub info: Option<(String, bool)>,
    /// L'avviso della scadenza del certificato, da mostrare una volta.
    pub reminder: Option<String>,
    reminded: bool,
    inbox: Arc<Mutex<Vec<Msg>>>,
}

impl Default for SignUi {
    fn default() -> Self {
        SignUi {
            page: Page::Home,
            step: 1,
            files: Vec::new(),
            devices: Vec::new(),
            scan_errors: Vec::new(),
            scanning: false,
            scanned_once: false,
            device: 0,
            s: crate::read_settings(),
            place: 0,
            box_page: 0,
            box_center: (0.7, 0.88),
            more: false,
            pin: String::new(),
            error: None,
            running: false,
            dialog: None,
            info: None,
            reminder: None,
            reminded: false,
            inbox: Arc::default(),
        }
    }
}

type Wake = Arc<dyn Fn() + Send + Sync>;

/// Nel thread della firma: se si rompe (panic), dice lo stesso che è finita, con un errore.
struct SeSiRompe(Arc<Mutex<Vec<Msg>>>, Wake);

impl Drop for SeSiRompe {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(Msg::Finished(Some("errore interno durante la firma".into())));
            (self.1)();
        }
    }
}

/// Il nome che non c'è ancora nella cartella: «nome.ext», «nome (2).ext»…
fn free_name(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let p = dir.join(format!("{stem}.{ext}"));
    if !p.exists() {
        return p;
    }
    for k in 2.. {
        let p = dir.join(format!("{stem} ({k}).{ext}"));
        if !p.exists() {
            return p;
        }
    }
    unreachable!()
}

/// Si può trasformare in PDF con le conversioni di Cedro?
fn convertible(p: &Path) -> bool {
    use crate::engine::Kind;
    matches!(crate::engine::kind_of(p), Kind::Image | Kind::Text | Kind::Doc)
}

impl SignUi {
    pub fn device(&self) -> Option<&Device> {
        self.devices.get(self.device)
    }

    /// Il dispositivo da mostrare in alto: quello scelto se firma, se no il primo che firma.
    pub fn chip_device(&self) -> Option<&Device> {
        match self.device() {
            Some(d) if d.signing_cert().is_some() => Some(d),
            other => self.devices.iter().find(|d| d.signing_cert().is_some()).or(other),
        }
    }

    pub fn save_settings(&self) {
        let mut s = crate::read_settings();
        let k = &self.s;
        s.firma_driver = k.firma_driver.clone();
        s.firma_format = k.firma_format;
        s.firma_suffix = k.firma_suffix.clone();
        s.firma_existing = k.firma_existing;
        s.firma_dir = k.firma_dir.clone();
        s.firma_visible = k.firma_visible;
        s.firma_image = k.firma_image.clone();
        s.firma_corner = k.firma_corner;
        s.firma_reason = k.firma_reason.clone();
        s.firma_location = k.firma_location.clone();
        s.firma_keep_pin = k.firma_keep_pin;
        s.firma_remind = k.firma_remind;
        let _ = crate::write_settings(&s);
    }

    /// Cerca chiavette e carte (in un thread).
    pub fn scan(&mut self, wake: Wake) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        let inbox = self.inbox.clone();
        std::thread::spawn(move || {
            let (d, e) = std::panic::catch_unwind(pkcs11::scan).unwrap_or_else(|_| (Vec::new(), vec!["errore interno cercando la chiavetta".into()]));
            inbox.lock().unwrap_or_else(|e| e.into_inner()).push(Msg::Scanned(d, e));
            wake();
        });
    }

    /// Apre «Firma documenti» con questi file (passo 1).
    pub fn start(&mut self, paths: Vec<PathBuf>, wake: Wake) {
        if self.running {
            return;
        }
        if self.step == 4 {
            self.files.clear();
        }
        if self.files.is_empty() {
            // Coda nuova: la firma torna al suo posto (non dove era stata trascinata per altri file).
            self.place = 0;
        }
        self.page = Page::Sign;
        self.step = 1;
        self.error = None;
        self.s = crate::read_settings();
        self.add(paths);
        if !self.scanned_once {
            self.scan(wake);
        }
    }

    /// Aggiunge file (le cartelle: i file che contengono, non le sottocartelle).
    pub fn add(&mut self, paths: Vec<PathBuf>) {
        let mut list = Vec::new();
        for p in paths {
            if p.is_dir() {
                let mut inner: Vec<PathBuf> = std::fs::read_dir(&p).map(|rd| rd.flatten().map(|e| e.path()).filter(|x| x.is_file()).collect()).unwrap_or_default();
                inner.sort();
                list.extend(inner);
            } else if p.is_file() {
                list.push(p);
            }
        }
        for p in list {
            if self.files.iter().any(|f| f.path == p) {
                continue;
            }
            let data = std::fs::read(&p).unwrap_or_default();
            let is_pdf = firma::pades::is_pdf(&data);
            let signers = firmare::signer_names(&data);
            let (fields, pages) = if is_pdf { (pades_firma::empty_fields(&data), pades_firma::pages(&data).unwrap_or_default()) } else { (Vec::new(), Vec::new()) };
            self.files.push(SignFile { path: p, action: Action::P7m, out: PathBuf::new(), signers, fields, pages, state: State::Wait });
        }
        self.plan();
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.files.len() && !self.running {
            self.files.remove(i);
        }
    }

    /// Decide cosa fare di ogni file e come si chiamerà, dalle scelte.
    pub fn plan(&mut self) {
        let s = &self.s;
        let dir_opt = (!s.firma_dir.trim().is_empty()).then(|| PathBuf::from(s.firma_dir.trim()));
        let suffix = if s.firma_suffix.trim().is_empty() && dir_opt.is_none() { "_firmato".to_string() } else { s.firma_suffix.trim().to_string() };
        for f in &mut self.files {
            if matches!(f.state, State::Done { .. }) {
                continue;
            }
            let name = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let dir = dir_opt.clone().unwrap_or_else(|| f.path.parent().map(Path::to_path_buf).unwrap_or_default());
            let lower = name.to_ascii_lowercase();
            let is_pdf = lower.ends_with(".pdf") || !f.pages.is_empty();
            let envelope = !is_pdf && (lower.ends_with(".p7m") || lower.ends_with(".p7s")) && !f.signers.is_empty();
            f.action = if envelope {
                match s.firma_existing {
                    1 => Action::Counter,
                    2 => Action::P7m,
                    _ => Action::Parallel,
                }
            } else if is_pdf && s.firma_format != 1 {
                if f.signers.is_empty() {
                    Action::Pdf
                } else {
                    Action::PdfAgain
                }
            } else if s.firma_format == 2 && convertible(&f.path) {
                Action::ConvertPdf
            } else {
                Action::P7m
            };
            let stem = f.path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            f.out = match f.action {
                // Il file già firmato riceve la firma in più: stesso nome (accanto o nell'altra cartella).
                Action::Parallel | Action::Counter | Action::PdfAgain => dir.join(&name),
                Action::Pdf | Action::ConvertPdf => free_name(&dir, &format!("{stem}{suffix}"), "pdf"),
                Action::P7m => free_name(&dir, &name, "p7m"),
            };
        }
    }

    /// Il primo PDF della coda (quello che si vede nell'anteprima del passo 2).
    pub fn first_pdf(&self) -> Option<&SignFile> {
        self.files.iter().find(|f| matches!(f.action, Action::Pdf | Action::PdfAgain) && !f.pages.is_empty())
    }

    /// Le pagine e i campi del file da guardare per la posizione (PDF; i convertiti: A4).
    pub fn preview_pages(&self) -> Vec<[f32; 4]> {
        self.first_pdf().map(|f| f.pages.clone()).unwrap_or_default()
    }

    /// Dove va la firma in questo file, con le scelte fatte.
    pub fn place_for(&self, f: &SignFile) -> Place {
        let s = &self.s;
        if !s.firma_visible || self.place == 2 {
            return Place::Invisible;
        }
        if self.place == 0 {
            if let Some(field) = f.fields.first() {
                return Place::Field(field.name.clone());
            }
        }
        let pages = if f.pages.is_empty() { vec![[0.0, 0.0, 595.0, 842.0]] } else { f.pages.clone() };
        let last = pages.len() - 1;
        let (page, center) = if self.place == 1 && self.first_pdf().is_some_and(|p| p.path == f.path) {
            (self.box_page.min(last), Some(self.box_center))
        } else if self.place == 1 {
            // Gli altri PDF: stessa posizione, sull'ultima pagina se non ne hanno abbastanza.
            (self.box_page.min(last), Some(self.box_center))
        } else {
            (if s.firma_corner == 2 { 0 } else { last }, None)
        };
        let pb = pages[page];
        let mut rect = pades_firma::default_rect(pb);
        if s.firma_corner == 1 && center.is_none() {
            let w = rect[2] - rect[0];
            rect[0] = pb[0] + 42.0;
            rect[2] = rect[0] + w;
        }
        if let Some((cx, cy)) = center {
            let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
            let x = pb[0] + cx * (pb[2] - pb[0]);
            let y = pb[3] - cy * (pb[3] - pb[1]);
            rect = [x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0];
        } else if f.action == Action::PdfAgain {
            // Il PDF è già firmato: il riquadro nuovo va sopra a quelli che ci sono, non addosso.
            let h = rect[3] - rect[1];
            let up = ((h + 10.0) * f.signers.len() as f32).min((pb[3] - pb[1]) - h - 84.0).max(0.0);
            rect[1] += up;
            rect[3] += up;
        }
        Place::Rect { page: page as u32 + 1, rect }
    }

    /// Il riquadro in frazioni della pagina (x0, y0, x1, y1, y dall'alto) per disegnarlo nell'anteprima.
    pub fn box_frac(&self) -> Option<(usize, [f32; 4])> {
        let f = self.first_pdf()?;
        let (page, rect) = match self.place_for(f) {
            Place::Field(n) => {
                let fl = f.fields.iter().find(|x| x.name == n)?;
                (fl.page as usize - 1, fl.rect)
            }
            Place::Rect { page, rect } => (page as usize - 1, rect),
            Place::Invisible => return None,
        };
        let pb = *f.pages.get(page)?;
        let (w, h) = (pb[2] - pb[0], pb[3] - pb[1]);
        Some((page, [(rect[0] - pb[0]) / w, 1.0 - (rect[3] - pb[1]) / h, (rect[2] - pb[0]) / w, 1.0 - (rect[1] - pb[1]) / h]))
    }

    /// Si può firmare con il dispositivo scelto?
    pub fn can_sign(&self) -> bool {
        self.device().and_then(|d| d.signing_cert()).is_some()
    }

    /// Firma tutti i file con il PIN scritto.
    pub fn sign(&mut self, wake: Wake) {
        if self.running || self.files.is_empty() {
            return;
        }
        let Some(dev) = self.device().cloned() else {
            self.error = Some("Collega la chiavetta o inserisci la carta, poi premi «Cerca di nuovo».".into());
            return;
        };
        let Some(cert) = dev.signing_cert().cloned() else {
            self.error = Some("Questo dispositivo non ha un certificato di firma.".into());
            return;
        };
        if self.pin.is_empty() && !dev.pin.pinpad {
            self.error = Some("Scrivi il PIN della chiavetta.".into());
            return;
        }
        self.plan();
        self.error = None;
        self.running = true;
        let pin = std::mem::take(&mut self.pin);
        let jobs: Vec<(usize, SignFile, Place)> = self.files.iter().enumerate().filter(|(_, f)| !matches!(f.state, State::Done { .. })).map(|(i, f)| (i, f.clone(), self.place_for(f))).collect();
        for (i, _, _) in &jobs {
            self.files[*i].state = State::Wait;
        }
        let s = self.s.clone();
        let inbox = self.inbox.clone();
        let image = load_image(&s.firma_image);
        std::thread::spawn(move || {
            let send = |m: Msg| {
                inbox.lock().unwrap_or_else(|e| e.into_inner()).push(m);
                wake();
            };
            // Un panic (chiavetta, rete) non deve lasciare la firma «in corso» per sempre.
            let _finito = SeSiRompe(inbox.clone(), wake.clone());
            let pin_opt = (!pin.is_empty()).then_some(pin.as_str());
            let mut key = match pkcs11::TokenKey::open(&dev, &cert.id, pin_opt) {
                Ok(k) => k,
                Err(e) => {
                    send(Msg::Finished(Some(e)));
                    return;
                }
            };
            // Il certificato dell'autorità nella busta, se la chiavetta non ce l'ha (dal suo indirizzo AIA).
            if let Some(ca) = firma::revoca::fetch_issuer(&cert.cert) {
                key.add_chain(ca.der.clone());
            }
            let name = cert.cert.subject.person();
            for (i, f, place) in jobs {
                send(Msg::Start(i));
                if !s.firma_keep_pin {
                    // PIN chiesto alla chiavetta di nuovo per ogni file.
                    key.close();
                    key = match pkcs11::TokenKey::open(&dev, &cert.id, pin_opt) {
                        Ok(k) => k,
                        Err(e) => {
                            send(Msg::Done(i, Err(e.clone())));
                            send(Msg::Finished(Some(e)));
                            return;
                        }
                    };
                }
                let look = Look { name: name.clone(), reason: s.firma_reason.trim().to_string(), location: s.firma_location.trim().to_string(), when: firma::now(), image: image.clone() };
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sign_one(&f, &key, &place, &look))).unwrap_or_else(|_| Err("errore interno durante la firma".into()));
                let fatal = r.as_ref().err().filter(|e| e.starts_with("PIN") || e.contains("tolta") || e.contains("non risponde")).cloned();
                send(Msg::Done(i, r));
                if let Some(e) = fatal {
                    key.close();
                    send(Msg::Finished(Some(e)));
                    return;
                }
            }
            key.close();
            send(Msg::Finished(None));
        });
    }

    /// Cambia PIN o PUK, o sblocca (dalla finestrella).
    pub fn run_dialog(&mut self, wake: Wake) {
        let Some(d) = self.dialog.as_mut() else { return };
        let Some(dev) = self.devices.get(self.device).cloned() else { return };
        if d.new != d.again {
            d.error = Some("I due nuovi non sono uguali.".into());
            return;
        }
        if d.old.is_empty() || d.new.is_empty() {
            d.error = Some("Scrivi tutti i campi.".into());
            return;
        }
        if dev.pin.min_len > 0 && d.new.chars().count() < dev.pin.min_len && d.kind != 2 {
            d.error = Some(format!("Il nuovo PIN deve avere almeno {} cifre.", dev.pin.min_len));
            return;
        }
        d.busy = true;
        d.error = None;
        let (kind, old, new) = (d.kind, d.old.clone(), d.new.clone());
        let inbox = self.inbox.clone();
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(|| match kind {
                0 => pkcs11::change_pin(&dev, &old, &new).map(|_| "PIN cambiato.".to_string()),
                1 => pkcs11::unlock_pin(&dev, &old, &new).map(|_| "PIN sbloccato: ora vale quello nuovo.".to_string()),
                _ => pkcs11::change_puk(&dev, &old, &new).map(|_| "PUK cambiato.".to_string()),
            })
            .unwrap_or_else(|_| Err("errore interno parlando con la chiavetta".into()));
            inbox.lock().unwrap_or_else(|e| e.into_inner()).push(Msg::Dialog(r));
            wake();
        });
    }

    /// Controlla online se il certificato è stato revocato.
    pub fn check_revocation(&mut self, k: usize, wake: Wake) {
        let Some(c) = self.device().and_then(|d| d.certs.get(k)).cloned() else { return };
        let inbox = self.inbox.clone();
        self.info = Some(("Chiedo al certificatore…".into(), false));
        std::thread::spawn(move || {
            let msg = std::panic::catch_unwind(|| match firma::revoca::fetch_issuer(&c.cert) {
                None => ("Non trovo il certificato di chi l'ha rilasciato: serve internet.".to_string(), true),
                Some(ca) => {
                    let r = firma::revoca::check(&c.cert, &ca, &[], true);
                    match r.status {
                        firma::revoca::Status::Good => ("Il certificato non è revocato né sospeso.".to_string(), false),
                        firma::revoca::Status::Revoked { .. } => ("Il certificato risulta REVOCATO o sospeso: non usarlo per firmare.".to_string(), true),
                        _ => ("Non sono riuscito a chiederlo al certificatore. Riprova più tardi.".to_string(), true),
                    }
                }
            })
            .unwrap_or_else(|_| ("Non sono riuscito a chiederlo al certificatore. Riprova più tardi.".to_string(), true));
            inbox.lock().unwrap_or_else(|e| e.into_inner()).push(Msg::Info(msg.0, msg.1));
            wake();
        });
    }

    /// Le risposte dei thread. Vero se è cambiato qualcosa.
    pub fn pump(&mut self) -> bool {
        let got: Vec<Msg> = std::mem::take(&mut *self.inbox.lock().unwrap_or_else(|e| e.into_inner()));
        let changed = !got.is_empty();
        for m in got {
            match m {
                Msg::Scanned(d, e) => {
                    let keep = self.device().map(|d| d.key());
                    self.devices = d;
                    self.scan_errors = e;
                    self.scanning = false;
                    self.scanned_once = true;
                    // Si tiene quello scelto; se no il primo che firma.
                    self.device = keep
                        .and_then(|k| self.devices.iter().position(|d| d.key() == k))
                        .or_else(|| self.devices.iter().position(|d| d.signing_cert().is_some()))
                        .unwrap_or(0);
                    self.remind();
                }
                Msg::Start(i) => {
                    if let Some(f) = self.files.get_mut(i) {
                        f.state = State::Run;
                    }
                }
                Msg::Done(i, r) => {
                    if let Some(f) = self.files.get_mut(i) {
                        f.state = match r {
                            Ok((at, signers)) => State::Done { at, signers },
                            Err(e) => State::Failed(e),
                        };
                    }
                }
                Msg::Finished(err) => {
                    self.running = false;
                    // Dopo un PIN giusto la chiavetta toglie l'avviso «pochi tentativi»: si rilegge.
                    if let Some(d) = self.devices.get_mut(self.device) {
                        if let Some(p) = pkcs11::pin_state(d) {
                            d.pin = p;
                        }
                    }
                    match err {
                        Some(e) if self.files.iter().all(|f| !matches!(f.state, State::Done { .. })) => {
                            // Niente firmato: si resta sul PIN con l'errore.
                            for f in &mut self.files {
                                f.state = State::Wait;
                            }
                            self.error = Some(e);
                        }
                        Some(e) => {
                            self.error = Some(e);
                            self.step = 4;
                        }
                        None => self.step = 4,
                    }
                    if self.step == 4 {
                        for f in &self.files {
                            if matches!(f.state, State::Done { .. }) {
                                if let Ok(data) = std::fs::read(&f.out) {
                                    let opts = firma::Opts { online: false, ..Default::default() };
                                    let r = firma::verify_bytes(&f.out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), &data, &opts);
                                    super::firma::remember(&f.out, &r);
                                }
                            }
                        }
                    }
                }
                Msg::Dialog(r) => match r {
                    Ok(t) => {
                        self.dialog = None;
                        self.info = Some((t, false));
                        if let Some(d) = self.devices.get_mut(self.device) {
                            if let Some(p) = pkcs11::pin_state(d) {
                                d.pin = p;
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(d) = self.dialog.as_mut() {
                            d.busy = false;
                            d.error = Some(e);
                        }
                        if let Some(d) = self.devices.get_mut(self.device) {
                            if let Some(p) = pkcs11::pin_state(d) {
                                d.pin = p;
                            }
                        }
                    }
                },
                Msg::Info(t, bad) => self.info = Some((t, bad)),
            }
        }
        changed
    }

    pub fn busy(&self) -> bool {
        self.running || self.scanning || self.dialog.as_ref().is_some_and(|d| d.busy)
    }

    /// Torna all'inizio della sezione (dopo «Fatto» la coda si svuota).
    pub fn home(&mut self) {
        if self.running {
            return;
        }
        if self.step == 4 {
            self.files.clear();
            self.step = 1;
        }
        self.page = Page::Home;
        self.pin.clear();
        self.error = None;
        self.dialog = None;
        self.info = None;
    }

    /// «Avvisami prima che scada»: una volta, quando mancano 3 mesi o meno.
    fn remind(&mut self) {
        if self.reminded || !self.s.firma_remind {
            return;
        }
        let Some(left) = self.days_left() else { return };
        let who = self.device().map(|d| d.title()).unwrap_or_default();
        let t = match left {
            l if l < 0 => format!("Il certificato di firma della {who} è scaduto: chiedi il rinnovo al certificatore."),
            0 => format!("Il certificato di firma della {who} scade oggi: rinnovalo subito."),
            1 => format!("Il certificato di firma della {who} scade domani: rinnovalo subito."),
            l if l <= 7 => format!("Il certificato di firma della {who} scade tra {l} giorni: rinnovalo subito."),
            l if l <= 90 => format!("Il certificato di firma della {who} scade tra {l} giorni: rinnovalo per tempo."),
            _ => return,
        };
        self.reminded = true;
        self.reminder = Some(t);
    }

    /// Giorni alla scadenza del certificato di firma del dispositivo scelto.
    pub fn days_left(&self) -> Option<i64> {
        let c = self.device()?.signing_cert()?;
        Some((c.cert.not_after - firma::now()).div_euclid(86400))
    }
}

/// L'immagine del riquadro (PNG/JPG), ridotta: RGB e trasparenza.
fn load_image(path: &str) -> Option<(u32, u32, Vec<u8>, Option<Vec<u8>>)> {
    if path.trim().is_empty() {
        return None;
    }
    let img = image::open(path.trim()).ok()?;
    let img = img.thumbnail(400, 400).to_rgba8();
    let (w, h) = img.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut a = Vec::with_capacity((w * h) as usize);
    for p in img.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        a.push(p.0[3]);
    }
    let alpha = a.iter().any(|&x| x < 255).then_some(a);
    Some((w, h, rgb, alpha))
}

/// Firma un file e lo scrive: restituisce l'ora e chi l'ha firmato ora.
fn sign_one(f: &SignFile, key: &pkcs11::TokenKey, place: &Place, look: &Look) -> Result<(i64, Vec<String>), String> {
    let data = std::fs::read(&f.path).map_err(|e| format!("non riesco a leggere il file: {e}"))?;
    let now = firma::now();
    let out = match f.action {
        Action::Pdf | Action::PdfAgain => pades_firma::sign(&data, key, place, look)?,
        Action::ConvertPdf => {
            let tmp = crate::engine::private_temp_dir("cedro-firma")?;
            let o = crate::engine::perform_into(&f.path, "pdf", crate::engine::read_quality(), Some(&tmp)).map_err(|e| format!("non riesco a trasformarlo in PDF: {e}"))?;
            let pdf = std::fs::read(&o.path).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_dir_all(&tmp);
            let pages = pades_firma::pages(&pdf).unwrap_or_default();
            // Il posto: quello scelto, rifatto sulle pagine del PDF nuovo.
            let place = match place {
                Place::Rect { .. } | Place::Field(_) => {
                    let last = pages.last().copied().unwrap_or([0.0, 0.0, 595.0, 842.0]);
                    Place::Rect { page: pages.len().max(1) as u32, rect: pades_firma::default_rect(last) }
                }
                Place::Invisible => Place::Invisible,
            };
            pades_firma::sign(&pdf, key, &place, look)?
        }
        Action::P7m => firmare::cades(&data, key, true, Some(now))?,
        Action::Parallel => firmare::add_parallel(&data, key, Some(now))?,
        Action::Counter => firmare::add_counter(&data, key, Some(now), &[])?,
    };
    if let Some(d) = f.out.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("non riesco a creare la cartella: {e}"))?;
    }
    // Si scrive accanto e poi si sostituisce: se qualcosa va storto il file di prima resta.
    let tmp = f.out.with_extension("cedro-tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("non riesco a salvare: {e}"))?;
    std::fs::rename(&tmp, &f.out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("non riesco a salvare: {e}")
    })?;
    Ok((now, firmare::signer_names(&out)))
}

/// È già un file firmato (busta .p7m o PDF con firme)? Per smistare i file lasciati sulla sezione.
pub fn is_signed(p: &Path) -> bool {
    let l = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if [".p7m", ".p7s", ".m7m", ".tsd", ".tsr", ".p7e"].iter().any(|e| l.ends_with(e)) {
        return true;
    }
    // Non si leggono file enormi solo per guardarli.
    if std::fs::metadata(p).map(|m| m.len() > 200 << 20).unwrap_or(true) {
        return false;
    }
    let data = std::fs::read(p).unwrap_or_default();
    if firma::pades::is_pdf(&data) {
        return !firmare::signer_names(&data).is_empty();
    }
    firmare::is_envelope(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un PDF già firmato: il riquadro della firma nuova va sopra a quello di prima, non addosso.
    #[test]
    fn second_signature_box_goes_above() {
        let mut su = SignUi::default();
        su.s.firma_visible = true;
        su.s.firma_corner = 0;
        let page = [0.0, 0.0, 595.0, 842.0];
        let mk = |signers: Vec<String>, action| SignFile { path: "a.pdf".into(), action, out: "a.pdf".into(), signers, fields: Vec::new(), pages: vec![page], state: State::Wait };
        let first = mk(Vec::new(), Action::Pdf);
        let again = mk(vec!["MARIO ROSSI".into()], Action::PdfAgain);
        let (Place::Rect { rect: a, .. }, Place::Rect { rect: b, .. }) = (su.place_for(&first), su.place_for(&again)) else { panic!() };
        assert!(b[1] >= a[3], "{a:?} {b:?}");
    }
}
