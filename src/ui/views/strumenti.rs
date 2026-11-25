//! Cedro 4 Labs, «Converti dati e piastre» con il file di uno strumento: al centro la piastra coi
//! valori veri e i ruoli (o l'elenco, o i dati dello strumento, o Allotrope), a destra lo strumento
//! riconosciuto e cosa esce. Lo stato e i comandi stanno in `ui/strumenti.rs`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use super::*;
use crate::strumenti::{self, asm, excel, scelte, Dato, Lettura, Ruolo, Scelte};
use crate::ui::core::{HitKind, Scroll};
use crate::ui::gfx::{Rgb, Weight};
use crate::ui::model::FileState;
use crate::ui::strumenti::{pozzetto_di, Pronta, StrumAct, Uscita, Vista};
use crate::ui::theme::{LABS_INK, LABS_LEAF, LABS_LINE, LABS_MUTED, LABS_STAGE};

const WHITE: Rgb = (255, 255, 255);
const AMBRA: Rgb = (0xB2, 0x6A, 0x00);
const ROW_H: f32 = 30.0;
const MONO_H: f32 = 19.0;
/// Quante righe di Allotrope si mostrano al più (il file scritto è intero).
const ASM_RIGHE: usize = 1500;

fn a(x: StrumAct) -> Act {
    Act::Strum(x)
}

/// Un testo che viene dal file (o dallo strumento): non si traduce, è quello che c'è scritto.
fn d(s: &str) -> String {
    crate::i18n::lit(s)
}

/// Un numero come lo scrive chi legge: con la virgola, tranne in inglese.
fn num(v: f64, dec: usize) -> String {
    let mut s = format!("{v:.dec$}");
    // «-0,000»: un numero che arrotondato è zero non ha segno.
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s.remove(0);
    }
    if crate::i18n::english() {
        s
    } else {
        s.replace('.', ",")
    }
}

/// Quante cifre dopo la virgola per mostrare tre cifre significative sul valore più grande.
fn decimali(lo: f64, hi: f64) -> usize {
    let m = lo.abs().max(hi.abs());
    if m <= 0.0 || !m.is_finite() {
        return 2;
    }
    (3 - m.log10().floor() as i32).clamp(0, 4) as usize
}

fn colore_scala(daltonici: bool) -> [Rgb; 3] {
    let (b, m, h) = excel::scala(daltonici);
    let c = |v: u32| ((v >> 16) as u8, (v >> 8) as u8, v as u8);
    [c(b), c(m), c(h)]
}

fn colore(t: f32, s: &[Rgb; 3]) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        mix(s[0], s[1], t * 2.0)
    } else {
        mix(s[1], s[2], (t - 0.5) * 2.0)
    }
}

fn luma(c: Rgb) -> f32 {
    (0.299 * c.0 as f32 + 0.587 * c.1 as f32 + 0.114 * c.2 as f32) / 255.0
}

/// Un testo di Cedro tradotto adesso, per metterlo dentro una frase che poi si segna tutta come «già fatta».
fn tradotto(s: &str) -> String {
    crate::i18n::tr_string(s)
}

/// I titoli che scrive Cedro (si traducono); gli altri sono i nomi dello strumento e restano com'erano.
const TITOLI_DI_CEDRO: [&str; 8] = ["Gruppo", "Pozzetto", "Campione", "Bersaglio", "Ruolo", "Posizione", "Data e ora", "meno il bianco (calcolato da Cedro)"];

fn titolo_colonna(t: &str) -> String {
    if TITOLI_DI_CEDRO.contains(&t) {
        t.to_string()
    } else {
        d(t)
    }
}

fn asm_acceso(app: &App) -> bool {
    if app.simple_view {
        app.strum.pronta == Pronta::Allotrope
    } else {
        app.strum.scelte.asm
    }
}

fn schede(app: &App, l: &Lettura) -> Vec<Vista> {
    let mut v = Vec::new();
    if l.a_piastra() {
        v.push(Vista::Piastra);
    }
    v.extend([Vista::Elenco, Vista::Strumento]);
    if asm_acceso(app) {
        v.push(Vista::Allotrope);
    }
    v
}

fn colore_ruolo(r: Ruolo) -> Rgb {
    match r {
        Ruolo::Bianco => (120, 128, 140),
        Ruolo::Standard => (45, 120, 200),
        Ruolo::Controllo | Ruolo::ControlloPositivo | Ruolo::ControlloNegativo => (30, 150, 100),
        Ruolo::Riferimento => (140, 90, 190),
        Ruolo::Campione => (90, 90, 90),
    }
}

// ───────────────────────── cose calcolate una volta sola ─────────────────────────

struct Pronto {
    chiave: String,
    tab: excel::Tab,
    larghezze: Vec<f32>,
    asm: Vec<String>,
    asm_tagliato: bool,
}

thread_local! {
    static CACHE: RefCell<Option<Rc<Pronto>>> = const { RefCell::new(None) };
}

/// La tabella dell'Elenco e le righe di Allotrope, rifatte solo se cambia la lettura o una scelta.
fn pronto(f: &Frame, app: &App, path: &Path, l: &Arc<Lettura>, vista: Vista) -> Rc<Pronto> {
    let sc = app.strum_scelte_per(path);
    let chiave = format!("{}|{:p}|{:?}|{}|{}", path.display(), Arc::as_ptr(l), sc.excel_opz(), sc.asm_calcoli, sc.asm_percorso);
    if let Some(p) = CACHE.with(|c| c.borrow().clone()).filter(|p| p.chiave == chiave && (vista != Vista::Allotrope || !p.asm.is_empty())) {
        return p;
    }
    let tab = excel::tabella(l, &sc.excel_opz());
    let mut larghezze: Vec<f32> = Vec::new();
    for (c, t) in tab.intest.iter().enumerate() {
        let mut w = f.measure(&titolo_colonna(t), 14.0, SEMI);
        for r in tab.righe.iter().take(80) {
            let s = match r.get(c).and_then(|x| x.as_ref()) {
                Some(excel::Cella::Testo(t)) => d(t),
                Some(excel::Cella::Numero(v)) => format!("{v}"),
                None => String::new(),
            };
            w = w.max(f.measure(&s, 14.0, SANS));
        }
        larghezze.push((w + 28.0).clamp(70.0, 260.0));
    }
    let (asm, asm_tagliato) = if vista == Vista::Allotrope {
        let nome_asm = format!("{}.asm.json", crate::engine::stem(path));
        let o = sc.asm_opz(&path.display().to_string(), &nome_asm);
        let v = asm::scrivi(l, &o);
        let testo = serde_json::to_string_pretty(&v).unwrap_or_default();
        let mut righe: Vec<String> = testo.lines().map(str::to_string).collect();
        let t = righe.len() > ASM_RIGHE;
        righe.truncate(ASM_RIGHE);
        (righe, t)
    } else {
        (Vec::new(), false)
    };
    let p = Rc::new(Pronto { chiave, tab, larghezze, asm, asm_tagliato });
    CACHE.with(|c| *c.borrow_mut() = Some(p.clone()));
    p
}

// ───────────────────────── il centro ─────────────────────────

/// Il centro: le schede, la piastra o l'elenco, in uno spazio scuro con la tinta della pagina.
pub fn stage(f: &mut Frame, app: &App, i: usize, l: &Arc<Lettura>, area: R) {
    let path = app.files[i].path.clone();
    let pf = app.strum_file(&path);
    // Con uno strumento scelto a mano, finché il file non si legge (o non si legge affatto) non si mostra
    // la lettura di prima: sarebbe di un altro strumento.
    if pf.rileggendo || matches!(pf.riletto, Some(Err(_))) {
        let (_, bright) = super::labs::colors(app);
        f.p.rrect_a(area.0, area.1, area.2, area.3, 14.0, WHITE, 0.03);
        f.p.rrect_line(area.0, area.1, area.2, area.3, 14.0, mix(bright, WHITE, 0.3), 2.0, 0.4, Some((9.0, 6.0)));
        let msg = if pf.rileggendo { "Rileggo il file…" } else { "Con questo strumento il file non si legge." };
        f.text_c(msg, (area.0, area.1, area.2, area.3), 18.0, SEMI, LABS_MUTED);
        return;
    }
    let g = app.strum_gruppo(&path, l);
    let tab = schede(app, l);
    let vista = if tab.contains(&pf.vista) { pf.vista } else { tab[0] };
    // Le schede.
    let bar_h = 40.0;
    let labels: Vec<String> = tab.iter().map(|v| v.nome().to_string()).collect();
    let widths: Vec<f32> = labels.iter().map(|t| f.measure(t, 15.0, SEMI) + 32.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 8.0;
    f.p.rrect_a(area.0, area.1, total, bar_h, 20.0, WHITE, 0.08);
    let mut bx = area.0 + 4.0;
    for (k, v) in tab.iter().enumerate() {
        let r = (bx, area.1 + 4.0, widths[k], bar_h - 8.0);
        let act = a(StrumAct::Vista(*v));
        let on = *v == vista;
        if on {
            f.p.rrect(r.0, r.1, r.2, r.3, 16.0, Some((246, 242, 252)), None);
        } else if f.hovered(&act) {
            f.p.rrect_a(r.0, r.1, r.2, r.3, 16.0, WHITE, 0.1);
        }
        f.text_c(&labels[k], r, 15.0, SEMI, if on { LABS_STAGE } else { LABS_INK });
        f.focus_ring(r, 16.0, &act);
        f.hit(r, act);
        bx += widths[k];
    }
    // Lo strumento, a destra.
    let chip = d(&format!("{} · {}", scelte::nome_strumento(l), tradotto(l.tecnica.nome())));
    let cw = f.measure(&chip, 14.5, SEMI) + 28.0;
    if area.0 + total + 16.0 + cw <= area.0 + area.2 {
        let r = (area.0 + area.2 - cw, area.1 + 4.0, cw, bar_h - 8.0);
        f.p.rrect_line(r.0, r.1, r.2, r.3, 16.0, LABS_LINE, 1.5, 1.0, None);
        f.text_c(&chip, r, 14.5, SEMI, LABS_MUTED);
    }
    let mut top = area.1 + bar_h + 12.0;
    // Più piastre nello stesso file: si sfogliano.
    if vista == Vista::Piastra && l.gruppi.len() > 1 {
        let n = l.gruppi.len();
        let titolo = format!("{} · {}/{}", d(&l.gruppi[g].titolo), g + 1, n);
        for (k, (icon, to)) in [(ic::ARROW_LEFT, g.checked_sub(1)), (ic::ARROW_RIGHT, (g + 1 < n).then_some(g + 1))].into_iter().enumerate() {
            let bx = if k == 0 { area.0 } else { area.0 + 44.0 + f.measure(&titolo, 15.0, SEMI) + 16.0 };
            let r = (bx, top, 36.0, 32.0);
            match to {
                Some(t) => {
                    let act = a(StrumAct::Gruppo(t));
                    if f.hovered(&act) {
                        f.p.rrect_a(r.0, r.1, r.2, r.3, 10.0, WHITE, 0.12);
                    }
                    f.icon(icon, r.0 + 6.0, r.1 + 4.0, 24.0, LABS_INK);
                    f.hit(r, act);
                }
                None => f.icon(icon, r.0 + 6.0, r.1 + 4.0, 24.0, mix(LABS_INK, LABS_STAGE, 0.65)),
            }
        }
        f.p.text(&titolo, area.0 + 40.0, top, f.measure(&titolo, 15.0, SEMI) + 8.0, 32.0, 15.0, SEMI, LABS_INK, Align::Left);
        top += 40.0;
    }
    let body = (area.0, top, area.2, area.1 + area.3 - top);
    match vista {
        Vista::Piastra => piastra(f, app, &path, &l, g, body),
        Vista::Elenco => elenco(f, app, &path, &l, body),
        Vista::Strumento => dati_strumento(f, &l, body),
        Vista::Allotrope => json(f, app, &path, &l, body),
    }
}

/// Il valore da mostrare di una misura: quello letto, meno la media dei bianchi se si è scelto.
fn valore_eff(l: &Lettura, m: &strumenti::Misura, bianco: Option<f64>) -> Option<f64> {
    l.valore(m).map(|v| v - bianco.unwrap_or(0.0))
}

fn piastra(f: &mut Frame, app: &App, path: &Path, l: &Arc<Lettura>, g: usize, body: R) {
    let pf = app.strum_file(path);
    let sc = app.strum_scelte_per(path);
    let ruoli = sc.ruoli_veri();
    let gr = &l.gruppi[g];
    let (righe, colonne) = l.formato(g);
    let bianco = if sc.meno_bianco { excel::media_bianchi(l, g, &ruoli) } else { None };
    let scala = colore_scala(sc.daltonici);
    let vals: Vec<f64> = gr.misure.iter().filter(|m| !m.e_spettro()).filter_map(|m| valore_eff(l, m, bianco)).collect();
    let (lo, hi) = vals.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let (lo, hi) = if vals.is_empty() { (0.0, 1.0) } else { (lo, hi) };
    let dec = decimali(lo, hi);
    // Titolo e scala dei colori.
    let titolo = d(if gr.titolo.is_empty() { l.nome } else { &gr.titolo });
    f.p.text(&titolo, body.0, body.1, body.2 * 0.6, 30.0, 21.0, BOLD, LABS_INK, Align::Left);
    let unita = if l.unita.is_empty() { String::new() } else { format!(" ({})", l.unita) };
    let sotto = match l.curva().filter(|_| l.principale == crate::strumenti::PRINCIPALE_CURVA) {
        Some(c) => format!("{} · {}{}", tradotto(c.nome_riassunto()), d(&c.concetto), d(&unita)),
        None => d(&format!("{}{}", l.principale, unita)),
    };
    f.p.text(&sotto, body.0, body.1 + 28.0, body.2 * 0.6, 22.0, 14.5, SANS, LABS_MUTED, Align::Left);
    if !vals.is_empty() {
        let (lt, ht) = (num(lo, dec), num(hi, dec));
        let (lw, hw) = (f.measure(&lt, 13.5, SANS), f.measure(&ht, 13.5, SANS));
        let bw = 120.0;
        let by = body.1 + 12.0;
        let hx = body.0 + body.2 - hw - 4.0;
        let bx = hx - 8.0 - bw;
        for k in 0..24 {
            let t = k as f32 / 23.0;
            f.p.fill(bx + bw * k as f32 / 24.0, by, bw / 24.0 + 0.5, 10.0, colore(t, &scala));
        }
        f.p.text(&lt, bx - 8.0 - lw, by - 6.0, lw + 4.0, 22.0, 13.5, SANS, LABS_MUTED, Align::Left);
        f.p.text(&ht, hx, by - 6.0, hw + 4.0, 22.0, 13.5, SANS, LABS_MUTED, Align::Left);
        if bianco.is_some() {
            f.p.text("meno il bianco", bx - 8.0 - lw - 160.0, by + 14.0, 160.0 + lw, 20.0, 13.0, SANS, LABS_LEAF, Align::Right);
        }
    }
    // La griglia.
    let (testa_w, testa_h, sbox_h, riga_h) = (28.0f32, 22.0f32, 64.0f32, 26.0f32);
    let top = body.1 + 64.0;
    let avail_w = body.2 - testa_w;
    let avail_h = (body.3 - 64.0 - testa_h - sbox_h - riga_h - 14.0).max(40.0);
    let gap = if colonne > 24 { 1.0 } else if colonne > 12 { 2.0 } else { 3.0 };
    let cw = ((avail_w - gap * (colonne as f32 - 1.0)) / colonne as f32).floor().clamp(3.0, 84.0);
    let ch = ((avail_h - gap * (righe as f32 - 1.0)) / righe as f32).floor().clamp(3.0, 64.0);
    let (gw, gh) = (cw * colonne as f32 + gap * (colonne as f32 - 1.0), ch * righe as f32 + gap * (righe as f32 - 1.0));
    let gx = body.0 + testa_w + 2.0 + ((avail_w - 2.0 - gw) / 2.0).max(0.0);
    let gy = top + testa_h;
    // Intestazioni: la lettera della riga e il numero della colonna scelgono tutta la riga o la colonna.
    let passo_c = if cw < 22.0 { ((22.0 / (cw + gap)).ceil() as usize).max(1) } else { 1 };
    for c in 0..colonne {
        let x = gx + c as f32 * (cw + gap);
        let r = (x, top, cw, testa_h);
        let act = a(StrumAct::Colonna(c));
        if f.hovered(&act) {
            f.p.rrect_a(r.0, r.1, r.2, r.3, 6.0, WHITE, 0.12);
        }
        if c % passo_c == 0 {
            f.p.text(&(c + 1).to_string(), x - 4.0, top, cw + 8.0, testa_h, 12.5, SEMI, LABS_MUTED, Align::Center);
        }
        f.hit(r, act);
    }
    let passo_r = if ch < 16.0 { ((16.0 / (ch + gap)).ceil() as usize).max(1) } else { 1 };
    for r in 0..righe {
        let y = gy + r as f32 * (ch + gap);
        let rect = (gx - testa_w - 2.0, y, testa_w, ch);
        let act = a(StrumAct::Riga(r));
        if f.hovered(&act) {
            f.p.rrect_a(rect.0, rect.1, rect.2, rect.3, 6.0, WHITE, 0.12);
        }
        if r % passo_r == 0 {
            let nome = strumenti::pozzetto_nome(r, 0);
            let lettera = nome.trim_end_matches(|c: char| c.is_ascii_digit());
            f.p.text(lettera, rect.0, y, testa_w, ch, 12.5, SEMI, LABS_MUTED, Align::Center);
        }
        f.hit(rect, act);
    }
    // I pozzetti: tondi, come nell'anteprima approvata (al massimo 44 px), al centro della loro casella.
    // (al massimo 44 px; con pochi pozzetti fino a 60, perché non restino un puntino in uno spazio vuoto).
    let dm = cw.min(ch).min(if righe * colonne <= 24 { 60.0 } else { 44.0 });
    let grande = dm >= 32.0;
    let max_size = if dm >= 40.0 { 13.0 } else { 11.0 };
    let sopra = app.strum.sopra;
    for r in 0..righe {
        for c in 0..colonne {
            let (x, y) = (gx + c as f32 * (cw + gap), gy + r as f32 * (ch + gap));
            let (cx, cy) = (x + cw / 2.0, y + ch / 2.0);
            let Some(m) = pozzetto_di(l, g, r, c) else {
                f.p.circle(cx, cy, dm / 2.0, None, Some((LABS_LINE, 1.0)));
                continue;
            };
            let v = valore_eff(l, m, bianco);
            let (fondo, testo) = match v {
                Some(v) => {
                    let col = colore(((v - lo) / (hi - lo).max(1e-12)) as f32, &scala);
                    (col, num(v, dec))
                }
                None => ((70, 62, 84), l.testo(m).map(|t| if t.eq_ignore_ascii_case("undetermined") { "n.d.".to_string() } else { d(&t) }).unwrap_or_default()),
            };
            f.p.circle(cx, cy, dm / 2.0, Some(fondo), Some(((0, 0, 0), 1.0)));
            let scuro = luma(fondo) > 0.58;
            if grande && !testo.is_empty() {
                // Il numero va nel cerchio: si prova un carattere più piccolo, poi meno decimali (il valore esatto sta nella riga sotto).
                let mut cand = vec![testo.clone()];
                if let Some(v) = v {
                    cand.extend((0..dec).rev().map(|k| num(v, k)));
                }
                let (testo, size) = cand
                    .iter()
                    .find_map(|t| [max_size, max_size - 1.0, 10.5].into_iter().find(|&sz| sz >= 10.5 && f.measure(t, sz, BOLD) <= dm - 6.0).map(|sz| (t.clone(), sz)))
                    .unwrap_or_else(|| (cand.last().cloned().unwrap_or_default(), 10.5));
                f.p.text(&testo, cx - dm / 2.0 - 3.0, cy - dm / 2.0, dm + 6.0, dm, size, BOLD, if scuro { (27, 19, 37) } else { WHITE }, Align::Center);
            }
            let ruolo = strumenti::ruolo_di(m, &ruoli);
            if ruolo != Ruolo::Campione && dm >= 14.0 {
                // La lettera del ruolo sta in basso a destra, appena fuori dal pozzetto.
                let (bx, by) = (cx + dm / 2.0 - 2.0, cy + dm / 2.0 - 2.0);
                f.p.circle(bx, by, 8.0, Some(colore_ruolo(ruolo)), Some((WHITE, 1.2)));
                f.p.text(ruolo.lettera(), bx - 8.0, by - 8.0, 16.0, 16.0, 10.5, BOLD, WHITE, Align::Center);
            }
            let (sel, hot) = (pf.sel.contains(&(r, c)), sopra == Some((r, c)));
            if sel {
                f.p.circle(cx, cy, dm / 2.0 + 2.0, None, Some((WHITE, 3.0)));
            } else if hot {
                f.p.circle(cx, cy, dm / 2.0 + 1.0, None, Some((WHITE, 1.5)));
            }
        }
    }
    f.hit_kind((gx, gy, gw, gh), a(StrumAct::Piastra), HitKind::Slider);
    // Sotto la piastra: il pozzetto che si sfiora.
    let hy = gy + gh + 8.0;
    if let Some((r, c)) = sopra {
        if let Some(m) = pozzetto_di(l, g, r, c) {
            let valore = match valore_eff(l, m, bianco) {
                Some(v) => num(v, dec + 1),
                None => d(&l.testo(m).unwrap_or_default()),
            };
            let mut p: Vec<String> = vec![strumenti::pozzetto_nome(r, c)];
            if !m.campione.is_empty() {
                p.push(d(&m.campione));
            }
            if let Some(b) = &m.bersaglio {
                p.push(d(b));
            }
            p.push(valore);
            f.p.text(&p.join(" · "), body.0, hy, body.2, riga_h, 14.5, SEMI, LABS_INK, Align::Left);
        }
    } else {
        f.p.text("Passa sopra un pozzetto per vederne il valore.", body.0, hy, body.2, riga_h, 14.0, SANS, LABS_MUTED, Align::Left);
    }
    scelta(f, l, &sc, &pf.sel, g, bianco, (body.0, hy + riga_h + 6.0, body.2, sbox_h));
}

/// La scelta sulla piastra: quanti pozzetti, media, DS e CV; i ruoli da segnare.
fn scelta(f: &mut Frame, l: &Lettura, sc: &Scelte, sel: &std::collections::BTreeSet<(usize, usize)>, g: usize, bianco: Option<f64>, r: R) {
    f.p.rrect_a(r.0, r.1, r.2, r.3, 14.0, WHITE, 0.07);
    f.p.rrect_line(r.0, r.1, r.2, r.3, 14.0, LABS_LINE, 1.0, 1.0, None);
    let ruoli = sc.ruoli_veri();
    let n = sel.len();
    let nessuno = a(StrumAct::Nessuno);
    let tutti = a(StrumAct::Tutti);
    // «Tutta la piastra» e «Nessuno», piccoli, a destra in alto.
    let mut rx = r.0 + r.2 - 14.0;
    for (label, act, attivo) in [("Nessuno".to_string(), nessuno, n > 0), ("Tutta la piastra".to_string(), tutti, true)] {
        let w = f.measure(&label, 13.5, SEMI);
        rx -= w;
        if attivo {
            let hot = f.hovered(&act);
            f.p.text(&label, rx, r.1 + 6.0, w + 4.0, 24.0, 13.5, SEMI, if hot { WHITE } else { LABS_LEAF }, Align::Left);
            f.hit((rx - 6.0, r.1 + 6.0, w + 12.0, 24.0), act);
        }
        rx -= 18.0;
    }
    if n == 0 {
        let hint = "Clicca un pozzetto o trascina per sceglierne più d'uno. Poi segna il ruolo: finisce nel foglio «Pozzetti» dell'Excel.".to_string();
        let w = (rx - r.0 - 14.0).max(100.0);
        // Nel riquadro ci stanno due righe: se ne servono tre, il carattere si stringe.
        let size = [14.0f32, 13.0, 12.0].into_iter().find(|&sz| f.p.wrap_lines(&hint, sz, SANS, w, 3).len() <= 2).unwrap_or(12.0);
        f.para(&hint, r.0 + 14.0, r.1 + 10.0, w, size, SANS, LABS_MUTED, 1.3, 2);
        return;
    }
    // Media, DS e CV dei pozzetti scelti: calcolati da Cedro, e lo dice.
    let mut vs: Vec<f64> = Vec::new();
    let mut ruoli_scelti: Vec<Ruolo> = Vec::new();
    for &(rr, c) in sel {
        if let Some(m) = pozzetto_di(l, g, rr, c) {
            if let Some(v) = valore_eff(l, m, bianco) {
                vs.push(v);
            }
            ruoli_scelti.push(strumenti::ruolo_di(m, &ruoli));
        }
    }
    let dec = decimali(vs.iter().cloned().fold(0.0, f64::min), vs.iter().cloned().fold(0.0, f64::max)) + 1;
    let mut riga = tradotto(&if n == 1 { "1 pozzetto".to_string() } else { format!("{n} pozzetti") });
    if vs.len() > 1 {
        let m = vs.iter().sum::<f64>() / vs.len() as f64;
        let sd = (vs.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / (vs.len() - 1) as f64).sqrt();
        // Con la media vicina a zero (un bianco) il CV non vuol dire niente: non si scrive.
        let cv = (m.abs() > 1e-9 && sd / m.abs() * 100.0 < 10_000.0).then(|| sd / m.abs() * 100.0);
        riga = format!("{riga} · {} {} · {} {}", tradotto("media"), num(m, dec), tradotto("DS"), num(sd, dec));
        if let Some(cv) = cv {
            riga = format!("{riga} · CV {} %", num(cv, 1));
        }
    } else if let Some(v) = vs.first() {
        riga = format!("{riga} · {}", num(*v, dec));
    }
    // Numeri e parole sono già nella lingua scelta: il disegno non li traduce una seconda volta.
    let riga = d(&riga);
    let w = (rx - r.0 - 14.0).max(100.0);
    let size = [14.5f32, 13.5, 12.5].into_iter().find(|&sz| f.measure(&riga, sz, SEMI) <= w).unwrap_or(12.5);
    f.p.text(&riga, r.0 + 14.0, r.1 + 6.0, w, 24.0, size, SEMI, LABS_INK, Align::Left);
    if vs.len() > 1 {
        let rw = f.measure(&riga, size, SEMI);
        let nota = "calcolato da Cedro".to_string();
        let nw = f.measure(&nota, 13.0, SANS);
        if r.0 + 14.0 + rw + 10.0 + nw <= rx - 8.0 {
            f.p.text(&nota, r.0 + 14.0 + rw + 10.0, r.1 + 6.0, nw + 4.0, 24.0, 13.0, SANS, LABS_MUTED, Align::Left);
        }
    }
    // I ruoli: se tutti i pozzetti scelti ne hanno uno solo, quello si accende.
    let voci: Vec<(char, &str)> = if l.tecnica == strumenti::Tecnica::Qpcr { vec![('-', "Campione"), ('S', "Standard"), ('C', "Controllo")] } else { vec![('B', "Bianco"), ('S', "Standard"), ('C', "Controllo"), ('-', "Campione")] };
    let lettera = |x: Ruolo| x.lettera().chars().next().unwrap_or('-');
    let unico: Option<char> = {
        let mut it = ruoli_scelti.iter().map(|x| lettera(*x));
        let first = it.next();
        first.filter(|c| it.all(|d| d == *c))
    };
    let mut x = r.0 + 14.0;
    let y = r.1 + 34.0;
    for (c, nome) in voci {
        let label = nome.to_string();
        let w = f.measure(&label, 14.0, SEMI) + 26.0;
        let act = a(StrumAct::Ruolo(c));
        let on = unico == Some(c);
        let hot = f.hovered(&act);
        let rect = (x, y, w, 24.0);
        if on {
            f.p.rrect(rect.0, rect.1, rect.2, rect.3, 12.0, Some((246, 242, 252)), None);
        } else {
            f.p.rrect_a(rect.0, rect.1, rect.2, rect.3, 12.0, WHITE, if hot { 0.2 } else { 0.1 });
        }
        f.text_c(&label, rect, 14.0, SEMI, if on { LABS_STAGE } else { LABS_INK });
        f.focus_ring(rect, 12.0, &act);
        f.hit(rect, act);
        x += w + 8.0;
    }
}

// ───────────────────────── le altre schede ─────────────────────────

fn cella_testo(c: &Option<excel::Cella>) -> (String, bool) {
    match c {
        Some(excel::Cella::Testo(t)) => (t.clone(), false),
        Some(excel::Cella::Numero(v)) => {
            let s = if v.fract() == 0.0 && v.abs() < 1e12 { format!("{}", *v as i64) } else { format!("{:.6}", v).trim_end_matches('0').trim_end_matches('.').to_string() };
            (if crate::i18n::english() { s } else { s.replace('.', ",") }, true)
        }
        None => (String::new(), false),
    }
}

/// L'Elenco: il foglio che finisce nell'Excel, una riga per pozzetto (o per campione). Le colonne si
/// mostrano dalla prima finché ci stanno; una lettura di una sola riga (un contacellule) si legge dall'alto.
fn elenco(f: &mut Frame, app: &App, path: &Path, l: &Arc<Lettura>, body: R) {
    let p = pronto(f, app, path, l, Vista::Elenco);
    let n = p.tab.righe.len();
    if n == 1 {
        return elenco_verticale(f, &p, body);
    }
    // Quante colonne ci stanno, ognuna alla sua larghezza.
    let mut cols = 0usize;
    let mut tot = 8.0f32;
    for w in &p.larghezze {
        if tot + w > body.2 && cols >= 1 {
            break;
        }
        tot += w;
        cols += 1;
    }
    let cols = cols.max(1).min(p.tab.intest.len());
    let larg: Vec<f32> = p.larghezze.iter().take(cols).map(|w| w.min(body.2 - 8.0)).collect();
    let head_h = 34.0;
    f.p.rrect_a(body.0, body.1, body.2, head_h, 10.0, WHITE, 0.1);
    let mut x = body.0 + 4.0;
    for (c, t) in p.tab.intest.iter().take(cols).enumerate() {
        f.p.text(&titolo_colonna(t), x + 10.0, body.1, larg[c] - 12.0, head_h, 14.0, SEMI, LABS_INK, Align::Left);
        x += larg[c];
    }
    let area = (body.0, body.1 + head_h + 4.0, body.2, (body.3 - head_h - 4.0 - 28.0).max(60.0));
    let off = f.ui.scroll(Scroll::Strum);
    let first = (off / ROW_H).floor().max(0.0) as usize;
    let visibili = (area.3 / ROW_H).ceil() as usize + 2;
    let tab = p.tab.clone();
    super::scrolled(f, area, Scroll::Strum, |f| {
        for idx in first..(first + visibili).min(n) {
            let y = area.1 + idx as f32 * ROW_H;
            if idx % 2 == 1 {
                f.p.rrect_a(area.0, y, area.2 - 8.0, ROW_H, 6.0, WHITE, 0.035);
            }
            let mut x = area.0 + 4.0;
            for (c, cella) in tab.righe[idx].iter().take(cols).enumerate() {
                let (t, numero) = cella_testo(cella);
                if !t.is_empty() {
                    let w = larg[c];
                    // I nomi dei ruoli sono di Cedro e si traducono; il resto viene dal file.
                    let t = if numero || tab.intest[c] == "Ruolo" { t } else { d(&t) };
                    if numero {
                        f.p.text(&t, x, y, w - 14.0, ROW_H, 14.0, SANS, LABS_INK, Align::Right);
                    } else {
                        f.p.text(&t, x + 10.0, y, w - 12.0, ROW_H, 14.0, SANS, LABS_INK, Align::Left);
                    }
                }
                x += larg[c];
            }
        }
        n as f32 * ROW_H + 8.0
    });
    let mut nota = tradotto(if l.a_piastra() { "È il foglio «Pozzetti» di Excel: una riga per pozzetto." } else { "È il foglio dei campioni di Excel: una riga per campione." });
    if cols < p.tab.intest.len() {
        nota = format!("{nota} {}", tradotto(&format!("Qui le prime {cols} colonne di {}: nell'Excel ci sono tutte.", p.tab.intest.len())));
    }
    f.p.text(&d(&nota), body.0, body.1 + body.3 - 24.0, body.2, 22.0, 13.5, SANS, LABS_MUTED, Align::Left);
}

/// Una sola riga (una lettura sola): colonna per colonna, con il nome a sinistra.
fn elenco_verticale(f: &mut Frame, p: &Pronto, body: R) {
    let kw = 280.0f32.min(body.2 * 0.5);
    let vw = body.2 - kw - 24.0;
    let tab = p.tab.clone();
    super::scrolled(f, body, Scroll::Strum, |f| {
        let mut y = body.1 + 4.0;
        for (c, t) in tab.intest.iter().enumerate() {
            let (v, numero) = cella_testo(&tab.righe[0][c]);
            if v.is_empty() {
                continue;
            }
            let v = if numero || t == "Ruolo" { v } else { d(&v) };
            f.p.text(&titolo_colonna(t), body.0 + 8.0, y, kw - 8.0, 26.0, 14.5, SEMI, LABS_MUTED, Align::Left);
            f.p.text(&v, body.0 + kw + 8.0, y, vw, 26.0, 14.5, SANS, LABS_INK, Align::Left);
            y += 30.0;
            f.p.fill_a(body.0, y - 3.0, body.2 - 12.0, 1.0, LABS_LINE, 0.7);
        }
        y - body.1 + 8.0
    });
}

/// «Strumento»: produttore, modello, software, e l'impronta del file originale.
fn dati_strumento(f: &mut Frame, l: &Lettura, body: R) {
    let righe = excel::righe_strumento(l);
    let kw = 230.0f32.min(body.2 * 0.4);
    let vw = body.2 - kw - 24.0;
    let area = (body.0, body.1, body.2, body.3);
    super::scrolled(f, area, Scroll::Strum, |f| {
        let mut y = body.1 + 4.0;
        for (k, v) in &righe {
            // «Strumento letto come» ha il tipo di strumento (di Cedro) accanto al nome (dello strumento); gli avvisi sono di Cedro.
            let v = match k.as_str() {
                "Strumento letto come" => d(&format!("{} ({})", l.nome, tradotto(l.tecnica.nome()))),
                "Avviso" => v.clone(),
                _ => d(v),
            };
            let h = if !v.contains(' ') && v.chars().count() > 30 {
                f.p.text(&v, body.0 + kw + 8.0, y, vw, 24.0, 13.5, Weight::Mono, LABS_INK, Align::Left);
                24.0
            } else {
                f.para(&v, body.0 + kw + 8.0, y + 2.0, vw, 15.0, SANS, LABS_INK, 1.35, 6).max(24.0)
            };
            f.p.text(k, body.0 + 8.0, y, kw - 8.0, 24.0, 14.5, SEMI, LABS_MUTED, Align::Left);
            y += h + 10.0;
            f.p.fill_a(body.0, y - 5.0, body.2 - 12.0, 1.0, LABS_LINE, 0.7);
        }
        y - body.1 + 8.0
    });
}

/// «Allotrope»: il file come lo scrive Cedro, in chiaro. Compare solo se l'uscita è accesa.
fn json(f: &mut Frame, app: &App, path: &Path, l: &Arc<Lettura>, body: R) {
    let p = pronto(f, app, path, l, Vista::Allotrope);
    let sc = app.strum_scelte_per(path);
    let mut x = body.0;
    let mut pillola = |f: &mut Frame, t: &str, c: Rgb| {
        let w = f.measure(t, 13.5, SEMI) + 20.0;
        f.p.rrect_a(x, body.1, w, 26.0, 13.0, c, 0.18);
        f.text_c(t, (x, body.1, w, 26.0), 13.5, SEMI, c);
        x += w + 8.0;
    };
    pillola(f, &d(&asm::schema_breve(l.tecnica)), LABS_INK);
    if sc.asm_controlla {
        match asm::controlla(&asm::scrivi(l, &sc.asm_opz(&path.display().to_string(), &format!("{}.asm.json", crate::engine::stem(path))))) {
            Ok(()) => pillola(f, "controllato", LABS_LEAF),
            Err(_) => pillola(f, "non passa il controllo", (240, 160, 80)),
        }
    }
    if sc.asm_calcoli {
        pillola(f, "con i calcoli di Cedro", LABS_MUTED);
    }
    if p.asm_tagliato {
        pillola(f, "estratto: il file intero ha tutte le misure", LABS_MUTED);
    }
    let area = (body.0, body.1 + 36.0, body.2, body.3 - 36.0);
    f.p.rrect_a(area.0, area.1, area.2, area.3, 12.0, (255, 255, 255), 0.05);
    let off = f.ui.scroll(Scroll::Strum);
    let first = (off / MONO_H).floor().max(0.0) as usize;
    let visibili = (area.3 / MONO_H).ceil() as usize + 2;
    let n = p.asm.len();
    let inner = (area.0 + 12.0, area.1 + 6.0, area.2 - 24.0, area.3 - 12.0);
    super::scrolled(f, inner, Scroll::Strum, |f| {
        for idx in first..(first + visibili).min(n) {
            riga_json(f, &p.asm[idx], inner.0, inner.1 + idx as f32 * MONO_H, inner.2);
        }
        n as f32 * MONO_H + 8.0
    });
}

/// Una riga di JSON con i colori: chiavi, testi, numeri.
fn riga_json(f: &mut Frame, riga: &str, x: f32, y: f32, w: f32) {
    let sz = 13.0;
    let cw = f.measure("M", sz, Weight::Mono);
    let indent = riga.chars().take_while(|c| *c == ' ').count();
    let mut px = x + indent as f32 * cw;
    let resto = riga.trim_start();
    let chiave = (255, 214, 140);
    let testo = LABS_LEAF;
    let numero = (150, 200, 255);
    let mut parti: Vec<(String, Rgb)> = Vec::new();
    let mut chars = resto.chars().peekable();
    let mut buf = String::new();
    while let Some(c) = chars.next() {
        if c == '"' {
            if !buf.is_empty() {
                parti.push((std::mem::take(&mut buf), LABS_MUTED));
            }
            let mut s = String::from('"');
            while let Some(d) = chars.next() {
                s.push(d);
                if d == '\\' {
                    if let Some(e) = chars.next() {
                        s.push(e);
                    }
                } else if d == '"' {
                    break;
                }
            }
            let e_chiave = chars.peek() == Some(&':');
            parti.push((s, if e_chiave { chiave } else { testo }));
        } else if c.is_ascii_digit() || c == '-' {
            if !buf.is_empty() {
                parti.push((std::mem::take(&mut buf), LABS_MUTED));
            }
            let mut s = String::from(c);
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() || matches!(d, '.' | 'e' | 'E' | '+' | '-') {
                    s.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            parti.push((s, numero));
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        parti.push((buf, LABS_MUTED));
    }
    for (t, c) in parti {
        let tw = f.measure(&d(&t), sz, Weight::Mono);
        if px > x + w {
            break;
        }
        f.p.text(&d(&t), px, y, tw + 4.0, MONO_H, sz, Weight::Mono, c, Align::Left);
        px += tw;
    }
}

// ───────────────────────── il pannello ─────────────────────────

/// Gli strumenti tra cui scegliere a mano, per il menu «Cambia strumento».
pub fn strumenti_profili(app: &App) -> (Vec<(String, Act)>, Option<usize>) {
    let file = app.strum_corrente().map(|(i, _)| &app.files[i]);
    let ext = file.map(|f| crate::engine::extension(&f.path)).unwrap_or_default();
    let attuale = file.map(|f| app.strum_file(&f.path).profilo).unwrap_or_default();
    let mut items: Vec<(String, Act)> = vec![("Quello che Cedro riconosce".to_string(), a(StrumAct::Profilo(String::new())))];
    let mut sel = if attuale.is_empty() { Some(0) } else { None };
    for p in strumenti::profili().iter().filter(|p| ext.is_empty() || p.estensioni.contains(&ext.as_str())) {
        if p.id == attuale {
            sel = Some(items.len());
        }
        items.push((d(p.nome), a(StrumAct::Profilo(p.id.to_string()))));
    }
    (items, sel)
}

/// La scatola «strumento riconosciuto»: il nome, che cosa si è letto, e il modo di cambiare strumento.
fn riconosciuto(f: &mut Frame, app: &App, path: &Path, l: &Lettura, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let pf = app.strum_file(path);
    let (colore, titolo, riga): (Rgb, String, String) = if pf.rileggendo {
        (pal.muted, "Rileggo il file…".to_string(), String::new())
    } else if let Some(Err(e)) = &pf.riletto {
        (pal.bad, "Con questo strumento il file non si legge.".to_string(), e.clone())
    } else {
        (pal.ok, d(&scelte::nome_strumento(l)), d(&scelte::riga_lettura_ui(l)))
    };
    let manuale = !pf.profilo.is_empty();
    let inner_w = w - 28.0;
    // L'altezza dipende dal testo: si misura prima (anche il titolo, che per un errore è lungo, va a capo).
    let titolo_righe = f.p.wrap_lines(&titolo, 16.5, BOLD, w - 56.0, 2).len().max(1);
    let titolo_h = 6.0 + titolo_righe as f32 * 22.0;
    let lines = if riga.is_empty() { 0 } else { f.p.wrap_lines(&riga, 14.0, SANS, inner_w, 4).len() };
    let link_h = 24.0;
    let h = 12.0 + titolo_h + lines as f32 * 19.0 + 6.0 + link_h * if manuale { 2.0 } else { 1.0 } + 10.0;
    f.p.rrect(x, y, w, h, 12.0, Some(mix(colore, pal.surface, 0.9)), Some((mix(colore, pal.surface, 0.5), 1.5)));
    let mut cy = y + 12.0;
    if pf.rileggendo {
        f.spinner(x + 24.0, cy + 13.0, 9.0, pal.muted);
        f.animating = true;
    } else {
        f.p.disc_a(x + 24.0, cy + 13.0, 11.0, colore, 0.18);
        f.icon(if colore == pal.ok { ic::CHECK } else { ic::CLOSE }, x + 15.0, cy + 4.0, 18.0, colore);
    }
    f.para(&titolo, x + 44.0, cy + 2.0, w - 56.0, 16.5, BOLD, pal.text, 1.3, 2);
    cy += titolo_h;
    if !riga.is_empty() {
        cy += f.para(&riga, x + 14.0, cy, inner_w, 14.0, SANS, pal.muted, 1.35, 4) + 6.0;
    }
    let link = |f: &mut Frame, t: &str, act: Act, cy: f32| {
        let t = t.to_string();
        let lw = f.measure(&t, 14.5, SEMI);
        let hot = f.hovered(&act);
        f.p.text(&t, x + 14.0, cy, lw + 4.0, link_h, 14.5, SEMI, if hot { pal.text } else { pal.primary }, Align::Left);
        f.p.fill(x + 14.0, cy + 19.0, lw, 1.0, if hot { pal.text } else { pal.primary });
        f.focus_ring((x + 10.0, cy, lw + 8.0, link_h), 6.0, &act);
        f.hit((x + 10.0, cy, lw + 8.0, link_h), act);
    };
    if manuale {
        link(f, "Torna a quello che Cedro riconosce", a(StrumAct::Profilo(String::new())), cy);
        cy += link_h;
    }
    link(f, "Non è questo? Cambia strumento", a(StrumAct::CambiaStrumento), cy);
    h
}

/// Cosa è uscito dall'ultimo «Converti»: le note per chi ha convertito.
fn esito(f: &mut Frame, app: &App, i: usize, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let path = &app.files[i].path;
    // Le note valgono per l'ultimo «Converti»: se dopo si è cambiato qualcosa, non si mostrano più.
    if app.files[i].state != FileState::Done || !app.is_fresh_done(i) {
        return 0.0;
    }
    let Some(e) = app.strum_esiti.get(path) else { return 0.0 };
    let mut cy = y;
    for n in &e.note {
        let c = if n.attenzione { AMBRA } else { pal.ok };
        let lines = f.p.wrap_lines(&n.testo, 14.0, SANS, w - 28.0, 9).len();
        let h = 12.0 + 24.0 + lines as f32 * 19.0 + 10.0;
        f.p.rrect(x, cy, w, h, 12.0, Some(mix(c, pal.surface, 0.9)), Some((mix(c, pal.surface, 0.55), 1.2)));
        f.text(&n.titolo, x + 14.0, cy + 10.0, w - 28.0, 24.0, 15.0, BOLD, pal.text);
        f.para(&n.testo, x + 14.0, cy + 34.0, w - 28.0, 14.0, SANS, pal.muted, 1.35, 9);
        cy += h + 10.0;
    }
    cy - y
}

fn riga_spunta(f: &mut Frame, x: f32, y: f32, w: f32, label: &str, hint: &str, on: bool, act: Act) -> f32 {
    let pal = f.pal;
    // Il commento può andare a capo (al massimo due righe) invece di tagliarsi.
    let righe = if hint.is_empty() { 0 } else { f.p.wrap_lines(hint, 14.0, SANS, w - 34.0, 2).len().max(1) };
    let h = if righe == 0 { 30.0 } else { 30.0 + 20.0 * righe as f32 };
    f.checkbox(x, y + 3.0, 22.0, on, None);
    f.text(label, x + 34.0, y, w - 34.0, 28.0, 15.5, SEMI, pal.text);
    if righe > 0 {
        f.para(hint, x + 34.0, y + 27.0, w - 34.0, 14.0, SANS, pal.muted, 1.4, 2);
    }
    let r = (x - 4.0, y - 2.0, w + 8.0, h + 2.0);
    f.focus_ring(r, 8.0, &act);
    f.hit(r, act);
    h + 6.0
}

/// Due o tre scelte esclusive in fila (come «Semplice | Completa»).
fn segmento(f: &mut Frame, r: R, voci: &[(&str, bool, Act)]) {
    let pal = f.pal;
    f.p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(pal.alt), None);
    let w = (r.2 - 6.0) / voci.len() as f32;
    for (k, (label, on, act)) in voci.iter().enumerate() {
        let b = (r.0 + 3.0 + k as f32 * w, r.1 + 3.0, w, r.3 - 6.0);
        if *on {
            f.p.shadow(b.0, b.1, b.2, b.3, 8.0, 4.0, 0.14, 1.0);
            f.p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(pal.surface), None);
        }
        let hot = !*on && f.hovered(act);
        // Se la scritta non ci sta, si stringe un po' invece di toccare il bordo.
        let size = [14.5f32, 13.5, 12.5].into_iter().find(|&sz| f.measure(label, sz, SEMI) <= b.2 - 14.0).unwrap_or(12.0);
        f.text_c(label, b, size, SEMI, if *on || hot { pal.text } else { pal.muted });
        f.focus_ring(b, 8.0, act);
        f.hit(b, act.clone());
    }
}

/// Il corpo del pannello di destra con il file di uno strumento: lo strumento, le note, e cosa esce.
pub fn side_body(f: &mut Frame, app: &App, i: usize, l: &Lettura, body: R, x: f32, w: f32, deep: Rgb) {
    let pal = f.pal;
    let path = app.files[i].path.clone();
    let sc = app.strum_scelte_per(&path);
    // La colonna delle etichette («Scala colori», «Valori»…) è larga quanto serve, così i comandi accanto hanno più posto.
    let col = ["Scala colori", "Valori", "Schema", "Percorso"].iter().map(|t| f.measure(t, 15.0, SEMI)).fold(0.0f32, f32::max) + 16.0;
    super::scrolled(f, body, Scroll::Inspector, |f| {
        let mut cy = body.1 + 2.0;
        cy += riconosciuto(f, app, &path, l, x, cy, w) + 12.0;
        let nota = esito(f, app, i, x, cy, w);
        if nota > 0.0 {
            cy += nota + 4.0;
        }
        // Se con lo strumento scelto il file non si legge (o si sta rileggendo), niente scelte di uscita: il resto è del pannello vecchio.
        let pf = app.strum_file(&path);
        if pf.rileggendo || matches!(pf.riletto, Some(Err(_))) {
            return cy - body.1 + 18.0;
        }
        if app.simple_view {
            let scelte_pronte: [(Pronta, &str, &str, &str); 3] = [
                (Pronta::Excel, "xlsx", "Excel", if l.a_piastra() { "mappa a colori e riga per pozzetto" } else { "numeri veri, una scheda per strumento" }),
                (Pronta::Csv, "csv", "CSV", if l.a_piastra() { "una riga per pozzetto" } else { "tabella per Excel" }),
                (Pronta::Allotrope, "json", "Allotrope", "per LIMS e archivi dati"),
            ];
            for (p, id, label, hint) in scelte_pronte {
                let v = app.voce(id);
                let v = crate::ui::voci::Voce { label: v.label, hint: v.hint, glyph: v.glyph, color: v.color };
                super::labs::choice_row(f, (x, cy, w, 62.0), &v, label, hint, app.strum.pronta == p, a(StrumAct::Pronta(p)), deep);
                cy += 70.0;
            }
            cy += f.para("Le copie stanno accanto agli originali, con le impostazioni consigliate.", x, cy + 4.0, w, 14.5, SANS, pal.muted, 1.4, 3) + 12.0;
        } else {
            f.text("Esce in", x, cy, w, 24.0, 15.0, SEMI, pal.muted);
            cy += 30.0;
            let excel_hint = if l.a_piastra() { "Mappa, Pozzetti, Strumento" } else { "elenco e foglio Strumento" };
            let csv_hint = if l.a_piastra() { "una riga per pozzetto" } else { "la tabella dei campioni" };
            cy += riga_spunta(f, x, cy, w, "Excel", excel_hint, sc.excel, a(StrumAct::Esce(Uscita::Excel)));
            cy += riga_spunta(f, x, cy, w, "CSV", csv_hint, sc.csv, a(StrumAct::Esce(Uscita::Csv)));
            cy += riga_spunta(f, x, cy, w, "Allotrope (.asm.json)", "per LIMS e archivi dati", sc.asm, a(StrumAct::Esce(Uscita::Allotrope)));
            if sc.asm {
                cy += riga_spunta(f, x, cy, w, "CSV piatto", "una riga per misura", sc.piatto, a(StrumAct::Esce(Uscita::Piatto)));
            }
            f.hline(x, cy + 4.0, w);
            cy += 18.0;
            if l.a_piastra() {
                f.text("Piastra", x, cy, w, 24.0, 15.0, SEMI, pal.muted);
                cy += 30.0;
                f.text("Scala colori", x, cy, col, 38.0, 15.0, SEMI, pal.text);
                segmento(f, (x + col, cy, w - col, 38.0), &[("Blu → rosso", !sc.daltonici, a(StrumAct::Daltonici(false))), ("Daltonici", sc.daltonici, a(StrumAct::Daltonici(true)))]);
                cy += 48.0;
                let ci_sono_bianchi = l.gruppi.iter().enumerate().any(|(g, _)| excel::media_bianchi(l, g, &sc.ruoli_veri()).is_some());
                if ci_sono_bianchi {
                    f.text("Valori", x, cy, col, 38.0, 15.0, SEMI, pal.text);
                    segmento(f, (x + col, cy, w - col, 38.0), &[("Grezzi", !sc.meno_bianco, a(StrumAct::MenoBianco(false))), ("Meno il bianco", sc.meno_bianco, a(StrumAct::MenoBianco(true)))]);
                    cy += 48.0;
                }
                cy += riassunto_ruoli(f, l, &sc, x, cy, w) + 10.0;
            }
            if sc.asm {
                f.text("Allotrope", x, cy, w, 24.0, 15.0, SEMI, pal.muted);
                cy += 30.0;
                f.text("Schema", x, cy, col, 30.0, 15.0, SEMI, pal.text);
                f.text(&d(&asm::schema_breve(l.tecnica)), x + col, cy, w - col, 30.0, 14.5, SANS, pal.muted);
                cy += 38.0;
                f.text("Percorso", x, cy, col, 38.0, 15.0, SEMI, pal.text);
                segmento(f, (x + col, cy, w - col, 38.0), &[("Solo il nome", !sc.asm_percorso, a(StrumAct::Percorso(false))), ("Completo", sc.asm_percorso, a(StrumAct::Percorso(true)))]);
                cy += 48.0;
                cy += riga_spunta(f, x, cy, w, "Aggiungi i calcoli di Cedro", "separati dai dati grezzi", sc.asm_calcoli, a(StrumAct::Calcoli));
                cy += riga_spunta(f, x, cy, w, "Controlla il file prima di salvarlo", "", sc.asm_controlla, a(StrumAct::Controlla));
                f.hline(x, cy + 2.0, w);
                cy += 16.0;
            }
            f.text("Salvataggio", x, cy, w, 24.0, 15.0, SEMI, pal.muted);
            cy += 32.0;
            f.text("Cartella", x, cy, 110.0, 42.0, 15.0, SEMI, pal.text);
            super::dest_select(f, app, (x + 116.0, cy, w - 116.0, 42.0), 15.0);
            cy += 48.0;
            cy += f.para(&super::dest_note(app), x, cy, w, 14.0, SANS, pal.muted, 1.4, 3) + 14.0;
            f.text("Se esiste già", x, cy, 110.0, 42.0, 15.0, SEMI, pal.text);
            super::exists_select(f, app, (x + 116.0, cy, w - 116.0, 42.0), 15.0);
            cy += 48.0;
            cy += f.para(&app.exists_example(), x, cy, w, 14.0, SANS, pal.muted, 1.4, 2);
        }
        cy - body.1 + 18.0
    });
}

/// «2 bianchi · 6 standard · 87 campioni. I ruoli si cambiano sulla piastra.»
fn riassunto_ruoli(f: &mut Frame, l: &Lettura, sc: &Scelte, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let ruoli = sc.ruoli_veri();
    let mut conta = [0usize; 4];
    let mut totale = 0usize;
    for m in l.misure().filter(|m| !m.e_spettro() && m.pozzetto.is_some()) {
        totale += 1;
        match strumenti::ruolo_di(m, &ruoli) {
            Ruolo::Bianco => conta[0] += 1,
            Ruolo::Standard => conta[1] += 1,
            Ruolo::Controllo | Ruolo::ControlloPositivo | Ruolo::ControlloNegativo => conta[2] += 1,
            Ruolo::Riferimento => conta[3] += 1,
            Ruolo::Campione => {}
        }
    }
    let mut p: Vec<String> = Vec::new();
    for (n, (uno, molti)) in conta.iter().zip([("bianco", "bianchi"), ("standard", "standard"), ("controllo", "controlli"), ("riferimento", "riferimenti")]) {
        if *n > 0 {
            p.push(tradotto(&format!("{n} {}", if *n == 1 { uno } else { molti })));
        }
    }
    let campioni = totale - conta.iter().sum::<usize>();
    p.push(tradotto(&if campioni == 1 { "1 campione".to_string() } else { format!("{campioni} campioni") }));
    let riga = d(&format!("{}. {}", p.join(" · "), tradotto("I ruoli si cambiano sulla piastra.")));
    f.para(&riga, x, y, w, 14.0, SANS, pal.muted, 1.4, 4)
}

/// I dati chiusi: la mappa dei Dato per chi vuole sapere se un valore manca.
#[allow(dead_code)]
fn manca(d: &Dato) -> bool {
    matches!(d, Dato::Assente(_))
}
