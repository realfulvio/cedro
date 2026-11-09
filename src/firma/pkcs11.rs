//! La chiavetta di firma e la smart card (CNS, carta d'identità): si parla con il driver PKCS#11
//! del produttore (bit4id per la Aruba Key, Athena/Bit4id per le CNS, InfoCert, OpenSC…), caricato
//! quando serve. Si leggono i certificati, si controlla il PIN e si firma l'impronta: la chiave
//! privata non esce mai dalla chiavetta.

use super::asn1::{self, R};
use super::cert::Cert;
use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
use cryptoki::error::{Error, RvError};
use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, AttributeType, ObjectClass, ObjectHandle};
use cryptoki::session::{Session, UserType};
use cryptoki::slot::Slot;
use cryptoki::types::AuthPin;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// I driver che si conoscono, dal più specifico (produttore) al più generico (OpenSC).
#[cfg(target_os = "linux")]
const NAMES: &[&str] = &[
    "libbit4xpki.so",
    "libbit4ipki.so",
    "libbit4opki.so",
    "libbit4npki.so",
    "libbit4spki.so",
    "libbit4p11.so",
    "libASEP11.so",
    "libincryptoki2.so",
    "incryptoki2.so",
    "libinp11.so",
    "libcvP11.so",
    "libIDPrimePKCS11.so",
    "libeTPkcs11.so",
    "libcmP11.so",
    "libsiecap11.so",
    "libcnspkcs11.so",
    "opensc-pkcs11.so",
];

#[cfg(target_os = "linux")]
const DIRS: &[&str] = &[
    "/usr/lib",
    "/usr/lib64",
    "/usr/local/lib",
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
    "/usr/lib/pkcs11",
    "/usr/lib/x86_64-linux-gnu/pkcs11",
    "/usr/lib64/pkcs11",
    "/usr/lib/opensc",
    "/opt/bit4id/lib",
    "/opt/arubasign/lib",
];

#[cfg(windows)]
const NAMES: &[&str] = &[
    "bit4xpki.dll",
    "bit4ipki.dll",
    "bit4opki.dll",
    "bit4npki.dll",
    "bit4p11.dll",
    "asepkcs.dll",
    "incryptoki2.dll",
    "cvP11.dll",
    "OcsCryptoki.dll",
    "IDPrimePKCS11.dll",
    "eTPKCS11.dll",
    "cmP11.dll",
    "siecap11.dll",
    "aetpkss1.dll",
    "opensc-pkcs11.dll",
];

/// Dove si cercano i driver.
pub fn driver_candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    // Per le prove (SoftHSM) e per chi ha un driver strano: CEDRO_PKCS11=/percorso/driver.so
    if let Ok(v) = std::env::var("CEDRO_PKCS11") {
        out.extend(std::env::split_paths(&v).filter(|p| p.is_file()));
    }
    let manual = crate::read_settings().firma_driver;
    if !manual.is_empty() {
        let p = PathBuf::from(manual);
        if p.is_file() && !out.contains(&p) {
            out.push(p);
        }
    }
    #[cfg(target_os = "linux")]
    for n in NAMES {
        for d in DIRS {
            let p = Path::new(d).join(n);
            if p.is_file() && !out.iter().any(|x| x.file_name() == p.file_name()) {
                out.push(p);
            }
        }
    }
    #[cfg(windows)]
    {
        let sys = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");
        let pf = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        for n in NAMES {
            for p in [sys.join(n), pf.join("OpenSC Project").join("OpenSC").join("pkcs11").join(n)] {
                if p.is_file() && !out.iter().any(|x| x.file_name() == p.file_name()) {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn is_generic(p: &Path) -> bool {
    p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase().contains("opensc")).unwrap_or(false)
}

/// Il nome breve del driver da mostrare («bit4xpki»).
pub fn driver_name(p: &Path) -> String {
    let s = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    s.strip_prefix("lib").unwrap_or(&s).to_string()
}

/// I driver già caricati: si caricano una volta sola (alcuni non amano essere chiusi e riaperti).
fn loaded() -> &'static Mutex<Vec<(PathBuf, Pkcs11)>> {
    static L: OnceLock<Mutex<Vec<(PathBuf, Pkcs11)>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(Vec::new()))
}

fn module(path: &Path) -> R<Pkcs11> {
    let mut l = loaded().lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, m)) = l.iter().find(|(p, _)| p == path) {
        return Ok(m.clone());
    }
    let m = Pkcs11::new(path).map_err(|e| format!("il driver {} non si carica: {e}", driver_name(path)))?;
    match m.initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK)) {
        Ok(()) | Err(Error::Pkcs11(RvError::CryptokiAlreadyInitialized, _)) => {}
        Err(e) => return Err(format!("il driver {} non parte: {}", driver_name(path), say(&e))),
    }
    l.push((path.to_path_buf(), m.clone()));
    Ok(m)
}

/// Che tipo di dispositivo è (per l'icona e il nome).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevKind {
    /// Chiavetta USB con dentro il chip (Aruba Key, InfoCert, Namirial…).
    UsbKey,
    /// Smart card in un lettore: CNS, tessera sanitaria, firma su carta.
    Card,
}

/// Un certificato della chiavetta.
#[derive(Clone, Debug)]
pub struct DevCert {
    pub der: Vec<u8>,
    pub cert: Cert,
    pub id: Vec<u8>,
    pub label: String,
    /// C'è la chiave privata (si può firmare o entrare nei siti).
    pub has_key: bool,
    /// È un certificato di firma (non ripudio), non solo per entrare nei siti.
    pub signing: bool,
}

/// Lo stato del PIN (da quello che dice il driver).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PinState {
    pub count_low: bool,
    pub final_try: bool,
    pub locked: bool,
    pub to_be_changed: bool,
    pub puk_locked: bool,
    pub puk_final_try: bool,
    /// Il PIN si scrive sul lettore (tastierino), non al computer.
    pub pinpad: bool,
    pub min_len: usize,
    pub max_len: usize,
}

impl PinState {
    /// Cosa dire dei tentativi: il driver dice solo se ne restano pochi o uno, non quanti.
    pub fn tries_text(&self) -> &'static str {
        if self.locked {
            "PIN bloccato: sbloccalo con il PUK"
        } else if self.final_try {
            "ultimo tentativo prima del blocco"
        } else if self.count_low {
            "hai già sbagliato: pochi tentativi prima del blocco"
        } else {
            "se lo sbagli troppe volte si blocca"
        }
    }
}

/// Un dispositivo collegato: chiavetta o carta.
#[derive(Clone, Debug)]
pub struct Device {
    pub driver: PathBuf,
    pub slot: u64,
    pub kind: DevKind,
    pub label: String,
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub reader: String,
    pub certs: Vec<DevCert>,
    pub pin: PinState,
}

impl Device {
    /// Il certificato con cui si firma (non ripudio, con la chiave).
    pub fn signing_cert(&self) -> Option<&DevCert> {
        self.certs.iter().find(|c| c.has_key && c.signing)
    }

    /// Il nome per le persone: «Aruba Key», «Tessera sanitaria CNS», o quello scritto sul chip.
    pub fn title(&self) -> String {
        let all = format!("{} {} {} {}", self.label, self.manufacturer, self.model, self.reader).to_lowercase();
        let driver = driver_name(&self.driver).to_lowercase();
        if all.contains("cns") || all.contains("tessera") || all.contains("ts-cns") || all.contains("sanitaria") {
            return "Tessera sanitaria CNS".into();
        }
        if all.contains("aruba") || (driver.starts_with("bit4") && self.kind == DevKind::UsbKey && all.contains("bit4id")) {
            return "Aruba Key".into();
        }
        if all.contains("infocert") || driver.contains("incryptoki") {
            return if self.kind == DevKind::Card { "Smart card InfoCert".into() } else { "Chiavetta InfoCert".into() };
        }
        if all.contains("cie") || all.contains("carta d'identit") {
            return "Carta d'identità elettronica".into();
        }
        let l = self.label.trim();
        if !l.is_empty() {
            return l.to_string();
        }
        match self.kind {
            DevKind::UsbKey => "Chiavetta di firma".into(),
            DevKind::Card => "Smart card".into(),
        }
    }

    /// Il titolare (dal certificato di firma, o dal primo).
    pub fn owner(&self) -> String {
        self.signing_cert().or(self.certs.first()).map(|c| c.cert.subject.person()).unwrap_or_default()
    }

    /// Una chiave per riconoscerla tra una ricerca e l'altra.
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.serial, self.label, self.slot)
    }
}

/// Le parole per un errore del driver.
pub fn say(e: &Error) -> String {
    match e {
        Error::Pkcs11(rv, _) => match rv {
            RvError::PinIncorrect => "PIN sbagliato".into(),
            RvError::PinLocked => "PIN bloccato: sbloccalo con il PUK (Chiavetta e certificati › Sblocca con il PUK)".into(),
            RvError::PinLenRange => "il PIN ha troppe o troppo poche cifre".into(),
            RvError::PinInvalid => "il PIN contiene caratteri non validi".into(),
            RvError::PinExpired => "il PIN è scaduto: cambialo".into(),
            RvError::TokenNotPresent | RvError::DeviceRemoved | RvError::SessionHandleInvalid | RvError::SessionClosed => "la chiavetta è stata tolta".into(),
            RvError::DeviceError | RvError::DeviceMemory => "la chiavetta non risponde: toglila e rimettila".into(),
            RvError::FunctionCanceled => "annullato".into(),
            RvError::UserTypeInvalid | RvError::UserNotLoggedIn => "serve il PIN".into(),
            other => format!("errore della chiavetta ({other})"),
        },
        Error::LibraryLoading(l) => format!("driver non caricato: {l}"),
        other => format!("errore della chiavetta ({other})"),
    }
}

fn text(s: &str) -> String {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\0').to_string()
}

fn read_certs(s: &Session) -> Vec<DevCert> {
    let mut out = Vec::new();
    let Ok(handles) = s.find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE)]) else { return out };
    let keys: Vec<Vec<u8>> = s
        .find_objects(&[Attribute::Class(ObjectClass::PRIVATE_KEY)])
        .unwrap_or_default()
        .into_iter()
        .filter_map(|h| s.get_attributes(h, &[AttributeType::Id]).ok())
        .filter_map(|a| a.into_iter().find_map(|x| if let Attribute::Id(v) = x { Some(v) } else { None }))
        .collect();
    for h in handles {
        let Ok(attrs) = s.get_attributes(h, &[AttributeType::Value, AttributeType::Id, AttributeType::Label]) else { continue };
        let (mut der, mut id, mut label) = (Vec::new(), Vec::new(), String::new());
        for a in attrs {
            match a {
                Attribute::Value(v) => der = v,
                Attribute::Id(v) => id = v,
                Attribute::Label(v) => label = String::from_utf8_lossy(&v).into_owned(),
                _ => {}
            }
        }
        let Ok(cert) = Cert::parse(&der) else { continue };
        // Le chiavi private spesso si vedono solo dopo il PIN: senza PIN si suppone che ci sia per i
        // certificati personali (non quelli delle autorità).
        let has_key = keys.contains(&id) || (keys.is_empty() && !cert.is_ca);
        let signing = !cert.is_ca && (cert.non_repudiation || cert.qc_sscd && cert.qc_esign);
        out.push(DevCert { der, cert, id, label, has_key, signing });
    }
    // Prima quello di firma.
    out.sort_by_key(|c| (!c.signing, !c.has_key, c.cert.is_ca));
    out
}

/// Cerca chiavette e carte collegate. Restituisce i dispositivi e i problemi incontrati.
pub fn scan() -> (Vec<Device>, Vec<String>) {
    let mut devs: Vec<Device> = Vec::new();
    let mut errors = Vec::new();
    let drivers = driver_candidates();
    let (specific, generic): (Vec<PathBuf>, Vec<PathBuf>) = drivers.into_iter().partition(|p| !is_generic(p));
    for (round, list) in [specific, generic].into_iter().enumerate() {
        // OpenSC solo se i driver dei produttori non hanno trovato niente.
        if round == 1 && !devs.is_empty() {
            break;
        }
        for path in list {
            let m = match module(&path) {
                Ok(m) => m,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let slots = match m.get_slots_with_token() {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("{}: {}", driver_name(&path), say(&e)));
                    continue;
                }
            };
            for slot in slots {
                let Ok(ti) = m.get_token_info(slot) else { continue };
                let si = m.get_slot_info(slot).ok();
                let serial = text(ti.serial_number());
                let label = text(ti.label());
                if devs.iter().any(|d| !serial.is_empty() && d.serial == serial && d.label == label) {
                    continue;
                }
                let reader = si.as_ref().map(|s| text(s.slot_description())).unwrap_or_default();
                let manufacturer = text(ti.manufacturer_id());
                let model = text(ti.model());
                let low = format!("{reader} {model} {manufacturer}").to_lowercase();
                let usb_key = ["minilector", "aruba", "token", "key", "digital dna", "dna", "bit4id", "usb"].iter().any(|k| low.contains(k)) && !low.contains("cns");
                let certs = match m.open_ro_session(slot) {
                    Ok(s) => read_certs(&s),
                    Err(_) => Vec::new(),
                };
                let pin = PinState {
                    count_low: ti.user_pin_count_low(),
                    final_try: ti.user_pin_final_try(),
                    locked: ti.user_pin_locked(),
                    to_be_changed: ti.user_pin_to_be_changed(),
                    puk_locked: ti.so_pin_locked(),
                    puk_final_try: ti.so_pin_final_try(),
                    pinpad: ti.protected_authentication_path(),
                    min_len: ti.min_pin_length(),
                    max_len: ti.max_pin_length(),
                };
                devs.push(Device { driver: path.clone(), slot: slot.id(), kind: if usb_key { DevKind::UsbKey } else { DevKind::Card }, label, manufacturer, model, serial, reader, certs, pin });
            }
        }
    }
    // Prima quelli che firmano.
    devs.sort_by_key(|d| d.signing_cert().is_none());
    (devs, errors)
}

fn slot_of(m: &Pkcs11, id: u64) -> R<Slot> {
    m.get_slots_with_token().map_err(|e| say(&e))?.into_iter().find(|s| s.id() == id).ok_or_else(|| "la chiavetta non è più collegata".to_string())
}

fn pin(p: &str) -> AuthPin {
    AuthPin::new(p.to_string().into())
}

/// La chiave della chiavetta, aperta con il PIN: firma finché non la si chiude.
pub struct TokenKey {
    session: Mutex<Session>,
    key: ObjectHandle,
    rsa: bool,
    always_auth: bool,
    pin: Option<AuthPin>,
    cert: Vec<u8>,
    chain: Vec<Vec<u8>>,
}

impl TokenKey {
    /// Apre la chiavetta con il PIN (None = PIN sul tastierino del lettore) per il certificato `cert_id`.
    pub fn open(dev: &Device, cert_id: &[u8], user_pin: Option<&str>) -> R<TokenKey> {
        let m = module(&dev.driver)?;
        let slot = slot_of(&m, dev.slot)?;
        let s = m.open_ro_session(slot).map_err(|e| say(&e))?;
        let p = user_pin.map(pin);
        match s.login(UserType::User, p.as_ref()) {
            Ok(()) | Err(Error::Pkcs11(RvError::UserAlreadyLoggedIn, _)) => {}
            Err(e) => return Err(say(&e)),
        }
        let keys = s.find_objects(&[Attribute::Class(ObjectClass::PRIVATE_KEY), Attribute::Id(cert_id.to_vec())]).map_err(|e| say(&e))?;
        let mut key = keys.first().copied();
        if key.is_none() {
            // Qualche chiavetta non mette lo stesso ID: si prende l'unica chiave di firma.
            let all = s.find_objects(&[Attribute::Class(ObjectClass::PRIVATE_KEY)]).map_err(|e| say(&e))?;
            if all.len() == 1 {
                key = all.first().copied();
            }
        }
        let key = key.ok_or("nella chiavetta non trovo la chiave del certificato di firma")?;
        let attrs = s.get_attributes(key, &[AttributeType::KeyType, AttributeType::AlwaysAuthenticate]).unwrap_or_default();
        let mut rsa = true;
        let mut always_auth = false;
        for a in attrs {
            match a {
                Attribute::KeyType(k) => rsa = k == cryptoki::object::KeyType::RSA,
                Attribute::AlwaysAuthenticate(b) => always_auth = b,
                _ => {}
            }
        }
        let dc = dev.certs.iter().find(|c| c.id == cert_id).ok_or("certificato non trovato")?;
        // La catena: i certificati delle autorità che stanno sulla chiavetta.
        let mut chain = Vec::new();
        let mut cur = dc.cert.clone();
        for _ in 0..4 {
            match dev.certs.iter().find(|c| c.cert.subject_raw == cur.issuer_raw && c.der != cur.der) {
                Some(up) => {
                    chain.push(up.der.clone());
                    cur = up.cert.clone();
                }
                None => break,
            }
        }
        Ok(TokenKey { session: Mutex::new(s), key, rsa, always_auth, pin: p, cert: dc.der.clone(), chain })
    }

    /// Aggiunge alla busta il certificato dell'autorità preso da internet (se non è sulla chiavetta).
    pub fn add_chain(&mut self, der: Vec<u8>) {
        if !self.chain.contains(&der) {
            self.chain.push(der);
        }
    }

    pub fn close(self) {
        if let Ok(s) = self.session.into_inner() {
            let _ = s.logout();
        }
    }
}

impl super::firmare::Key for TokenKey {
    fn cert(&self) -> &[u8] {
        &self.cert
    }

    fn chain(&self) -> Vec<Vec<u8>> {
        self.chain.clone()
    }

    fn sign_sha256(&self, digest: &[u8]) -> R<Vec<u8>> {
        let s = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if self.always_auth {
            // Le chiavi «firma qualificata» vogliono il PIN a ogni firma.
            s.login(UserType::ContextSpecific, self.pin.as_ref()).map_err(|e| say(&e))?;
        }
        if self.rsa {
            let alg = asn1::seq(&[&asn1::oid_der(super::oid::SHA256), &[0x05, 0x00]]);
            let info = asn1::seq(&[&alg, &asn1::tlv(0x04, digest)]);
            s.sign(&Mechanism::RsaPkcs, self.key, &info).map_err(|e| say(&e))
        } else {
            let raw = s.sign(&Mechanism::Ecdsa, self.key, digest).map_err(|e| say(&e))?;
            Ok(ecdsa_der(&raw))
        }
    }
}

/// La firma ECDSA del PKCS#11 è r‖s: nel CMS va in DER.
fn ecdsa_der(raw: &[u8]) -> Vec<u8> {
    let half = raw.len() / 2;
    let int = |b: &[u8]| {
        let mut v: Vec<u8> = b.iter().copied().skip_while(|&x| x == 0).collect();
        if v.is_empty() || v[0] & 0x80 != 0 {
            v.insert(0, 0);
        }
        asn1::tlv(0x02, &v)
    };
    asn1::seq(&[&int(&raw[..half]), &int(&raw[half..])])
}

fn rw_session(dev: &Device) -> R<Session> {
    let m = module(&dev.driver)?;
    let slot = slot_of(&m, dev.slot)?;
    m.open_rw_session(slot).map_err(|e| say(&e))
}

/// Cambia il PIN.
pub fn change_pin(dev: &Device, old: &str, new: &str) -> R<()> {
    let s = rw_session(dev)?;
    s.login(UserType::User, Some(&pin(old))).map_err(|e| say(&e))?;
    let r = s.set_pin(&pin(old), &pin(new)).map_err(|e| say(&e));
    let _ = s.logout();
    r
}

/// Sblocca il PIN con il PUK e ne mette uno nuovo.
pub fn unlock_pin(dev: &Device, puk: &str, new: &str) -> R<()> {
    let s = rw_session(dev)?;
    s.login(UserType::So, Some(&pin(puk))).map_err(|e| match say(&e).as_str() {
        "PIN sbagliato" => "PUK sbagliato".to_string(),
        x if x.starts_with("PIN bloccato") => "anche il PUK è bloccato: serve il certificatore".to_string(),
        x => x.to_string(),
    })?;
    let r = s.init_pin(&pin(new)).map_err(|e| say(&e));
    let _ = s.logout();
    r
}

/// Cambia il PUK.
pub fn change_puk(dev: &Device, old: &str, new: &str) -> R<()> {
    let s = rw_session(dev)?;
    s.login(UserType::So, Some(&pin(old))).map_err(|e| match say(&e).as_str() {
        "PIN sbagliato" => "PUK sbagliato".to_string(),
        x => x.to_string(),
    })?;
    let r = s.set_pin(&pin(old), &pin(new)).map_err(|e| say(&e));
    let _ = s.logout();
    r
}

/// Lo stato del PIN aggiornato (dopo un tentativo sbagliato).
pub fn pin_state(dev: &Device) -> Option<PinState> {
    let m = module(&dev.driver).ok()?;
    let slot = slot_of(&m, dev.slot).ok()?;
    let ti = m.get_token_info(slot).ok()?;
    Some(PinState {
        count_low: ti.user_pin_count_low(),
        final_try: ti.user_pin_final_try(),
        locked: ti.user_pin_locked(),
        to_be_changed: ti.user_pin_to_be_changed(),
        puk_locked: ti.so_pin_locked(),
        puk_final_try: ti.so_pin_final_try(),
        ..dev.pin
    })
}

/// Il dispositivo condiviso tra i thread (la UI e chi firma).
pub type Shared = Arc<Mutex<Vec<Device>>>;

/// Prova con SoftHSM2 (PKCS#11 software): parte solo se c'è CEDRO_SOFTHSM_PIN (lo prepara
/// tools/prova-softhsm.sh, in CI e in locale).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::firma::verify::{verify_bytes, Opts};
    use crate::firma::Level;

    #[test]
    fn softhsm() {
        let (Ok(user), Ok(so)) = (std::env::var("CEDRO_SOFTHSM_PIN"), std::env::var("CEDRO_SOFTHSM_SO_PIN")) else {
            eprintln!("SoftHSM non preparato: salto (tools/prova-softhsm.sh)");
            return;
        };
        let (devs, errors) = scan();
        assert!(!devs.is_empty(), "nessun dispositivo: {errors:?}");
        let dev = devs.iter().find(|d| d.signing_cert().is_some()).expect("manca il certificato di firma").clone();
        let c = dev.signing_cert().unwrap().clone();
        assert!(c.cert.subject.person().contains("MARIO"), "{}", c.cert.subject.person());
        eprintln!("{} · {} · {}", dev.title(), dev.owner(), dev.pin.tries_text());

        // PIN sbagliato.
        let e = TokenKey::open(&dev, &c.id, Some("000000")).err().unwrap();
        assert_eq!(e, "PIN sbagliato");

        // Firma .p7m e PDF.
        let k = TokenKey::open(&dev, &c.id, Some(&user)).unwrap();
        let p7m = crate::firma::firmare::cades(b"Determina 412", &k, true, Some(crate::firma::now())).unwrap();
        let r = verify_bytes("d.txt.p7m", &p7m, &Opts { online: false, ..Default::default() });
        let doc = r.signers[0].checks.iter().find(|x| x.what == "Documento").unwrap();
        assert_eq!(doc.level, Level::Ok, "{}", doc.text);
        let two = crate::firma::firmare::add_parallel(&p7m, &k, None).unwrap();
        assert_eq!(verify_bytes("d.txt.p7m", &two, &Opts { online: false, ..Default::default() }).signers.len(), 2);
        let pdf = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/firma/determina_firmata.pdf")).unwrap();
        let look = crate::firma::pades_firma::Look { name: "MARIO ROSSI".into(), reason: "Approvazione".into(), location: "Bondeno".into(), when: crate::firma::now(), image: None };
        let signed = crate::firma::pades_firma::sign(&pdf, &k, &crate::firma::pades_firma::Place::Rect { page: 1, rect: [330.0, 60.0, 540.0, 124.0] }, &look).unwrap();
        let r = verify_bytes("d.pdf", &signed, &Opts { online: false, ..Default::default() });
        assert_eq!(r.signers.len(), 2);
        assert!(r.signers.iter().all(|s| s.checks.iter().find(|x| x.what == "Documento").unwrap().level == Level::Ok));
        if let Ok(dir) = std::env::var("CEDRO_SOFTHSM_OUT") {
            let _ = std::fs::write(std::path::Path::new(&dir).join("softhsm.pdf"), &signed);
            let _ = std::fs::write(std::path::Path::new(&dir).join("softhsm.txt.p7m"), &two);
        }
        k.close();

        // Cambio del PIN, sblocco con il PUK, cambio del PUK: alla fine tutto come prima.
        change_pin(&dev, &user, "998877").unwrap();
        assert_eq!(change_pin(&dev, &user, "112233").err().as_deref(), Some("PIN sbagliato"));
        unlock_pin(&dev, &so, &user).unwrap();
        change_puk(&dev, &so, "55667788").unwrap();
        change_puk(&dev, "55667788", &so).unwrap();
        let k = TokenKey::open(&dev, &c.id, Some(&user)).unwrap();
        k.close();
    }
}
