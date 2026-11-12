//! I pezzi dell'interfaccia per il telefono, in stile Material 3 Expressive (il linguaggio di Android 16):
//! superfici tonali al posto di bordi e ombre, pulsanti a pillola, chip, righe grandi da toccare,
//! interruttori 52×32, barra di navigazione da 64 dp con la pastiglia 56×32, il timbro smerlato della firma.
//! I colori vengono dalla tavolozza del tema: i toni sono mescolanze del fondo col colore principale.

use cedro::ui::gfx::{Align, Painter, Weight};
use cedro::ui::theme::{mix, Palette};

use super::{ic, App, Azione, Forma, R};

type Rgb = (u8, u8, u8);

/// I toni di superficie di M3, ricavati dalla tavolozza del tema.
#[derive(Clone, Copy)]
pub struct Toni {
    pub fondo: Rgb,
    /// Contenitori bassi (elenchi, schede), medi (barra di navigazione) e alti (pulsanti tonali).
    pub basso: Rgb,
    pub medio: Rgb,
    pub alto: Rgb,
    /// Il contenitore del colore principale e il testo sopra.
    pub pri_c: Rgb,
    pub su_pri_c: Rgb,
    pub testo: Rgb,
    pub variante: Rgb,
    pub contorno: Rgb,
}

pub fn toni(pal: &Palette, scuro: bool) -> Toni {
    let f = pal.bg;
    let k = if scuro { 1.6 } else { 1.0 };
    Toni {
        fondo: f,
        basso: mix(mix(f, pal.text, 0.03 * k), pal.primary, 0.035),
        medio: mix(mix(f, pal.text, 0.05 * k), pal.primary, 0.06),
        alto: mix(mix(f, pal.text, 0.07 * k), pal.primary, 0.09),
        pri_c: mix(pal.surface, pal.primary, if scuro { 0.42 } else { 0.2 }),
        su_pri_c: if scuro { mix(pal.text, pal.primary, 0.15) } else { mix(pal.primary, (0, 0, 0), 0.45) },
        testo: pal.text,
        variante: pal.muted,
        contorno: mix(pal.border, pal.muted, 0.25),
    }
}

/// Il contenitore di un colore (il tono chiaro, o scuro nei temi scuri) e il colore da usarci sopra.
pub fn contenitore(c: Rgb, sup: Rgb, scuro: bool) -> (Rgb, Rgb) {
    if scuro {
        (mix(sup, c, 0.3), mix(c, (255, 255, 255), 0.55))
    } else {
        (mix(sup, c, 0.17), mix(c, (0, 0, 0), 0.35))
    }
}

/// Il timbro smerlato (forma «cookie» di M3 Expressive): 10 lobi, nel quadrato 24×24.
pub fn timbro() -> &'static str {
    static D: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    D.get_or_init(|| {
        let mut s = String::new();
        for i in 0..=120 {
            let t = std::f32::consts::TAU * i as f32 / 120.0;
            let r = 11.0 - 0.95 + 0.95 * (10.0 * t).cos();
            let (x, y) = (12.0 + r * t.cos(), 12.0 + r * t.sin());
            s.push_str(&format!("{}{x:.2} {y:.2} ", if i == 0 { "M" } else { "L" }));
        }
        s.push('Z');
        s
    })
}

/// Il timbro pieno di lato `s` con il segno del giudizio dentro.
pub fn sigillo(p: &mut Painter, x: f32, y: f32, s: f32, fondo: Rgb, segno: &str, col_segno: Rgb) {
    // Due passate: il riempimento di `icon_fill` è appena trasparente.
    p.icon_fill(timbro(), x, y, s, fondo);
    p.icon_fill(timbro(), x, y, s, fondo);
    let k = s * 0.5;
    p.icon(segno, x + (s - k) / 2.0, y + (s - k) / 2.0, k, col_segno);
    p.icon(segno, x + (s - k) / 2.0 + 0.4, y + (s - k) / 2.0, k, col_segno);
}

impl App {
    pub(super) fn toni(&self) -> Toni {
        toni(self.pal(), self.scuro())
    }

    /// Il pulsante principale: pillola piena alta 56, senza ombra. Senza azione è spento.
    pub(super) fn pillola(&mut self, p: &mut Painter, rc: R, icona: Option<&str>, testo: &str, az: Option<Azione>, colore: Option<Rgb>, vista: R) {
        let pal = self.pal();
        let t = self.toni();
        let c = colore.unwrap_or(pal.primary);
        let premuto = az.is_some() && self.premuto == az;
        let (fondo, fg) = match az {
            Some(_) => (if premuto { mix(c, (0, 0, 0), 0.14) } else { c }, if colore.is_some() { (255, 255, 255) } else { pal.on_primary }),
            None => (t.alto, mix(t.variante, t.fondo, 0.25)),
        };
        p.rrect(rc.0, rc.1, rc.2, rc.3, rc.3 / 2.0, Some(fondo), None);
        let dim = if rc.3 >= 52.0 { 16.5 } else { 15.0 };
        let tw = p.measure(testo, dim, Weight::Bold).min(rc.2 - 60.0);
        let iw = if icona.is_some() { 30.0 } else { 0.0 };
        let x0 = rc.0 + (rc.2 - tw - iw) / 2.0;
        if let Some(i) = icona {
            p.icon(i, x0, rc.1 + (rc.3 - 22.0) / 2.0, 22.0, fg);
        }
        p.text(testo, x0 + iw, rc.1, tw + 4.0, rc.3, dim, Weight::Bold, fg, Align::Left);
        if let Some(a) = az {
            self.colpo(rc, a, vista);
        }
    }

    /// Un pulsante tonale (pillola chiara, alta 44): azioni secondarie. Restituisce la larghezza.
    pub(super) fn tonale(&mut self, p: &mut Painter, x: f32, y: f32, icona: Option<&str>, testo: &str, az: Azione, vista: R) -> f32 {
        let t = self.toni();
        let w = p.measure(testo, 14.5, Weight::Semibold) + if icona.is_some() { 54.0 } else { 36.0 };
        let premuto = self.premuto == Some(az);
        p.rrect(x, y, w, 44.0, 22.0, Some(if premuto { mix(t.alto, t.testo, 0.08) } else { t.alto }), None);
        let mut tx = x + 18.0;
        if let Some(i) = icona {
            p.icon(i, x + 14.0, y + 12.0, 20.0, t.testo);
            tx = x + 40.0;
        }
        p.text(testo, tx, y, w - (tx - x) - 14.0, 44.0, 14.5, Weight::Semibold, t.testo, Align::Left);
        self.colpo((x, y, w, 44.0), az, vista);
        w
    }

    /// Una chip (filtro): icona colorata, testo, e un numero facoltativo. Restituisce la larghezza.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn chip(p: &mut Painter, t: &Toni, x: f32, y: f32, icona: &str, col: Rgb, testo: &str, on: bool, numero: usize, scuro: bool) -> f32 {
        let w = Self::largo_chip(p, testo, numero);
        let (cont, su) = contenitore(col, t.fondo, scuro);
        p.rrect(x, y, w, 36.0, 12.0, Some(if on { cont } else { t.basso }), None);
        p.icon(if on { ic::CHECK } else { icona }, x + 10.0, y + 9.0, 18.0, if on { su } else { col });
        p.text(testo, x + 34.0, y, w - 40.0, 36.0, 13.5, if on { Weight::Semibold } else { Weight::Regular }, if on { su } else { t.variante }, Align::Left);
        if numero > 0 {
            let n = numero.to_string();
            let nw = p.measure(&n, 11.0, Weight::Bold) + 12.0;
            let nx = x + w - nw - 10.0;
            p.rrect(nx, y + 9.0, nw, 18.0, 9.0, Some(if on { su } else { col }), None);
            p.text(&n, nx, y + 9.0, nw, 18.0, 11.0, Weight::Bold, if on { cont } else { (255, 255, 255) }, Align::Center);
        }
        w
    }

    pub(super) fn largo_chip(p: &Painter, testo: &str, numero: usize) -> f32 {
        let n = if numero > 0 { p.measure(&numero.to_string(), 11.0, Weight::Bold) + 20.0 } else { 0.0 };
        p.measure(testo, 13.5, Weight::Semibold) + 48.0 + n
    }

    /// Una riga grande da toccare dentro un contenitore: iconcina (colore, tratto, parte piena), etichetta
    /// piccola, valore, e la freccia della tendina o del passo successivo.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn campo(&mut self, p: &mut Painter, rc: R, icona: Option<(Rgb, &str, &str)>, etichetta: &str, valore: &str, freccia: &str, az: Azione, vista: R) {
        let t = self.toni();
        let scuro = self.scuro();
        let premuto = self.premuto == Some(az);
        p.rrect(rc.0, rc.1, rc.2, rc.3, 18.0, Some(if premuto { mix(t.fondo, t.testo, 0.06) } else { t.fondo }), None);
        let mut tx = rc.0 + 16.0;
        if let Some((c, tratto, pieno)) = icona {
            let (cont, _) = contenitore(c, t.fondo, scuro);
            let s = 40.0;
            let (ix, iy) = (rc.0 + 10.0, rc.1 + (rc.3 - s) / 2.0);
            p.rrect(ix, iy, s, s, 12.0, Some(cont), None);
            let k = 22.0;
            if !pieno.is_empty() {
                p.icon_fill(pieno, ix + (s - k) / 2.0, iy + (s - k) / 2.0, k, c);
            }
            p.icon(tratto, ix + (s - k) / 2.0, iy + (s - k) / 2.0, k, c);
            tx = ix + s + 12.0;
        }
        let w = rc.0 + rc.2 - tx - 40.0;
        if etichetta.is_empty() {
            p.text(valore, tx, rc.1, w, rc.3, 16.0, Weight::Semibold, t.testo, Align::Left);
        } else {
            p.text(etichetta, tx, rc.1 + rc.3 / 2.0 - 21.0, w, 18.0, 12.0, Weight::Regular, t.variante, Align::Left);
            p.text(valore, tx, rc.1 + rc.3 / 2.0 - 2.0, w, 24.0, 16.0, Weight::Semibold, t.testo, Align::Left);
        }
        if !freccia.is_empty() {
            p.icon(freccia, rc.0 + rc.2 - 34.0, rc.1 + rc.3 / 2.0 - 10.0, 20.0, t.variante);
        }
        self.colpo(rc, az, vista);
    }

    /// Una riga con titolo, spiegazione e interruttore, dentro un contenitore. Restituisce l'altezza.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn riga_si_no(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, titolo: &str, nota: &str, on: bool, chiave: u64, az: Azione, vista: R) -> f32 {
        let t = self.toni();
        let nh = if nota.is_empty() { 0.0 } else { p.wrap_height(nota, 12.5, Weight::Regular, w - 96.0, 17.0, 3) };
        let h = (40.0 + nh + 8.0).max(64.0);
        let premuto = self.premuto == Some(az);
        p.rrect(x, y, w, h, 18.0, Some(if premuto { mix(t.fondo, t.testo, 0.06) } else { t.fondo }), None);
        let ty = if nota.is_empty() { y + (h - 24.0) / 2.0 } else { y + 12.0 };
        p.text(titolo, x + 16.0, ty, w - 96.0, 24.0, 15.5, Weight::Semibold, t.testo, Align::Left);
        if !nota.is_empty() {
            p.text_wrap(nota, x + 16.0, y + 36.0, w - 96.0, 12.5, Weight::Regular, t.variante, 17.0, 3);
        }
        let v = self.anim.verso(super::anim::Chiave::Interruttore(chiave), if on { 1.0 } else { 0.0 }, 180.0);
        interruttore(p, self.pal(), &t, x + w - 68.0, y + h / 2.0, v);
        self.colpo((x, y, w, h), az, vista);
        h
    }

    /// La barra di navigazione di M3: 64 dp, pastiglia 56×32 dietro l'icona scelta (scivola), nome sotto.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn barra_nav(&mut self, p: &mut Painter, w: f32, h: f32, voci: &[(&str, &str, Rgb)], scelta: usize, az: impl Fn(usize) -> Azione) {
        let t = self.toni();
        let scuro = self.scuro();
        let (ml, _, mr, mb) = self.margini;
        let nav = 64.0;
        let ny = h - nav;
        p.fill(-ml, ny, w + ml + mr, nav + mb, t.medio);
        let bw = w / voci.len() as f32;
        let gx = self.anim.verso(super::anim::Chiave::Scheda, scelta as f32 * bw, 240.0);
        let col_scelta = voci.get(scelta).map(|v| v.2).unwrap_or(self.pal().primary);
        let (cont, su) = contenitore(col_scelta, t.medio, scuro);
        p.rrect(gx + bw / 2.0 - 28.0, ny + 6.0, 56.0, 32.0, 16.0, Some(cont), None);
        for (k, (nome, icona, _)) in voci.iter().enumerate() {
            let on = k == scelta;
            let bx = k as f32 * bw;
            p.icon(icona, bx + bw / 2.0 - 12.0, ny + 10.0, 24.0, if on { su } else { t.variante });
            p.text(nome, bx, ny + 40.0, bw, 18.0, 12.0, Weight::Semibold, if on { t.testo } else { t.variante }, Align::Center);
            self.colpi.push((Forma::R((bx, ny, bw, nav)), az(k)));
        }
    }
}

/// L'interruttore di M3 (52×32): spento ha il bordo e il pallino piccolo, acceso il pallino grande.
pub fn interruttore(p: &mut Painter, pal: &Palette, t: &Toni, x: f32, cy: f32, v: f32) {
    let fondo = mix(t.alto, pal.primary, v);
    p.rrect(x, cy - 16.0, 52.0, 32.0, 16.0, Some(fondo), None);
    if v < 0.5 {
        p.rrect(x + 1.0, cy - 15.0, 50.0, 30.0, 15.0, None, Some((mix(t.contorno, pal.primary, v), 2.0)));
    }
    let r = 8.0 + 4.0 * v;
    let cx = x + 16.0 + 20.0 * v;
    p.circle(cx, cy, r, Some(mix(t.contorno, pal.on_primary, v)), None);
    if v > 0.6 {
        p.icon(ic::CHECK, cx - 7.0, cy - 7.0, 14.0, pal.primary);
    }
}

/// La spunta tonda di un elemento d'elenco.
pub fn spunta_tonda(p: &mut Painter, pal: &Palette, t: &Toni, cx: f32, cy: f32, on: bool) {
    if on {
        p.circle(cx, cy, 12.0, Some(pal.primary), None);
        p.icon(ic::CHECK, cx - 8.0, cy - 8.0, 16.0, pal.on_primary);
        p.icon(ic::CHECK, cx - 7.6, cy - 8.0, 16.0, pal.on_primary);
    } else {
        p.circle(cx, cy, 11.0, None, Some((t.contorno, 2.0)));
    }
}
