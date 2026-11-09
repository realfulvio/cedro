//! Le piccole animazioni, come sul computer (`cedro::ui::anim`): i fogli che salgono dal basso,
//! la linea della categoria e la pastiglia della scheda che scivolano, il fondo dei pulsanti che
//! sfuma dopo il tocco, gli interruttori che scorrono.
//!
//! Lo schermo si ridisegna tutto a ogni fotogramma: qui si tiene, per ogni chiave, un valore che va
//! verso il suo obiettivo in un tempo dato. Finché qualcosa si muove `in_moto` chiede altri
//! fotogrammi; quando tutto è fermo il ciclo torna ad aspettare gli eventi.
//! Con «Rimuovi animazioni» di Android (`calmo`) i valori saltano subito alla fine.

use std::time::Instant;

/// Che cosa si anima.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chiave {
    /// Il foglio a comparsa (elenco, opzioni, password): sale dal basso.
    Foglio,
    /// La linea sotto la categoria scelta (x e larghezza).
    Categoria,
    CategoriaLargo,
    /// La pastiglia della scheda scelta nella barra in basso.
    Scheda,
    /// Il fondo di un pulsante appena toccato (numero del pulsante).
    Tocco(u64),
    /// Un interruttore (numero dell'interruttore).
    Interruttore(u64),
    /// Una pagina che entra da destra (il visualizzatore delle firme).
    Pagina,
}

struct Moto {
    k: Chiave,
    da: f32,
    a: f32,
    t0: Instant,
    ms: f32,
    visto: u64,
    /// L'ultimo fotogramma disegnato aveva già il valore finale.
    fermo: bool,
    /// Dove era arrivato (0..1) e in che fotogramma: sui telefoni lenti ogni fotogramma avanza
    /// al massimo di un terzo, così l'animazione si vede anche se un disegno dura 100 ms.
    fatto: f32,
    quando: u64,
}

impl Moto {
    /// Il valore a quest'ora, e si annota se è quello finale (il fotogramma dopo non serve).
    fn ora(&mut self, adesso: Instant, fotogramma: u64) -> f32 {
        if self.quando != fotogramma {
            let t = if self.ms <= 0.0 { 1.0 } else { (adesso.duration_since(self.t0).as_secs_f32() * 1000.0 / self.ms).min(1.0) };
            self.fatto = t.min(self.fatto + PASSO_MAX);
            self.quando = fotogramma;
        }
        self.fermo = self.fatto >= 1.0;
        self.da + (self.a - self.da) * esci(self.fatto)
    }
}

/// Quanto può avanzare un'animazione in un fotogramma.
const PASSO_MAX: f32 = 0.34;

/// Rallenta verso la fine (cubica).
pub fn esci(p: f32) -> f32 {
    let q = 1.0 - p.clamp(0.0, 1.0);
    1.0 - q * q * q
}

#[derive(Default)]
pub struct Anim {
    voci: Vec<Moto>,
    fotogramma: u64,
    adesso: Option<Instant>,
    /// Animazioni spente (Android: «Rimuovi animazioni», o le fotografie della `vista`).
    pub calmo: bool,
}

impl Anim {
    /// All'inizio di ogni disegno.
    pub fn inizio(&mut self) {
        self.fotogramma += 1;
        self.adesso = Some(Instant::now());
    }

    /// A fine disegno: si dimentica quello che non è stato disegnato (quando torna, ricompare animato).
    pub fn fine(&mut self) {
        let f = self.fotogramma;
        self.voci.retain(|m| m.visto >= f);
    }

    fn ora(&self) -> Instant {
        self.adesso.unwrap_or_else(Instant::now)
    }

    /// Un valore che compare: la prima volta che si disegna va da 0 a 1 in `ms`.
    pub fn comparsa(&mut self, k: Chiave, ms: f32) -> f32 {
        if self.calmo {
            return 1.0;
        }
        let adesso = self.ora();
        let f = self.fotogramma;
        if let Some(m) = self.voci.iter_mut().find(|m| m.k == k) {
            m.visto = f;
            return m.ora(adesso, f);
        }
        self.voci.push(Moto { k, da: 0.0, a: 1.0, t0: adesso, ms, visto: f, fermo: false, fatto: 0.0, quando: f });
        0.0
    }

    /// Un valore che scivola verso `obiettivo`: la prima volta ci sta già, poi ci va in `ms`.
    pub fn verso(&mut self, k: Chiave, obiettivo: f32, ms: f32) -> f32 {
        if self.calmo {
            return obiettivo;
        }
        let adesso = self.ora();
        let f = self.fotogramma;
        if let Some(m) = self.voci.iter_mut().find(|m| m.k == k) {
            m.visto = f;
            if (m.a - obiettivo).abs() > 0.01 {
                let da = m.ora(adesso, f);
                *m = Moto { k, da, a: obiettivo, t0: adesso, ms, visto: f, fermo: false, fatto: 0.0, quando: f };
                return da;
            }
            return m.ora(adesso, f);
        }
        self.voci.push(Moto { k, da: obiettivo, a: obiettivo, t0: adesso, ms: 0.0, visto: f, fermo: true, fatto: 1.0, quando: f });
        obiettivo
    }

    /// Fa ripartire un valore da `da` (il tocco su un pulsante: il fondo poi sfuma verso 0).
    pub fn spingi(&mut self, k: Chiave, da: f32, a: f32, ms: f32) {
        if self.calmo {
            return;
        }
        let adesso = Instant::now();
        let f = self.fotogramma;
        self.voci.retain(|m| m.k != k);
        self.voci.push(Moto { k, da, a, t0: adesso, ms, visto: f, fermo: false, fatto: 0.0, quando: f });
    }

    /// Il valore di ora, se la chiave si sta muovendo (senza crearla).
    pub fn se_c_e(&mut self, k: Chiave) -> Option<f32> {
        let adesso = self.ora();
        let f = self.fotogramma;
        self.voci.iter_mut().find(|m| m.k == k).map(|m| {
            m.visto = f;
            m.ora(adesso, f)
        })
    }

    /// Qualcosa si muove ancora: serve un altro fotogramma (finché non se n'è disegnato uno col valore finale).
    pub fn in_moto(&self) -> bool {
        self.voci.iter().any(|m| !m.fermo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparsa_parte_da_zero_e_arriva_a_uno() {
        let mut a = Anim::default();
        a.inizio();
        assert_eq!(a.comparsa(Chiave::Foglio, 180.0), 0.0);
        assert!(a.in_moto());
        a.fine();
        std::thread::sleep(std::time::Duration::from_millis(200));
        // Anche se il tempo è passato tutto, un fotogramma avanza al massimo di un terzo.
        a.inizio();
        assert!(a.comparsa(Chiave::Foglio, 180.0) < 0.8);
        a.fine();
        for _ in 0..3 {
            a.inizio();
            a.comparsa(Chiave::Foglio, 180.0);
            a.fine();
        }
        a.inizio();
        assert!((a.comparsa(Chiave::Foglio, 180.0) - 1.0).abs() < 1e-3);
        assert!(!a.in_moto());
        // Non disegnato per un fotogramma: si dimentica, e la volta dopo ricompare animato.
        a.fine();
        a.inizio();
        a.fine();
        a.inizio();
        assert_eq!(a.comparsa(Chiave::Foglio, 180.0), 0.0);
    }

    #[test]
    fn verso_la_prima_volta_sta_fermo() {
        let mut a = Anim::default();
        a.inizio();
        assert_eq!(a.verso(Chiave::Categoria, 40.0, 200.0), 40.0);
        assert!(!a.in_moto());
        a.inizio();
        assert_eq!(a.verso(Chiave::Categoria, 140.0, 200.0), 40.0);
        assert!(a.in_moto());
    }

    #[test]
    fn calmo_salta_alla_fine() {
        let mut a = Anim { calmo: true, ..Default::default() };
        a.inizio();
        assert_eq!(a.comparsa(Chiave::Foglio, 180.0), 1.0);
        assert!(!a.in_moto());
    }
}
