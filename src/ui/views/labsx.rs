//! Cedro 4 Labs, una finestra sola: in alto i comandi del laboratorio con il loro tasto, a sinistra i file e i
//! casi, al centro una tabella sola con tutti i file (di ogni tipo), a destra un pannello che fa una cosa
//! alla volta (i file scelti, oppure il comando scelto, oppure le procedure), in basso il registro di oggi.
//! Una selezione sola, come in Esplora file. Il colore serve solo a riconoscere tipo di file e comando.

use super::labs::{choice_row, choices, go_button, lab_icon, OPT_LABS};
use super::*;
use crate::ui::core::Scroll;
use crate::ui::datipers::Stato;
use crate::ui::gfx::Rgb;
use crate::ui::labsx::{Pannello, Sha, StatoFil, Tipo};
use crate::ui::model::{FileState, Lab};
use crate::ui::theme::{lab_colors, LABS_FRAME, LABS_INK, LABS_LINE, LABS_MUTED, LABS_STAGE};

const TOP: f32 = 64.0;
const CMD: f32 = 68.0;
const LEFT: f32 = 244.0;
const PANEL: f32 = 392.0;
const CHIUSO: f32 = 46.0;
const STATUS: f32 = 34.0;
const LOG_HEAD: f32 = 42.0;
const LOG_ROW: f32 = 44.0;
const WHITE: Rgb = (255, 255, 255);
/// Il colore dei dati personali trovati, e quello di «da controllare».
const ARANCIO: Rgb = (0xB4, 0x53, 0x09);
const BLU: Rgb = (0x1F, 0x5F, 0xA8);

/// Il comando «Procedure» usa il colore di «Firma»: l'indaco, che nessun altro comando del laboratorio ha.
const PROCEDURE: Lab = Lab::Firma;

/// Un testo già tradotto qui, segnato perché chi lo disegna non lo traduca una seconda volta.
fn tt(s: &str) -> String {
    crate::i18n::lit(&crate::i18n::tr_string(s))
}

pub fn draw(f: &mut Frame, app: &App) {
    super::labs::title_bar(f, app);
    comandi(f, app, (0.0, TOP, f.w, CMD));
    // Una pagina intera (firma, trascrizione, registro) prende anche il posto del registro in basso.
    let intera = app.lavori_aperti || app.lx_pagina();
    let log_h = if intera { 0.0 } else { registro_h(app) };
    let body = (0.0, TOP + CMD, f.w, f.h - TOP - CMD - log_h - STATUS);
    corpo(f, app, body);
    if !intera {
        registro(f, app, (0.0, body.1 + body.3, f.w, log_h));
    }
    stato(f, app, (0.0, f.h - STATUS, f.w, STATUS));
    if app.lx.anteprima {
        anteprima(f, app);
    }
    if app.lx.scheda {
        scheda(f, app);
    }
    if app.lx.errore.is_some() {
        errore(f, app);
    }
    if app.lx.proc_nome.is_some() {
        proc_nome(f, app);
    }
    if app.lx.aiuto {
        aiuto(f);
    }
}


// ------------------------------------------------------------------ i comandi

fn nome_comando(lab: Lab) -> String {
    tt(match lab {
        Lab::Immagini => "Immagini e vetrini",
        Lab::Anonimizza => "Togli i nomi",
        Lab::Figura => "Figura",
        Lab::Dati => "Dati e piastre",
        Lab::Referti => "Firma e referti",
        Lab::Trascrivi => "Trascrivi",
        Lab::Converti => "Converti",
        Lab::Pdf => "PDF",
        other => other.name(),
    })
}

fn tasto_di(lab: Lab) -> &'static str {
    match lab {
        Lab::Converti => "7",
        Lab::Pdf => "8",
        _ => ["1", "2", "3", "4", "5", "6"][Lab::LAB.iter().position(|l| *l == lab).unwrap_or(0)],
    }
}

/// Una lettera per riconoscere il comando nelle caselline di «Già fatto».
fn lettera(lab: Lab) -> &'static str {
    match lab {
        Lab::Immagini => "I",
        Lab::Anonimizza => "T",
        Lab::Figura => "F",
        Lab::Dati => "D",
        Lab::Referti => "R",
        Lab::Trascrivi => "T",
        Lab::Converti => "C",
        Lab::Pdf => "P",
        _ => "·",
    }
}

/// Un nome lungo su due righe, spezzato allo spazio più vicino al mezzo («Imágenes y / preparaciones»).
fn due_righe(nome: &str) -> (String, String) {
    // Il nome arriva già tradotto e segnato: si spezza il testo vero e i due pezzi si segnano di nuovo.
    let (a, b) = due_righe_vere(&crate::i18n::tr(nome));
    (crate::i18n::lit(&a), if b.is_empty() { b } else { crate::i18n::lit(&b) })
}

fn due_righe_vere(nome: &str) -> (String, String) {
    let n = nome.chars().count();
    let meta = n / 2;
    let spazi: Vec<usize> = nome.char_indices().filter(|(_, c)| *c == ' ').map(|(i, _)| i).collect();
    let Some(&i) = spazi.iter().min_by_key(|&&i| (nome[..i].chars().count() as i64 - meta as i64).abs()) else {
        return (nome.to_string(), String::new());
    };
    if n < 12 {
        return (nome.to_string(), String::new());
    }
    (nome[..i].to_string(), nome[i + 1..].to_string())
}

#[derive(Clone, Copy, PartialEq)]
enum Cmd {
    Lab(Lab),
    Procedure,
    Sep,
}

/// La barra dei comandi: ogni comando ha il suo colore e il suo tasto. Il numero sul comando compare solo
/// quando prende una parte dei file scelti (quanti); quello che non ne prende nessuno si spegne un po'.
fn comandi(f: &mut Frame, app: &App, r: R) {
    f.p.fill(r.0, r.1, r.2, r.3, LABS_FRAME);
    f.p.fill(r.0, r.1 + r.3 - 1.0, r.2, 1.0, LABS_LINE);
    let mut voci: Vec<Cmd> = Lab::LAB.iter().map(|l| Cmd::Lab(*l)).collect();
    voci.push(Cmd::Sep);
    voci.extend(Lab::STANDARD.iter().map(|l| Cmd::Lab(*l)));
    voci.push(Cmd::Sep);
    voci.push(Cmd::Procedure);
    let nome = |c: Cmd| match c {
        Cmd::Lab(l) => nome_comando(l),
        Cmd::Procedure => tt("Procedure"),
        Cmd::Sep => String::new(),
    };
    let scelti = app.lx_scelti();
    let e_acceso = |c: Cmd| match c {
        Cmd::Lab(l) => app.lx.pannello == Pannello::Azione && app.lab == l,
        Cmd::Procedure => app.lx.pannello == Pannello::Procedure,
        Cmd::Sep => false,
    };
    // Quattro misure: nome e tasto per tutti; solo il nome; il nome su due righe (lingue con nomi lunghi);
    // se ancora non basta il nome resta solo al comando aperto e gli altri tengono colore, numero e tasto.
    let misura = |c: Cmd, hint: bool, due: bool, solo_icone: bool, f: &Frame| -> f32 {
        match c {
            Cmd::Sep => 16.0,
            c if solo_icone && !e_acceso(c) => 8.0 + 34.0 + 8.0,
            c => {
                let lw = if due { let (a, b) = due_righe(&nome(c)); f.measure(&a, 13.5, SEMI).max(f.measure(&b, 13.5, SEMI)) } else { f.measure(&nome(c), 15.0, SEMI) };
                8.0 + 34.0 + 9.0 + lw + 8.0 + if hint { 30.0 } else { 0.0 }
            }
        }
    };
    let largo = |hint: bool, due: bool, solo_icone: bool, f: &Frame| -> f32 { voci.iter().map(|c| misura(*c, hint, due, solo_icone, f)).sum::<f32>() + 24.0 };
    let hint = largo(true, false, false, f) <= r.2;
    let due = !hint && largo(false, false, false, f) > r.2;
    let solo_icone = due && largo(false, true, false, f) > r.2;
    let extra = (r.2 - largo(hint, due, solo_icone, f)).max(0.0);
    let gap = (extra / (voci.len() as f32 - 1.0)).min(14.0);
    let mut x = 14.0;
    let (y, h) = (r.1 + 7.0, r.3 - 14.0);
    for c in voci {
        if c == Cmd::Sep {
            f.p.fill(x + 7.0, r.1 + 14.0, 1.0, r.3 - 28.0, LABS_LINE);
            x += 16.0 + gap;
            continue;
        }
        let (lab, act, tasto, acceso, n) = match c {
            Cmd::Lab(l) => (l, Act::Lab(l), tasto_di(l), app.lx.pannello == Pannello::Azione && app.lab == l, if scelti.is_empty() { None } else { Some(app.lx_per_comando(l)) }),
            _ => (PROCEDURE, Act::LxProcedure, "9", app.lx.pannello == Pannello::Procedure, None),
        };
        let nm = nome(c);
        let lw = f.measure(&nm, 15.0, SEMI);
        let icona_sola = solo_icone && !acceso;
        let w = misura(c, hint, due, solo_icone, f);
        let item = (x, y, w, h);
        let spento = n == Some(0) && !acceso;
        let (deep, bright) = lab_colors(lab);
        if acceso {
            f.p.rrect(item.0, item.1, item.2, item.3, 11.0, Some(mix(bright, LABS_FRAME, 0.78)), Some((mix(bright, LABS_FRAME, 0.5), 1.0)));
        } else {
            let t = f.hover_t(&act, true);
            if t > 0.01 {
                f.p.rrect_a(item.0, item.1, item.2, item.3, 11.0, WHITE, 0.07 * t);
            }
        }
        let (tx, ty) = (item.0 + 8.0, item.1 + (item.3 - 34.0) / 2.0);
        let icona = if c == Cmd::Procedure { ic::MENU } else { lab_icon(lab) };
        if spento {
            f.p.rrect(tx, ty, 34.0, 34.0, 9.0, Some(mix(deep, LABS_FRAME, 0.62)), None);
            f.icon(icona, tx + 6.0, ty + 6.0, 22.0, mix(WHITE, LABS_FRAME, 0.55));
        } else {
            f.p.rrect_vgrad(tx, ty, 34.0, 34.0, 9.0, bright, deep);
            f.p.rrect_line(tx, ty, 34.0, 34.0, 9.0, WHITE, 1.0, 0.22, None);
            f.icon(icona, tx + 6.0, ty + 6.0, 22.0, WHITE);
        }
        if let Some(n) = n.filter(|n| *n > 0 && *n < scelti.len()) {
            let s = n.to_string();
            let bw = (f.measure(&s, 12.0, BOLD) + 10.0).max(20.0);
            f.p.rrect(tx + 34.0 - bw + 6.0, ty - 7.0, bw, 20.0, 10.0, Some(LABS_STAGE), Some((WHITE, 1.5)));
            f.p.text(&s, tx + 34.0 - bw + 6.0, ty - 7.0, bw, 20.0, 12.0, BOLD, WHITE, Align::Center);
        }
        let col = if spento { mix(LABS_INK, LABS_FRAME, 0.55) } else { LABS_INK };
        if icona_sola {
        } else if due {
            let (a, b) = due_righe(&nm);
            let cy = item.1 + item.3 / 2.0;
            if b.is_empty() {
                f.p.text(&a, tx + 43.0, item.1, w - 51.0, item.3, 13.5, SEMI, col, Align::Left);
            } else {
                f.p.text(&a, tx + 43.0, cy - 17.0, w - 51.0, 17.0, 13.5, SEMI, col, Align::Left);
                f.p.text(&b, tx + 43.0, cy, w - 51.0, 17.0, 13.5, SEMI, col, Align::Left);
            }
        } else {
            f.p.text(&nm, tx + 43.0, item.1, lw + 4.0, item.3, 15.0, SEMI, col, Align::Left);
        }
        if hint && !icona_sola {
            let kx = tx + 43.0 + lw + 8.0;
            f.p.rrect(kx, item.1 + (item.3 - 22.0) / 2.0, 22.0, 22.0, 5.0, None, Some((LABS_LINE, 1.0)));
            f.p.text(tasto, kx, item.1 + (item.3 - 22.0) / 2.0, 22.0, 22.0, 12.0, SEMI, LABS_MUTED, Align::Center);
        }
        f.focus_ring(item, 11.0, &act);
        f.hit(item, act);
        x += w + gap;
    }
}

// ------------------------------------------------------------------ il corpo

fn corpo(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.work);
    lato(f, app, (r.0, r.1, LEFT, r.3));
    let area = (r.0 + LEFT, r.1, r.2 - LEFT, r.3);
    if app.lavori_aperti || app.lx_pagina() {
        // Una pagina intera: sopra, «Torna all'elenco».
        let back = if app.lavori_aperti { Act::LavoriApri } else { Act::LxPannello(Pannello::Dettagli) };
        torna(f, (area.0 + 22.0, area.1 + 14.0), back);
        let inner = (area.0 + 12.0, area.1 + 54.0, area.2 - 24.0, area.3 - 64.0);
        if app.lavori_aperti {
            super::labs::lavori_page(f, app, inner);
        } else {
            super::labs::page(f, app, inner);
        }
        return;
    }
    if app.files.is_empty() {
        vuoto(f, app, area);
        return;
    }
    let pw = if app.lx.panel_chiuso { CHIUSO } else { PANEL };
    let c = (area.0, area.1, area.2 - pw, area.3);
    match app.strum_corrente().filter(|_| app.lx.pannello == Pannello::Azione) {
        // L'export di uno strumento: sopra l'elenco, sotto la sua piastra (o la tabella dei campioni) da segnare.
        Some((i, l)) => {
            let th = (c.3 * 0.34).clamp(150.0, 300.0).floor();
            centro(f, app, (c.0, c.1, c.2, th));
            piastra(f, app, i, &l, (c.0, c.1 + th, c.2, c.3 - th));
        }
        None => centro(f, app, c),
    }
    pannello(f, app, (area.0 + area.2 - pw, area.1, pw, area.3));
}

/// La piastra del file di uno strumento, nello spazio scuro con la tinta del comando.
fn piastra(f: &mut Frame, app: &App, i: usize, l: &std::sync::Arc<crate::strumenti::Lettura>, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.surface);
    f.p.fill(r.0, r.1, r.2, 1.0, pal.border);
    let s = (r.0 + 12.0, r.1 + 12.0, r.2 - 24.0, r.3 - 24.0);
    let (_, bright) = lab_colors(app.lab);
    f.p.rrect(s.0, s.1, s.2, s.3, 14.0, Some(LABS_STAGE), None);
    f.p.rrect_vgrad(s.0, s.1, s.2, s.3, 14.0, mix(bright, LABS_STAGE, 0.84), mix(bright, LABS_STAGE, 0.95));
    super::strumenti::stage(f, app, i, l, (s.0 + 16.0, s.1 + 14.0, s.2 - 32.0, s.3 - 28.0));
}

/// «Torna all'elenco», con Esc.
fn torna(f: &mut Frame, p: (f32, f32), act: Act) {
    let label = tt("Torna all'elenco");
    let w = 16.0 + 24.0 + f.measure(&label, 14.5, SEMI) + 10.0 + 38.0 + 14.0;
    pulsante_tasto(f, p.0, p.1, "Torna all'elenco", "Esc", Some(ic::ARROW_LEFT), act, true);
    let _ = w;
}

/// La tabella vuota: «Che file hai?», le sei tessere e i tre passi.
fn vuoto(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.surface);
    let (x, w) = (r.0 + 30.0, r.2 - 60.0);
    f.p.text(&tt("Che file hai?"), x, r.1 + 24.0, w, 40.0, 28.0, BOLD, pal.text, Align::Left);
    f.p.text(&tt("Trascinali qui dentro, oppure scegli il tipo. Tutto resta su questo computer."), x, r.1 + 64.0, w, 26.0, 15.5, SANS, pal.muted, Align::Left);
    let tessere: [(&str, &str, Lab); 6] = [
        ("Vetrino", "SVS · NDPI · MRXS", Lab::Immagini),
        ("Radiografia", "DICOM, anche da CD", Lab::Immagini),
        ("Microscopia", "CZI, ND2, LIF, TIFF a 16 bit", Lab::Immagini),
        ("Tabella", "CSV, Excel, piastre", Lab::Dati),
        ("Referto", "PDF, Word", Lab::Referti),
        ("Audio", "Dettato o riunione", Lab::Trascrivi),
    ];
    let gap = 14.0;
    let tw = ((w - 2.0 * gap) / 3.0).floor();
    let th = 70.0;
    let top = r.1 + 108.0;
    for (k, (nome, sub, lab)) in tessere.into_iter().enumerate() {
        let (deep, _) = lab_colors(lab);
        let (tx, ty) = (x + (k % 3) as f32 * (tw + gap), top + (k / 3) as f32 * (th + gap));
        let act = Act::LabGo(Lab::Home, sub);
        let t = f.hover_t(&act, true);
        f.p.rrect(tx, ty, tw, th, 12.0, Some(mix(pal.surface, deep, 0.04 + 0.06 * t)), Some((mix(pal.border, deep, 0.5), 1.5)));
        f.p.text(&tt(nome), tx + 18.0, ty + 10.0, tw - 36.0, 28.0, 18.0, BOLD, deep, Align::Left);
        f.p.text(&tt(sub), tx + 18.0, ty + 38.0, tw - 36.0, 22.0, 13.5, SANS, pal.muted, Align::Left);
        f.focus_ring((tx, ty, tw, th), 12.0, &act);
        f.hit((tx, ty, tw, th), act);
    }
    let sy = top + 2.0 * th + gap + 26.0;
    let passi = [("1 · Metti i file", "Trascinali, oppure «Aggiungi file»."), ("2 · Scegli il comando", "In alto, con il suo colore e il suo tasto."), ("3 · Esegui", "A destra, con poche scelte pronte.")];
    let pw = ((w - 2.0 * gap) / 3.0).floor();
    for (k, (t, s)) in passi.into_iter().enumerate() {
        let px = x + k as f32 * (pw + gap);
        f.p.rrect(px, sy, pw, 76.0, 10.0, Some(pal.alt), None);
        f.p.text(&tt(t), px + 16.0, sy + 10.0, pw - 32.0, 24.0, 15.0, BOLD, pal.text, Align::Left);
        f.para(&tt(s), px + 16.0, sy + 38.0, pw - 32.0, 14.0, SANS, pal.muted, 1.35, 2);
    }
    let _ = app;
}

// ------------------------------------------------------------------ a sinistra: i file e i casi

/// Una riga della colonna di sinistra: nome (e una riga sotto, se c'è) e quanti file.
fn riga_lato(f: &mut Frame, r: R, nome: &str, sotto: &str, n: usize, on: bool, act: Act, mono: bool) {
    let pal = f.pal;
    let t = f.hover_t(&act, !on);
    if on {
        f.p.rrect(r.0, r.1, r.2, r.3, 9.0, Some(pal.surface), Some((pal.primary, 1.5)));
    } else if t > 0.01 {
        f.p.rrect_a(r.0, r.1, r.2, r.3, 9.0, pal.alt, t);
    }
    let cw = f.measure(&n.to_string(), 14.0, SANS);
    let right = r.0 + r.2 - 12.0;
    let tw = (right - cw - 12.0 - (r.0 + 12.0)).max(20.0);
    if sotto.is_empty() {
        f.p.text(nome, r.0 + 12.0, r.1, tw, r.3, 14.5, if on || mono { SEMI } else { SANS }, pal.text, Align::Left);
    } else {
        f.p.text(nome, r.0 + 12.0, r.1 + 5.0, tw, 22.0, 14.5, BOLD, pal.text, Align::Left);
        f.p.text(sotto, r.0 + 12.0, r.1 + 26.0, tw, 18.0, 12.5, SANS, pal.muted, Align::Left);
    }
    f.p.text(&n.to_string(), right - cw - 2.0, r.1, cw + 4.0, if sotto.is_empty() { r.3 } else { 32.0 }, 14.0, SANS, pal.muted, Align::Right);
    f.focus_ring(r, 9.0, &act);
    f.hit(r, act);
}

/// A sinistra: «Tutti i file», «Senza caso» e i casi (con «+ Nuovo»). Scegliere un caso è lavorare su quel caso:
/// la tabella mostra i suoi file e quelli che si aggiungono ci entrano.
fn lato(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, mix(pal.work, pal.surface, 0.5));
    f.p.fill(r.0 + r.2 - 1.0, r.1, 1.0, r.3, pal.border);
    let (x, w) = (r.0 + 12.0, r.2 - 24.0);
    let foot = 106.0;
    let body = (r.0, r.1, r.2, r.3 - foot);
    let (_, senza) = app.lx_conta_casi();
    let casi = app.lx_casi_elenco();
    super::scrolled(f, body, Scroll::LxSide, |f| {
        let mut y = body.1 + 14.0;
        titolo_sezione(f, x, y, w, "File");
        y += 24.0;
        riga_lato(f, (x, y, w, 34.0), &tt("Tutti i file"), "", app.files.len(), app.lx.caso.is_none(), Act::LxFiltroCaso(None), false);
        y += 38.0;
        if !app.files.is_empty() {
            let on = app.lx.caso.as_deref() == Some("");
            riga_lato(f, (x, y, w, 34.0), &tt("Senza caso"), "", senza, on, Act::LxFiltroCaso(if on { None } else { Some(String::new()) }), false);
            y += 38.0;
        }
        y += 8.0;
        titolo_sezione(f, x, y, w - 90.0, "Casi");
        let nuovo = tt("Nuovo");
        let nw = f.measure(&nuovo, 14.0, SEMI) + 26.0;
        let b = (x + w - nw, y - 6.0, nw, 28.0);
        let act = Act::LxNuovoCaso;
        let t = f.hover_t(&act, true);
        f.hover_fill(b, 7.0, pal.alt, t);
        f.icon(ic::ADD, b.0 + 4.0, b.1 + 7.0, 14.0, pal.primary);
        f.p.text(&nuovo, b.0 + 20.0, b.1, nw - 20.0, b.3, 14.0, SEMI, pal.primary, Align::Left);
        f.focus_ring(b, 7.0, &act);
        f.hit(b, act);
        y += 28.0;
        if casi.is_empty() {
            y += f.para(&tt("Nessun caso ancora. «Nuovo» ne crea uno: i file che aggiungi ci entrano."), x + 4.0, y, w - 8.0, 13.0, SANS, pal.muted, 1.35, 4) + 6.0;
        }
        for (code, nota, n) in &casi {
            let on = app.lx.caso.as_deref() == Some(code.as_str());
            riga_lato(f, (x, y, w, 46.0), code, nota, *n, on, Act::LxFiltroCaso(if on { None } else { Some(code.clone()) }), true);
            y += 48.0;
        }
        y - body.1 + 10.0
    });
    f.p.fill(r.0, r.1 + r.3 - foot, r.2 - 1.0, 1.0, pal.border);
    f.button((x, r.1 + r.3 - foot + 14.0, w, 38.0), "Aggiungi file", Some(ic::ADD), 15.0, Act::AddFiles);
    f.button((x, r.1 + r.3 - foot + 58.0, w, 38.0), "Aggiungi cartella", Some(ic::FOLDER), 15.0, Act::AddFolder);
}

// ------------------------------------------------------------------ la tabella

/// La larghezza di un pulsante della barra della tabella (nome e tasto).
fn larghezza_pulsante(f: &Frame, label: &str, tasto: &str, icon: bool) -> f32 {
    let label = tt(label);
    let lw = f.measure(&label, 14.5, SEMI);
    let kw = if tasto.is_empty() { 0.0 } else { f.measure(tasto, 12.0, SEMI).max(10.0) + 14.0 };
    14.0 + if icon { 26.0 } else { 0.0 } + lw + if kw > 0.0 { 10.0 + kw } else { 0.0 } + 14.0
}

/// Il pulsante della barra della tabella: nome e tasto. Spento non risponde.
fn pulsante_tasto(f: &mut Frame, x: f32, y: f32, label: &str, tasto: &str, icon: Option<&str>, act: Act, on: bool) -> f32 {
    let pal = f.pal;
    let w = larghezza_pulsante(f, label, tasto, icon.is_some());
    let label = tt(label);
    let lw = f.measure(&label, 14.5, SEMI);
    let kw = if tasto.is_empty() { 0.0 } else { f.measure(tasto, 12.0, SEMI).max(10.0) + 14.0 };
    let iw = if icon.is_some() { 26.0 } else { 0.0 };
    let r = (x, y, w, 34.0);
    let t = f.hover_t(&act, on);
    f.p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(mix(pal.surface, pal.alt, t)), Some((pal.border, 1.0)));
    let c = if on { pal.text } else { pal.muted };
    let mut tx = r.0 + 14.0;
    if let Some(i) = icon {
        f.icon(i, tx, r.1 + 8.0, 18.0, c);
        tx += iw;
    }
    f.p.text(&label, tx, r.1, lw + 2.0, r.3, 14.5, SEMI, c, Align::Left);
    if kw > 0.0 {
        let kx = tx + lw + 10.0;
        f.p.rrect(kx, r.1 + 7.0, kw, 20.0, 5.0, Some(pal.alt), Some((pal.border, 1.0)));
        f.p.text(tasto, kx, r.1 + 7.0, kw, 20.0, 12.0, SEMI, pal.muted, Align::Center);
    }
    f.focus_ring(r, 8.0, &act);
    if on {
        f.hit(r, act);
    }
    w
}

/// Una pillola di filtro.
fn pillola(f: &mut Frame, x: f32, y: f32, label: &str, on: bool, act: Act, colore: Option<Rgb>) -> f32 {
    let pal = f.pal;
    let label = tt(label);
    let tw = f.measure(&label, 13.5, SEMI);
    let dot = if colore.is_some() { 16.0 } else { 0.0 };
    let w = tw + 24.0 + dot;
    let r = (x, y, w, 30.0);
    let t = f.hover_t(&act, !on);
    if on {
        f.p.rrect(r.0, r.1, r.2, r.3, 15.0, Some(pal.text), None);
    } else {
        f.p.rrect(r.0, r.1, r.2, r.3, 15.0, Some(mix(pal.surface, pal.alt, t)), Some((pal.border, 1.0)));
    }
    let mut tx = r.0 + 12.0;
    if let Some(c) = colore {
        f.p.rrect(tx, r.1 + 10.0, 10.0, 10.0, 3.0, Some(c), None);
        tx += 16.0;
    }
    f.p.text(&label, tx, r.1, tw + 4.0, r.3, 13.5, SEMI, if on { pal.surface } else { pal.text }, Align::Left);
    f.focus_ring(r, 15.0, &act);
    f.hit(r, act);
    w
}

/// La barra sopra la tabella: cerca, i filtri (stato e tipi che ci sono) e, a destra, i file scelti e i
/// comandi sulla selezione. Torna l'altezza che ha preso.
fn barra(f: &mut Frame, app: &App, r: R) -> f32 {
    let pal = f.pal;
    let (x0, w) = (r.0 + 14.0, r.2 - 28.0);
    let (mut x, mut y) = (x0, r.1 + 10.0);
    let q = (x, y, 236.0, 30.0);
    f.p.rrect(q.0, q.1, q.2, q.3, 8.0, Some(pal.surface), Some((pal.border, 1.0)));
    f.icon(ic::SEARCH, q.0 + 9.0, q.1 + 6.0, 18.0, pal.muted);
    f.field((q.0 + 28.0, q.1, q.2 - 28.0, q.3), Field::LxCerca, &app.lx.ricerca, &tt("Cerca  ( / )"), 14.0, false);
    x += q.2 + 10.0;
    let nessuno = app.lx.stato.is_none() && app.lx.tipo.is_none();
    let (da_fare, fatti, errori) = app.lx_conta_stati();
    let mut voci: Vec<(String, bool, Act, Option<Rgb>)> = vec![(String::from("Tutti"), nessuno, Act::LxTutti, None)];
    for (label, n, s) in [("Da fare", da_fare, StatoFil::DaFare), ("Fatti", fatti, StatoFil::Fatti), ("Con errori", errori, StatoFil::Errori)] {
        if n == 0 && app.lx.stato != Some(s) {
            continue;
        }
        let on = app.lx.stato == Some(s);
        voci.push((label.to_string(), on, Act::LxStato(if on { None } else { Some(s) }), None));
    }
    for (t, _) in app.lx_conta_tipi() {
        let on = app.lx.tipo == Some(t);
        voci.push((t.nome().to_string(), on, Act::LxTipo(if on { None } else { Some(t) }), Some(t.colore())));
    }
    for (label, on, act, c) in voci {
        let w1 = f.measure(&tt(&label), 13.5, SEMI) + 24.0 + if c.is_some() { 16.0 } else { 0.0 };
        if x + w1 > x0 + w && x > x0 {
            x = x0;
            y += 38.0;
        }
        x += pillola(f, x, y, &label, on, act, c) + 7.0;
    }
    // A destra, sull'ultima riga: quanti file sono scelti e cosa se ne fa.
    let scelti = app.lx_scelti();
    let nota = if scelti.is_empty() {
        format!("{} {}", app.lx_righe().len(), tt("file"))
    } else {
        crate::i18n::lit(&app.lx_scelti_frase().replace(" selezionati", "").replace(" selezionato", "")) + " " + &tt(if scelti.len() == 1 { "scelto" } else { "scelti" })
    };
    let nota = if scelti.is_empty() { nota } else { nota.replace(" · ", " · ") };
    let nw = f.measure(&nota, 14.0, SEMI);
    let (bw1, bw2) = (larghezza_pulsante(f, "Caso", "C", true), larghezza_pulsante(f, "Togli", "Canc", false));
    let gw = nw + 14.0 + bw1 + 8.0 + bw2;
    if x + gw > x0 + w && x > x0 {
        y += 38.0;
    }
    let gx = x0 + w - gw;
    f.p.text(&nota, gx, y, nw + 4.0, 30.0, 14.0, SEMI, pal.text, Align::Left);
    let on = !scelti.is_empty();
    pulsante_tasto(f, gx + nw + 14.0, y - 2.0, "Caso", "C", Some(ic::LINK), Act::LxAssegna, on);
    pulsante_tasto(f, gx + nw + 14.0 + bw1 + 8.0, y - 2.0, "Togli", "Canc", None, Act::RemoveChecked, on && !app.busy());
    y + 40.0 - r.1
}

/// Le colonne della tabella: (x, larghezza) di tipo, nome, caso (se serve), contenuto, dati personali, già fatto.
struct Colonne {
    tipo: (f32, f32),
    nome: (f32, f32),
    caso: Option<(f32, f32)>,
    cont: (f32, f32),
    dati: (f32, f32),
    fatto: (f32, f32),
}

fn colonne(w: f32, con_caso: bool) -> Colonne {
    let fisse = 56.0 + 76.0 + if con_caso { 126.0 } else { 0.0 } + 116.0 + 132.0 + 12.0;
    let flex = (w - fisse).max(200.0);
    let nome = (flex * 0.46).floor();
    let cont = flex - nome;
    let mut x = 56.0;
    let mut presa = |cw: f32| {
        let c = (x, cw);
        x += cw;
        c
    };
    let tipo = presa(76.0);
    let nome = presa(nome);
    let caso = if con_caso { Some(presa(126.0)) } else { None };
    let cont = presa(cont);
    let dati = presa(116.0);
    let fatto = presa(132.0);
    Colonne { tipo, nome, caso, cont, dati, fatto }
}

fn centro(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.surface);
    let mut top = r.1;
    if let Some(code) = app.lx.caso.as_deref().filter(|c| !c.is_empty()) {
        top += fascia(f, app, (r.0, top, r.2, 0.0), code);
    }
    let bh = barra(f, app, (r.0, top, r.2, 0.0));
    let hy = top + bh;
    let righe = app.lx_righe();
    let col = colonne(r.2, app.lx.caso.as_deref().map_or(true, |c| c.is_empty()));
    f.p.fill(r.0, hy, r.2, 34.0, mix(pal.work, pal.surface, 0.4));
    f.p.fill(r.0, hy + 33.0, r.2, 1.0, pal.border);
    f.p.fill(r.0, hy, r.2, 1.0, pal.border);
    let tutti = !righe.is_empty() && righe.iter().all(|&i| app.files[i].checked);
    f.checkbox(r.0 + 18.0, hy + 8.0, 18.0, tutti, Some(Act::LxSceglitutti));
    let intest: [(&str, Option<(f32, f32)>); 6] = [("Tipo", Some(col.tipo)), ("Nome", Some(col.nome)), ("Caso", col.caso), ("Contenuto", Some(col.cont)), ("Dati personali", Some(col.dati)), ("Già fatto", Some(col.fatto))];
    for (nome, c) in intest {
        if let Some(c) = c {
            f.p.text(&tt(nome), r.0 + c.0, hy, c.1 - 6.0, 34.0, 13.0, SEMI, pal.muted, Align::Left);
        }
    }
    let rh = if app.lx.comode { 46.0 } else { 36.0 };
    let area = (r.0, hy + 34.0, r.2, r.1 + r.3 - hy - 34.0);
    if righe.is_empty() {
        f.text_c(&tt("Nessun file con questi filtri."), (area.0, area.1 + 30.0, area.2, 30.0), 15.5, SANS, pal.muted);
        return;
    }
    let off = f.ui.scroll(Scroll::LxTabella);
    super::scrolled(f, area, Scroll::LxTabella, |f| {
        let first = ((off / rh).floor() as usize).saturating_sub(1);
        let last = (((off + area.3) / rh).ceil() as usize + 1).min(righe.len());
        for (pos, &i) in righe.iter().enumerate().take(last).skip(first) {
            riga(f, app, i, (area.0, area.1 + pos as f32 * rh, area.2, rh), &col);
        }
        righe.len() as f32 * rh + 8.0
    });
}

/// La fascia del caso scelto: codice, nota, le due scorciatoie e le tappe da «Accettato» a «Consegnato».
/// Torna l'altezza che prende.
fn fascia(f: &mut Frame, app: &App, r: R, code: &str) -> f32 {
    let pal = f.pal;
    let h = 96.0;
    f.p.fill(r.0, r.1, r.2, h, mix(pal.work, pal.surface, 0.55));
    f.p.fill(r.0, r.1 + h - 1.0, r.2, 1.0, pal.border);
    let x = r.0 + 14.0;
    let (b1, b2) = (larghezza_pulsante(f, "Scheda del caso", "", false), larghezza_pulsante(f, "Pacchetto per consulto", "", false));
    let bx = r.0 + r.2 - 14.0 - b2;
    pulsante_tasto(f, bx, r.1 + 10.0, "Pacchetto per consulto", "", None, Act::LxPacchetto, true);
    pulsante_tasto(f, bx - 8.0 - b1, r.1 + 10.0, "Scheda del caso", "", None, Act::LxScheda, true);
    let cw = f.measure(code, 19.0, BOLD);
    f.p.text(code, x, r.1 + 8.0, cw + 4.0, 36.0, 19.0, BOLD, pal.text, Align::Left);
    let nota = app.lx_nota_caso(code);
    f.p.text(&nota, x + cw + 14.0, r.1 + 8.0, (bx - 8.0 - b1 - (x + cw + 14.0) - 12.0).max(40.0), 36.0, 14.0, SANS, pal.muted, Align::Left);
    let fatte = app.lx_tappe_di(code) as usize;
    let n = crate::caso::TAPPE.len();
    let gap = 6.0;
    let sw = ((r.2 - 28.0 - gap * (n as f32 - 1.0)) / n as f32).floor();
    for (k, nome) in crate::caso::TAPPE.iter().enumerate() {
        let b = (x + k as f32 * (sw + gap), r.1 + 56.0, sw, 28.0);
        let act = Act::LxTappa(k as u8);
        let (bg, fg) = if k < fatte {
            (mix(pal.ok, pal.surface, 0.86), pal.ok)
        } else if k == fatte {
            (pal.primary, pal.surface)
        } else {
            (pal.alt, pal.muted)
        };
        let t = f.hover_t(&act, true);
        f.p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(mix(bg, pal.text, 0.06 * t)), None);
        let label = tt(nome);
        let lw = f.measure(&label, 13.0, SEMI);
        let total = lw + if k < fatte { 20.0 } else { 0.0 };
        let sx = b.0 + (b.2 - total) / 2.0;
        if k < fatte {
            f.icon(ic::CHECK, sx, b.1 + 7.0, 14.0, fg);
        }
        f.p.text(&label, sx + if k < fatte { 20.0 } else { 0.0 }, b.1, lw + 4.0, b.3, 13.0, SEMI, fg, Align::Left);
        f.focus_ring(b, 8.0, &act);
        f.hit(b, act);
    }
    h
}

fn riga(f: &mut Frame, app: &App, i: usize, r: R, col: &Colonne) {
    let pal = f.pal;
    let file = &app.files[i];
    let act = Act::LxRiga(i);
    let t = f.hover_t(&act, !file.checked);
    if file.checked {
        f.p.rrect_a(r.0, r.1, r.2, r.3, 0.0, pal.primary, 0.11);
        f.p.fill(r.0, r.1, 3.0, r.3, pal.primary);
    } else if t > 0.01 {
        f.p.rrect_a(r.0, r.1, r.2, r.3, 0.0, pal.alt, t);
    }
    f.p.fill(r.0, r.1 + r.3 - 1.0, r.2, 1.0, mix(pal.border, pal.surface, 0.4));
    f.hit(r, act);
    f.checkbox(r.0 + 18.0, r.1 + (r.3 - 18.0) / 2.0, 18.0, file.checked, Some(Act::LxCasella(i)));
    let (tipo, sigla) = app.lx_tipo(i);
    let sw = (f.measure(&sigla, 12.0, BOLD) + 16.0).clamp(46.0, col.tipo.1 - 8.0);
    f.p.rrect(r.0 + col.tipo.0, r.1 + (r.3 - 22.0) / 2.0, sw, 22.0, 5.0, Some(tipo.colore()), None);
    f.p.text(&sigla, r.0 + col.tipo.0, r.1 + (r.3 - 22.0) / 2.0, sw, 22.0, 12.0, BOLD, WHITE, Align::Center);
    // I nomi lunghi si tagliano con «…» alla stessa misura degli altri, invece di rimpicciolirsi.
    let nome = crate::i18n::lit(&f.p.ellipsize(&crate::i18n::lit(&file.name()), 14.5, SEMI, col.nome.1 - 10.0));
    f.p.text(&nome, r.0 + col.nome.0, r.1, col.nome.1 - 10.0, r.3, 14.5, SEMI, pal.text, Align::Left);
    if let Some(c) = col.caso {
        let caso = app.lx_caso_di(&file.path);
        if caso.is_empty() {
            f.p.text("—", r.0 + c.0, r.1, c.1 - 8.0, r.3, 14.0, SANS, pal.muted, Align::Left);
        } else {
            let cw = f.measure(&caso, 14.0, SEMI).min(c.1 - 10.0);
            f.p.text(&caso, r.0 + c.0, r.1, c.1 - 8.0, r.3, 14.0, SEMI, pal.primary, Align::Left);
            f.p.fill(r.0 + c.0, r.1 + r.3 / 2.0 + 9.0, cw, 1.0, pal.primary);
            f.hit((r.0 + c.0 - 4.0, r.1 + 2.0, cw + 8.0, r.3 - 4.0), Act::LxFiltroCaso(Some(caso)));
        }
    }
    f.p.text(&contenuto(app, i), r.0 + col.cont.0, r.1, col.cont.1 - 10.0, r.3, 14.0, SANS, pal.muted, Align::Left);
    dati_chip(f, app, i, (r.0 + col.dati.0, r.1, col.dati.1 - 8.0, r.3));
    gia_fatto(f, app, i, (r.0 + col.fatto.0, r.1, col.fatto.1 - 8.0, r.3));
}

/// «Già fatto»: una casellina colorata per ogni comando che ha lavorato il file, e «Apri» il risultato;
/// «Da fare» se niente; «Errore: perché?» se è andato storto (il clic lo spiega).
fn gia_fatto(f: &mut Frame, app: &App, i: usize, r: R) {
    let pal = f.pal;
    let file = &app.files[i];
    match file.state {
        FileState::Running => {
            let testo = file.percent.map(|p| format!("{} {}%", tt("In corso"), (p * 100.0).round() as i32)).unwrap_or_else(|| tt("In corso"));
            f.p.circle(r.0 + 5.0, r.1 + r.3 / 2.0, 4.5, Some(BLU), None);
            f.p.text(&testo, r.0 + 16.0, r.1, r.2 - 16.0, r.3, 13.5, SEMI, BLU, Align::Left);
            if let Some(p) = file.percent {
                f.p.rrect(r.0 + 16.0, r.1 + r.3 - 8.0, r.2 - 16.0, 3.0, 1.5, Some(pal.border), None);
                f.p.rrect(r.0 + 16.0, r.1 + r.3 - 8.0, (r.2 - 16.0) * p.clamp(0.0, 1.0), 3.0, 1.5, Some(BLU), None);
            }
            f.animating = true;
            return;
        }
        FileState::Failed => {
            let testo = tt("Errore: perché?");
            let tw = f.measure(&testo, 13.5, SEMI);
            f.p.text(&testo, r.0, r.1, tw + 4.0, r.3, 13.5, SEMI, pal.bad, Align::Left);
            f.p.fill(r.0, r.1 + r.3 / 2.0 + 9.0, tw, 1.0, pal.bad);
            f.hit((r.0 - 4.0, r.1 + 2.0, tw + 8.0, r.3 - 4.0), Act::LxErrore(i));
            return;
        }
        _ => {}
    }
    let fatti = app.lx.fatto.get(&file.path).map(|v| v.as_slice()).unwrap_or(&[]);
    if fatti.is_empty() {
        let (c, t) = if file.state == FileState::Skipped { (pal.muted, "Saltato") } else { (pal.muted, "Da fare") };
        f.p.circle(r.0 + 5.0, r.1 + r.3 / 2.0, 4.0, Some(c), None);
        // Se la traduzione di «Da fare» non sta nella colonna, si usa la più corta «In attesa».
        let mut testo = tt(t);
        if f.measure(&testo, 13.5, SANS) > r.2 - 18.0 {
            testo = tt("In attesa");
        }
        f.p.text(&testo, r.0 + 16.0, r.1, r.2 - 16.0, r.3, 13.5, SANS, pal.text, Align::Left);
        return;
    }
    let mut x = r.0;
    for fa in fatti.iter().take(4) {
        let (deep, _) = lab_colors(fa.lab);
        f.p.rrect(x, r.1 + (r.3 - 20.0) / 2.0, 20.0, 20.0, 5.0, Some(deep), None);
        f.p.text(lettera(fa.lab), x, r.1 + (r.3 - 20.0) / 2.0, 20.0, 20.0, 11.5, BOLD, WHITE, Align::Center);
        x += 24.0;
    }
    if fatti.iter().any(|x| x.out.is_some()) {
        let apri = tt("Apri");
        let aw = f.measure(&apri, 13.5, SEMI);
        f.p.text(&apri, x + 4.0, r.1, aw + 4.0, r.3, 13.5, SEMI, pal.primary, Align::Left);
        f.hit((x, r.1 + 2.0, aw + 12.0, r.3 - 4.0), Act::LxApri(i));
    }
}

// ------------------------------------------------------------------ il pannello

/// La testata del pannello: una freccia per tornare (se serve), la piastrella del comando, il titolo, una riga
/// sotto e il pulsante che chiude il pannello.
fn testata(f: &mut Frame, r: R, back: bool, tile: Option<(Lab, &str)>, titolo: &str, sub: &str) {
    let pal = f.pal;
    f.p.fill(r.0, r.1 + r.3 - 1.0, r.2, 1.0, pal.border);
    let mut x = r.0 + 14.0;
    if back {
        let b = (x - 4.0, r.1 + (r.3 - 32.0) / 2.0, 32.0, 32.0);
        let act = Act::LxPannello(Pannello::Dettagli);
        let t = f.hover_t(&act, true);
        f.hover_fill(b, 8.0, pal.alt, t);
        f.icon(ic::ARROW_LEFT, b.0 + 8.0, b.1 + 8.0, 16.0, pal.text);
        f.focus_ring(b, 8.0, &act);
        f.hit(b, act);
        x += 34.0;
    }
    if let Some((lab, icona)) = tile {
        let (deep, bright) = lab_colors(lab);
        let ty = r.1 + (r.3 - 36.0) / 2.0;
        f.p.rrect_vgrad(x, ty, 36.0, 36.0, 10.0, bright, deep);
        f.icon(icona, x + 7.0, ty + 7.0, 22.0, WHITE);
        x += 46.0;
    }
    let tw = r.0 + r.2 - 52.0 - x;
    if sub.is_empty() {
        f.p.text(titolo, x, r.1, tw, r.3, 16.5, BOLD, pal.text, Align::Left);
    } else {
        f.p.text(titolo, x, r.1 + 9.0, tw, 24.0, 16.5, BOLD, pal.text, Align::Left);
        f.p.text(sub, x, r.1 + 33.0, tw, 20.0, 13.5, SANS, pal.muted, Align::Left);
    }
    let b = (r.0 + r.2 - 42.0, r.1 + (r.3 - 32.0) / 2.0, 32.0, 32.0);
    let act = Act::LxPannelloToggle;
    let t = f.hover_t(&act, true);
    f.hover_fill(b, 8.0, pal.alt, t);
    f.icon(ic::PANEL, b.0 + 8.0, b.1 + 8.0, 16.0, pal.muted);
    f.focus_ring(b, 8.0, &act);
    f.hit(b, act);
}

fn pannello(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.surface);
    f.p.fill(r.0, r.1, 1.0, r.3, pal.border);
    if app.lx.panel_chiuso {
        let b = (r.0 + 7.0, r.1 + 14.0, 32.0, 32.0);
        let act = Act::LxPannelloToggle;
        let t = f.hover_t(&act, true);
        f.hover_fill(b, 8.0, pal.alt, t);
        f.icon(ic::PANEL, b.0 + 8.0, b.1 + 8.0, 16.0, pal.muted);
        f.focus_ring(b, 8.0, &act);
        f.hit(b, act);
        return;
    }
    let th = 62.0;
    let testa = (r.0, r.1, r.2, th);
    let corpo = (r.0, r.1 + th, r.2, r.3 - th);
    match app.lx.pannello {
        Pannello::Dettagli => {
            let n = app.lx_scelti().len();
            let (t, s) = if n == 0 {
                (tt("Dettagli"), String::new())
            } else {
                let bytes: u64 = app.lx_scelti().iter().filter_map(|&i| app.media_of(&app.files[i].path)).map(|m| m.bytes).sum();
                (if n == 1 { tt("1 file scelto") } else { format!("{n} {}", tt("file scelti")) }, if bytes > 0 { super::super::model::human_size(bytes) } else { String::new() })
            };
            testata(f, testa, false, None, &t, &s);
            dettagli(f, app, corpo);
        }
        Pannello::Azione => {
            let su = app.targets().len();
            let sub = if su == 0 {
                tt("Nessun file adatto")
            } else if su == 1 {
                tt("Su 1 file")
            } else {
                format!("{} {su} {}", tt("Su"), tt("file"))
            };
            testata(f, testa, true, Some((app.lab, lab_icon(app.lab))), &nome_comando(app.lab), &sub);
            azione(f, app, corpo);
        }
        Pannello::Procedure => {
            testata(f, testa, true, Some((PROCEDURE, ic::MENU)), &tt("Procedure"), &tt("Lavori salvati, sempre uguali"));
            procedure(f, app, corpo);
        }
    }
}

/// Una riga di conti con i numeri in grassetto: «2 Vetrino · 1 DICOM». Torna l'altezza.
fn conti(f: &mut Frame, x: f32, y: f32, voci: &[(usize, String)], colore: Rgb) -> f32 {
    let mut cx = x;
    for (k, (n, nome)) in voci.iter().enumerate() {
        if k > 0 {
            f.p.text("·", cx, y, 12.0, 22.0, 14.0, SANS, colore, Align::Left);
            cx += 14.0;
        }
        let ns = n.to_string();
        let nw = f.measure(&ns, 14.0, BOLD);
        f.p.text(&ns, cx, y, nw + 2.0, 22.0, 14.0, BOLD, colore, Align::Left);
        cx += nw + 5.0;
        let tw = f.measure(nome, 14.0, SANS);
        f.p.text(nome, cx, y, tw + 2.0, 22.0, 14.0, SANS, colore, Align::Left);
        cx += tw + 7.0;
    }
    22.0
}

/// Una riga di «Cosa puoi farci»: il tasto, il nome e, se prende solo una parte dei file, «2 su 3».
fn riga_comando(f: &mut Frame, r: R, lab: Option<Lab>, nome: &str, tasto: &str, parte: Option<(usize, usize)>, act: Act) {
    let pal = f.pal;
    let t = f.hover_t(&act, true);
    f.p.rrect(r.0, r.1, r.2, r.3, 9.0, Some(mix(pal.surface, pal.alt, t)), Some((pal.border, 1.0)));
    let (deep, _) = lab_colors(lab.unwrap_or(PROCEDURE));
    f.p.rrect(r.0 + 10.0, r.1 + (r.3 - 22.0) / 2.0, 22.0, 22.0, 6.0, Some(deep), None);
    f.p.text(tasto, r.0 + 10.0, r.1 + (r.3 - 22.0) / 2.0, 22.0, 22.0, 12.0, BOLD, WHITE, Align::Center);
    f.p.text(nome, r.0 + 42.0, r.1, r.2 - 120.0, r.3, 14.5, SEMI, pal.text, Align::Left);
    if let Some((n, tot)) = parte {
        let s = format!("{n} {} {tot}", tt("su"));
        f.p.text(&s, r.0 + r.2 - 90.0, r.1, 78.0, r.3, 12.5, SANS, pal.muted, Align::Right);
    }
    f.focus_ring(r, 9.0, &act);
    f.hit(r, act);
}

fn dettagli(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let (x, w) = (r.0 + 18.0, r.2 - 36.0);
    let scelti = app.lx_scelti();
    if scelti.is_empty() {
        f.p.rrect(x, r.1 + 16.0, w, 74.0, 10.0, Some(pal.alt), None);
        f.para(&tt("Scegli uno o più file. Poi un comando in alto: qui compaiono le sue scelte e «Esegui»."), x + 14.0, r.1 + 28.0, w - 28.0, 14.0, SANS, pal.muted, 1.4, 4);
        return;
    }
    super::scrolled(f, r, Scroll::LxPanel, |f| {
        let mut y = r.1 + 16.0;
        // Cosa c'è tra i file scelti.
        let mut per_tipo: Vec<(Tipo, usize)> = Vec::new();
        for &i in &scelti {
            let t = app.lx_tipo(i).0;
            match per_tipo.iter_mut().find(|(k, _)| *k == t) {
                Some(e) => e.1 += 1,
                None => per_tipo.push((t, 1)),
            }
        }
        let voci: Vec<(usize, String)> = per_tipo.iter().map(|(t, n)| (*n, tt(t.nome()))).collect();
        f.p.rrect(x, y, w, 40.0, 10.0, Some(pal.alt), None);
        conti(f, x + 14.0, y + 9.0, &voci, pal.text);
        y += 50.0;
        let con_dati = scelti.iter().filter(|&&i| app.lx_dati(i).stato == Stato::Campi).count();
        let da_ctrl = scelti.iter().filter(|&&i| app.lx_dati(i).stato == Stato::DaControllare).count();
        if con_dati + da_ctrl > 0 {
            let mut v = Vec::new();
            if con_dati > 0 {
                v.push((con_dati, tt("con dati personali")));
            }
            if da_ctrl > 0 {
                v.push((da_ctrl, tt("da controllare")));
            }
            f.p.rrect(x, y, w, 40.0, 10.0, Some(mix(ARANCIO, pal.surface, 0.88)), None);
            conti(f, x + 14.0, y + 9.0, &v, ARANCIO);
            y += 50.0;
        }
        y += 8.0;
        titolo_sezione(f, x - 4.0, y, w, "Cosa puoi farci");
        y += 28.0;
        let tot = scelti.len();
        let mut qualcuno = false;
        for lab in Lab::LAB.into_iter().chain(Lab::STANDARD) {
            let n = app.lx_per_comando(lab);
            if n == 0 {
                continue;
            }
            qualcuno = true;
            riga_comando(f, (x, y, w, 38.0), Some(lab), &nome_comando(lab), tasto_di(lab), if n < tot { Some((n, tot)) } else { None }, Act::Lab(lab));
            y += 44.0;
        }
        if !qualcuno {
            y += f.para(&tt("Nessun comando lavora questi file."), x, y, w, 14.0, SANS, pal.muted, 1.4, 2) + 6.0;
        }
        riga_comando(f, (x, y, w, 38.0), None, &tt("Procedure"), "9", None, Act::LxProcedure);
        y += 54.0;
        if scelti.len() == 1 {
            y += file_dettagli(f, app, scelti[0], x, y, w);
        }
        y - r.1 + 12.0
    });
}

/// Il comando scelto: le sue opzioni vere del motore, il pulsante grande e «Salva come procedura».
fn azione(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let (deep, _) = lab_colors(app.lab);
    let (x, w) = (r.0 + 20.0, r.2 - 40.0);
    let mut y = r.1 + 14.0;
    // Quello che il comando non prende resta fuori, e lo si dice.
    let (fuori, tipi) = app.lx_restano_fuori(app.lab);
    if fuori > 0 {
        let nomi = tipi.iter().map(|t| tt(t.nome())).collect::<Vec<_>>().join(", ");
        let testo = if fuori == 1 {
            format!("1 {} {nomi}. {}", tt("resta fuori:"), tt("Questo comando non lo lavora."))
        } else {
            format!("{fuori} {} {nomi}. {}", tt("restano fuori:"), tt("Questo comando non li lavora."))
        };
        let nr = f.p.wrap_lines(&testo, 14.0, SANS, w - 28.0, 4).len().max(1);
        let h = 22.0 + nr as f32 * 19.0;
        f.p.rrect(x, y, w, h, 10.0, Some(mix(ARANCIO, pal.surface, 0.88)), None);
        f.para(&testo, x + 14.0, y + 11.0, w - 28.0, 14.0, SANS, pal.text, 1.35, 4);
        y += h + 12.0;
    }
    let passo = app.lx_passo_attuale().is_some();
    let quaderno = app.lavoro_appena && app.finished() && !app.lavori.is_empty();
    let foot_h = 142.0 + if quaderno { 52.0 } else { 0.0 } + if passo { 52.0 } else { 0.0 };
    let foot_y = r.1 + r.3 - foot_h;
    let body = (r.0 + 1.0, y, r.2 - 2.0, foot_y - y);
    if let Some((j, l)) = app.strum_corrente() {
        // L'export di uno strumento: lo strumento riconosciuto, cosa esce e i ruoli della piastra.
        let (altre, act) = if app.simple_view { ("Tutte le opzioni", Act::SetSimple(false)) } else { ("Solo le scelte pronte", Act::SetSimple(true)) };
        let altre = tt(altre);
        let aw = f.measure(&altre, 14.0, SEMI);
        f.p.text(&altre, x + 2.0, y, aw + 4.0, 24.0, 14.0, SEMI, pal.primary, Align::Left);
        f.p.fill(x + 2.0, y + 21.0, aw, 1.0, pal.primary);
        f.hit((x, y, aw + 8.0, 26.0), act);
        let body = (body.0, y + 34.0, body.2, (body.3 - 34.0).max(0.0));
        super::strumenti::side_body(f, app, j, &l, body, x, w, deep);
    } else if f.simple {
        let ids = choices(app);
        super::scrolled(f, body, Scroll::Inspector, |f| {
            let mut cy = body.1 + 2.0;
            let titolo = tt("Come").to_uppercase();
            f.p.text(&titolo, x + 2.0, cy, w, 20.0, 11.5, SEMI, pal.muted, Align::Left);
            cy += 24.0;
            for id in &ids {
                let v = app.voce(id);
                choice_row(f, (x, cy, w, 58.0), &v, v.label, v.hint, app.action == *id, Act::Action(id), deep);
                cy += 66.0;
            }
            if !ids.iter().any(|id| app.action == *id) {
                let line = format!("Stai usando una voce scelta in «Completa»: {}.", super::action_title(app, false));
                cy += f.para(&line, x, cy + 4.0, w, 14.5, SANS, pal.muted, 1.4, 3) + 12.0;
            }
            let altre = tt("Tutte le opzioni");
            let aw = f.measure(&altre, 14.0, SEMI);
            f.p.text(&altre, x + 2.0, cy, aw + 4.0, 24.0, 14.0, SEMI, pal.primary, Align::Left);
            f.p.fill(x + 2.0, cy + 21.0, aw, 1.0, pal.primary);
            f.hit((x, cy, aw + 8.0, 26.0), Act::SetSimple(false));
            cy += 34.0;
            cy += f.para(&super::summary_line(app), x, cy, w, 14.5, SANS, pal.muted, 1.4, 4);
            cy - body.1 + 16.0
        });
    } else {
        super::scrolled(f, body, Scroll::Inspector, |f| {
            let mut cy = body.1 + 4.0;
            let altre = tt("Solo le scelte pronte");
            let aw = f.measure(&altre, 14.0, SEMI);
            f.p.text(&altre, x + 2.0, cy, aw + 4.0, 24.0, 14.0, SEMI, pal.primary, Align::Left);
            f.p.fill(x + 2.0, cy + 21.0, aw, 1.0, pal.primary);
            f.hit((x, cy, aw + 8.0, 26.0), Act::SetSimple(true));
            cy += 36.0;
            let fixed = matches!(app.lab, Lab::Anonimizza | Lab::Figura);
            if fixed {
                let v = app.action_voce();
                f.text(v.label, x, cy, w, 26.0, 17.0, SEMI, pal.text);
                cy += 30.0;
                if !v.hint.is_empty() {
                    cy += f.para(v.hint, x, cy, w, 14.5, SANS, pal.muted, 1.4, 3) + 16.0;
                }
            }
            cy += super::options_panel(f, app, x, cy, w, OPT_LABS, !fixed);
            f.hline(x, cy, w);
            cy += 16.0;
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
            cy - body.1 + 18.0
        });
    }
    f.hline(r.0 + 1.0, foot_y, r.2 - 2.0);
    let go = (x, foot_y + 18.0, w, 58.0);
    go_button(f, app, go, deep);
    let mut by = go.1 + go.3 + 8.0;
    if passo {
        f.button((x, by, w, 42.0), "Salva come procedura", Some(ic::COPY), 14.5, Act::LxProcSalva);
        by += 50.0;
    }
    if !super::notice(f, app, (x - 6.0, by + 2.0, w + 12.0, 44.0), 14.0) {
        super::go_extras(f, app, (go.0, go.1, go.2, go.3 + 4.0), false);
    }
    if quaderno {
        let label = if app.lavoro_appena_copiato { "Copiato" } else { "Copia per il quaderno" };
        f.button((x, by + 48.0, w, 42.0), label, Some(if app.lavoro_appena_copiato { ic::CHECK } else { ic::COPY }), 15.0, Act::QuadernoUltimo);
    }
}

/// «Procedure»: i lavori salvati. Una procedura rifà un comando con le stesse scelte sui file scelti.
fn procedure(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    let (deep, _) = lab_colors(PROCEDURE);
    let (x, w) = (r.0 + 20.0, r.2 - 40.0);
    let lista = &app.lx.procedure;
    if lista.is_empty() {
        f.p.rrect(x, r.1 + 16.0, w, 110.0, 10.0, Some(pal.alt), None);
        f.para(&tt("Nessuna procedura ancora. Scegli un comando, regola le scelte e premi «Salva come procedura»: la ritrovi qui, sempre uguale."), x + 14.0, r.1 + 28.0, w - 28.0, 14.0, SANS, pal.muted, 1.4, 6);
        return;
    }
    let foot_h = 142.0;
    let foot_y = r.1 + r.3 - foot_h;
    let body = (r.0 + 1.0, r.1, r.2 - 2.0, foot_y - r.1);
    let scelta = app.lx.proc_scelta.min(lista.len() - 1);
    super::scrolled(f, body, Scroll::LxPanel, |f| {
        let mut y = body.1 + 16.0;
        for (k, p) in lista.iter().enumerate() {
            let on = k == scelta;
            let act = Act::LxProcScegli(k);
            let t = f.hover_t(&act, !on);
            let rr = (x, y, w, 58.0);
            if on {
                f.p.rrect(rr.0, rr.1, rr.2, rr.3, 11.0, Some(mix(deep, pal.surface, 0.91)), Some((deep, 1.5)));
            } else {
                f.p.rrect(rr.0, rr.1, rr.2, rr.3, 11.0, Some(mix(pal.surface, pal.alt, t)), Some((pal.border, 1.0)));
            }
            f.p.circle(rr.0 + 22.0, rr.1 + rr.3 / 2.0, 8.0, None, Some((if on { deep } else { pal.border }, 1.5)));
            if on {
                f.p.circle(rr.0 + 22.0, rr.1 + rr.3 / 2.0, 4.0, Some(deep), None);
            }
            f.p.text(&p.nome, rr.0 + 42.0, rr.1 + 8.0, rr.2 - 54.0, 22.0, 15.0, BOLD, pal.text, Align::Left);
            let n = p.passi.len();
            let su = app.lx_procedura_su(k);
            let sotto = format!("{n} {} · {}", tt(if n == 1 { "passo" } else { "passi" }), if su == 0 { tt("nessun file scelto adatto") } else { format!("{} {su} {}", tt("lavora"), tt("dei file scelti")) });
            f.p.text(&sotto, rr.0 + 42.0, rr.1 + 31.0, rr.2 - 54.0, 20.0, 13.0, SANS, pal.muted, Align::Left);
            f.focus_ring(rr, 11.0, &act);
            f.hit(rr, act);
            y += 66.0;
        }
        y += 8.0;
        let nome = lista[scelta].nome.clone();
        titolo_sezione(f, x - 4.0, y, w, &format!("{} «{}»", tt("I passi di"), crate::i18n::lit(&nome)));
        y += 28.0;
        for (k, p) in lista[scelta].passi.iter().enumerate() {
            let (comando, voce) = app.lx_passo_titolo(p);
            let (comando, voce) = (crate::i18n::lit(&comando), crate::i18n::lit(&voce));
            let lab = crate::ui::labsx::lab_da_id(&p.lab).unwrap_or(Lab::Converti);
            let (d, _) = lab_colors(lab);
            f.p.rrect(x, y, w, 38.0, 9.0, Some(pal.alt), None);
            f.p.circle(x + 20.0, y + 19.0, 10.0, Some(d), None);
            f.p.text(&(k + 1).to_string(), x + 10.0, y, 20.0, 38.0, 12.0, BOLD, WHITE, Align::Center);
            f.p.text(&comando, x + 38.0, y, w - 50.0, 38.0, 14.0, SEMI, pal.text, Align::Left);
            let cw = f.measure(&comando, 14.0, SEMI);
            f.p.text(&format!("· {voce}"), x + 46.0 + cw, y, (w - 58.0 - cw).max(20.0), 38.0, 13.0, SANS, pal.muted, Align::Left);
            y += 44.0;
        }
        y - body.1 + 12.0
    });
    f.hline(r.0 + 1.0, foot_y, r.2 - 2.0);
    let su = app.lx_procedura_su(scelta);
    let abilitata = su > 0 && !app.busy() && app.lx.proc_corso.is_none();
    let label = if su == 1 { tt("Esegui su 1 file") } else { format!("{} {su} {}", tt("Esegui su"), tt("file")) };
    f.primary((x, foot_y + 18.0, w, 54.0), &label, Some(ic::ARROW_RIGHT), 17.0, Act::LxProcVia, abilitata);
    f.button((x, foot_y + 82.0, w, 40.0), "Togli questa procedura", Some(ic::TRASH), 14.0, Act::LxProcTogli(scelta));
    if let Some(n) = app.notice.as_ref() {
        let _ = n;
    }
}

// ------------------------------------------------------------------ il registro di oggi

/// La colonna «Contenuto»: la riga che descrive il file, o peso e misure.
fn contenuto(app: &App, i: usize) -> String {
    let file = &app.files[i];
    let m = app.media_of(&file.path);
    if m.is_some_and(|m| m.slide.is_some() || m.dicom.is_some() || m.micro.is_some()) {
        return super::waiting_line(app, i);
    }
    let (size, extra) = super::file_meta(app, i);
    let mut parts = vec![extra, size];
    if let Some(a) = m.and_then(|m| m.about.as_ref()) {
        if let Some(d) = a.duration {
            let s = d.round() as u64;
            parts.insert(0, if s >= 60 { format!("{} min {:02} s", s / 60, s % 60) } else { format!("{s} s") });
        }
    }
    parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
}

fn chip(f: &mut Frame, r: R, testo: &str, fg: Rgb, bg: Rgb) {
    let tw = (f.measure(testo, 12.5, SEMI) + 20.0).min(r.2);
    let h = 24.0;
    let y = r.1 + (r.3 - h) / 2.0;
    f.p.rrect(r.0, y, tw, h, 12.0, Some(bg), None);
    f.p.text(testo, r.0, y, tw, h, 12.5, SEMI, fg, Align::Center);
}

fn dati_chip(f: &mut Frame, app: &App, i: usize, r: R) {
    let pal = f.pal;
    let file = &app.files[i];
    let d = app.lx_dati(i);
    // Un file di cui «Togli i nomi» ha già fatto la copia.
    let pulita = file.state == FileState::Done && file.done_key.as_deref().is_some_and(|k| k.contains("|strip|"));
    if pulita {
        chip(f, r, &tt("Copia pulita"), pal.ok, mix(pal.ok, pal.surface, 0.88));
        return;
    }
    match d.stato {
        Stato::Campi => {
            let t = if d.n() == 1 { tt("1 campo") } else { format!("{} {}", d.n(), tt("campi")) };
            chip(f, r, &t, ARANCIO, mix(ARANCIO, pal.surface, 0.88));
        }
        Stato::DaControllare => chip(f, r, &tt("Da controllare"), BLU, mix(BLU, pal.surface, 0.9)),
        Stato::Nessuno => {
            f.icon(ic::CHECK, r.0 + 2.0, r.1 + (r.3 - 16.0) / 2.0, 16.0, pal.muted);
            f.p.text(&tt("Nessuno"), r.0 + 24.0, r.1, r.2 - 24.0, r.3, 13.5, SANS, pal.muted, Align::Left);
        }
        Stato::NonLetto => f.p.text("…", r.0 + 2.0, r.1, 30.0, r.3, 14.0, SANS, pal.muted, Align::Left),
    }
}

fn chiave_valore(f: &mut Frame, x: f32, y: f32, w: f32, k: &str, v: &str) -> f32 {
    let pal = f.pal;
    f.p.text(&tt(k), x, y, 110.0, 24.0, 14.0, SANS, pal.muted, Align::Left);
    let h = f.para(v, x + 118.0, y + 3.0, w - 118.0, 14.0, SANS, pal.text, 1.3, 3);
    h.max(24.0) + 2.0
}

fn file_dettagli(f: &mut Frame, app: &App, i: usize, x: f32, y0: f32, w: f32) -> f32 {
    let pal = f.pal;
    let file = &app.files[i];
    let mut y = y0;
    // La miniatura e chi è.
    let th = (92.0, 68.0);
    f.thumb(app, i, (x, y, th.0, th.1), 8.0);
    f.hit((x, y, th.0, th.1), Act::LxAnteprima);
    let tx = x + th.0 + 14.0;
    let tw = w - th.0 - 14.0;
    let h = f.para(&file.name(), tx, y - 2.0, tw, 16.5, BOLD, pal.text, 1.25, 2);
    let (tipo, sigla) = app.lx_tipo(i);
    let (size, _) = super::file_meta(app, i);
    let sub = [tt(tipo.nome()), sigla.clone(), size].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let sub = if sub.len() > 1 && sub[0] == sub[1] { sub[1..].join(" · ") } else { sub.join(" · ") };
    f.p.text(&sub, tx, y - 2.0 + h + 2.0, tw, 20.0, 13.5, SANS, pal.muted, Align::Left);
    let caso = app.lx_caso_di(&file.path);
    let mut ly = y - 2.0 + h + 24.0;
    if !caso.is_empty() {
        f.p.text(&caso, tx, ly, tw, 20.0, 13.5, SEMI, pal.primary, Align::Left);
        ly += 20.0;
    }
    if let Some(t) = app.lx.dalle.get(&file.path) {
        let s = crate::nametemplate::local(*t);
        f.p.text(&format!("{} {:02}:{:02}", tt("In coda dalle"), s.hour, s.minute), tx, ly, tw, 20.0, 13.0, SANS, pal.muted, Align::Left);
    }
    y += th.1.max(ly - y + 20.0) + 6.0;
    // L'anteprima grande, dal bottone.
    f.button((x, y, w, 34.0), "Apri l'anteprima grande", Some(ic::ZOOM_IN), 14.0, Act::LxAnteprima);
    y += 34.0 + 18.0;
    // Cosa c'è dentro.
    titolo_sezione(f, x - 4.0, y, w, "Cosa c'è dentro");
    y += 26.0;
    let m = app.media_of(&file.path);
    y += chiave_valore(f, x, y, w, "Tipo", &format!("{} · {}", tt(tipo.nome()), sigla));
    if let Some(m) = m {
        if m.bytes > 0 {
            y += chiave_valore(f, x, y, w, "Peso", &super::super::model::human_size(m.bytes));
        }
        if let Some((pw, ph)) = m.size {
            y += chiave_valore(f, x, y, w, "Dimensioni", &format!("{pw} × {ph} px"));
        }
        if let Some(p) = m.pages {
            y += chiave_valore(f, x, y, w, "Pagine", &p.to_string());
        }
        if let Some(a) = m.about.as_ref() {
            if !a.kind_label.is_empty() {
                y += chiave_valore(f, x, y, w, "Genere", &a.kind_label);
            }
            for (k, v) in a.facts.iter().take(5) {
                y += chiave_valore(f, x, y, w, k, v);
            }
        }
        if m.slide.is_some() || m.dicom.is_some() || m.micro.is_some() {
            y += chiave_valore(f, x, y, w, "Descrizione", &super::waiting_line(app, i));
        }
    } else {
        f.spinner(x + 10.0, y + 12.0, 8.0, pal.muted);
        f.animating = true;
        y += 28.0;
    }
    y += 14.0;
    // I dati che identificano.
    let d = app.lx_dati(i);
    titolo_sezione(f, x - 4.0, y, w, "Dati che identificano");
    if d.n() > 0 {
        let s = d.n().to_string();
        let tw = f.measure(&tt("Dati che identificano").to_uppercase(), 11.5, SEMI);
        f.p.circle(x + 4.0 + tw + 14.0, y + 11.0, 9.0, Some(ARANCIO), None);
        f.p.text(&s, x + 4.0 + tw + 5.0, y + 2.0, 18.0, 18.0, 11.5, BOLD, WHITE, Align::Center);
    }
    y += 26.0;
    match d.stato {
        Stato::Campi => {
            for c in &d.campi {
                f.p.circle(x + 5.0, y + 11.0, 3.5, Some(ARANCIO), None);
                y += f.para(&tt(c), x + 18.0, y + 3.0, w - 18.0, 14.0, SANS, pal.text, 1.3, 2).max(20.0) + 3.0;
            }
            y += 4.0;
            y += f.para(&tt("«Togli i nomi» li toglie dalla copia, non dall'originale."), x, y, w, 13.5, SANS, pal.muted, 1.35, 3) + 8.0;
        }
        Stato::DaControllare => {
            y += f.para(&format!("{} {}.", tt("Da controllare:"), tt(&d.nota)), x, y, w, 14.0, SANS, pal.text, 1.35, 3) + 8.0;
        }
        Stato::Nessuno => {
            y += f.para(&tt("Nessun dato personale trovato."), x, y, w, 14.0, SANS, pal.muted, 1.35, 2) + 8.0;
        }
        Stato::NonLetto => {
            y += f.para(&tt("Non ho ancora letto questo file."), x, y, w, 14.0, SANS, pal.muted, 1.35, 2) + 8.0;
        }
    }
    y += 10.0;
    // La storia.
    titolo_sezione(f, x - 4.0, y, w, "Storia");
    y += 26.0;
    let storia = match (&file.state, &file.output) {
        (FileState::Done, Some(o)) => {
            let nome = o.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            format!("{} {}", tt("Elaborato in"), nome)
        }
        (FileState::Failed, _) => file.message.clone(),
        _ => tt("Mai elaborato. L'originale non viene mai modificato."),
    };
    y += f.para(&storia, x, y, w, 14.0, SANS, pal.text, 1.35, 4) + 14.0;
    // L'impronta.
    titolo_sezione(f, x - 4.0, y, w, "Impronta SHA-256");
    y += 26.0;
    match app.lx.sha.get(&file.path) {
        Some(Sha::Pronta(h)) => {
            let (a, b) = h.split_at(32);
            f.p.text(a, x, y, w, 18.0, 12.5, SANS, pal.text, Align::Left);
            f.p.text(b, x, y + 18.0, w, 18.0, 12.5, SANS, pal.text, Align::Left);
            y += 40.0;
        }
        Some(Sha::Non) => {
            f.p.text(&tt("Non si è potuta calcolare."), x, y, w, 20.0, 13.5, SANS, pal.muted, Align::Left);
            y += 24.0;
        }
        _ => {
            f.spinner(x + 8.0, y + 10.0, 7.0, pal.muted);
            f.p.text(&tt("Calcolo…"), x + 24.0, y, w, 20.0, 13.5, SANS, pal.muted, Align::Left);
            f.animating = true;
            y += 24.0;
        }
    }
    y - y0
}

fn anteprima(f: &mut Frame, app: &App) {
    let Some(i) = app.selected.filter(|&i| i < app.files.len()) else { return };
    let pal = f.pal;
    f.p.fill_a(0.0, 0.0, f.w, f.h, (0, 0, 0), 0.55);
    f.hit((0.0, 0.0, f.w, f.h), Act::LxAnteprima);
    let (w, h) = ((f.w - 120.0).min(1180.0), f.h - 110.0);
    let (x, y) = ((f.w - w) / 2.0, 54.0);
    f.p.shadow(x, y, w, h, 14.0, 20.0, 0.4, 8.0);
    f.p.rrect(x, y, w, h, 14.0, Some(LABS_STAGE), None);
    f.hit((x, y, w, h), Act::Nothing);
    f.p.text(&app.files[i].name(), x + 22.0, y + 8.0, w - 150.0, 44.0, 16.5, BOLD, LABS_INK, Align::Left);
    let cb = (x + w - 112.0, y + 9.0, 96.0, 34.0);
    f.p.rrect_a(cb.0, cb.1, cb.2, cb.3, 8.0, WHITE, if f.hovered(&Act::LxAnteprima) { 0.2 } else { 0.1 });
    f.p.text(&tt("Chiudi"), cb.0, cb.1, cb.2, cb.3, 14.5, SEMI, LABS_INK, Align::Center);
    f.hit(cb, Act::LxAnteprima);
    let _ = pal;
    super::labs::viewer(f, app, i, (x + 18.0, y + 56.0, w - 36.0, h - 74.0));
}

fn titolo_sezione(f: &mut Frame, x: f32, y: f32, w: f32, s: &str) {
    let pal = f.pal;
    let s = tt(s).to_uppercase();
    f.p.text(&s, x + 4.0, y, w, 22.0, 11.5, SEMI, pal.muted, Align::Left);
}


fn registro_h(app: &App) -> f32 {
    if !app.lx.registro_aperto {
        return LOG_HEAD;
    }
    let n = crate::caso::di_oggi(&app.lavori).len().clamp(1, 3);
    LOG_HEAD + n as f32 * LOG_ROW + 6.0
}

fn registro(f: &mut Frame, app: &App, r: R) {
    let pal = f.pal;
    f.p.fill(r.0, r.1, r.2, r.3, pal.surface);
    f.p.fill(r.0, r.1, r.2, 1.0, pal.border);
    let oggi = crate::caso::di_oggi(&app.lavori);
    let act = Act::LxRegistro;
    let t = f.hover_t(&act, true);
    f.hover_fill((r.0, r.1 + 1.0, r.2, LOG_HEAD - 1.0), 0.0, pal.alt, t);
    f.icon(if app.lx.registro_aperto { ic::CHEVRON_DOWN } else { ic::ARROW_RIGHT }, r.0 + 16.0, r.1 + 13.0, 16.0, pal.text);
    f.p.text(&tt("Registro di oggi"), r.0 + 42.0, r.1, 200.0, LOG_HEAD, 14.5, BOLD, pal.text, Align::Left);
    let hw = f.measure(&tt("Registro di oggi"), 14.5, BOLD);
    let sub = if oggi.is_empty() { tt("ancora nessun lavoro") } else { format!("{} {} · {}", oggi.len(), tt(if oggi.len() == 1 { "lavoro" } else { "lavori" }), tt("gli originali non sono mai stati toccati")) };
    f.p.text(&sub, r.0 + 54.0 + hw, r.1, r.2 - hw - 70.0, LOG_HEAD, 13.5, SANS, pal.muted, Align::Left);
    f.focus_ring((r.0, r.1, r.2, LOG_HEAD), 0.0, &act);
    f.hit((r.0, r.1, r.2, LOG_HEAD), act);
    // Il registro completo (tutti i giorni) è una pagina a parte: il collegamento sta nell'intestazione.
    let tutto = tt("Tutto il registro");
    let tw = f.measure(&tutto, 13.5, SEMI) + 28.0;
    f.button((r.0 + r.2 - 16.0 - tw, r.1 + 5.0, tw, LOG_HEAD - 10.0), "Tutto il registro", None, 13.5, Act::LavoriApri);
    if !app.lx.registro_aperto {
        return;
    }
    for (k, l) in oggi.iter().take(3).enumerate() {
        let y = r.1 + LOG_HEAD + k as f32 * LOG_ROW;
        f.p.fill(r.0 + 16.0, y, r.2 - 32.0, 1.0, mix(pal.border, pal.surface, 0.4));
        f.p.text(&l.ora(), r.0 + 16.0, y, 60.0, LOG_ROW, 14.0, SANS, pal.muted, Align::Left);
        let what = tt(&l.what);
        f.p.text(&what, r.0 + 82.0, y, 200.0, LOG_ROW, 14.5, BOLD, pal.text, Align::Left);
        let nome = match l.outputs.as_slice() {
            [] => String::new(),
            [uno] => uno.clone(),
            [uno, resto @ ..] => format!("{uno} + {} {}", resto.len(), tt("altri")),
        };
        let caso = if l.caso.is_empty() { String::new() } else { format!("{}  ", l.caso) };
        f.p.text(&format!("{caso}{nome}"), r.0 + 290.0, y, r.2 - 290.0 - 460.0, LOG_ROW, 14.0, SANS, pal.text, Align::Left);
        let bx = r.0 + r.2 - 16.0;
        let q = (bx - 172.0, y + 6.0, 172.0, 32.0);
        let copied = app.quaderno_copiato == Some(k);
        f.button(q, if copied { "Copiato" } else { "Per il quaderno" }, Some(if copied { ic::CHECK } else { ic::COPY }), 13.5, Act::Quaderno(k));
        if !l.dir.is_empty() {
            let o = (q.0 - 8.0 - 150.0, y + 6.0, 150.0, 32.0);
            f.button(o, "Apri cartella", Some(ic::FOLDER), 13.5, Act::OpenPath(std::path::PathBuf::from(&l.dir)));
            f.p.text(&l.dir, r.0 + r.2 - 16.0 - 172.0 - 8.0 - 150.0 - 8.0 - 300.0, y, 300.0, LOG_ROW, 12.5, SANS, pal.muted, Align::Right);
        }
    }
}

// ------------------------------------------------------------------ la riga di stato

fn stato(f: &mut Frame, app: &App, r: R) {
    f.p.fill(r.0, r.1, r.2, r.3, LABS_FRAME);
    f.p.fill(r.0, r.1, r.2, 1.0, LABS_LINE);
    let dove = match &app.dest {
        Some(d) => super::super::model::short_dir(d),
        None => tt("accanto all'originale"),
    };
    let x = r.0 + 16.0;
    let t = tt("Cartella di uscita");
    let tw = f.measure(&t, 13.5, SANS);
    f.p.text(&t, x, r.1, tw + 4.0, r.3, 13.5, SANS, LABS_MUTED, Align::Left);
    f.p.text(&dove, x + tw + 10.0, r.1, 420.0, r.3, 13.5, SEMI, LABS_INK, Align::Left);
    // A destra: la riga dei componenti, le righe comode e le scorciatoie.
    let mut rx = r.0 + r.2 - 16.0;
    let (nota, c) = match app.job.as_ref() {
        Some(_) => (tt("Al lavoro…"), crate::ui::theme::LABS_LEAF),
        None => (tt("Pronto"), crate::ui::theme::LABS_LEAF),
    };
    let nw = f.measure(&nota, 13.5, SEMI);
    f.p.text(&nota, rx - nw, r.1, nw + 2.0, r.3, 13.5, SEMI, c, Align::Left);
    f.p.circle(rx - nw - 14.0, r.1 + r.3 / 2.0, 4.0, Some(c), None);
    rx -= nw + 38.0;
    let righe = tt(if app.lx.comode { "Righe comode" } else { "Righe compatte" });
    let rw = f.measure(&righe, 13.5, SEMI);
    let act = Act::LxComode;
    let t = f.hover_t(&act, true);
    f.p.rrect_a(rx - rw - 20.0, r.1 + 5.0, rw + 20.0, r.3 - 10.0, 6.0, WHITE, 0.06 + 0.1 * t);
    f.p.text(&righe, rx - rw - 10.0, r.1, rw + 4.0, r.3, 13.5, SEMI, LABS_INK, Align::Left);
    f.hit((rx - rw - 20.0, r.1 + 5.0, rw + 20.0, r.3 - 10.0), act);
    rx -= rw + 34.0;
    let hint = tt("per le scorciatoie");
    let hw = f.measure(&hint, 13.5, SANS);
    f.p.text(&hint, rx - hw, r.1, hw + 2.0, r.3, 13.5, SANS, LABS_MUTED, Align::Left);
    let pw = f.measure(&tt("Premi"), 13.5, SANS);
    f.p.text("?", rx - hw - 16.0, r.1, 14.0, r.3, 13.5, BOLD, LABS_INK, Align::Center);
    f.p.text(&tt("Premi"), rx - hw - 22.0 - pw, r.1, pw + 4.0, r.3, 13.5, SANS, LABS_MUTED, Align::Left);
    f.hit((rx - hw - 30.0 - pw, r.1 + 4.0, hw + pw + 40.0, r.3 - 8.0), Act::LxAiuto);
}

// ------------------------------------------------------------------ finestrelle: anteprima grande, scheda, errore, nome, scorciatoie

/// Una finestrella al centro, con lo sfondo scuro che si chiude con un clic.
fn finestrella(f: &mut Frame, w: f32, h: f32, chiudi: Act) -> R {
    let pal = f.pal;
    f.p.fill_a(0.0, 0.0, f.w, f.h, (0, 0, 0), 0.45);
    f.hit((0.0, 0.0, f.w, f.h), chiudi);
    let w = w.min(f.w - 32.0);
    let (x, y) = ((f.w - w) / 2.0, ((f.h - h) / 2.0).max(8.0));
    f.p.shadow(x, y, w, h, 14.0, 20.0, 0.4, 8.0);
    f.p.rrect(x, y, w, h, 14.0, Some(pal.surface), None);
    f.hit((x, y, w, h), Act::Nothing);
    (x, y, w, h)
}

/// La scheda del caso scelto: la nota, le tappe e i lavori fatti.
fn scheda(f: &mut Frame, app: &App) {
    let pal = f.pal;
    let Some(code) = app.lx.caso.clone().filter(|c| !c.is_empty()) else { return };
    let lavori = app.lx_lavori_del_caso(&code);
    let nr = lavori.len().min(6);
    let h = 250.0 + nr as f32 * 40.0 + if lavori.is_empty() { 30.0 } else { 0.0 };
    let (x, y, w, h) = finestrella(f, 640.0, h, Act::LxScheda);
    let (cx, cw) = (x + 28.0, w - 56.0);
    f.p.text(&code, cx, y + 20.0, cw - 130.0, 36.0, 24.0, BOLD, pal.text, Align::Left);
    f.button((x + w - 28.0 - 110.0, y + 20.0, 110.0, 38.0), "Chiudi", None, 15.0, Act::LxScheda);
    let nota = app.lx_nota_caso(&code);
    let nota = if nota.is_empty() { tt("Nessuna nota. «Modifica» per scriverla.") } else { nota };
    f.para(&nota, cx, y + 64.0, cw - 130.0, 15.0, SANS, pal.muted, 1.4, 2);
    f.button((x + w - 28.0 - 110.0, y + 64.0, 110.0, 38.0), "Modifica", None, 15.0, Act::CasoApri);
    let (in_coda, _) = app.lx_conta_casi();
    let n_coda = in_coda.iter().find(|(c, _)| *c == code).map(|c| c.1).unwrap_or(0);
    let mut yy = y + 118.0;
    let riga = format!("{n_coda} {} · {} {}", tt(if n_coda == 1 { "file in coda" } else { "file in coda" }), lavori.len(), tt(if lavori.len() == 1 { "lavoro fatto" } else { "lavori fatti" }));
    f.p.text(&riga, cx, yy, cw, 24.0, 14.5, SEMI, pal.text, Align::Left);
    yy += 34.0;
    titolo_sezione(f, cx - 4.0, yy, cw, "Tappe");
    yy += 26.0;
    let fatte = app.lx_tappe_di(&code) as usize;
    let n = crate::caso::TAPPE.len();
    let sw = ((cw - 5.0 * 6.0) / n as f32).floor();
    for (k, nome) in crate::caso::TAPPE.iter().enumerate() {
        let b = (cx + k as f32 * (sw + 6.0), yy, sw, 28.0);
        let (bg, fg) = if k < fatte { (mix(pal.ok, pal.surface, 0.86), pal.ok) } else if k == fatte { (pal.primary, pal.surface) } else { (pal.alt, pal.muted) };
        f.p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(bg), None);
        f.text_c(&tt(nome), b, 12.5, SEMI, fg);
        f.hit(b, Act::LxTappa(k as u8));
    }
    yy += 44.0;
    titolo_sezione(f, cx - 4.0, yy, cw, "Lavori fatti");
    yy += 26.0;
    if lavori.is_empty() {
        f.p.text(&tt("Ancora nessuno: ogni lavoro di questo caso finisce qui."), cx, yy, cw, 24.0, 14.0, SANS, pal.muted, Align::Left);
    }
    for l in lavori.iter().take(6) {
        let giorno = l.giorno();
        f.p.text(&format!("{giorno} {}", l.ora()), cx, yy, 150.0, 36.0, 13.0, SANS, pal.muted, Align::Left);
        f.p.text(&tt(&l.what), cx + 156.0, yy, 190.0, 36.0, 14.0, BOLD, pal.text, Align::Left);
        let nome = match l.outputs.as_slice() {
            [] => String::new(),
            [uno] => uno.clone(),
            [uno, resto @ ..] => format!("{uno} + {} {}", resto.len(), tt("altri")),
        };
        f.p.text(&nome, cx + 352.0, yy, cw - 352.0, 36.0, 13.5, SANS, pal.text, Align::Left);
        yy += 40.0;
    }
    let _ = h;
}

/// «Errore: perché?»: il motivo, con le parole del motore.
fn errore(f: &mut Frame, app: &App) {
    let pal = f.pal;
    let Some(i) = app.lx.errore.filter(|&i| i < app.files.len()) else { return };
    let file = &app.files[i];
    let (x, y, w, h) = finestrella(f, 560.0, 250.0, Act::LxErrore(i));
    let (cx, cw) = (x + 28.0, w - 56.0);
    f.p.text(&tt("Perché non è riuscito"), cx, y + 20.0, cw, 34.0, 21.0, BOLD, pal.text, Align::Left);
    f.p.text(&file.name(), cx, y + 58.0, cw, 24.0, 14.5, SEMI, pal.muted, Align::Left);
    let msg = if file.message.is_empty() { tt("Il motore non ha dato un motivo.") } else { file.message.clone() };
    f.p.rrect(cx, y + 92.0, cw, 84.0, 10.0, Some(mix(pal.bad, pal.surface, 0.92)), None);
    f.para(&msg, cx + 14.0, y + 104.0, cw - 28.0, 14.5, SANS, pal.text, 1.4, 3);
    f.button((x + w - 28.0 - 120.0, y + h - 62.0, 120.0, 42.0), "Chiudi", None, 15.0, Act::LxErrore(i));
}

/// Il nome con cui si salva la procedura.
fn proc_nome(f: &mut Frame, app: &App) {
    let pal = f.pal;
    let Some(nome) = app.lx.proc_nome.as_ref() else { return };
    let (x, y, w, h) = finestrella(f, 540.0, 262.0, Act::LxProcNomeChiudi);
    let (cx, cw) = (x + 28.0, w - 56.0);
    f.p.text(&tt("Salva come procedura"), cx, y + 20.0, cw, 34.0, 22.0, BOLD, pal.text, Align::Left);
    let passo = app.lx_passo_attuale();
    let cosa = passo.as_ref().map(|p| app.lx_passo_titolo(p)).map(|(c, v)| crate::i18n::lit(&format!("{c} · {v}"))).unwrap_or_default();
    f.para(&format!("{cosa} — {}", tt("con tutte le scelte di adesso")), cx, y + 58.0, cw, 14.5, SANS, pal.muted, 1.4, 3);
    f.text("Nome", cx, y + 112.0, cw, 22.0, 14.0, SEMI, pal.muted);
    f.field((cx, y + 138.0, cw, 42.0), Field::LxProcNome, nome, "Vetrino per consulto", 16.0, false);
    let bw = 130.0;
    f.button((x + w - 28.0 - 2.0 * bw - 12.0, y + h - 62.0, bw, 42.0), "Annulla", None, 15.0, Act::LxProcNomeChiudi);
    f.primary((x + w - 28.0 - bw, y + h - 62.0, bw, 42.0), "Salva", None, 15.0, Act::LxProcNomeOk, !nome.trim().is_empty());
}

fn aiuto(f: &mut Frame) {
    let pal = f.pal;
    f.p.fill_a(0.0, 0.0, f.w, f.h, (0, 0, 0), 0.45);
    f.hit((0.0, 0.0, f.w, f.h), Act::LxAiuto);
    let (w, h) = (700.0f32.min(f.w - 60.0), 590.0);
    let (x, y) = ((f.w - w) / 2.0, ((f.h - h) / 2.0).max(8.0));
    f.p.shadow(x, y, w, h, 14.0, 20.0, 0.4, 8.0);
    f.p.rrect(x, y, w, h, 14.0, Some(pal.surface), None);
    f.hit((x, y, w, h), Act::Nothing);
    f.p.text(&tt("Scorciatoie da tastiera"), x + 28.0, y + 18.0, w - 220.0, 40.0, 22.0, BOLD, pal.text, Align::Left);
    f.button((x + w - 28.0 - 120.0, y + 20.0, 120.0, 38.0), "Chiudi", None, 15.0, Act::LxAiuto);
    let voci: [(&str, &str); 15] = [
        ("1 … 9", "I comandi, nell'ordine della barra (9 = Procedure)"),
        ("↑  ↓", "Sposta la selezione (con Maiusc la allarga)"),
        ("Spazio", "Aggiunge o toglie il file dalla selezione"),
        ("Ctrl + A", "Sceglie tutto l'elenco"),
        ("Ctrl + clic", "Aggiunge o toglie un file dalla selezione"),
        ("Maiusc + clic", "Sceglie tutti i file da dove eri partito"),
        ("Invio", "Esegue il comando aperto"),
        ("C", "Assegna un caso ai file scelti"),
        ("P", "Anteprima grande del file"),
        ("/", "Cerca nell'elenco"),
        ("Canc", "Toglie dalla coda (il file resta sul disco)"),
        ("Esc", "Torna indietro di un passo"),
        ("Tab", "Passa da un comando al successivo"),
        ("?", "Questo foglio"),
        ("", ""),
    ];
    let mut yy = y + 78.0;
    for (k, d) in voci.into_iter().filter(|(k, _)| !k.is_empty()) {
        let kw = (f.measure(k, 14.0, SEMI) + 20.0).max(54.0);
        f.p.rrect(x + 28.0, yy, kw, 28.0, 6.0, Some(pal.alt), Some((pal.border, 1.0)));
        f.p.text(k, x + 28.0, yy, kw, 28.0, 14.0, SEMI, pal.text, Align::Center);
        f.p.text(&tt(d), x + 28.0 + 130.0, yy, w - 28.0 - 130.0 - 28.0, 28.0, 15.0, SANS, pal.text, Align::Left);
        yy += 31.0;
    }
    f.p.text(&tt("Le lettere e i numeri funzionano quando non stai scrivendo in un campo."), x + 28.0, y + h - 40.0, w - 56.0, 28.0, 13.5, SANS, pal.muted, Align::Left);
}
