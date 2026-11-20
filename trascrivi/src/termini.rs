//! La correzione dei termini tecnici dopo la trascrizione, quando è scelto un glossario.
//!
//! Whisper sbaglia i termini rari quasi sempre di poco: «mastocitooma», «vimblastina», «emangio sarcoma»,
//! «lagoaspirato». Ogni parola che non è nel dizionario e ci assomiglia molto (una o due lettere di differenza,
//! più per le parole lunghe) diventa il termine giusto, con la desinenza detta («mastocitomi» resta plurale).
//! Due o tre parole che attaccate fanno un termine si uniscono, anche quando la prima è una preposizione che si è
//! presa l'inizio del termine («con angiocarcinoma», «di fenidamina»); due termini detti attaccati si staccano
//! («emangiosarcomasplenico», «toxocaracati»). Gli articoli che si sono presi una lettera del termine la rendono
//! («una denoma» → «un adenoma», «dell'alipasi» → «della lipasi», «dell'infonodo» → «del linfonodo»); i nomi propri
//! dopo «test di» o «secondo» si correggono anche corti («COMS» → «Coombs»). Il dizionario è `termini.txt` (con il
//! glossario «veterinaria») più le parole scritte da chi usa Cedro; senza glossario il testo non si tocca. Le parole
//! italiane vere non si toccano mai, anche se somigliano a un termine («regressione» non diventa «progressione»).

use std::collections::{HashMap, HashSet};

/// Il dizionario dei termini di veterinaria, anatomia patologica e laboratorio.
const VETERINARIA: &str = include_str!("termini.txt");

/// Le parole italiane vere, da non toccare: quelle di Wikipedia in italiano (itwiki-2022-08-30.txt di
/// github.com/IlyaSemenov/wikipedia-word-frequency, licenza MIT) di almeno 6 lettere usate almeno 5 volte, più
/// quelle di 4 e 5 lettere usate almeno 1000 volte (le più rare, corte, sono quasi tutte nomi e sigle), in un filtro
/// di Bloom da 1,3 MB. Il filtro non sbaglia mai su una parola vera; su circa una parola storpiata su duemila dice
/// «è vera» e quella resta com'è. Si rigenera col test `rigenera_parole` quando cambia `termini.txt`.
const PAROLE: &[u8] = include_bytes!("parole.bloom");

/// Quanti bit del filtro guarda ogni parola.
const SONDE: u64 = 11;

/// Le parole più corte di così non si correggono: troppe parole normali sono a una lettera da un termine.
const MIN_LETTERE: usize = 6;

/// Le parole corte del filtro: sotto non ce ne sono.
const MIN_CORTE: usize = 4;

/// Gli esami che seguono «Referto» in testa a un referto: «Effetto citologico» è «Referto citologico».
const ESAMI: &[&str] = &[
    "istologico", "citologico", "ematologico", "parassitologico", "ecografico", "necroscopico", "cardiologico",
    "radiologico", "radiografico", "microbiologico", "batteriologico", "sierologico", "endoscopico",
    "ecocardiografico", "elettrocardiografico", "tomografico", "coprologico", "dermatologico", "oculistico",
    "anatomopatologico", "immunoistochimico", "citofluorimetrico", "virologico", "istopatologico", "citopatologico",
];

/// Le parole vere che whisper scrive al posto di «Referto»; le altre devono essere storpiate.
const COME_REFERTO: &[&str] = &["effetto", "perfetto", "affetto"];

/// Le parole che annunciano un nome proprio dopo «di»/«del»: «test di Coombs», «colorazione di Giemsa».
const ANCORE: &[&str] = &[
    "test", "prova", "colorazione", "metodo", "tecnica", "reazione", "segno", "malattia", "sindrome", "morbo",
    "classificazione", "criteri", "criterio", "scala",
];

/// Parole vere che dopo certe parole sono quasi sempre un termine detto male: «esame culturale» è «esame colturale».
/// (parole prima, radice sbagliata, radice giusta).
const CONFUSE: &[(&[&str], &str, &str)] = &[(&["esame", "esami"], "cultural", "coltural")];

pub struct Termini {
    voci: Vec<Voce>,
    /// Le chiavi esatte (minuscole, senza accenti) dei termini: una parola già giusta non si tocca.
    esatte: HashSet<String>,
    /// Le radici (la chiave senza l'ultima vocale): «linfonodi» è giusto se c'è «linfonodo».
    radici: HashSet<String>,
    /// Il termine per chiave esatta, per le parole unite.
    per_chiave: HashMap<String, usize>,
    /// Il termine per radice: «adenomi» ha la radice di «adenoma».
    per_radice: HashMap<String, usize>,
    /// Le voci di più parole per chiave tutta attaccata: «toxocaracati» è «Toxocara cati».
    frasi: HashMap<String, String>,
    /// I termini di 4 e 5 lettere per radice («otite», «cisti»): troppo corti per correggerli da soli, servono
    /// quando l'articolo si è preso la prima lettera («una tita» → «un'otite»).
    corte: HashMap<String, String>,
    /// I nomi propri del dizionario («Coombs», «Patnaik»), per chiave: si correggono dopo «test di» e «secondo».
    nomi: Vec<(Vec<char>, String)>,
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
        let mut voci = Vec::new();
        for voce in g.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if voce.eq_ignore_ascii_case("veterinaria") {
                voci.extend(VETERINARIA.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')));
            } else {
                voci.push(voce);
            }
        }
        let t = Termini::da_voci(&voci);
        (!t.voci.is_empty()).then_some(t)
    }

    fn da_voci(voci: &[&str]) -> Termini {
        let mut t = Termini {
            voci: Vec::new(),
            esatte: HashSet::new(),
            radici: HashSet::new(),
            per_chiave: HashMap::new(),
            per_radice: HashMap::new(),
            frasi: HashMap::new(),
            corte: HashMap::new(),
            nomi: Vec::new(),
        };
        for voce in voci {
            let parole = parole_di(voce);
            if parole.len() > 1 {
                let k: String = parole.iter().map(|p| chiave(p)).collect();
                if k.chars().count() >= 8 {
                    t.frasi.entry(k).or_insert_with(|| voce.to_string());
                }
            }
            for forma in parole {
                let k = chiave(&forma);
                let n = k.chars().count();
                if n >= MIN_CORTE && forma.starts_with(char::is_uppercase) && !sigla(&forma) && !t.nomi.iter().any(|(_, f)| *f == forma) {
                    t.nomi.push((k.chars().collect(), forma.clone()));
                }
                if n < MIN_LETTERE {
                    if n >= MIN_CORTE {
                        t.corte.entry(radice(&k)).or_insert(forma);
                    }
                    continue;
                }
                if t.esatte.contains(&k) {
                    continue;
                }
                let r = radice(&k);
                t.radici.insert(r.clone());
                t.per_radice.entry(r.clone()).or_insert(t.voci.len());
                t.per_chiave.insert(k.clone(), t.voci.len());
                t.esatte.insert(k);
                t.voci.push(Voce { forma, conti: conti(&r), radice: r.chars().collect() });
            }
        }
        t
    }

    /// Il testo con i termini corretti; il resto (spazi, punteggiatura, maiuscole) com'era.
    pub fn correggi(&self, testo: &str) -> String {
        let pezzi: Vec<Pezzo> = testo.split(' ').map(Pezzo::da).collect();
        let mut out: Vec<String> = Vec::with_capacity(pezzi.len());
        let mut i = 0;
        'pezzi: while i < pezzi.len() {
            if let Some((fatto, n)) = self.referto(&pezzi, i).or_else(|| self.articolo(&pezzi, i)) {
                out.push(fatto);
                i += n;
                continue;
            }
            if let Some(fatto) = self.nome(&pezzi, i).or_else(|| self.confusa(&pezzi, i)) {
                out.push(fatto);
                i += 1;
                continue;
            }
            // Due o tre parole che attaccate fanno un termine: «emangio sarcoma», «ani socariosi».
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

    /// La chiave è un termine del dizionario (anche con un'altra desinenza).
    fn termine(&self, k: &str) -> bool {
        self.esatte.contains(k) || self.radici.contains(&radice(k))
    }

    /// «Effetto citologico», «Per effetto istologico»: «Referto» detto male in testa a un referto. Solo col
    /// dizionario veterinario, solo a inizio frase e solo davanti al nome di un esame.
    fn referto(&self, pezzi: &[Pezzo], i: usize) -> Option<(String, usize)> {
        if !self.esatte.contains("referto") || !inizio_frase(pezzi, i) {
            return None;
        }
        // Whisper a volte lo spezza in due: «Per effetto».
        let salta = usize::from(chiave(pezzi[i].parola) == "per" && pezzi[i].dopo.is_empty() && pezzi.get(i + 1).is_some_and(|p| chiave(p.parola) == "effetto"));
        let p = pezzi.get(i + salta)?;
        let k = chiave(p.parola);
        if k == "referto" || !p.dopo.is_empty() || !p.elisione.is_empty() || !(6..=9).contains(&k.chars().count()) {
            return None;
        }
        if italiana(&k) && !COME_REFERTO.contains(&k.as_str()) {
            return None;
        }
        let r: Vec<char> = "referto".chars().collect();
        distanza(&k.chars().collect::<Vec<_>>(), &r, 3)?;
        let e = pezzi.get(i + salta + 1)?;
        let ke = chiave(e.parola);
        let esame = (e.prima.is_empty() && ESAMI.contains(&ke.as_str()))
            // «Referto clinico, cane…»: «effetto clinico» da solo è italiano, con la virgola dopo no.
            || (ke == "clinico" && e.dopo.starts_with(','))
            || (ke == "di" && e.dopo.is_empty() && pezzi.get(i + salta + 2).is_some_and(|l| chiave(l.parola) == "laboratorio"));
        if !esame {
            return None;
        }
        Some((format!("{}{}", pezzi[i].prima, maiuscola_come("referto", pezzi[i].parola)), salta + 1))
    }

    /// «una denoma», «uno tite»: l'articolo si è preso la prima vocale del termine.
    fn articolo(&self, pezzi: &[Pezzo], i: usize) -> Option<(String, usize)> {
        let (a, w) = (&pezzi[i], pezzi.get(i + 1)?);
        let v = match chiave(a.parola).as_str() {
            "una" => 'a',
            "uno" => 'o',
            _ => return None,
        };
        if !a.dopo.is_empty() || !a.elisione.is_empty() || !w.prima.is_empty() || !w.elisione.is_empty() || w.ha_cifre() || sigla(w.parola) {
            return None;
        }
        let k = chiave(w.parola);
        if k.chars().count() < 3 || k.starts_with(vocale) || self.termine(&k) || self.corte.contains_key(&radice(&k)) || italiana(&k) {
            return None;
        }
        // Prima la vocale dell'articolo, poi le altre: «una tita» era «un'otite».
        let esatta = std::iter::once(v).chain("aeiou".chars().filter(|&c| c != v)).find_map(|c| {
            let r = radice(&format!("{c}{k}"));
            self.per_radice.get(&r).map(|&j| self.voci[j].forma.clone()).or_else(|| self.corte.get(&r).cloned())
        });
        let forma = match esatta {
            Some(f) => f,
            None if k.chars().count() >= MIN_LETTERE - 1 => self.vicina(&format!("{v}{k}"), &format!("{v}{}", w.parola)).filter(|(d, _)| *d <= 1)?.1,
            None => return None,
        };
        let kf = chiave(&forma);
        let maschile = kf.ends_with('o') || ["ma", "ore", "ale", "ace"].iter().any(|s| kf.ends_with(s));
        let un = maiuscola_come("un", a.parola);
        Some((format!("{}{un}{}{forma}{}", a.prima, if maschile { " " } else { "'" }, w.dopo), 2))
    }

    /// «test di COMS», «secondo Patnek»: un nome proprio annunciato, corretto anche se corto.
    fn nome(&self, pezzi: &[Pezzo], i: usize) -> Option<String> {
        let p = &pezzi[i];
        if i == 0 || !p.prima.is_empty() || !p.elisione.is_empty() || p.ha_cifre() || !p.parola.starts_with(char::is_uppercase) || !pezzi[i - 1].dopo.is_empty() {
            return None;
        }
        let kp = chiave(pezzi[i - 1].parola);
        let dopo_test = matches!(kp.as_str(), "di" | "del") && i >= 2 && pezzi[i - 2].dopo.is_empty() && ANCORE.contains(&chiave(pezzi[i - 2].parola).as_str());
        // Dopo «secondo» una sigla resta sigla («secondo IRIS»); dopo «test di» è il nome detto in fretta.
        if !(dopo_test || (kp == "secondo" && !sigla(p.parola))) {
            return None;
        }
        let k: Vec<char> = chiave(p.parola).chars().collect();
        if k.len() < 3 || self.nomi.iter().any(|(c, _)| *c == k) || italiana(&k.iter().collect::<String>()) {
            return None;
        }
        let max = if k.len() <= 3 { 1 } else { 2 };
        let mut migliore: Option<(usize, &str)> = None;
        let mut pari = false;
        for (c, f) in &self.nomi {
            // Una sigla detta per un nome è più corta, mai più lunga: «COMS» è «Coombs», non «Tris».
            if sigla(p.parola) && c.len() <= k.len() {
                continue;
            }
            let Some(d) = distanza(&k, c, max) else { continue };
            match migliore {
                Some((bd, _)) if d == bd => pari = true,
                Some((bd, _)) if d > bd => {}
                _ => {
                    migliore = Some((d, f));
                    pari = false;
                }
            }
        }
        let (_, f) = migliore.filter(|_| !pari)?;
        Some(format!("{}{}{}", p.prima, f, p.dopo))
    }

    /// «esame culturale»: una parola vera che lì è un termine detto male.
    fn confusa(&self, pezzi: &[Pezzo], i: usize) -> Option<String> {
        let p = &pezzi[i];
        if i == 0 || !pezzi[i - 1].dopo.is_empty() || !p.prima.is_empty() || !p.elisione.is_empty() {
            return None;
        }
        let (k, kp) = (chiave(p.parola), chiave(pezzi[i - 1].parola));
        CONFUSE.iter().find_map(|(prima, sbagliata, giusta)| {
            if !prima.contains(&kp.as_str()) || radice(&k) != *sbagliata || !self.radici.contains(*giusta) || p.parola.chars().count() != k.chars().count() {
                return None;
            }
            let desinenza: String = p.parola.chars().skip(sbagliata.chars().count()).collect();
            Some(format!("{}{giusta}{desinenza}{}", p.prima, p.dopo))
        })
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
        let chiavi: Vec<String> = pezzi.iter().map(|p| chiave(p.parola)).collect();
        let k: String = chiavi.concat();
        if k.chars().count() < 8 {
            return None;
        }
        let intero = |f: &str| format!("{}{}{}{}", primo.prima, primo.elisione, maiuscola_come(f, primo.parola), ultimo.dopo);
        // Attaccate fanno un termine («tubulo papillare») o una voce di più parole dette male («toxocara cati» no:
        // quella è già giusta, ma «Toxo caracati» sì).
        if let Some(&j) = self.per_chiave.get(&k) {
            return Some(intero(&self.voci[j].forma));
        }
        if let Some(v) = self.frasi.get(&k).filter(|v| parole_di(v).len() > n) {
            return Some(intero(v));
        }
        if n != 2 {
            return None;
        }
        // «lago aspirato»: l'articolo attaccato a un termine giusto.
        if primo.elisione.is_empty() && k.starts_with('l') {
            if let Some(f) = self.per_chiave.get(&k[1..]).map(|&j| &self.voci[j].forma).filter(|f| f.starts_with(|c: char| "aeiouAEIOU".contains(c))) {
                let l = if primo.parola.starts_with('L') { "L'" } else { "l'" };
                return Some(format!("{}{l}{f}{}", primo.prima, ultimo.dopo));
            }
        }
        // «dell'info modo»: la «l» dell'articolo e le due parole sono un termine solo.
        if let Some(base) = senza_l(primo.elisione) {
            let k2 = format!("l{k}");
            if !italiana(&k2) && !self.termine(&k) {
                let detta = format!("{}{}{}", if base.is_empty() { &primo.elisione[..1] } else { "l" }, primo.parola, ultimo.parola);
                if let Some((_, c)) = self.vicina(&k2, &detta).filter(|(d, _)| *d <= 1) {
                    return Some(format!("{}{base}{c}{}", primo.prima, ultimo.dopo));
                }
            }
        }
        if !primo.elisione.is_empty() {
            return None;
        }
        // «emangio sarcomasplenico»: due termini divisi nel punto sbagliato.
        let originale: String = pezzi.iter().map(|p| p.parola).collect();
        if let Some(diviso) = self.dividi(&originale, chiavi[0].chars().count()) {
            return Some(format!("{}{diviso}{}", primo.prima, ultimo.dopo));
        }
        // «con angiocarcinoma», «di fenidamina», «per ecogeno»: una parolina che era l'inizio del termine. Solo se
        // nessuna delle due è un termine, almeno una non è italiana e attaccate sono più vicine a un termine di
        // quanto lo sia la seconda da sola.
        let (k0, k1) = (&chiavi[0], &chiavi[1]);
        let strana = |k: &str| k.chars().count() >= MIN_CORTE && !italiana(k);
        // «e modepsid» è «e emodepside»: una lettera sola è una parola a sé.
        if !(2..=4).contains(&k0.chars().count()) || k1.chars().count() < 5 || self.termine(k0) || self.termine(k1) || italiana(&k) || !(strana(k0) || strana(k1)) {
            return None;
        }
        // Una lettera di differenza al massimo: «un continenta» non è «incontinenza».
        let (d, c) = self.vicina(&k, &originale).filter(|(d, _)| *d <= 1)?;
        if self.vicina(k1, ultimo.parola).is_some_and(|(d1, _)| d1 <= d) {
            return None;
        }
        Some(format!("{}{}{}", primo.prima, maiuscola_come(&c, primo.parola), ultimo.dopo))
    }

    /// Una parola che è due termini attaccati («emangiosarcomasplenico»), divisa dove serve; `confine` è dove era
    /// già divisa (0 se era una parola sola): lì non conta.
    fn dividi(&self, parola: &str, confine: usize) -> Option<String> {
        let k = chiave(parola);
        let c: Vec<char> = parola.chars().collect();
        let kc: Vec<char> = k.chars().collect();
        if kc.len() != c.len() || kc.len() < 2 * MIN_LETTERE || self.termine(&k) || italiana(&k) {
            return None;
        }
        let j = (MIN_LETTERE..=kc.len() - MIN_LETTERE).rev().find(|&j| {
            j != confine && self.termine(&kc[..j].iter().collect::<String>()) && self.termine(&kc[j..].iter().collect::<String>())
        })?;
        Some(format!("{} {}", c[..j].iter().collect::<String>(), c[j..].iter().collect::<String>()))
    }

    fn parola(&self, p: &Pezzo) -> Option<String> {
        if p.parola.is_empty() || p.ha_cifre() || sigla(p.parola) {
            return None;
        }
        let k = chiave(p.parola);
        let n = k.chars().count();
        let intera = |s: &str| format!("{}{}{}{}", p.prima, p.elisione, s, p.dopo);
        // «tubulo-papillare»: un termine di una parola scritto col trattino.
        if p.parola.contains('-') {
            if let Some(f) = self.per_chiave.get(&k).map(|&j| &self.voci[j].forma).filter(|f| !f.contains('-')) {
                return Some(intera(&maiuscola_come(f, p.parola)));
            }
        }
        // «prednisolona», «coleciste»: la radice è giusta ma quella desinenza quel termine non ce l'ha.
        if n >= MIN_LETTERE && !self.esatte.contains(&k) && !italiana(&k) {
            if let Some(f) = self.per_radice.get(&radice(&k)).map(|&j| &self.voci[j].forma) {
                if desinenza_sbagliata(f, p.parola) {
                    return Some(intera(&maiuscola_come(f, p.parola)));
                }
            }
        }
        if n < MIN_LETTERE || self.termine(&k) || italiana(&k) {
            return None;
        }
        // «toxocaracati»: una voce di più parole detta tutta attaccata.
        if let Some(v) = self.frasi.get(&k) {
            return Some(intera(&maiuscola_come(v, p.parola)));
        }
        // «emangiosarcomasplenico»: due termini attaccati.
        if let Some(d) = self.dividi(p.parola, 0) {
            return Some(intera(&d));
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
        // «dell'alipasi», «l'acitometria»: la «a» era dell'articolo. Non per i nomi («dell'Amassone» non è
        // «della Masson»).
        if let (Some(base), Some(resto), false) = (con_a(p.elisione), k.strip_prefix('a'), p.parola.starts_with(char::is_uppercase)) {
            let comune = |f: &String| f.starts_with(char::is_lowercase);
            let r = radice(resto);
            if resto.chars().count() >= MIN_CORTE && (self.per_radice.get(&r).is_some_and(|&j| comune(&self.voci[j].forma)) || self.corte.get(&r).is_some_and(comune)) {
                let r: String = p.parola.chars().skip(1).collect();
                return Some(format!("{}{base}{r}{}", p.prima, p.dopo));
            }
        }
        // «l'eptoaspira», «dell'infonodo»: la «l» dell'articolo era l'inizio del termine, se così il termine è più
        // vicino.
        let vicina = self.vicina(&k, p.parola);
        if let Some(base) = senza_l(p.elisione) {
            let k2 = format!("l{k}");
            // «linfoma» è un termine: avanti; «legame» è una parola vera ma non un termine: resta com'è.
            if self.termine(&k2) || !italiana(&k2) {
                let detta = format!("{}{}", if base.is_empty() { &p.elisione[..1] } else { "l" }, p.parola);
                if let Some((d2, c)) = self.vicina(&k2, &detta) {
                    if d2 <= 1 && vicina.as_ref().is_none_or(|(d, _)| d2 < *d) {
                        return Some(format!("{}{base}{c}{}", p.prima, p.dopo));
                    }
                }
            }
        }
        // Dopo un apostrofo ci va una vocale: «l'iscampione» non diventa «l'campione».
        let (_, c) = vicina?;
        if !p.elisione.is_empty() && !c.chars().next().is_some_and(|c| vocale(chiave(&c.to_string()).chars().next().unwrap_or('x'))) {
            return None;
        }
        Some(intera(&c))
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
            (Some(f), Some(g)) if chiave(&f) != chiave(&g.to_string()) && !desinenza_sbagliata(&v.forma, detta) => {
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

/// La desinenza detta non esiste per quel termine: le parole in «-si» e «-sti» non cambiano («necrosi», «cisti»),
/// quelle in «-one» fanno solo «-oni» («prednisolone», mai «prednisolona»).
fn desinenza_sbagliata(forma: &str, detta: &str) -> bool {
    let (f, d) = (chiave(forma), chiave(detta));
    let Some(fine) = d.chars().last().filter(|&c| vocale(c)) else { return false };
    if f.chars().last() == Some(fine) {
        return false;
    }
    ((f.ends_with("si") || f.ends_with("sti")) && f.chars().count() >= 5) || (f.ends_with("one") && matches!(fine, 'a' | 'o'))
}

/// Il pezzo apre una frase: è il primo o quello prima finisce con un punto.
fn inizio_frase(pezzi: &[Pezzo], i: usize) -> bool {
    i == 0 || pezzi[i - 1].dopo.ends_with(['.', '!', '?', ':', ';'])
}

/// L'articolo con l'apostrofo senza la sua «l», quando la «l» era del termine: «dell'infonodo» → «del linfonodo»,
/// «l'eptospira» → «Leptospira».
fn senza_l(elisione: &str) -> Option<String> {
    let base = match chiave(elisione).as_str() {
        "l" => "",
        "dell" => "del ",
        "nell" => "nel ",
        "dall" => "dal ",
        "all" => "al ",
        "sull" => "sul ",
        _ => return None,
    };
    Some(maiuscola_come(base, elisione))
}

/// L'articolo intero quando la «a» dopo l'apostrofo era sua: «dell'alipasi» → «della lipasi».
fn con_a(elisione: &str) -> Option<String> {
    let base = match chiave(elisione).as_str() {
        "l" => "la ",
        "dell" => "della ",
        "nell" => "nella ",
        "dall" => "dalla ",
        "all" => "alla ",
        "sull" => "sulla ",
        "un" => "una ",
        _ => return None,
    };
    Some(maiuscola_come(base, elisione))
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
            let lettere = k.chars().count();
            if lettere < MIN_CORTE || p.chars().any(|c| c.is_uppercase()) || !viste.insert(k.clone()) {
                continue;
            }
            // Le parole corte rare sono quasi tutte nomi e sigle («tita», «agbo»): contano solo quelle comuni.
            if lettere < MIN_LETTERE {
                if n >= 1000 {
                    parole.push(k);
                }
                continue;
            }
            if n >= 5 { parole.push(k) } else { rare.push(k) }
        }
        // L'elenco toglie gli apostrofi: «lagoaspirato» è «l'agoaspirato», non una parola da proteggere.
        let termine = |r: &str| r.chars().count() >= 5 && t.termine(r);
        let attaccata = |k: &str| ["l", "d", "dell", "nell", "dall", "all", "sull", "un", "quell"].iter().any(|a| k.strip_prefix(a).is_some_and(termine));
        parole.retain(|k| !attaccata(k));
        // 16 bit per parola e 11 sonde: una parola su duemila non inserita risulta «vera».
        let bit = (parole.len() as u64 * 16).div_ceil(8) * 8;
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
        // Anche quando i pezzi sono termini anche da soli.
        assert_eq!(t.correggi("un carcinoma tubulo papillare"), "un carcinoma tubulopapillare");
        assert_eq!(t.correggi("Lago aspirato del linfonodo"), "L'agoaspirato del linfonodo");
        // Con la punteggiatura in mezzo restano separate.
        assert_eq!(t.correggi("emangio, sarcoma"), "emangio, sarcoma");
    }

    #[test]
    fn i_termini_attaccati_si_staccano() {
        let t = vet();
        assert_eq!(t.correggi("uova di toxocaracati."), "uova di Toxocara cati.");
        assert_eq!(t.correggi("un emangiosarcomasplenico"), "un emangiosarcoma splenico");
        assert_eq!(t.correggi("un emangio sarcomasplenico"), "un emangiosarcoma splenico");
        // Due termini giusti restano come sono.
        assert_eq!(t.correggi("emangiosarcoma splenico"), "emangiosarcoma splenico");
    }

    #[test]
    fn la_parolina_davanti_era_l_inizio_del_termine() {
        let t = vet();
        assert_eq!(t.correggi("La diagnosi è con angiocarcinoma."), "La diagnosi è colangiocarcinoma.");
        assert_eq!(t.correggi("si somministra di fenidamina"), "si somministra difenidramina");
        assert_eq!(t.correggi("il fegato è per ecogeno"), "il fegato è iperecogeno");
        // Una preposizione davanti a un termine o a una parola vera resta.
        assert_eq!(t.correggi("compatibile con linfoma"), "compatibile con linfoma");
        assert_eq!(t.correggi("con ecografia addominale"), "con ecografia addominale");
        // Una parola vera detta con la desinenza sbagliata non è un pezzo di termine.
        assert_eq!(t.correggi("un continenta"), "un continenta");
        assert_eq!(t.correggi("toltrazuril e modepsid."), "toltrazuril e emodepside.");
    }

    #[test]
    fn l_articolo_rende_la_lettera_presa() {
        let t = vet();
        assert_eq!(t.correggi("è presente una denoma"), "è presente un adenoma");
        assert_eq!(t.correggi("compatibile con una tita esterna"), "compatibile con un'otite esterna");
        assert_eq!(t.correggi("Uno tite."), "Un'otite.");
        assert_eq!(t.correggi("il valore dell'alipasi"), "il valore della lipasi");
        assert_eq!(t.correggi("con l'acitometria a flusso"), "con la citometria a flusso");
        // Le parole vere e i termini giusti restano.
        assert_eq!(t.correggi("una cisti e una mina"), "una cisti e una mina");
        assert_eq!(t.correggi("dell'Amassone"), "dell'Amassone");
        assert_eq!(t.correggi("dell'agoaspirato"), "dell'agoaspirato");
    }

    #[test]
    fn l_articolo_attaccato_si_stacca() {
        let t = vet();
        assert_eq!(t.correggi("Lagoaspirato del linfonodo"), "L'agoaspirato del linfonodo");
        assert_eq!(t.correggi("dell'agoaspirato"), "dell'agoaspirato");
    }

    #[test]
    fn la_l_dell_articolo_torna_al_termine() {
        let t = vet();
        assert_eq!(t.correggi("la l'eptoaspira"), "la Leptospira");
        assert_eq!(t.correggi("è un l'infoma"), "è un linfoma");
        assert_eq!(t.correggi("l'eptospira"), "Leptospira");
        assert_eq!(t.correggi("l'agoaspirato dell'infonodo"), "l'agoaspirato del linfonodo");
        assert_eq!(t.correggi("l'agoaspirato dell'info modo"), "l'agoaspirato del linfonodo");
    }

    #[test]
    fn i_nomi_annunciati_si_correggono() {
        let t = vet();
        assert_eq!(t.correggi("Il test di COMS diretto è positivo."), "Il test di Coombs diretto è positivo.");
        assert_eq!(t.correggi("grado due secondo Patnek."), "grado due secondo Patnaik.");
        // Senza l'annuncio, o se è una sigla dopo «secondo» o un nome vero, restano.
        assert_eq!(t.correggi("il COMS diretto"), "il COMS diretto");
        assert_eq!(t.correggi("stadio due secondo IRIS"), "stadio due secondo IRIS");
        assert_eq!(t.correggi("secondo Marco"), "secondo Marco");
    }

    #[test]
    fn referto_detto_male_in_testa() {
        let t = vet();
        assert_eq!(t.correggi("Effetto citologico, cane di nove anni."), "Referto citologico, cane di nove anni.");
        assert_eq!(t.correggi("Per effetto istologico, gatto."), "Referto istologico, gatto.");
        assert_eq!(t.correggi("senza atipie. Efferto ematologico, cane"), "senza atipie. Referto ematologico, cane");
        assert_eq!(t.correggi("Effetto clinico, cane bovaro"), "Referto clinico, cane bovaro");
        assert_eq!(t.correggi("è riservata. Perfetto ematologico, cane"), "è riservata. Referto ematologico, cane");
        assert_eq!(t.correggi("Afferto citologico gatto"), "Referto citologico gatto");
        // L'italiano vero resta.
        assert_eq!(t.correggi("Effetto clinico atteso"), "Effetto clinico atteso");
        assert_eq!(t.correggi("Reperto ecografico normale"), "Reperto ecografico normale");
        assert_eq!(t.correggi("Sospetto ecografico di massa"), "Sospetto ecografico di massa");
        assert_eq!(t.correggi("con un effetto istologico"), "con un effetto istologico");
        // Senza il dizionario veterinario niente.
        let altro = Termini::per_glossario(Some("Lipofectamine")).expect("dizionario");
        assert_eq!(altro.correggi("Effetto citologico, cane"), "Effetto citologico, cane");
    }

    #[test]
    fn esame_culturale_e_colturale() {
        let t = vet();
        assert_eq!(t.correggi("Si consiglia l'esame culturale con antibiogramma."), "Si consiglia l'esame colturale con antibiogramma.");
        assert_eq!(t.correggi("esami culturali"), "esami colturali");
        assert_eq!(t.correggi("il patrimonio culturale"), "il patrimonio culturale");
    }

    #[test]
    fn le_desinenze_che_non_esistono_si_correggono() {
        let t = vet();
        assert_eq!(t.correggi("con prednisolona e"), "con prednisolone e");
        assert_eq!(t.correggi("la coleciste"), "la colecisti");
        assert_eq!(t.correggi("un carcinoma tubulo-papillare"), "un carcinoma tubulopapillare");
        // Il plurale vero resta.
        assert_eq!(t.correggi("le lesioni"), "le lesioni");
        assert_eq!(t.correggi("i mastocitomi"), "i mastocitomi");
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
        for p in ["regressione", "asportato", "formazione", "valutazione", "l'influenza", "l'esenzione", "un'analisi", "una nota", "uno studio", "dell'anno"] {
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
