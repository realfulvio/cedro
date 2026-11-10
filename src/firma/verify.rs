//! La verifica completa di un file firmato e il suo rapporto in parole semplici: chi ha firmato,
//! se la firma vale e perché, com'è fatto il file (buste una dentro l'altra) e il documento dentro.

use super::cert::Cert;
use super::cms::{self, SignedData, SignerInfo, Timestamp};
use super::revoca::{self, Status};
use super::trust::{self, Kind, Store};
use super::{asn1, crypto, date, date_time, long_date, oid, pades, relative, title_case};
use base64::Engine;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Level {
    #[default]
    Ok,
    Warn,
    Bad,
}

/// Una riga dei controlli: «Documento · non modificato».
#[derive(Clone, Debug)]
pub struct Check {
    pub level: Level,
    pub what: &'static str,
    pub text: String,
    /// Conta per il giudizio sulla firma? (la mancanza della marca è un avviso leggero)
    pub counts: bool,
}

/// Dove sta la firma in un PDF.
#[derive(Clone, Debug, Default)]
pub struct Place {
    pub page: Option<u32>,
    pub field: Option<String>,
    pub visible: bool,
    /// Il riquadro sulla pagina, in frazioni (x0, y0, x1, y1; y dall'alto).
    pub rect: Option<[f32; 4]>,
}

#[derive(Clone, Debug, Default)]
pub struct Signer {
    pub name: String,
    pub initials: String,
    /// Titolo e organizzazione dal certificato, e dove sta la firma.
    pub role: String,
    /// L'ora della firma: certa (da una marca) o dichiarata.
    pub when: Option<i64>,
    pub when_certain: bool,
    pub level: Level,
    pub summary: String,
    pub why: String,
    pub checks: Vec<Check>,
    pub details: Vec<(String, String)>,
    pub chain: Vec<String>,
    pub cert: Option<Cert>,
    pub chain_certs: Vec<Cert>,
    /// «CAdES-T · SHA-256 · RSA 2048»
    pub kind: String,
    /// In quale busta (0 = la più esterna; per i PDF la busta del PDF).
    pub layer: usize,
    /// Controfirma della firma con questo indice.
    pub counter_of: Option<usize>,
    pub place: Option<Place>,
    /// Per i PDF modificati: la versione firmata.
    pub revision_end: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// Busta firmata (.p7m) o firma staccata (.p7s).
    Envelope,
    /// Busta con la marca temporale (.tsd, .m7m).
    Timestamped,
    /// Il documento, con dentro eventualmente le sue firme (PDF).
    Document,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub kind: LayerKind,
    pub label: String,
    /// Gli indici delle firme di questo strato.
    pub signers: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    Pdf,
    Image,
    Text,
    Xml,
    Word,
    Office,
    Other,
    None,
}

/// Una marca temporale sul documento (.tsd, .m7m, .tsr, marca del PDF).
#[derive(Clone, Debug)]
pub struct TsReport {
    pub time: i64,
    pub tsa: String,
    pub level: Level,
    pub text: String,
    pub qualified: bool,
    pub serial: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    /// Oggi (o all'ora certa della marca, se c'è).
    Now,
    /// Quando è stato firmato.
    Signing,
    /// Un altro giorno.
    Custom(i64),
}

#[derive(Clone, Debug)]
pub struct Opts {
    pub at: At,
    /// Chiedere online la revoca e i certificati mancanti.
    pub online: bool,
    /// Per le firme staccate (.p7s): il file originale.
    pub detached: Option<Vec<u8>>,
    /// Per le prove: l'ora di «adesso».
    pub now: Option<i64>,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { at: At::Now, online: true, detached: None, now: None }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub name: String,
    pub level: Level,
    pub headline: String,
    pub detail: String,
    pub signers: Vec<Signer>,
    pub layers: Vec<Layer>,
    pub timestamps: Vec<TsReport>,
    /// Il documento dentro le buste.
    pub doc: Vec<u8>,
    pub doc_name: String,
    pub doc_kind: Option<DocKind>,
    pub checked_at: i64,
    pub online: bool,
    pub trust_updated: Option<i64>,
    /// Il file non è firmato, o non si legge: il perché.
    pub error: Option<String>,
    /// Firma staccata senza il file originale.
    pub needs_original: bool,
}

impl Report {
    /// Il file non è firmato né marcato (o non si legge): non c'è niente da giudicare.
    pub fn unsigned(&self) -> bool {
        self.error.is_some() && self.signers.is_empty() && self.timestamps.is_empty() && !self.error.as_deref().is_some_and(|e| e.contains("cifrato"))
    }

    /// La revoca è stata davvero chiesta online per tutte le firme (non solo «controllo acceso»).
    pub fn revoca_online(&self) -> bool {
        self.online && !self.signers.iter().any(|s| s.checks.iter().any(|c| c.what == "Revoca" && c.text == "non controllata"))
    }

    /// Il documento così com'era alla firma `i` (per i PDF cambiati dopo).
    pub fn signed_revision(&self, i: usize) -> Option<&[u8]> {
        let end = self.signers.get(i)?.revision_end?;
        self.doc.get(..end)
    }

    /// Le firme da mostrare in elenco (ogni controfirma subito dopo quella che controfirma).
    pub fn ordered(&self) -> Vec<usize> {
        let mut out = Vec::new();
        fn add(r: &Report, i: usize, out: &mut Vec<usize>) {
            out.push(i);
            for (j, s) in r.signers.iter().enumerate() {
                if s.counter_of == Some(i) {
                    add(r, j, out);
                }
            }
        }
        for (i, s) in self.signers.iter().enumerate() {
            if s.counter_of.is_none() {
                add(self, i, &mut out);
            }
        }
        out
    }
}

pub fn verify_path(path: &Path, mut opts: Opts) -> Report {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return Report { name, error: Some(format!("non riesco a leggere il file: {e}")), ..Default::default() },
    };
    // Firma staccata: l'originale ha lo stesso nome senza .p7s.
    // Lo stesso per una marca staccata (.tsr): il file marcato le sta accanto.
    let lower = name.to_ascii_lowercase();
    if opts.detached.is_none() && (lower.ends_with(".p7s") || lower.ends_with(".tsr")) {
        let orig = path.with_file_name(&name[..name.len() - 4]);
        if let Ok(d) = std::fs::read(orig) {
            opts.detached = Some(d);
        }
    }
    verify_bytes(&name, &data, &opts)
}

fn strip_ext(name: &str, exts: &[&str]) -> String {
    let l = name.to_ascii_lowercase();
    for e in exts {
        if l.ends_with(&format!(".{e}")) {
            return name[..name.len() - e.len() - 1].to_string();
        }
    }
    name.to_string()
}

/// Busta in base64 o PEM (succede con i .p7m scaricati da alcuni portali).
fn decode_text(data: &[u8]) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(data.get(..data.len().min(1 << 26))?).ok()?;
    let t = s.trim();
    let body = if t.starts_with("-----BEGIN") {
        t.lines().filter(|l| !l.starts_with("-----")).collect::<String>()
    } else if t.starts_with("MII") || t.starts_with("MIA") || t.starts_with("MIE") {
        t.to_string()
    } else {
        return None;
    };
    let clean: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(clean).ok()
}

/// Le parti di un .m7m (MIME): la busta firmata e la marca.
fn mime_parts(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let head = String::from_utf8_lossy(&data[..data.len().min(4096)]).to_ascii_lowercase();
    if !head.contains("multipart") {
        return None;
    }
    let text = String::from_utf8_lossy(data);
    let lower = text.to_ascii_lowercase();
    let bpos = lower.find("boundary=")? + 9;
    let rest = &text[bpos..];
    let boundary: String = rest.trim_start_matches('"').chars().take_while(|c| *c != '"' && *c != ';' && *c != '\r' && *c != '\n').collect();
    let sep = format!("--{boundary}");
    let mut p7m = None;
    let mut tsr = None;
    for part in text.split(&sep).skip(1) {
        let Some(split) = part.find("\r\n\r\n").map(|i| (i, 4)).or_else(|| part.find("\n\n").map(|i| (i, 2))) else { continue };
        let (h, body) = (&part[..split.0], &part[split.0 + split.1..]);
        let hl = h.to_ascii_lowercase();
        let b64 = hl.contains("base64");
        let bytes = if b64 {
            let clean: String = body.chars().filter(|c| !c.is_whitespace()).collect();
            base64::engine::general_purpose::STANDARD.decode(clean.trim_end_matches("--")).ok()
        } else {
            Some(body.as_bytes().to_vec())
        };
        let Some(bytes) = bytes else { continue };
        if hl.contains("timestamp") || hl.contains(".tsr") {
            tsr = Some(bytes);
        } else if hl.contains("pkcs7") || hl.contains(".p7m") {
            p7m = Some(bytes);
        }
    }
    Some((p7m?, tsr?))
}

/// RFC 5544 TimeStampedData (.tsd): il contenuto e le sue marche.
fn parse_tsd(n: &asn1::Node) -> asn1::R<(Option<Vec<u8>>, Vec<Vec<u8>>)> {
    let k = n.children()?;
    let mut content = None;
    let mut tokens = Vec::new();
    for x in k.iter().skip(1) {
        match (x.tag, x.ctx()) {
            (0x04, _) | (0x24, _) => content = Some(x.octets()?),
            (_, Some(0)) => {
                for tc in x.children()? {
                    if let Some(tok) = tc.children()?.first() {
                        tokens.push(tok.raw.to_vec());
                    }
                }
            }
            _ => {}
        }
    }
    Ok((content, tokens))
}

pub fn doc_kind(name: &str, data: &[u8]) -> DocKind {
    let l = name.to_ascii_lowercase();
    let ext = l.rsplit('.').next().unwrap_or("");
    if pades::is_pdf(data) {
        return DocKind::Pdf;
    }
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) || data.starts_with(b"\x89PNG") || data.starts_with(b"GIF8") || data.starts_with(b"II*\0") || data.starts_with(b"MM\0*") || data.starts_with(b"BM") {
        return DocKind::Image;
    }
    match ext {
        "docx" | "doc" | "odt" | "rtf" => return DocKind::Word,
        "xlsx" | "xls" | "ods" | "pptx" | "ppt" | "odp" => return DocKind::Office,
        "xml" => return DocKind::Xml,
        "txt" | "csv" | "log" | "md" | "json" | "eml" => return DocKind::Text,
        _ => {}
    }
    let head = &data[..data.len().min(512)];
    if head.starts_with(b"<?xml") || head.starts_with(b"\xEF\xBB\xBF<?xml") {
        return DocKind::Xml;
    }
    if !data.is_empty() && std::str::from_utf8(&data[..data.len().min(4096)]).is_ok_and(|s| !s.chars().any(|c| c.is_control() && !c.is_whitespace())) {
        return DocKind::Text;
    }
    DocKind::Other
}

/// Un'ora certa, con chi l'ha data.
#[derive(Clone, Debug)]
struct Proof {
    time: i64,
    tsa: String,
    ok: bool,
}

fn earliest(a: Option<Proof>, b: Option<Proof>) -> Option<Proof> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if x.ok && (!y.ok || x.time <= y.time) { x } else { y }),
        (x, y) => x.or(y),
    }
}

fn short_tsa(name: &str) -> String {
    let first = name.split([' ', ',']).next().unwrap_or(name);
    let first = first.trim_end_matches("PEC");
    if first.len() < 3 || first.eq_ignore_ascii_case("tsa") || first.eq_ignore_ascii_case("time") {
        name.to_string()
    } else {
        first.to_string()
    }
}

fn ts_report(t: &Timestamp, store: &Store) -> TsReport {
    let qualified = t.tsa.as_ref().is_some_and(|c| {
        store.find(c).iter().chain(store.issuers_of(c).iter()).any(|e| e.kind == Kind::QualifiedTsa && e.granted_at(t.gen_time))
    });
    let (level, text) = if !t.signature_ok {
        (Level::Bad, "la marca è rovinata: la sua firma non torna".to_string())
    } else if qualified {
        (Level::Ok, format!("marca qualificata {}", short_tsa(&t.tsa_name)))
    } else if store.entries.is_empty() {
        (Level::Ok, format!("marca {}", short_tsa(&t.tsa_name)))
    } else {
        (Level::Warn, format!("marca {}, servizio fuori dall'elenco UE", short_tsa(&t.tsa_name)))
    };
    TsReport { time: t.gen_time, tsa: t.tsa_name.clone(), level, text, qualified, serial: t.serial_hex.clone() }
}

struct Ctx<'a> {
    opts: &'a Opts,
    store: std::sync::Arc<Store>,
    report: Report,
    now: i64,
}

pub fn verify_bytes(name: &str, data: &[u8], opts: &Opts) -> Report {
    let store = trust::store();
    let mut cx = Ctx { opts, store: store.clone(), report: Report::default(), now: opts.now.unwrap_or_else(super::now) };
    cx.report.name = name.to_string();
    cx.report.checked_at = cx.now;
    cx.report.online = opts.online;
    cx.report.trust_updated = store.updated;
    if let Err(e) = run(&mut cx, name, data) {
        cx.report.error = Some(e);
    }
    finish(&mut cx.report);
    cx.report
}

fn run(cx: &mut Ctx, name: &str, data: &[u8]) -> asn1::R<()> {
    let mut name = name.to_string();
    let mut cur: Vec<u8> = data.to_vec();
    let mut outer_proof: Option<Proof> = None;
    let mut depth = 0;
    let mut tsr_used = false;
    loop {
        depth += 1;
        if depth > 12 {
            return Err("troppe buste una dentro l'altra".into());
        }
        if pades::is_pdf(&cur) {
            break;
        }
        if let Some(d) = decode_text(&cur) {
            cur = d;
            continue;
        }
        if let Some((p7m, tsr)) = mime_parts(&cur) {
            let label = format!("Busta con marca temporale ({})", if name.to_ascii_lowercase().ends_with(".m7m") { ".m7m" } else { "MIME" });
            let mut proof = None;
            if let Ok(t) = cms::timestamp_from_tsr(&tsr) {
                let mut r = ts_report(&t, &cx.store);
                if !t.covers(&p7m) {
                    r.level = Level::Bad;
                    r.text = "la marca non corrisponde a questo file".into();
                }
                proof = Some(Proof { time: t.gen_time, tsa: short_tsa(&t.tsa_name), ok: r.level != Level::Bad });
                cx.report.timestamps.push(r);
            }
            outer_proof = earliest(outer_proof, proof);
            cx.report.layers.push(Layer { kind: LayerKind::Timestamped, label, signers: vec![] });
            name = strip_ext(&name, &["m7m"]);
            if !name.to_ascii_lowercase().ends_with(".p7m") {
                name.push_str(".p7m");
            }
            cur = p7m;
            continue;
        }
        let Ok((ty, node)) = cms::content_info(&cur) else {
            // Risposta di marca (.tsr, TimeStampResp): dentro c'è la marca vera e propria.
            let tok = asn1::parse(&cur).and_then(|n| n.children()).ok().and_then(|k| k.get(1).map(|t| t.raw.to_vec()));
            match tok {
                Some(t) if cms::parse_timestamp(&t).is_ok() => {
                    cur = t;
                    continue;
                }
                _ => break,
            }
        };
        match ty.as_str() {
            oid::TIMESTAMPED_DATA => {
                let (content, tokens) = parse_tsd(&node)?;
                let content = content.ok_or("il .tsd non contiene il file")?;
                let mut proof = None;
                for tok in tokens {
                    if let Ok(t) = cms::parse_timestamp(&tok) {
                        let mut r = ts_report(&t, &cx.store);
                        if !t.covers(&content) {
                            r.level = Level::Bad;
                            r.text = "la marca non corrisponde a questo file".into();
                        }
                        proof = earliest(proof, Some(Proof { time: t.gen_time, tsa: short_tsa(&t.tsa_name), ok: r.level != Level::Bad }));
                        cx.report.timestamps.push(r);
                    }
                }
                outer_proof = earliest(outer_proof, proof);
                cx.report.layers.push(Layer { kind: LayerKind::Timestamped, label: "Busta con marca temporale (.tsd)".into(), signers: vec![] });
                name = strip_ext(&name, &["tsd"]);
                cur = content;
            }
            oid::SIGNED_DATA => {
                let sd = cms::parse_signed_data(&node)?;
                if sd.content_type == oid::TST_INFO {
                    // Una marca da sola (.tsr): di quale file non si sa.
                    let t = cms::parse_timestamp(&cur).or_else(|_| cms::timestamp_from_tsr(&cur))?;
                    let mut r = ts_report(&t, &cx.store);
                    if let Some(orig) = &cx.opts.detached {
                        if !t.covers(orig) {
                            r.level = Level::Bad;
                            r.text = "la marca non corrisponde al file indicato".into();
                        }
                    }
                    let proof = Proof { time: t.gen_time, tsa: short_tsa(&t.tsa_name), ok: r.level != Level::Bad };
                    cx.report.timestamps.push(r);
                    cx.report.layers.push(Layer { kind: LayerKind::Timestamped, label: "Marca temporale (.tsr)".into(), signers: vec![] });
                    name = strip_ext(&name, &["tsr"]);
                    match cx.opts.detached.clone() {
                        // Il file marcato: se è a sua volta firmato si controllano anche le sue firme.
                        Some(d) if !tsr_used => {
                            tsr_used = true;
                            outer_proof = earliest(outer_proof, Some(proof));
                            cur = d;
                            continue;
                        }
                        Some(d) => cur = d,
                        None => {
                            cx.report.needs_original = true;
                            cur = Vec::new();
                        }
                    }
                    break;
                }
                let detached = sd.content.is_none();
                let content = match &sd.content {
                    Some(c) => c.clone(),
                    None => match &cx.opts.detached {
                        Some(d) => d.clone(),
                        None => {
                            cx.report.needs_original = true;
                            Vec::new()
                        }
                    },
                };
                let label = if detached { "Firma staccata (.p7s)" } else if depth == 1 || cx.report.layers.iter().all(|l| l.kind != LayerKind::Envelope) { "Busta esterna" } else { "Busta interna" };
                let layer_ix = cx.report.layers.len();
                cx.report.layers.push(Layer { kind: LayerKind::Envelope, label: label.into(), signers: vec![] });
                let proof = envelope(cx, &sd, &content, layer_ix, outer_proof.clone(), detached && cx.report.needs_original);
                outer_proof = earliest(outer_proof, proof);
                name = strip_ext(&name, &["p7m", "p7s", "p7c"]);
                cur = content;
                if cx.report.needs_original {
                    break;
                }
            }
            oid::ENVELOPED_DATA => {
                return Err("il file è cifrato (.p7e): per aprirlo serve la chiavetta o la CNS di chi lo deve leggere".into());
            }
            _ => break,
        }
    }
    // Rinomina gli strati: con una sola busta è «Busta firmata».
    let envs: Vec<usize> = cx.report.layers.iter().enumerate().filter(|(_, l)| l.kind == LayerKind::Envelope && l.label != "Firma staccata (.p7s)").map(|(i, _)| i).collect();
    if envs.len() == 1 {
        cx.report.layers[envs[0]].label = "Busta firmata".into();
    } else if envs.len() > 2 {
        for (k, &i) in envs.iter().enumerate() {
            cx.report.layers[i].label = match k {
                0 => "Busta esterna".into(),
                k if k == envs.len() - 1 => "Busta interna".into(),
                k => format!("Busta intermedia ({}ª)", k + 1),
            };
        }
    }
    cx.report.doc_name = name.clone();
    cx.report.doc_kind = Some(if cur.is_empty() { DocKind::None } else { doc_kind(&name, &cur) });
    let doc_layer = cx.report.layers.len();
    cx.report.layers.push(Layer { kind: LayerKind::Document, label: name.clone(), signers: vec![] });
    if pades::is_pdf(&cur) {
        pdf(cx, &cur, doc_layer, outer_proof);
    }
    cx.report.doc = cur;
    if cx.report.signers.is_empty() && cx.report.timestamps.is_empty() && !cx.report.needs_original {
        return Err(if cx.report.doc_kind == Some(DocKind::Pdf) {
            "questo PDF non ha firme digitali".into()
        } else {
            "questo file non è firmato digitalmente (o non riconosco il formato)".into()
        });
    }
    Ok(())
}

/// Le firme di una busta (con le controfirme). Ritorna l'ora certa più vecchia data da queste firme.
fn envelope(cx: &mut Ctx, sd: &SignedData, content: &[u8], layer: usize, outer: Option<Proof>, missing: bool) -> Option<Proof> {
    let mut best: Option<Proof> = None;
    for si in &sd.signers {
        let p = signer(cx, si, sd, layer, None, &outer, missing, &|alg| crypto::digest(alg, content));
        best = earliest(best, p);
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn signer(cx: &mut Ctx, si: &SignerInfo, sd: &SignedData, layer: usize, counter_of: Option<usize>, outer: &Option<Proof>, missing: bool, digest: &dyn Fn(&str) -> asn1::R<Vec<u8>>) -> Option<Proof> {
    let chk = cms::check_signer(si, &sd.certs, digest);
    // Le marche sulla firma.
    let mut own: Option<Proof> = None;
    let mut ts_ok = true;
    for tok in si.unsigned_attrs(oid::SIG_TIMESTAMP) {
        if let Ok(t) = cms::parse_timestamp(tok) {
            let good = t.signature_ok && t.covers(&si.signature);
            ts_ok &= good;
            own = earliest(own, Some(Proof { time: t.gen_time, tsa: short_tsa(&t.tsa_name), ok: good }));
        }
    }
    let proof = earliest(own.clone().filter(|p| p.ok), outer.clone().filter(|p| p.ok));
    let kind = format!("CAdES-{}", if own.is_some() { "T" } else { "BES" });
    let mut s = evaluate(cx, &chk, si.signing_time(), proof.clone(), own.is_some() && !ts_ok, &sd.certs, &sd.crls, &si.digest_alg, &kind, missing);
    s.layer = layer;
    s.counter_of = counter_of;
    if let Some(parent) = counter_of {
        let pn = cx.report.signers.get(parent).map(|p| title_case(&p.name)).unwrap_or_default();
        s.role = if s.role.is_empty() { format!("controfirma {pn}") } else { format!("{} · controfirma {pn}", s.role) };
    }
    let ix = cx.report.signers.len();
    cx.report.signers.push(s);
    cx.report.layers[layer].signers.push(ix);
    // Le controfirme: firmano il valore della firma.
    for raw in si.unsigned_attrs(oid::COUNTER_SIGNATURE) {
        let Ok(n) = asn1::parse(raw) else { continue };
        let Ok(csi) = cms::parse_signer(&n) else { continue };
        let sig = si.signature.clone();
        signer(cx, &csi, sd, layer, Some(ix), &proof, false, &move |alg| crypto::digest(alg, &sig));
    }
    let sig_ok = chk.signature_ok && chk.digest_ok;
    if sig_ok {
        proof
    } else {
        None
    }
}

/// Le firme dentro il PDF.
fn pdf(cx: &mut Ctx, data: &[u8], layer: usize, outer: Option<Proof>) {
    let sigs = pades::signatures(data);
    // Le marche sul documento (DocTimeStamp): danno l'ora certa alle firme che coprono.
    let mut doc_ts: Vec<(usize, Proof)> = Vec::new();
    for ps in sigs.iter().filter(|s| s.doc_timestamp) {
        let Ok(t) = cms::parse_timestamp(&ps.contents) else { continue };
        let Ok(parts) = ps.parts(data) else { continue };
        let ok_cover = crypto::digest_parts(&t.hash_alg, &parts).map(|d| d == t.imprint).unwrap_or(false);
        let mut r = ts_report(&t, &cx.store);
        if !ok_cover {
            r.level = Level::Bad;
            r.text = "la marca del PDF non corrisponde più al documento".into();
        }
        doc_ts.push((ps.range[1], Proof { time: t.gen_time, tsa: short_tsa(&t.tsa_name), ok: r.level != Level::Bad && ok_cover }));
        cx.report.timestamps.push(r);
    }
    let dss = data.windows(4).any(|w| w == b"/DSS");
    // CRL e certificati del DSS: uguali per tutte le firme, si leggono una volta (ogni lettura carica tutto il PDF).
    let (dss_crl_list, dss_cert_list) = if dss { (dss_crls(data), dss_certs(data)) } else { (Vec::new(), Vec::new()) };
    for ps in sigs.iter().filter(|s| !s.doc_timestamp) {
        let Ok(parts) = ps.parts(data) else { continue };
        let sd = match cms::content_info(&ps.contents).and_then(|(_, n)| cms::parse_signed_data(&n)) {
            Ok(sd) => sd,
            Err(e) => {
                cx.report.signers.push(Signer {
                    name: ps.name.clone().unwrap_or_else(|| "Firma illeggibile".into()),
                    level: Level::Bad,
                    summary: "Non valida".into(),
                    why: format!("Non riesco a leggere questa firma: {e}."),
                    layer,
                    ..Default::default()
                });
                continue;
            }
        };
        let Some(si) = sd.signers.first() else { continue };
        let sha1_embedded = ps.sub_filter == "adbe.pkcs7.sha1";
        let content_digest = |alg: &str| -> asn1::R<Vec<u8>> {
            if sha1_embedded {
                // Il PDF è dentro la busta come impronta SHA-1.
                let inner = crypto::digest_parts(oid::SHA1, &parts)?;
                let c = sd.content.clone().unwrap_or_default();
                let wrapped = asn1::parse(&c).and_then(|n| n.octets()).unwrap_or(c);
                if wrapped != inner {
                    return Ok(Vec::new());
                }
                return crypto::digest(alg, &sd.content.clone().unwrap_or_default());
            }
            crypto::digest_parts(alg, &parts)
        };
        let chk = cms::check_signer(si, &sd.certs, &content_digest);
        let mut own = None;
        let mut ts_bad = false;
        for tok in si.unsigned_attrs(oid::SIG_TIMESTAMP) {
            if let Ok(t) = cms::parse_timestamp(tok) {
                let good = t.signature_ok && t.covers(&si.signature);
                ts_bad |= !good;
                own = earliest(own, Some(Proof { time: t.gen_time, tsa: short_tsa(&t.tsa_name), ok: good }));
            }
        }
        // Una marca del documento più avanti nel file copre anche questa firma.
        let later_doc_ts = doc_ts.iter().filter(|(start2, _)| *start2 >= ps.end()).map(|(_, p)| p.clone()).filter(|p| p.ok).min_by_key(|p| p.time);
        let proof = earliest(earliest(own.clone().filter(|p| p.ok), later_doc_ts), outer.clone().filter(|p| p.ok));
        let level_name = if ps.sub_filter.starts_with("adbe.") {
            "PKCS#7".to_string()
        } else if dss && proof.is_some() {
            "PAdES B-LT".into()
        } else if proof.is_some() {
            "PAdES B-T".into()
        } else {
            "PAdES B-B".into()
        };
        let mut crls = sd.crls.clone();
        crls.extend(dss_crl_list.iter().cloned());
        let mut certs_pool = sd.certs.clone();
        certs_pool.extend(dss_cert_list.iter().cloned());
        let claimed = si.signing_time().or(ps.m);
        let mut s = evaluate(cx, &chk, claimed, proof, ts_bad, &certs_pool, &crls, &si.digest_alg, &level_name, false);
        // Cosa è cambiato dopo.
        let after = pades::changes_after(data, ps.end());
        let changed_pages = match &after {
            pades::After::Content(p) => Some(p.clone()),
            _ => None,
        };
        if let Some(c) = s.checks.iter_mut().find(|c| c.what == "Documento") {
            if c.level == Level::Ok {
                match &after {
                    pades::After::Nothing | pades::After::OnlySignatures => c.text = "PDF non modificato".into(),
                    pades::After::Annotations => {
                        c.level = Level::Warn;
                        c.text = "dopo la firma aggiunte note o campi compilati".into();
                    }
                    pades::After::Content(_) => {
                        c.level = Level::Bad;
                        c.text = "cambiato dopo la firma".into();
                    }
                }
            }
        }
        if !ps.hole_is_contents(data) {
            // Oggetti nascosti nel buco della firma: quello che si vede può non essere quello firmato.
            if let Some(c) = s.checks.iter_mut().find(|c| c.what == "Documento") {
                c.level = Level::Bad;
                c.text = "la firma non copre tutto il file".into();
            }
            let rest = if s.why.starts_with("Il certificato era valido") { String::new() } else { s.why.clone() };
            s.why = format!("La firma non copre tutto il file: una parte del PDF può essere stata cambiata dopo la firma senza che si veda. {rest}").trim().to_string();
        }
        if !matches!(after, pades::After::Nothing) {
            s.revision_end = Some(ps.end());
        }
        if let Some(pages) = &changed_pages {
            let list = pages.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ");
            s.details.insert(2.min(s.details.len()), ("Cosa è cambiato".into(), format!("{} {list}, dopo la firma", if pages.len() == 1 { "pagina" } else { "pagine" })));
        }
        let place = Place { page: ps.page, field: ps.field.clone(), visible: ps.visible, rect: ps.rect };
        let mut where_ = String::new();
        if let Some(p) = ps.page {
            where_ = format!("pagina {p}");
            if let Some(f) = &ps.field {
                where_.push_str(&format!(", spazio «{f}»"));
            }
            if !ps.visible {
                where_.push_str(", firma non visibile");
            }
            s.details.push(("Dove".into(), where_.clone()));
        }
        if let Some(r) = &ps.reason {
            s.details.push(("Motivo".into(), r.clone()));
        }
        if let Some(l) = &ps.location {
            s.details.push(("Luogo".into(), l.clone()));
        }
        let inside = if ps.visible && ps.page.is_some() { format!("firma dentro il PDF, {}", where_.split(',').next().unwrap_or("")) } else { "firma dentro il PDF".into() };
        s.role = if s.role.is_empty() { inside } else { format!("{} · {inside}", s.role) };
        s.place = Some(place);
        s.layer = layer;
        rejudge(&mut s, changed_pages.is_some(), true);
        let ix = cx.report.signers.len();
        cx.report.signers.push(s);
        cx.report.layers[layer].signers.push(ix);
    }
}

fn dss_items(data: &[u8], key: &[u8]) -> Vec<Vec<u8>> {
    if !data.windows(4).any(|w| w == b"/DSS") {
        return vec![];
    }
    let Ok(doc) = lopdf::Document::load_mem(data) else { return vec![] };
    let Ok(cat) = doc.catalog() else { return vec![] };
    let Ok((_, lopdf::Object::Dictionary(dss))) = cat.get(b"DSS").and_then(|o| doc.dereference(o)) else { return vec![] };
    let Ok((_, lopdf::Object::Array(a))) = dss.get(key).and_then(|o| doc.dereference(o)) else { return vec![] };
    a.iter()
        .filter_map(|o| match doc.dereference(o) {
            Ok((_, lopdf::Object::Stream(s))) => s.decompressed_content().ok().or_else(|| Some(s.content.clone())),
            _ => None,
        })
        .collect()
}

fn dss_crls(data: &[u8]) -> Vec<Vec<u8>> {
    dss_items(data, b"CRLs")
}

fn dss_certs(data: &[u8]) -> Vec<Cert> {
    dss_items(data, b"Certs").iter().filter_map(|d| Cert::parse(d).ok()).collect()
}

/// La catena dal certificato del firmatario fino a un certificatore dell'elenco UE.
struct Chain {
    certs: Vec<Cert>,
    tl: Option<trust::Entry>,
    broken: bool,
}

fn build_chain(c: &Cert, pool: &[Cert], store: &Store, online: bool) -> Chain {
    let mut certs = vec![c.clone()];
    let mut cur = c.clone();
    let mut broken = false;
    for step in 0..6 {
        if step > 0 {
            if let Some(e) = store.find(&cur).into_iter().find(|e| e.kind == Kind::QualifiedCa) {
                return Chain { certs, tl: Some(e.clone()), broken };
            }
        }
        if let Some(e) = store.issuers_of(&cur).into_iter().find(|e| e.kind == Kind::QualifiedCa && crypto::cert_signed_by(&cur, &e.cert)) {
            certs.push(e.cert.clone());
            return Chain { certs, tl: Some(e.clone()), broken };
        }
        if cur.self_signed() {
            break;
        }
        let by_name: Vec<&Cert> = pool.iter().filter(|x| x.subject_raw == cur.issuer_raw || cms::same_name(&x.subject_raw, &cur.issuer_raw)).filter(|x| x.der != cur.der).collect();
        let next = by_name.iter().find(|x| crypto::cert_signed_by(&cur, x)).map(|x| (*x).clone());
        let next = match next {
            Some(n) => Some(n),
            None if online => revoca::fetch_issuer(&cur),
            None => None,
        };
        match next {
            Some(n) => {
                certs.push(n.clone());
                cur = n;
            }
            None => {
                broken = !by_name.is_empty();
                break;
            }
        }
    }
    Chain { certs, tl: None, broken }
}

fn role_of(c: &Cert) -> String {
    let t = c.subject.get(oid::TITLE);
    let o = c.subject.get(oid::ORG).filter(|o| !o.eq_ignore_ascii_case("non presente") && !o.eq_ignore_ascii_case("not present"));
    let ou = c.subject.get(oid::OU).filter(|o| !o.eq_ignore_ascii_case("non presente"));
    [t, o.or(ou)].into_iter().flatten().map(|s| s.to_string()).collect::<Vec<_>>().join(", ")
}

fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase()
}

fn tax_code(c: &Cert) -> Option<String> {
    c.tax_code().filter(|s| !s.is_empty())
}

/// Tutti i controlli di una firma e il giudizio in parole semplici.
#[allow(clippy::too_many_arguments)]
fn evaluate(cx: &mut Ctx, chk: &cms::SignerCheck, claimed: Option<i64>, proof: Option<Proof>, ts_bad: bool, pool: &[Cert], crls: &[Vec<u8>], digest_alg: &str, kind: &str, missing: bool) -> Signer {
    let mut s = Signer::default();
    let Some(c) = chk.cert.clone() else {
        s.name = "Firmatario sconosciuto".into();
        s.initials = "?".into();
        s.level = Level::Bad;
        s.summary = "Non valida".into();
        s.why = format!("Non trovo il certificato di chi ha firmato: {}.", chk.error.clone().unwrap_or_else(|| "manca nella busta".into()));
        s.checks.push(Check { level: Level::Bad, what: "Certificato", text: "mancante".into(), counts: true });
        return s;
    };
    s.name = c.subject.person();
    s.initials = initials(&s.name);
    s.role = role_of(&c);
    s.when = proof.as_ref().map(|p| p.time).or(claimed);
    s.when_certain = proof.is_some();
    let at = match cx.opts.at {
        At::Now => proof.as_ref().map(|p| p.time).unwrap_or(cx.now),
        At::Signing => proof.as_ref().map(|p| p.time).or(claimed).unwrap_or(cx.now),
        At::Custom(t) => t,
    };
    let mut why: Vec<String> = Vec::new();

    // Documento
    let doc = if missing {
        why.push("Manca il file originale: una firma staccata (.p7s) si controlla insieme al file che ha firmato. Indicalo e la controllo.".into());
        Check { level: Level::Warn, what: "Documento", text: "manca il file originale".into(), counts: true }
    } else if !chk.digest_ok {
        why.push("Dopo la firma qualcuno ha cambiato il documento: la firma non copre più quello che vedi.".into());
        Check { level: Level::Bad, what: "Documento", text: "cambiato dopo la firma".into(), counts: true }
    } else if !chk.signature_ok {
        why.push(format!("La firma non torna con il certificato: il file è rovinato o è stato manomesso{}.", chk.error.as_ref().map(|e| format!(" ({e})")).unwrap_or_default()));
        Check { level: Level::Bad, what: "Documento", text: "firma rovinata".into(), counts: true }
    } else {
        Check { level: Level::Ok, what: "Documento", text: "non modificato".into(), counts: true }
    };
    s.checks.push(doc);

    // Certificato
    let chain = build_chain(&c, pool, &cx.store, cx.opts.online);
    let mut cert_level = Level::Ok;
    let mut cert_text = "qualificato, in elenco UE".to_string();
    let qualified_cert = c.qc_compliance;
    match &chain.tl {
        Some(e) if e.granted_at(at) => {
            if !qualified_cert {
                cert_level = Level::Warn;
                cert_text = "non qualificato".into();
                why.push("Il certificato non è qualificato: la firma è avanzata, non ha il valore di una firma qualificata.".into());
            }
        }
        Some(e) => {
            cert_level = Level::Warn;
            cert_text = "certificatore non più accreditato".into();
            why.push(format!("{} a quella data non era accreditato nell'elenco europeo.", e.provider));
        }
        None if chain.broken => {
            cert_level = Level::Bad;
            cert_text = "catena dei certificati rotta".into();
            why.push("Il certificato non risulta rilasciato dal certificatore che dichiara: potrebbe essere falso.".into());
        }
        None if cx.store.entries.is_empty() => {
            cert_level = Level::Warn;
            cert_text = "elenco UE non ancora scaricato".into();
            why.push("Non ho ancora l'elenco europeo dei certificatori: lo scarico appena c'è la rete, poi riapri il file.".into());
        }
        None => {
            cert_level = Level::Warn;
            cert_text = "certificatore fuori dall'elenco UE".into();
            why.push("Chi ha rilasciato il certificato non è nell'elenco europeo dei certificatori: la firma è corretta ma non ha il valore di una firma qualificata.".into());
        }
    }
    if at < c.not_before {
        cert_level = Level::Bad;
        cert_text = format!("non ancora valido il {}", date(at));
        why.push(format!("Alla data della verifica il certificato non era ancora valido (vale dal {}).", date(c.not_before)));
    } else if at > c.not_after {
        cert_level = Level::Bad;
        cert_text = format!("scaduto il {}", date(c.not_after));
        why.push(format!(
            "Il certificato è scaduto il {} e non c'è una marca temporale che provi che la firma è di prima: a questa data la firma non vale più.",
            date(c.not_after)
        ));
    } else if cx.now > c.not_after && cert_level == Level::Ok {
        cert_level = Level::Warn;
        cert_text = "scaduto dopo la firma".into();
        why.push(format!("Il suo certificato è scaduto il {}, ma la marca temporale prova che ha firmato prima: la firma resta valida.", long_date(c.not_after)));
    }
    if chk.ess_ok == Some(false) {
        cert_level = Level::Bad;
        cert_text = "diverso da quello indicato nella firma".into();
        why.push("Il certificato nella busta non è quello indicato dentro la firma: qualcuno l'ha sostituito.".into());
    }
    s.checks.push(Check { level: cert_level, what: "Certificato", text: cert_text, counts: true });

    // Revoca
    let issuer = chain.certs.get(1).cloned();
    let rev = issuer.as_ref().map(|i| revoca::check(&c, i, crls, cx.opts.online));
    let revk = match &rev {
        Some(r) => match &r.status {
            Status::Good => Check { level: Level::Ok, what: "Revoca", text: "non revocato".into(), counts: true },
            Status::Revoked { at: rat, reason } => {
                let r_txt = reason.clone().map(|x| format!(" · {x}")).unwrap_or_default();
                if *rat <= at {
                    why.insert(
                        0,
                        format!(
                            "Il suo certificato è stato revocato il {}, prima della firma. Questa firma non ha valore legale: va chiesto di rifirmare con un certificato nuovo.",
                            long_date(*rat)
                        ),
                    );
                    s.details.push(("Revocato il".into(), format!("{}{r_txt}", date(*rat))));
                    Check { level: Level::Bad, what: "Revoca", text: format!("revocato il {}", date(*rat)), counts: true }
                } else {
                    s.details.push(("Revocato il".into(), format!("{}{r_txt}, dopo la firma", date(*rat))));
                    why.push(format!("Il certificato è stato revocato il {}, dopo la firma: la marca temporale prova che la firma è di prima.", long_date(*rat)));
                    Check { level: Level::Warn, what: "Revoca", text: "revocato dopo la firma".into(), counts: true }
                }
            }
            Status::Unknown(e) => {
                if cx.opts.online {
                    let plain = if e.contains("non conosce") {
                        "Il certificatore non conosce questo certificato: non so dire se è stato revocato.".to_string()
                    } else if e.contains("non dice dove") {
                        "Il certificato non dice a chi chiedere se è stato revocato.".to_string()
                    } else {
                        "Non sono riuscito a chiedere al certificatore se il certificato è stato revocato: forse manca la connessione o il servizio non risponde. Riprova più tardi.".to_string()
                    };
                    why.push(plain);
                    s.details.push(("Revoca, errore".into(), e.clone()));
                    Check { level: Level::Warn, what: "Revoca", text: "non controllata".into(), counts: true }
                } else {
                    Check { level: Level::Warn, what: "Revoca", text: "controllo online spento".into(), counts: false }
                }
            }
        },
        None => Check { level: Level::Warn, what: "Revoca", text: "non controllata".into(), counts: !cx.store.entries.is_empty() },
    };
    s.checks.push(revk);

    // Data certa
    let dc = if ts_bad {
        why.push("La marca temporale sulla firma non è valida.".into());
        Check { level: Level::Warn, what: "Data certa", text: "marca non valida".into(), counts: true }
    } else if let Some(p) = &proof {
        Check { level: Level::Ok, what: "Data certa", text: format!("marca {}", p.tsa), counts: true }
    } else if kind.starts_with("PAdES") || kind == "PKCS#7" {
        Check { level: Level::Warn, what: "Data certa", text: "ora del PDF, senza marca".into(), counts: false }
    } else {
        Check { level: Level::Warn, what: "Data certa", text: "nessuna marca".into(), counts: false }
    };
    s.checks.push(dc);

    // Dettagli tecnici
    let mut d: Vec<(String, String)> = Vec::new();
    if let Some(cf) = tax_code(&c) {
        d.push(("Codice fiscale".into(), cf));
    }
    d.push(("Certificatore".into(), c.issuer.common_name().to_string()));
    d.push(("Validità".into(), format!("{} – {}", date(c.not_before), date(c.not_after))));
    match (&proof, claimed) {
        (Some(p), _) => d.push(("Ora della firma".into(), format!("{}, marca {}", date_time(p.time), p.tsa))),
        (None, Some(t)) => d.push(("Ora della firma".into(), format!("{}, dichiarata dal suo computer", date_time(t)))),
        _ => {}
    }
    if let Some(t) = c.subject.get(oid::TITLE) {
        d.push(("Ruolo nel certificato".into(), t.to_string()));
    }
    d.push(("Tipo di firma".into(), format!("{kind} · {} · {}", crypto::hash_name(digest_alg), crypto::key_name(&c.key_alg, &c.key_params, &c.key))));
    if let Some(r) = &rev {
        if !r.source.is_empty() {
            d.push(("Revoca".into(), format!("controllata con {} {}", r.source, relative(cx.now))));
        }
    }
    d.push(("Numero di serie".into(), c.serial_hex()));
    if c.qc_sscd {
        d.push(("Dispositivo".into(), "sicuro (QSCD): chiavetta, smart card o firma remota qualificata".into()));
    }
    if crypto::hash_name(digest_alg) == "SHA-1" {
        d.push(("Attenzione".into(), "impronta SHA-1: oggi è considerata debole".into()));
    }
    s.details.extend(d);

    // Catena
    let mut ch: Vec<String> = Vec::new();
    if let Some(e) = &chain.tl {
        ch.push(if e.country == "IT" { "Elenco UE (AgID)".into() } else { format!("Elenco UE ({})", Store::country_name(&e.country)) });
    }
    for x in chain.certs.iter().skip(1).rev() {
        ch.push(x.subject.common_name().to_string());
    }
    ch.push(s.name.clone());
    s.chain = ch;
    s.chain_certs = chain.certs.clone();
    s.cert = Some(c);
    s.kind = kind.to_string();
    s.why = why.join(" ");
    rejudge(&mut s, false, false);
    s
}

/// Il giudizio dai controlli.
fn rejudge(s: &mut Signer, doc_changed: bool, pdf: bool) {
    s.level = s.checks.iter().filter(|c| c.counts).map(|c| c.level).max().unwrap_or(Level::Ok);
    let doc_bad = doc_changed || s.checks.iter().any(|c| c.what == "Documento" && c.level == Level::Bad && c.text.contains("cambiato"));
    s.summary = match s.level {
        Level::Ok => "Firma valida".into(),
        Level::Warn => "Valida, con un avviso".into(),
        Level::Bad if doc_bad => "Non valida: documento cambiato".into(),
        Level::Bad => "Non valida".into(),
    };
    if doc_changed && pdf && !s.why.contains("versione firmata") {
        s.why = format!("Dopo la firma qualcuno ha cambiato il PDF: la firma non copre più quello che vedi. Puoi vedere la versione firmata, com'era al momento della firma. {}", s.why.replace("Dopo la firma qualcuno ha cambiato il documento: la firma non copre più quello che vedi. ", "")).trim().to_string();
    }
    if s.level == Level::Warn {
        if let Some(c) = s.checks.iter().find(|c| c.what == "Documento" && c.level == Level::Warn && c.text.contains("note")) {
            let _ = c;
            if !s.why.contains("note") {
                s.why = format!("Dopo la firma nel PDF sono state aggiunte note o compilati dei campi; il testo delle pagine è quello firmato. {}", s.why).trim().to_string();
            }
        }
    }
    if s.why.is_empty() {
        let doc = if pdf { "il PDF non è stato cambiato dopo la firma" } else { "il documento non è stato modificato" };
        s.why = if s.when_certain {
            format!("Il certificato era valido e non revocato quando ha firmato, {doc}. L'ora è certa grazie alla marca temporale.")
        } else {
            format!("Il certificato era valido e non revocato quando ha firmato, {doc}.")
        };
    }
}

/// La fascia in alto: il giudizio su tutto il file.
fn finish(r: &mut Report) {
    if r.error.is_some() && r.signers.is_empty() {
        r.level = Level::Bad;
        r.headline = if r.error.as_deref().is_some_and(|e| e.contains("cifrato")) { "File cifrato".into() } else { "Nessuna firma da controllare".into() };
        r.detail = r.error.clone().unwrap_or_default();
        r.detail = upper_first(&r.detail);
        return;
    }
    if r.needs_original {
        r.level = Level::Warn;
        r.headline = "Manca il file originale".into();
        r.detail = if r.signers.is_empty() {
            "È una marca temporale staccata (.tsr): indica il file a cui si riferisce e la controllo.".into()
        } else {
            "È una firma staccata (.p7s): indica il file che è stato firmato e la controllo.".into()
        };
        return;
    }
    let n = r.signers.len();
    let bad: Vec<&Signer> = r.signers.iter().filter(|s| s.level == Level::Bad).collect();
    let warn: Vec<&Signer> = r.signers.iter().filter(|s| s.level == Level::Warn).collect();
    let ts_level = r.timestamps.iter().map(|t| t.level).max().unwrap_or(Level::Ok);
    r.level = r.signers.iter().map(|s| s.level).max().unwrap_or(Level::Ok).max(ts_level);
    let doc_changed = r.signers.iter().any(|s| s.checks.iter().any(|c| c.what == "Documento" && c.level == Level::Bad && c.text.contains("cambiato")));
    let doc_line = if doc_changed { "" } else { " Il documento non è stato modificato." };
    if n == 0 {
        // Solo marche (.tsr o .tsd di un file non firmato).
        let t = &r.timestamps[0];
        r.headline = if t.level == Level::Bad { "Marca temporale non valida".into() } else { "Marca temporale valida".into() };
        r.detail = format!("Data certa: {} alle {}, rilasciata da {}.", long_date(t.time), super::hm(t.time), t.tsa);
        if t.level != Level::Ok {
            r.detail = format!("{} {}", r.detail, upper_first(&t.text));
        }
        return;
    }
    if n == 1 {
        let s = &r.signers[0];
        r.headline = s.summary.clone();
        r.detail = match s.level {
            Level::Ok => {
                let when = s.when.map(|t| format!(" il {} alle {}", long_date(t), super::hm(t))).unwrap_or_default();
                format!("Firmato da {}{when}{}.", title_case(&s.name), if s.when_certain { ", con marca temporale" } else { "" })
            }
            _ => s.why.clone(),
        };
    } else if bad.is_empty() && warn.is_empty() {
        r.headline = format!("{n} firme, tutte valide");
        r.detail = format!("Hanno firmato {}.{doc_line}", names(&r.signers.iter().collect::<Vec<_>>()));
    } else if bad.is_empty() {
        r.headline = format!("{n} firme valide, con un avviso");
        r.detail = format!("Da guardare: {}.", names(&warn));
    } else {
        r.headline = format!("{n} firme: {} {}, {} no", n - bad.len(), if n - bad.len() == 1 { "vale" } else { "valgono" }, bad.len());
        if n == bad.len() {
            r.headline = format!("{n} firme, nessuna vale");
        }
        let first = bad[0];
        let reason = first.checks.iter().find(|c| c.counts && c.level == Level::Bad).map(|c| format!("{} {}", c.what.to_lowercase(), c.text)).unwrap_or_default();
        r.detail = if bad.len() == 1 {
            format!("La firma di {} non è valida ({reason}).{doc_line}", title_case(&first.name))
        } else {
            format!("Non valgono le firme di {}.{doc_line}", names(&bad))
        };
    }
    if ts_level == Level::Bad {
        r.detail.push_str(" Una marca temporale del file non è valida.");
    }
}

fn names(v: &[&Signer]) -> String {
    let n: Vec<String> = v.iter().map(|s| title_case(&s.name)).collect();
    match n.len() {
        0 => String::new(),
        1 => n[0].clone(),
        _ => format!("{} e {}", n[..n.len() - 1].join(", "), n[n.len() - 1]),
    }
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
