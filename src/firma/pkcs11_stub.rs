//! Su Android la chiavetta non c'è: stesse forme di `pkcs11`, ma non trova niente.

use super::asn1::R;
use super::cert::Cert;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevKind {
    UsbKey,
    Card,
}

#[derive(Clone, Debug)]
pub struct DevCert {
    pub der: Vec<u8>,
    pub cert: Cert,
    pub id: Vec<u8>,
    pub label: String,
    pub has_key: bool,
    pub signing: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PinState {
    pub count_low: bool,
    pub final_try: bool,
    pub locked: bool,
    pub to_be_changed: bool,
    pub puk_locked: bool,
    pub puk_final_try: bool,
    pub pinpad: bool,
    pub min_len: usize,
    pub max_len: usize,
}

impl PinState {
    pub fn tries_text(&self) -> &'static str {
        "se lo sbagli troppe volte si blocca"
    }
}

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
    pub fn signing_cert(&self) -> Option<&DevCert> {
        self.certs.iter().find(|c| c.has_key && c.signing)
    }
    pub fn title(&self) -> String {
        self.label.clone()
    }
    pub fn owner(&self) -> String {
        String::new()
    }
    pub fn key(&self) -> String {
        self.serial.clone()
    }
}

pub fn driver_candidates() -> Vec<PathBuf> {
    Vec::new()
}

pub fn driver_name(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn scan() -> (Vec<Device>, Vec<String>) {
    (Vec::new(), Vec::new())
}

pub struct TokenKey {
    cert: Vec<u8>,
}

impl TokenKey {
    pub fn open(_: &Device, _: &[u8], _: Option<&str>) -> R<TokenKey> {
        Err("su questo sistema la chiavetta non si può usare".into())
    }
    pub fn add_chain(&mut self, _: Vec<u8>) {}
    pub fn close(self) {}
}

impl super::firmare::Key for TokenKey {
    fn cert(&self) -> &[u8] {
        &self.cert
    }
    fn sign_sha256(&self, _: &[u8]) -> R<Vec<u8>> {
        Err("non disponibile".into())
    }
}

pub fn change_pin(_: &Device, _: &str, _: &str) -> R<()> {
    Err("non disponibile".into())
}
pub fn unlock_pin(_: &Device, _: &str, _: &str) -> R<()> {
    Err("non disponibile".into())
}
pub fn change_puk(_: &Device, _: &str, _: &str) -> R<()> {
    Err("non disponibile".into())
}
pub fn pin_state(_: &Device) -> Option<PinState> {
    None
}
