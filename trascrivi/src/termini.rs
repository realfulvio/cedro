//! La correzione dei termini tecnici dopo la trascrizione, quando è scelto un glossario.
//!
//! Whisper sbaglia i termini rari quasi sempre di poco: «mastocitooma», «vimblastina», «emangio sarcoma»,
//! «lagoaspirato». Ogni parola che non è nel dizionario e ci assomiglia molto (una o due lettere di differenza,
//! più per le parole lunghe) diventa il termine giusto, con la desinenza detta («mastocitomi» resta plurale).
//! Due o tre parole che attaccate fanno un termine si uniscono. Il dizionario è `termini.txt` (con il glossario
//! «veterinaria») più le parole scritte da chi usa Cedro; senza glossario il testo non si tocca. Le parole italiane
//! vere non si toccano mai, anche se somigliano a un termine («regressione» non diventa «progressione»).

use std::collections::{HashMap, HashSet};

/// Il dizionario dei termini di veterinaria, anatomia patologica e laboratorio.
const VETERINARIA: &str = include_str!("termini.txt");

/// Le parole italiane vere, da non toccare: quelle di Wikipedia in italiano (itwiki-2022-08-30.txt di
/// github.com/IlyaSemenov/wikipedia-word-frequency, licenza MIT) di almeno 6 lettere usate almeno 5 volte, circa
/// 650 mila, in un filtro di Bloom da 775 kB. Il filtro non sbaglia mai su una parola vera; su circa una parola
/// storpiata su cento dice «è vera» e quella resta com'è. Si rigenera col test `rigenera_parole` quando cambia
/// `termini.txt`.
const PAROLE: &[u8] = include_bytes!("parole.bloom");

/// Quanti bit del filtro guarda ogni parola.
const SONDE: u64 = 7;

/// Le parole più corte di così non si correggono: troppe parole normali sono a una lettera da un termine.
const MIN_LETTERE: usize = 6;

pub struct Termini {
    voci: Vec<Voce>,
    /// Le chiavi esatte (minuscole, senza accenti) dei termini: una parola già giusta non si tocca.
    esatte: HashSet<String>,
    /// Le radici (la chiave senza l'ultima vocale): «linfonodi» è giusto se c'è «linfonodo».
    radici: HashSet<String>,
    /// Il termine per chiave esatta, per le parole unite.
    per_chiave: HashMap<String, usize>,
}

struct Voce {
    /// Come si scrive: con accenti e maiuscole («Leptospira», «TRIzol»).
    forma: String,
    radice: Vec<char>,
    /// Quante volte c'è ogni lettera nella radice: scarta in fretta i termini troppo diversi.
    conti: [u8; 27],
}

impl Termini {
    /// Il dizionario per il glossario chiesto: «veterinaria» porta i termini pronti, le altre voci (separate da
    /// virgole) si aggiungono così come sono. Nessun glossario, nessuna correzione.
    pub fn per_glossario(glossario: Option<&str>) -> Option<Termini> {
        let g = glossario?;
        let mut parole = Vec::new();
        for voce in g.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if voce.eq_ignore_ascii_case("veterinaria") {
                parole.extend(VETERINARIA.lines().filter(|l| !l.trim_start().starts_with('#')).flat_map(parole_di));
            } else {
                parole.extend(parole_di(voce));
            }
        }
        let t = Termini::da_parole(parole);
        (!t.voci.is_empty()).then_some(t)
    }

    fn da_parole(parole: Vec<String>) -> Termini {
        let mut t = Termini { voci: Vec::new(), esatte: HashSet::new(), radici: HashSet::new(), per_chiave: HashMap::new() };
        for forma in parole {
            let k = chiave(&forma);
            if k.chars().count() < MIN_LETTERE || t.esatte.contains(&k) {
                continue;
            }
            let r = radice(&k);
            t.radici.insert(r.clone());
            t.per_chiave.insert(k.clone(), t.voci.len());
            t.esatte.insert(k);
            t.voci.push(Voce { forma, conti: conti(&r), radice: r.chars().collect() });
        }
        t
    }

    /// Il testo con i termini corretti; il resto (spazi, punteggiatura, maiuscole) com'era.
    pub fn correggi(&self, testo: &str) -> String {
        let pezzi: Vec<Pezzo> = testo.split(' ').map(Pezzo::da).collect();
        let mut out: Vec<String> = Vec::with_capacity(pezzi.len());
        let mut i = 0;
        'pezzi: while i < pezzi.len() {
            // Due o tre parole che attaccate fanno esattamente un termine: «emangio sarcoma», «ani socariosi».
            for n in [3usize, 2] {
                if let Some(unito) = self.unisci(&pezzi[i..pezzi.len().min(i + n)], n) {
                    out.push(unito);
                    i += n;
                    continue 'pezzi;
                }
            }
            out.push(self.parola(&pezzi[i]).unwrap_or_else(|| pezzi[i].testo.to_string()));
            i += 1;
        }
        out.join(" ")
    }

    fn unisci(&self, pezzi: &[Pezzo], n: usize) -> Option<String> {
        if pezzi.len() < n || pezzi.iter().any(|p| p.parola.is_empty() || p.ha_cifre()) {
            return None;
        }
        // Solo la prima può avere qualcosa davanti e solo l'ultima qualcosa dopo: «tumore, sarcoma» non si unisce.
        let (primo, ultimo) = (&pezzi[0], &pezzi[n - 1]);
        if pezzi.iter().enumerate().any(|(j, p)| (j > 0 && !p.prima.is_empty()) || (j + 1 < n && !p.dopo.is_empty()) || (j > 0 && !p.elisione.is_empty())) {
            return None;
        }
        let k: String = pezzi.iter().map(|p| chiave(p.parola)).collect();
        if k.chars().count() < 8 || pezzi.iter().all(|p| self.esatte.contains(&chiave(p.parola))) {
            return None;
        }
        let v = &self.voci[*self.per_chiave.get(&k)?];
        Some(format!("{}{}{}{}", primo.prima, primo.elisione, maiuscola_come(&v.forma, primo.parola), ultimo.dopo))
    }

    fn parola(&self, p: &Pezzo) -> Option<String> {
        if p.parola.is_empty() || p.ha_cifre() || sigla(p.parola) {
            return None;
        }
        let k = chiave(p.parola);
        let n = k.chars().count();
        if n < MIN_LETTERE || self.esatte.contains(&k) || self.radici.contains(&radice(&k)) || italiana(&k) {
            return None;
        }
        // «lagoaspirato»: l'articolo attaccato a un termine giusto.
        if p.elisione.is_empty() && n >= 8 && k.starts_with('l') {
            if let Some(&i) = self.per_chiave.get(&k[1..]) {
                let f = &self.voci[i].forma;
                if f.chars().next().is_some_and(|c| "aeiouAEIOU".contains(c)) {
                    let l = if p.parola.starts_with('L') { "L'" } else { "l'" };
                    return Some(format!("{}{l}{}{}", p.prima, f, p.dopo));
                }
            }
        }
        // «l'eptoaspira»: la «l» dell'articolo era l'inizio del termine, se così il termine è più vicino.
        let vicina = self.vicina(&k, p.parola);
        if matches!(p.elisione, "l'" | "L'" | "l’" | "L’") {
            let k2 = format!("l{k}");
            // «linfoma» è un termine: avanti; «legame» è una parola vera ma non un termine: resta com'è.
            if self.esatte.contains(&k2) || self.radici.contains(&radice(&k2)) || !italiana(&k2) {
                if let Some((d2, c)) = self.vicina(&k2, &format!("{}{}", &p.elisione[..1], p.parola)) {
                    if d2 <= 1 && vicina.as_ref().is_none_or(|(d, _)| d2 < *d) {
                        return Some(format!("{}{}{}", p.prima, c, p.dopo));
                    }
                }
            }
        }
        // Dopo un apostrofo ci va una vocale: «l'iscampione» non diventa «l'campione».
        let (_, c) = vicina?;
        if !p.elisione.is_empty() && !c.chars().next().is_some_and(|c| vocale(chiave(&c.to_string()).chars().next().unwrap_or('x'))) {
            return None;
        }
        Some(format!("{}{}{}{}", p.prima, p.elisione, c, p.dopo))
    }

    /// Il termine a una o due lettere di distanza, se è uno solo; con la desinenza della parola detta.
    fn vicina(&self, k: &str, detta: &str) -> Option<(usize, String)> {
        let rk = radice(k);
        let cr = conti(&rk);
        let r: Vec<char> = rk.chars().collect();
        let max = match r.len() {
            0..=8 => 1,
            9..=12 => 2,
            _ => 3,
        };
        let mut migliore: Option<(usize, usize)> = None;
        let mut pari = false;
        for (i, v) in self.voci.iter().enumerate() {
            // Ogni lettera cambiata sposta due conti, ogni lettera in più o in meno uno: sotto questo non si scende.
            let scarto: usize = cr.iter().zip(&v.conti).map(|(a, b)| usize::from(a.abs_diff(*b))).sum();
            if v.radice.len().abs_diff(r.len()) > max || scarto.div_ceil(2) > max {
                continue;
            }
            let Some(d) = distanza(&r, &v.radice, max) else { continue };
            match migliore {
                Some((bd, bi)) if d == bd && self.voci[bi].radice != v.radice => pari = true,
                Some((bd, _)) if d >= bd => {}
                _ => {
                    migliore = Some((d, i));
                    pari = false;
                }
            }
        }
        let (d, i) = migliore.filter(|_| !pari)?;
        let v = &self.voci[i];
        // La radice giusta con la desinenza detta: «mastocitoomi» → «mastocitomi».
        let finale = |s: &str| s.chars().last().filter(|c| chiave(&c.to_string()).chars().next().is_some_and(vocale));
        let forma = match (finale(detta).map(|c| c.to_lowercase().collect::<String>()), finale(&v.forma)) {
            (Some(f), Some(g)) if chiave(&f) != chiave(&g.to_string()) => {
                let mut s: String = v.forma.chars().take(v.forma.chars().count() - 1).collect();
                s.push_str(&f);
                s
            }
            _ => v.forma.clone(),
        };
        Some((d, maiuscola_come(&forma, detta)))
    }
}

/// Una parola del testo con quello che ha attaccato: «(l'agoaspirato,» = «(» + «l'» + «agoaspirato» + «,».
struct Pezzo<'a> {
    testo: &'a str,
    prima: &'a str,
    elisione: &'a str,
    parola: &'a str,
    dopo: &'a str,
}

impl<'a> Pezzo<'a> {
    fn da(testo: &'a str) -> Pezzo<'a> {
        let inizio = testo.find(|c: char| c.is_alphanumeric()).unwrap_or(testo.len());
        let fine = testo.rfind(|c: char| c.is_alphanumeric()).map_or(inizio, |i| i + testo[i..].chars().next().map_or(1, char::len_utf8));
        let (prima, centro, dopo) = (&testo[..inizio], &testo[inizio..fine.max(inizio)], &testo[fine.max(inizio)..]);
        // L'articolo o la preposizione con l'apostrofo resta com'è: si corregge quello che viene dopo.
        let (elisione, parola) = match centro.rfind(['\'', '’']) {
            Some(a) if centro[..a].chars().count() <= 5 => {
                let a = a + centro[a..].chars().next().map_or(1, char::len_utf8);
                (&centro[..a], &centro[a..])
            }
            _ => ("", centro),
        };
        Pezzo { testo, prima, elisione, parola, dopo }
    }

    fn ha_cifre(&self) -> bool {
        self.parola.chars().any(|c| c.is_ascii_digit())
    }
}

/// Le parole di una voce del dizionario: «ematossilina eosina» sono due, «May-Grunwald» anche.
fn parole_di(voce: &str) -> Vec<String> {
    voce.split(|c: char| c.is_whitespace() || c == '-' || c == '/' || c == '\'' || c == '’')
        .map(|p| p.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|p| !p.is_empty() && p.chars().all(char::is_alphabetic))
        .map(str::to_string)
        .collect()
}

/// Minuscola e senza accenti, per confrontare.
fn chiave(parola: &str) -> String {
    parola
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            c => c,
        })
        .filter(|c| c.is_alphabetic())
        .collect()
}

fn conti(k: &str) -> [u8; 27] {
    let mut c = [0u8; 27];
    for l in k.chars() {
        let i = if l.is_ascii_lowercase() { (l as u8 - b'a') as usize } else { 26 };
        c[i] = c[i].saturating_add(1);
    }
    c
}

fn vocale(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// La parola senza la vocale finale: la desinenza cambia col numero e col genere («emorragiche» ha la radice di
/// «emorragico», «emorragic»).
fn radice(k: &str) -> String {
    let mut s = k.to_string();
    if s.chars().count() > 4 && s.chars().last().is_some_and(vocale) {
        s.pop();
        if s.ends_with("ch") || s.ends_with("gh") {
            s.pop();
        }
    }
    s
}

/// La parola (chiave) è italiana vera: è nel filtro delle parole di Wikipedia.
fn italiana(k: &str) -> bool {
    let bit = (PAROLE.len() * 8) as u64;
    bit > 0 && sonde(k, bit).all(|b| PAROLE[(b / 8) as usize] & (1 << (b % 8)) != 0)
}

/// I bit del filtro per una parola: due hash (FNV-1a rimescolato) combinati, come d'uso per i filtri di Bloom.
fn sonde(k: &str, bit: u64) -> impl Iterator<Item = u64> {
    fn mescola(mut z: u64) -> u64 {
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    let fnv = k.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    let h1 = mescola(fnv);
    let h2 = mescola(h1 ^ 0x9e37_79b9_7f4a_7c15) | 1;
    (0..SONDE).map(move |i| h1.wrapping_add(i.wrapping_mul(h2)) % bit)
}

/// «PAS», «DMEM», «GAPDH»: le sigle non si correggono.
fn sigla(parola: &str) -> bool {
    let lettere: Vec<char> = parola.chars().filter(|c| c.is_alphabetic()).collect();
    lettere.len() >= 2 && lettere.iter().all(|c| c.is_uppercase())
}

/// Il termine con l'iniziale maiuscola se la parola detta l'aveva (inizio frase); i nomi propri e i marchi
/// («Leptospira», «TRIzol») restano come nel dizionario.
fn maiuscola_come(forma: &str, detta: &str) -> String {
    if forma.chars().any(char::is_uppercase) || !detta.chars().next().is_some_and(char::is_uppercase) {
        return forma.to_string();
    }
    let mut c = forma.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// La distanza tra due parole (lettere da cambiare, aggiungere, togliere o scambiare di posto), se non supera `max`.
fn distanza(a: &[char], b: &[char], max: usize) -> Option<usize> {
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > max {
        return None;
    }
    // Tre righe della tabella bastano: quella in corso, la precedente e quella prima ancora (per gli scambi).
    let mut due: Vec<usize> = vec![0; m + 1];
    let mut una: Vec<usize> = (0..=m).collect();
    let mut ora: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        ora[0] = i;
        let mut minimo = i;
        for j in 1..=m {
            let costo = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (una[j] + 1).min(ora[j - 1] + 1).min(una[j - 1] + costo);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(due[j - 2] + 1);
            }
            ora[j] = v;
            minimo = minimo.min(v);
        }
        if minimo > max {
            return None;
        }
        std::mem::swap(&mut due, &mut una);
        std::mem::swap(&mut una, &mut ora);
    }
    (una[m] <= max).then_some(una[m])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rigenera `parole.bloom` da un elenco di parole con la loro frequenza («parola conteggio» per riga, come
    /// itwiki-2022-08-30.txt di github.com/IlyaSemenov/wikipedia-word-frequency, licenza MIT):
    /// `CEDRO_PAROLE=itwiki.txt cargo test rigenera_parole -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn rigenera_parole() {
        let elenco = std::fs::read_to_string(std::env::var("CEDRO_PAROLE").expect("CEDRO_PAROLE")).expect("elenco");
        let t = vet();
        let mut viste = HashSet::new();
        let (mut parole, mut rare) = (Vec::new(), Vec::new());
        for riga in elenco.lines() {
            let mut c = riga.split_whitespace();
            let (Some(p), Some(n)) = (c.next(), c.next().and_then(|n| n.parse::<u64>().ok())) else { continue };
            let k = chiave(p);
            if k.chars().count() < MIN_LETTERE || p.chars().any(|c| c.is_uppercase()) || !viste.insert(k.clone()) {
                continue;
            }
            if n >= 5 { parole.push(k) } else { rare.push(k) }
        }
        // L'elenco toglie gli apostrofi: «lagoaspirato» è «l'agoaspirato», non una parola da proteggere.
        let termine = |r: &str| r.chars().count() >= 5 && (t.esatte.contains(r) || t.radici.contains(&radice(r)));
        let attaccata = |k: &str| ["l", "d", "dell", "nell", "dall", "all", "sull", "un", "quell"].iter().any(|a| k.strip_prefix(a).is_some_and(termine));
        parole.retain(|k| !attaccata(k));
        // Circa 9,6 bit per parola e 7 sonde: una parola su cento non inserita risulta «vera».
        let bit = ((parole.len() as u64 * 96).div_ceil(10)).div_ceil(8) * 8;
        let mut filtro = vec![0u8; (bit / 8) as usize];
        for k in &parole {
            for b in sonde(k, bit) {
                filtro[(b / 8) as usize] |= 1 << (b % 8);
            }
        }
        let falsi = rare.iter().filter(|k| sonde(k, bit).all(|b| filtro[(b / 8) as usize] & (1 << (b % 8)) != 0)).count();
        println!("{} parole, {} byte, falsi «veri» {:.2} % su {} parole rare", parole.len(), filtro.len(), 100.0 * falsi as f64 / rare.len().max(1) as f64, rare.len());
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/src/parole.bloom"), filtro).expect("scrivo");
    }

    fn vet() -> Termini {
        Termini::per_glossario(Some("veterinaria")).expect("dizionario")
    }

    #[test]
    fn i_termini_detti_male_tornano_giusti() {
        let t = vet();
        assert_eq!(t.correggi("La diagnosi è mastocitooma cutaneo."), "La diagnosi è mastocitoma cutaneo.");
        assert_eq!(t.correggi("Si somministra vimblastina e prelnisolone."), "Si somministra vinblastina e prednisolone.");
        assert_eq!(t.correggi("Un carcinoma mammario tubulopapliare di grado due"), "Un carcinoma mammario tubulopapillare di grado due");
        assert_eq!(t.correggi("La colorazione immunoistochiimica per Ki-67"), "La colorazione immunoistochimica per Ki-67");
    }

    #[test]
    fn la_desinenza_detta_resta() {
        let t = vet();
        assert_eq!(t.correggi("due mastocitoomi"), "due mastocitomi");
        // Una parola giusta al plurale non si tocca.
        assert_eq!(t.correggi("i linfonodi regionali"), "i linfonodi regionali");
    }

    #[test]
    fn le_parole_spezzate_si_uniscono() {
        let t = vet();
        assert_eq!(t.correggi("un emangio sarcoma splenico"), "un emangiosarcoma splenico");
        assert_eq!(t.correggi("con ani socariosi marcata."), "con anisocariosi marcata.");
        assert_eq!(t.correggi("Pio torace."), "Piotorace.");
        // Con la punteggiatura in mezzo restano separate.
        assert_eq!(t.correggi("emangio, sarcoma"), "emangio, sarcoma");
    }

    #[test]
    fn l_articolo_attaccato_si_stacca() {
        let t = vet();
        assert_eq!(t.correggi("Lagoaspirato del linfonodo"), "L'agoaspirato del linfonodo");
        assert_eq!(t.correggi("dell'agoaspirato"), "dell'agoaspirato");
    }

    #[test]
    fn la_l_dell_articolo_torna_al_termine() {
        assert_eq!(vet().correggi("la l'eptoaspira"), "la Leptospira");
        assert_eq!(vet().correggi("è un l'infoma"), "è un linfoma");
        assert_eq!(vet().correggi("l'eptospira"), "Leptospira");
    }

    #[test]
    fn i_plurali_in_che_e_chi_sono_giusti() {
        assert_eq!(vet().correggi("aree emorragiche"), "aree emorragiche");
    }

    #[test]
    fn sigle_numeri_e_parole_corte_restano() {
        let t = vet();
        assert_eq!(t.correggi("PAS e DMEM al 10%, Ki-67 22"), "PAS e DMEM al 10%, Ki-67 22");
        assert_eq!(t.correggi("le ossa sono moli"), "le ossa sono moli");
    }

    #[test]
    fn senza_glossario_niente_correzione() {
        assert!(Termini::per_glossario(None).is_none());
        assert!(Termini::per_glossario(Some("  ")).is_none());
    }

    #[test]
    fn le_parole_scritte_da_chi_usa_cedro_valgono() {
        let t = Termini::per_glossario(Some("Lipofectamine, ossimetria")).expect("dizionario");
        assert_eq!(t.correggi("con la lipofettamine"), "con la Lipofectamine");
        // Il dizionario veterinario non c'è: «mastocitooma» resta.
        assert_eq!(t.correggi("mastocitooma"), "mastocitooma");
        // Una parola vera vicina a una parola scritta resta com'è: «accordi» non è «Accorsi».
        let t = Termini::per_glossario(Some("Accorsi, Bondeno")).expect("dizionario");
        assert_eq!(t.correggi("gli accordi presi"), "gli accordi presi");
        assert_eq!(t.correggi("dottor Accorzi"), "dottor Accorsi");
    }

    #[test]
    fn le_parole_vere_non_si_toccano() {
        let t = vet();
        for p in ["regressione", "asportato", "formazione", "valutazione", "l'influenza", "l'esenzione"] {
            assert_eq!(t.correggi(p), p);
        }
    }

    #[test]
    fn la_distanza_conta_scambi_e_lettere_in_piu() {
        let c = |s: &str| s.chars().collect::<Vec<_>>();
        assert_eq!(distanza(&c("vimblastina"), &c("vinblastina"), 2), Some(1));
        assert_eq!(distanza(&c("tubulopapliare"), &c("tubulopapillare"), 3), Some(2));
        assert_eq!(distanza(&c("abc"), &c("xyz"), 1), None);
    }
}
