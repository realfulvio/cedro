//! Il Cedro di partenza sul telefono (disegno approvato il 12/11/2026): la casa della 0.44 a misura di tocco.
//! Cornice scura della tinta (Cedro, Notte, Bosco, Carta), in alto il logo con Lavori e Impostazioni, in basso
//! la barra con le sei attività (Inizio, Converti, PDF, Trascrivi, Video, Firma) con le icone a gradiente.
//! «Inizio» ha le sei tessere con la foto del tipo di file, la riga del link e gli ultimi file. Ogni attività ha
//! il file grande sul suo alone, la striscia dei file e un solo pannello chiaro con tutti i comandi veri del
//! motore (Semplice: formato e pulsante; Completa: anche le opzioni), il pulsante sempre sotto.
//! Le scelte, i formati e le opzioni sono quelli di sempre: qui cambia solo come si vedono.

use cedro::ui::model::Lab;
use cedro::ui::theme::{kind_bright, kind_color, page_colors};
use cedro::ui::views::cedro::tile_image;

use super::*;
use crate::ui::tinte;

const ICO_HOME: &str = "M4 11l8-7 8 7v8a1 1 0 0 1-1 1h-4v-6h-6v6H5a1 1 0 0 1-1-1z";
const ICO_CONV: &str = "M4 8h13l-3-3M20 16H7l3 3";
const ICO_PDF: &str = "M7 3h7l4 4v14H7zM14 3v4h4M10 13h5M10 17h3";
const ICO_TR: &str = "M4 10v4M8 6v12M12 3v18M16 7v10M20 10v4";
const ICO_WEB: &str = "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18";
const ICO_FIRMA: &str = "M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6zM9 12l2.5 2.5L15.5 10";
const ICO_PLAY: &str = "M8 5l11 7-11 7z";
const ICO_TRASH: &str = "M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13";
const ICO_PASTE: &str = "M9 4h6v3H9zM7 5.5H5V21h14V5.5h-2M9 12h6M9 16h4";
const ICO_FOLDER: &str = "M3 6h6l2 2h10v11H3z";

const BIANCO: Rgb = (255, 255, 255);
/// Le scritte sulla zona scura del file (uguali in tutte le tinte).
const SC_INK: Rgb = (0xF4, 0xEF, 0xFA);
const SC_MUT: Rgb = (0xB9, 0xAD, 0xCA);

const TOP: f32 = 52.0;
const NAVB: f32 = 64.0;
const PAD: f32 = 10.0;
const GAP: f32 = 9.0;
const PP: f32 = 11.0;
const RIGA: f32 = 34.0;
const FMT_H: f32 = 62.0;
const CTA_H: f32 = 52.0;
const STAGE_MIN: f32 = 120.0;

/// Le sei pagine della barra: colore pieno e acceso, icona, nome.
fn pagine() -> [(Lab, &'static str, &'static str); 6] {
    [(Lab::Home, ICO_HOME, "Inizio"), (Lab::Converti, ICO_CONV, "Converti"), (Lab::Pdf, ICO_PDF, "PDF"), (Lab::Trascrivi, ICO_TR, "Trascrivi"), (Lab::Video, ICO_WEB, "Video"), (Lab::Firma, ICO_FIRMA, "Firma")]
}

/// Le sei tessere di «Inizio»: tipo di file, titolo, sotto, indice dell'immagine.
const TESSERE: [(Kind, &str, &str); 6] = [
    (Kind::Image, "Immagini", "JPEG, PNG, WebP, HEIC"),
    (Kind::Pdf, "PDF", "Unisci, dividi, in Word"),
    (Kind::Doc, "Documenti", "Word, Excel, testo"),
    (Kind::Audio, "Audio", "MP3, FLAC, trascrivi"),
    (Kind::Video, "Video", "MP4, MKV, solo audio"),
    (Kind::Archive, "Archivi", "ZIP, 7z, estrai"),
];

fn gradiente(p: &mut Painter, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, cb: Rgb) {
    p.rrect_vgrad(x, y, w, h, r, mix(cb, BIANCO, 0.06), mix(c, (0, 0, 0), 0.04));
}

/// L'icona a gradiente (`.ic`): piastrella con l'icona bianca.
fn piastrella_grad(p: &mut Painter, x: f32, y: f32, s: f32, c: Rgb, cb: Rgb, icona: &str) {
    let r = (s * 0.3).round();
    p.shadow(x, y, s, s, r, s * 0.18, 0.28, s * 0.14);
    p.rrect_vgrad(x, y, s, s, r, cb, c);
    p.rrect_line(x, y, s, s, r, BIANCO, 1.0, 0.22, None);
    let k = (s * 0.6).round();
    p.icon(icona, x + (s - k) / 2.0, y + (s - k) / 2.0, k, BIANCO);
}

impl App {
    /// Colori della pagina aperta: pieno e acceso.
    fn colori_pagina(&self) -> (Rgb, Rgb) {
        let c = &self.casa;
        page_colors(match c.att {
            _ if c.inizio => Lab::Home,
            Att::Conv if c.categoria == Kind::Pdf => Lab::Pdf,
            Att::Conv => Lab::Converti,
            Att::Tras => Lab::Trascrivi,
            Att::Yt | Att::Web => Lab::Video,
            Att::Firma => Lab::Firma,
        })
    }

    fn indice_nav(&self) -> usize {
        let c = &self.casa;
        match c.att {
            _ if c.inizio => 0,
            Att::Conv if c.categoria == Kind::Pdf => 2,
            Att::Conv => 1,
            Att::Tras => 3,
            Att::Yt | Att::Web => 4,
            Att::Firma => 5,
        }
    }

    pub(super) fn casa_cedro(&mut self, p: &mut Painter, w: f32, h: f32) {
        let tinta = self.tema.tint();
        p.glow(w * 0.15, TOP * 0.3, w * 1.25, tinta.frame_hi, 1.0);
        // La testata: il logo, i Lavori e le Impostazioni.
        p.symbol(16.0, (TOP - 28.0) / 2.0, 28.0);
        p.text("Cedro", 50.0, 0.0, 120.0, TOP, 19.0, Weight::Bold, tinta.ink, Align::Left);
        self.testata_destra(p, w, 0.0, TOP, false);
        let vista = (0.0, TOP, w, h - NAVB - TOP);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let fine = if self.casa.inizio {
            self.cedro_inizio(p, w, vista)
        } else if self.casa.att == Att::Firma {
            let y0 = TOP + 4.0 - self.scorri;
            y0 + self.corpo_firma(p, PAD + 6.0, y0, w - 2.0 * (PAD + 6.0), vista) + 8.0
        } else {
            self.cedro_attivita(p, w, vista)
        };
        p.pop_clip();
        self.scorri_max = (fine + self.scorri - (vista.1 + vista.3)).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(vista);
        let scelta = self.indice_nav();
        self.barra_cedro(p, w, h, Some(scelta));
    }

    // ------------------------------------------------------------------ la barra in basso

    pub(in crate::ui) fn barra_cedro(&mut self, p: &mut Painter, w: f32, h: f32, scelta: Option<usize>) {
        let tinta = self.tema.tint();
        let (ml, _, mr, mb) = self.margini;
        let ny = h - NAVB;
        let fondo = if tinta.light { (255, 255, 255) } else { mix(tinta.frame, (0, 0, 0), 0.4) };
        p.fill(-ml, ny, w + ml + mr, NAVB + mb, mix(tinta.frame, fondo, 0.7));
        p.fill(-ml, ny, w + ml + mr, 1.0, tinta.line);
        let bw = w / 6.0;
        for (k, (lab, icona, nome)) in pagine().into_iter().enumerate() {
            let (c, cb) = page_colors(lab);
            let bx = k as f32 * bw;
            let on = Some(k) == scelta;
            if on {
                p.rrect(bx + 3.0, ny + 5.0, bw - 6.0, NAVB - 8.0, 13.0, Some(mix(tinta.frame, cb, 0.22)), Some((mix(tinta.frame, cb, 0.5), 1.0)));
            }
            piastrella_grad(p, bx + bw / 2.0 - 15.0, ny + 9.0, 30.0, c, cb, icona);
            p.text(nome, bx, ny + 41.0, bw, 18.0, 11.5, if on { Weight::Bold } else { Weight::Semibold }, if on { tinta.ink } else { tinta.muted }, Align::Center);
            self.colpi.push((Forma::R((bx, ny, bw, NAVB)), Azione::Casa(CasaAz::Nav(k))));
        }
    }

    // ------------------------------------------------------------------ Inizio

    fn cedro_inizio(&mut self, p: &mut Painter, w: f32, vista: R) -> f32 {
        let tinta = self.tema.tint();
        let x = PAD + 6.0;
        let cw = w - 2.0 * x;
        let mut y = TOP + 6.0 - self.scorri;
        p.text("Da che file parti?", x, y, cw, 32.0, 25.0, Weight::Bold, tinta.ink, Align::Left);
        y += 34.0;
        y += p.text_wrap("Tocca un tipo, oppure condividi un file a Cedro da un'altra app.", x, y, cw, 13.5, Weight::Regular, tinta.muted, 19.0, 2) + 12.0;
        // Le sei tessere con la foto.
        let tw = (cw - GAP) / 2.0;
        let th = 112.0;
        for (i, (kind, titolo, sotto)) in TESSERE.iter().enumerate() {
            let tx = x + (i % 2) as f32 * (tw + GAP);
            let ty = y + (i / 2) as f32 * (th + GAP);
            let (c, cb) = (kind_color(*kind), kind_bright(*kind));
            p.shadow(tx, ty, tw, th, 15.0, 8.0, 0.3, 6.0);
            p.rrect(tx, ty, tw, th, 15.0, Some((12, 10, 16)), None);
            if let Some(img) = tile_image(i) {
                p.image(&img, tx, ty, tw, th, 15.0, Fit::Cover);
            }
            // La fascia del colore sotto la scritta, che sfuma verso l'alto.
            let scuro_c = mix(c, (0, 0, 0), 0.5);
            for k in 0..14 {
                let a = (k as f32 + 1.0) / 14.0;
                let yy = ty + th - 56.0 + k as f32 * 4.0;
                if yy + 4.0 <= ty + th - 18.0 {
                    p.fill_a(tx, yy, tw, 4.0, scuro_c, 0.9 * a * a);
                }
            }
            p.push_clip(tx, ty + th - 18.0, tw, 18.0);
            p.rrect_a(tx, ty + th - 60.0, tw, 60.0, 15.0, scuro_c, 0.92);
            p.pop_clip();
            p.rrect_line(tx, ty, tw, th, 15.0, BIANCO, 1.0, 0.14, None);
            p.text(titolo, tx + 11.0, ty + th - 42.0, tw - 16.0, 24.0, 16.5, Weight::Bold, BIANCO, Align::Left);
            p.text(sotto, tx + 11.0, ty + th - 22.0, tw - 14.0, 18.0, 11.5, Weight::Semibold, BIANCO, Align::Left);
            if self.premuto == Some(Azione::Casa(CasaAz::Tessera(i))) {
                p.rrect_a(tx, ty, tw, th, 15.0, BIANCO, 0.16);
            }
            self.colpo((tx, ty, tw, th), Azione::Casa(CasaAz::Tessera(i)), vista);
            let _ = cb;
        }
        y += 3.0 * th + 2.0 * GAP + 14.0;
        // La riga del link: apre Video, dove si incolla e si scarica.
        let (cv, cvb) = page_colors(Lab::Video);
        let rh = 52.0;
        p.rrect_a(x, y, cw, rh, 13.0, tinta.hover, 0.08);
        p.rrect_line(x, y, cw, rh, 13.0, tinta.line, 1.0, 1.0, None);
        piastrella_grad(p, x + 8.0, y + 11.0, 30.0, cv, cvb, ICO_WEB);
        let btn = p.measure("Scarica", 13.5, Weight::Bold) + 28.0;
        let campo = (x + 46.0, y + 8.0, cw - 46.0 - btn - 14.0, 36.0);
        p.rrect_a(campo.0, campo.1, campo.2, campo.3, 9.0, (0, 0, 0), 0.14);
        p.rrect_line(campo.0, campo.1, campo.2, campo.3, 9.0, tinta.line, 1.5, 1.0, None);
        let (testo, col) = if self.casa.link.is_empty() { ("Incolla un link di YouTube o del web".to_string(), tinta.muted) } else { (p.ellipsize(&self.casa.link, 13.0, Weight::Regular, campo.2 - 18.0), tinta.ink) };
        p.text(&testo, campo.0 + 9.0, campo.1, campo.2 - 14.0, campo.3, 13.0, Weight::Regular, col, Align::Left);
        let bx = x + cw - btn - 8.0;
        gradiente(p, bx, y + 8.0, btn, 36.0, 9.0, cv, cvb);
        p.text("Scarica", bx, y + 8.0, btn, 36.0, 13.5, Weight::Bold, BIANCO, Align::Center);
        self.colpo((x, y, cw, rh), Azione::Casa(CasaAz::Nav(4)), vista);
        y += rh + 14.0;
        // Gli ultimi file.
        let ultimi: Vec<usize> = (0..self.recenti.len()).filter(|i| self.recenti[*i].kind != Kind::Other).take(4).collect();
        if !ultimi.is_empty() {
            p.text("Ultimi file", x, y, cw, 20.0, 12.5, Weight::Bold, tinta.muted, Align::Left);
            y += 24.0;
            let cella = (cw - 3.0 * 7.0) / 4.0;
            for (k, i) in ultimi.iter().copied().enumerate() {
                let r = self.recenti[i].clone();
                let rc = (x + k as f32 * (cella + 7.0), y, cella, 58.0);
                let cb = kind_bright(r.kind);
                p.rrect(rc.0, rc.1, rc.2, rc.3, 10.0, Some((0x24, 0x1D, 0x2E)), None);
                match &r.thumb {
                    Some(img) => p.image(img, rc.0, rc.1, rc.2, rc.3, 10.0, Fit::Cover),
                    None => p.icon(icona_kind(r.kind), rc.0 + rc.2 / 2.0 - 13.0, rc.1 + rc.3 / 2.0 - 13.0, 26.0, cb),
                }
                p.push_clip(rc.0, rc.1 + rc.3 - 4.0, rc.2, 4.0);
                p.fill(rc.0, rc.1 + rc.3 - 4.0, rc.2, 4.0, cb);
                p.pop_clip();
                p.rrect_line(rc.0, rc.1, rc.2, rc.3, 10.0, BIANCO, 1.0, 0.12, None);
                self.colpo(rc, Azione::Casa(CasaAz::RecenteVai(i)), vista);
            }
            y += 58.0 + 10.0;
        }
        y + 10.0 + self.scorri
    }

    // ------------------------------------------------------------------ una attività

    /// La pagina di Converti, PDF, Trascrivi o Video: il file sul suo alone e sotto il pannello chiaro.
    fn cedro_attivita(&mut self, p: &mut Painter, w: f32, vista: R) -> f32 {
        let completa = !self.imp.simple;
        let (c, cb) = self.colori_pagina();
        let x = PAD;
        let cw = w - 2.0 * PAD;
        let ph = self.altezza_pannello(completa);
        let libero = vista.3 - 4.0 - GAP - 6.0;
        let sh = (libero - ph).max(STAGE_MIN);
        let y0 = TOP + 4.0 - self.scorri;
        match self.casa.att {
            Att::Tras => self.palco_tras(p, (x, y0, cw, sh), c, cb, vista),
            Att::Yt | Att::Web => self.palco_web(p, (x, y0, cw, sh), c, cb),
            _ => self.palco_file(p, (x, y0, cw, sh), c, cb, vista),
        }
        let py = y0 + sh + GAP;
        self.pannello_cedro(p, (x, py, cw, ph), c, cb, completa, vista);
        py + ph + 8.0 + self.scorri
    }

    /// Il riquadro scuro del file: fondo, alone del colore e bordo.
    fn palco(&self, p: &mut Painter, rc: R, cb: Rgb) {
        let tinta = self.tema.tint();
        let _ = tinta;
        p.rrect(rc.0, rc.1, rc.2, rc.3, 17.0, Some((0x09, 0x07, 0x0C)), None);
        p.push_clip(rc.0, rc.1, rc.2, rc.3);
        p.glow(rc.0 + rc.2 / 2.0, rc.1 + rc.3 * 0.42, rc.2 * 0.72, cb, 0.5);
        p.pop_clip();
    }

    fn palco_file(&mut self, p: &mut Painter, rc: R, c: Rgb, cb: Rgb, vista: R) {
        let _ = c;
        let pdf = self.casa.categoria == Kind::Pdf;
        self.palco(p, rc, cb);
        p.push_clip(rc.0, rc.1, rc.2, rc.3);
        let (x, w) = (rc.0, rc.2);
        // I tipi di file in coda, come chip in alto.
        let mut cx = x + 8.0;
        let cy = rc.1 + 8.0;
        let cat: Vec<(usize, &conversioni::Category)> = Casa::categorie().iter().enumerate().filter(|(_, k)| k.kind == self.casa.categoria || self.casa.quanti(k.kind) > 0).collect();
        for (i, k) in cat {
            let on = k.kind == self.casa.categoria;
            let n = self.casa.quanti(k.kind);
            let nm = k.name;
            let tw = p.measure(nm, 12.5, Weight::Bold) + if n > 0 { p.measure(&n.to_string(), 12.5, Weight::Bold) + 6.0 } else { 0.0 } + 32.0;
            if cx + tw > x + w - 8.0 {
                break;
            }
            p.rrect(cx, cy, tw, 28.0, 14.0, Some(if on { BIANCO } else { (0x2A, 0x25, 0x31) }), None);
            p.circle(cx + 14.0, cy + 14.0, 4.0, Some(kind_bright(k.kind)), None);
            let fg = if on { (0x20, 0x17, 0x29) } else { (0xE9, 0xE2, 0xF2) };
            p.text(nm, cx + 24.0, cy, tw, 28.0, 12.5, Weight::Bold, fg, Align::Left);
            if n > 0 {
                let nw = p.measure(nm, 12.5, Weight::Bold);
                p.text(&n.to_string(), cx + 24.0 + nw + 6.0, cy, 30.0, 28.0, 12.5, Weight::Bold, mix(fg, (128, 128, 128), 0.3), Align::Left);
            }
            self.colpo((cx, cy - 8.0, tw, 44.0), Azione::Casa(CasaAz::Categoria(i)), vista);
            cx += tw + 5.0;
        }
        let vis = self.casa.visibili();
        let strip_h = 54.0;
        let area = (x + 12.0, rc.1 + 44.0, w - 24.0, rc.3 - 44.0 - strip_h - 22.0);
        if vis.is_empty() {
            self.palco_vuoto(p, rc, area, cb, vista);
        } else {
            let k = self.casa.mostrato.min(vis.len() - 1);
            let v = self.casa.coda[vis[k]].clone();
            if pdf {
                self.tavola_pagine(p, &v, area, cb);
            } else {
                // Il file in grande, 4:3 al massimo, con l'ombra.
                let aw = area.2.min(area.3 * 4.0 / 3.0);
                let (iw, ih) = match v.thumb.as_ref() {
                    Some(img) => {
                        let (a, b) = (img.w as f32, img.h as f32);
                        let k = (area.2 / a).min(area.3 / b);
                        (a * k, b * k)
                    }
                    None => (aw.min(area.3), aw.min(area.3)),
                };
                let (ix, iy) = (area.0 + (area.2 - iw) / 2.0, area.1 + (area.3 - ih) / 2.0);
                p.shadow(ix, iy, iw, ih, 9.0, 12.0, 0.6, 10.0);
                self.miniatura_voce(p, &v, (ix, iy, iw, ih), 9.0);
                p.rrect_line(ix, iy, iw, ih, 9.0, BIANCO, 1.0, 0.16, None);
            }
            // Nome e misure.
            let mut sotto = peso_corto(v.dim);
            if let Some((a, b)) = v.misure {
                sotto = format!("{a} × {b} · {sotto}");
            }
            if pdf {
                if let Some(n) = self.pdf_pagine.get(&v.path) {
                    sotto = format!("{n} pagine · {sotto}");
                }
            }
            let nome = p.ellipsize(&v.nome, 12.5, Weight::Bold, w - 30.0);
            let nw = p.measure(&nome, 12.5, Weight::Bold);
            let sw = p.measure(&sotto, 11.5, Weight::Regular);
            let x0 = x + (w - (nw + 10.0 + sw).min(w - 20.0)) / 2.0;
            p.text(&nome, x0, rc.1 + rc.3 - strip_h - 20.0, nw + 4.0, 18.0, 12.5, Weight::Bold, SC_INK, Align::Left);
            p.text(&sotto, x0 + nw + 10.0, rc.1 + rc.3 - strip_h - 20.0, (w - 24.0 - nw).max(0.0), 18.0, 11.5, Weight::Regular, SC_MUT, Align::Left);
        }
        self.striscia_file(p, (x, rc.1 + rc.3 - strip_h, w, strip_h), &vis, cb, vista);
        p.pop_clip();
        p.rrect_line(rc.0, rc.1, rc.2, rc.3, 17.0, BIANCO, 1.0, 0.07, None);
    }

    /// Nessun file in coda: un invito, con «Aggiungi file» e «Cartella».
    fn palco_vuoto(&mut self, p: &mut Painter, rc: R, area: R, cb: Rgb, vista: R) {
        let cx = rc.0 + rc.2 / 2.0;
        let cy = area.1 + area.3 / 2.0 - 12.0;
        p.circle(cx, cy - 18.0, 28.0, Some(mix((0x20, 0x1A, 0x28), cb, 0.25)), None);
        p.icon(ICO_FOLDER, cx - 14.0, cy - 32.0, 28.0, cb);
        let testo = if self.casa.att == Att::Tras { "Aggiungi un audio o un video da trascrivere" } else { "Aggiungi i file, o condividili a Cedro da un'altra app" };
        p.text_wrap(testo, rc.0 + 28.0, cy + 18.0, rc.2 - 56.0, 13.5, Weight::Semibold, SC_INK, 18.0, 2);
    }

    /// Le pagine del PDF sul fondo scuro, come fogli numerati.
    fn tavola_pagine(&mut self, p: &mut Painter, v: &Voce, area: R, cb: Rgb) {
        let n = self.pdf_pagine.get(&v.path).copied().unwrap_or(0) as usize;
        let n = if n == 0 { 8 } else { n.min(8) };
        let colonne = 4usize;
        let gap = 8.0;
        let fw = ((area.2 - gap * (colonne as f32 - 1.0)) / colonne as f32).min(80.0);
        let fh = fw * 297.0 / 210.0;
        let righe = n.div_ceil(colonne);
        let tot_w = fw * colonne.min(n) as f32 + gap * (colonne.min(n) as f32 - 1.0);
        let x0 = area.0 + (area.2 - tot_w) / 2.0;
        let y0 = area.1 + ((area.3 - (fh * righe as f32 + gap * (righe as f32 - 1.0))) / 2.0).max(0.0);
        for i in 0..n {
            let (fx, fy) = (x0 + (i % colonne) as f32 * (fw + gap), y0 + (i / colonne) as f32 * (fh + gap));
            if fy + fh > area.1 + area.3 + 4.0 {
                break;
            }
            p.shadow(fx, fy, fw, fh, 4.0, 6.0, 0.5, 5.0);
            p.rrect(fx, fy, fw, fh, 4.0, Some(BIANCO), None);
            p.rrect(fx + 6.0, fy + 8.0, fw * 0.5, 5.0, 1.0, Some(cb), None);
            for l in 0..((fh - 26.0) / 6.5) as usize {
                p.fill(fx + 6.0, fy + 20.0 + l as f32 * 6.5, fw - 12.0 - if l % 4 == 3 { fw * 0.3 } else { 0.0 }, 1.5, (0xD9, 0xD5, 0xE0));
            }
            p.circle(fx + fw - 4.0, fy + fh - 4.0, 8.5, Some((0x17, 0x12, 0x1D)), Some((mix((0x17, 0x12, 0x1D), BIANCO, 0.3), 1.5)));
            p.text(&(i + 1).to_string(), fx + fw - 12.5, fy + fh - 12.5, 17.0, 17.0, 10.0, Weight::Bold, BIANCO, Align::Center);
        }
    }

    /// La striscia in fondo al palco: le miniature dei file, «Aggiungi» e il cestino.
    fn striscia_file(&mut self, p: &mut Painter, rc: R, vis: &[usize], cb: Rgb, vista: R) {
        let mostrato = self.casa.mostrato.min(vis.len().saturating_sub(1));
        let aggiungi = 92.0;
        let cestino = 40.0;
        let (tw, th) = (50.0, 38.0);
        let max = (((rc.2 - 16.0 - aggiungi - cestino - 12.0) / (tw + 6.0)).floor() as usize).max(1);
        let ty = rc.1 + (rc.3 - th) / 2.0 + 2.0;
        let mut x = rc.0 + 10.0;
        for (k, i) in vis.iter().copied().take(max).enumerate() {
            let v = self.casa.coda[i].clone();
            let on = k == mostrato;
            self.miniatura_voce(p, &v, (x, ty, tw, th), 7.0);
            if !on {
                p.rrect_a(x, ty, tw, th, 7.0, (0x09, 0x07, 0x0C), 0.4);
            } else {
                p.rrect(x - 2.0, ty - 2.0, tw + 4.0, th + 4.0, 9.0, None, Some((cb, 2.0)));
            }
            // La spunta: tocca e il file entra o esce dal lavoro.
            let (sx, sy) = (x + tw - 9.0, ty + 9.0);
            if v.scelto {
                p.circle(sx, sy, 7.5, Some(cb), Some(((0x09, 0x07, 0x0C), 1.5)));
                p.icon(I_SPUNTA, sx - 5.0, sy - 5.0, 10.0, (0x15, 0x10, 0x1C));
            } else {
                p.circle(sx, sy, 7.5, Some((0x09, 0x07, 0x0C)), Some((BIANCO, 1.5)));
            }
            self.colpo((x, ty, tw - 18.0, th), Azione::Casa(CasaAz::Mostra(k)), vista);
            self.colpo((x + tw - 22.0, ty - 6.0, 28.0, 28.0), Azione::Casa(CasaAz::Spunta(i)), vista);
            x += tw + 6.0;
        }
        if vis.len() > max {
            p.text(&format!("+{}", vis.len() - max), x, ty, 30.0, th, 12.0, Weight::Bold, SC_MUT, Align::Left);
            x += 30.0;
        }
        p.rrect_line(x, ty, aggiungi, th, 7.0, BIANCO, 1.5, 0.35, Some((4.0, 3.0)));
        p.icon(ic::ADD, x + 9.0, ty + th / 2.0 - 8.0, 16.0, (0xE9, 0xE2, 0xF2));
        p.text("Aggiungi", x + 28.0, ty, aggiungi - 30.0, th, 12.5, Weight::Bold, (0xE9, 0xE2, 0xF2), Align::Left);
        self.colpo((x - 4.0, ty - 6.0, aggiungi + 8.0, th + 12.0), Azione::Casa(CasaAz::Aggiungi), vista);
        if !vis.is_empty() {
            let (bx, by) = (rc.0 + rc.2 - cestino - 6.0, rc.1 + (rc.3 - 34.0) / 2.0 + 2.0);
            p.circle(bx + 17.0, by + 17.0, 17.0, Some(mix((0x09, 0x07, 0x0C), BIANCO, 0.14)), None);
            p.icon(ICO_TRASH, bx + 8.0, by + 8.0, 18.0, BIANCO);
            self.colpo((bx - 4.0, by - 8.0, 48.0, 48.0), Azione::Casa(CasaAz::Togli(vis[mostrato])), vista);
        }
    }

    /// Trascrivi: l'onda dell'audio sul palco, e sotto la striscia dei file.
    fn palco_tras(&mut self, p: &mut Painter, rc: R, c: Rgb, cb: Rgb, vista: R) {
        let _ = c;
        self.palco(p, rc, cb);
        p.push_clip(rc.0, rc.1, rc.2, rc.3);
        let vis = self.casa.visibili();
        let strip_h = 54.0;
        if vis.is_empty() {
            let area = (rc.0 + 12.0, rc.1 + 8.0, rc.2 - 24.0, rc.3 - strip_h - 16.0);
            self.palco_vuoto(p, rc, area, cb, vista);
        } else {
            let k = self.casa.mostrato.min(vis.len() - 1);
            let v = self.casa.coda[vis[k]].clone();
            // L'onda: una barra per ogni tratto, dal nome del file (disegno fermo: il motore non la dà prima di trascrivere).
            let n = (((rc.2 - 28.0) / 5.0) as usize).clamp(20, 70);
            let mut s: u64 = v.nome.bytes().fold(1469598103934665603u64, |a, b| (a ^ b as u64).wrapping_mul(1099511628211)) | 1;
            let cy = rc.1 + (rc.3 - strip_h - 22.0) / 2.0;
            let massimo = (rc.3 - strip_h - 50.0).max(24.0);
            let bw = (rc.2 - 28.0) / n as f32;
            for i in 0..n {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                let f = (s % 1000) as f32 / 1000.0;
                let hh = (massimo * (0.16 + 0.84 * f * (0.45 + 0.55 * (i as f32 * 0.37).sin().abs()))).max(4.0);
                p.rrect(rc.0 + 14.0 + i as f32 * bw + 0.5, cy - hh / 2.0, (bw - 1.5).max(1.5), hh, 1.5, Some(cb), None);
            }
            let nome = p.ellipsize(&v.nome, 12.5, Weight::Bold, rc.2 - 30.0);
            let sotto = peso_corto(v.dim);
            let nw = p.measure(&nome, 12.5, Weight::Bold);
            p.text(&nome, rc.0 + 14.0, rc.1 + rc.3 - strip_h - 20.0, nw + 4.0, 18.0, 12.5, Weight::Bold, SC_INK, Align::Left);
            p.text(&sotto, rc.0 + 24.0 + nw, rc.1 + rc.3 - strip_h - 20.0, 90.0, 18.0, 11.5, Weight::Regular, SC_MUT, Align::Left);
        }
        self.striscia_file(p, (rc.0, rc.1 + rc.3 - strip_h, rc.2, strip_h), &vis, cb, vista);
        p.pop_clip();
        p.rrect_line(rc.0, rc.1, rc.2, rc.3, 17.0, BIANCO, 1.0, 0.07, None);
    }

    /// Video dal web: la scheda del video col pulsante di avvio, il sito sotto.
    fn palco_web(&mut self, p: &mut Painter, rc: R, c: Rgb, cb: Rgb) {
        let _ = c;
        self.palco(p, rc, cb);
        p.push_clip(rc.0, rc.1, rc.2, rc.3);
        let area = (rc.0 + 14.0, rc.1 + 10.0, rc.2 - 28.0, rc.3 - 52.0);
        let vw = area.2.min(area.3 * 16.0 / 9.0);
        let vh = vw * 9.0 / 16.0;
        let (vx, vy) = (area.0 + (area.2 - vw) / 2.0, area.1 + (area.3 - vh) / 2.0);
        p.shadow(vx, vy, vw, vh, 10.0, 12.0, 0.6, 10.0);
        p.rrect(vx, vy, vw, vh, 10.0, Some(mix((0x12, 0x0F, 0x16), cb, 0.22)), None);
        p.disc_a(vx + vw / 2.0, vy + vh / 2.0, 23.0, (0, 0, 0), 0.55);
        p.icon(ICO_PLAY, vx + vw / 2.0 - 11.0, vy + vh / 2.0 - 11.0, 22.0, BIANCO);
        p.rrect_line(vx, vy, vw, vh, 10.0, BIANCO, 1.0, 0.16, None);
        let link = self.casa.link.trim().to_string();
        let (t1, t2) = if let Some(u) = cedro::webvideo::find_url(&link) {
            let host = u.split_once("://").map(|(_, r)| r).unwrap_or(&u).split('/').next().unwrap_or("").to_string();
            (host, "Il video è pronto da scaricare".to_string())
        } else {
            ("Incolla un link".to_string(), "YouTube e gli altri siti, in un posto solo".to_string())
        };
        let t1 = p.ellipsize(&t1, 13.0, Weight::Bold, rc.2 - 30.0);
        p.text(&t1, rc.0, rc.1 + rc.3 - 40.0, rc.2, 20.0, 13.0, Weight::Bold, SC_INK, Align::Center);
        p.text(&t2, rc.0, rc.1 + rc.3 - 22.0, rc.2, 18.0, 11.5, Weight::Regular, SC_MUT, Align::Center);
        p.pop_clip();
        p.rrect_line(rc.0, rc.1, rc.2, rc.3, 17.0, BIANCO, 1.0, 0.07, None);
    }

    // ------------------------------------------------------------------ il pannello chiaro

    /// I formati da mostrare come tessere: i primi sette (più quello scelto, se è oltre) e «Altri» se ce ne sono di più.
    fn formati_visibili(&self) -> (Vec<usize>, bool) {
        let tutte = self.operazioni();
        let scelta = tutte.iter().position(|s| s.0 == self.casa.operazione()).unwrap_or(0);
        if tutte.len() <= 8 {
            return ((0..tutte.len()).collect(), false);
        }
        let mut v: Vec<usize> = (0..7).collect();
        if scelta >= 7 {
            v[6] = scelta;
        }
        (v, true)
    }

    fn righe_opzioni(&self) -> usize {
        let (a, b) = self.principali();
        a.is_some() as usize + b.is_some() as usize + (self.altre_n() > 0) as usize
    }

    /// L'altezza del pannello, calcolata con le stesse misure con cui si disegna.
    fn altezza_pannello(&self, completa: bool) -> f32 {
        let c = &self.casa;
        let testa = PP + 30.0 + GAP;
        match c.att {
            Att::Tras => testa + RIGA + if completa { GAP + RIGA } else { 0.0 } + GAP + CTA_H + PP,
            Att::Yt | Att::Web => testa + 44.0 + GAP + FMT_H + if completa && c.web == 3 { GAP + RIGA } else { 0.0 } + GAP + CTA_H + PP,
            _ => {
                let (v, _) = self.formati_visibili();
                let rows = if v.len() > 4 || self.operazioni().len() > 8 { 2.0 } else { 1.0 };
                let extra = if completa { self.righe_opzioni() as f32 * (GAP + RIGA) } else { 0.0 };
                testa + rows * FMT_H + (rows - 1.0) * 6.0 + extra + GAP + RIGA + GAP + CTA_H + PP
            }
        }
    }

    fn pannello_cedro(&mut self, p: &mut Painter, rc: R, c: Rgb, cb: Rgb, completa: bool, vista: R) {
        let q = tinte::pannello(self.tema);
        let (x, y, w, h) = rc;
        p.shadow(x, y, w, h, 17.0, 14.0, 0.32, -3.0);
        p.rrect(x, y, w, h, 17.0, Some(q.sf), None);
        // Si disegna con la palette chiara e il colore della pagina (la prende `pal()`).
        self.pannello = Some(c);
        let (ix, iw) = (x + PP, w - 2.0 * PP);
        let mut cy = y + PP;
        let titolo = match self.casa.att {
            Att::Tras => "Trascrivi",
            Att::Yt | Att::Web => "Scarica dal web",
            _ if self.casa.categoria == Kind::Pdf => "Cosa fare del PDF",
            _ => "Converti in",
        };
        p.text(titolo, ix, cy, iw - 150.0, 30.0, 15.5, Weight::Bold, q.ink, Align::Left);
        self.seg_vista(p, ix + iw, cy, completa, vista);
        cy += 30.0 + GAP;
        match self.casa.att {
            Att::Tras => {
                self.riga_precisione(p, ix, cy, iw, vista);
                cy += RIGA + GAP;
                if completa {
                    let testo = match self.voci {
                        0 => "Automatico".to_string(),
                        1 => "Una persona sola".to_string(),
                        n => format!("{n} persone"),
                    };
                    self.riga_scelta(p, ix, cy, iw, "Chi parla", &testo, Azione::Casa(CasaAz::Apri(Elenco::Voci)), vista);
                    cy += RIGA + GAP;
                }
            }
            Att::Yt | Att::Web => {
                let link = self.casa.link.clone();
                let bw = 96.0;
                p.rrect(ix, cy, iw, 44.0, 12.0, Some(q.alt), Some((q.line, 1.0)));
                self.campo_testo(p, (ix + 4.0, cy + 4.0, iw - bw - 12.0, 36.0), Campo::Link, &link, "Incolla il link", "", false, vista);
                let b = (ix + iw - bw - 4.0, cy + 4.0, bw, 36.0);
                let a = Azione::Casa(CasaAz::Incolla);
                p.rrect(b.0, b.1, b.2, b.3, 10.0, Some(if self.premuto == Some(a) { q.line } else { q.sf }), Some((q.line, 1.0)));
                p.icon(ICO_PASTE, b.0 + 10.0, b.1 + 9.0, 18.0, c);
                p.text("Incolla", b.0 + 26.0, b.1, b.2 - 28.0, b.3, 13.0, Weight::Bold, q.ink, Align::Center);
                self.colpo(b, a, vista);
                cy += 44.0 + GAP;
                let cw = (iw - 3.0 * 6.0) / 4.0;
                for (k, (id, nome, _)) in WEB.iter().enumerate() {
                    let on = self.casa.web == k;
                    let colore = if *id == "web-testo" { page_colors(Lab::Trascrivi).0 } else if *id == "web-audio" { kind_color(Kind::Audio) } else { c };
                    let gx = ix + k as f32 * (cw + 6.0);
                    self.tessera_formato(p, (gx, cy, cw, FMT_H), super::super::icona_di(id), "", colore, mix(colore, BIANCO, 0.35), nome, on, c, cb);
                    self.colpo((gx, cy, cw, FMT_H), Azione::Casa(CasaAz::Web(k)), vista);
                }
                cy += FMT_H + GAP;
                if completa && self.casa.web == 3 {
                    self.riga_precisione(p, ix, cy, iw, vista);
                    cy += RIGA + GAP;
                }
            }
            _ => {
                let tutte = self.operazioni();
                let (idx, altri) = self.formati_visibili();
                let scelta = self.casa.operazione();
                let mut celle: Vec<Option<usize>> = idx.into_iter().map(Some).collect();
                if altri {
                    celle.push(None);
                }
                let cw = (iw - 3.0 * 6.0) / 4.0;
                for (n, cella) in celle.iter().enumerate() {
                    let gx = ix + (n % 4) as f32 * (cw + 6.0);
                    let gy = cy + (n / 4) as f32 * (FMT_H + 6.0);
                    let rc = (gx, gy, cw, FMT_H);
                    match cella {
                        Some(k) => {
                            let (id, nome) = tutte[*k];
                            let v = voci::voce(self.casa.categoria, id);
                            self.tessera_formato(p, rc, v.glyph.stroke, v.glyph.fill, v.color, mix(v.color, BIANCO, 0.35), nome, id == scelta, c, cb);
                            self.colpo(rc, Azione::Casa(CasaAz::Formato(*k)), vista);
                        }
                        None => {
                            self.tessera_formato(p, rc, "M4 12h.01M12 12h.01M20 12h.01", "", mix(q.mut_, (0, 0, 0), 0.1), mix(q.mut_, BIANCO, 0.3), "Altri", false, c, cb);
                            self.colpo(rc, Azione::Casa(CasaAz::Apri(Elenco::Formato)), vista);
                        }
                    }
                }
                cy += (celle.len().div_ceil(4)) as f32 * FMT_H + (celle.len().div_ceil(4) as f32 - 1.0) * 6.0;
                if completa {
                    let (scelta_opz, sw) = self.principali();
                    if let Some((k, d)) = scelta_opz {
                        cy += GAP;
                        let tx = self.testo_scelta(d);
                        self.riga_scelta(p, ix, cy, iw, d.title, &tx, Azione::Casa(CasaAz::Apri(Elenco::Opzione(k))), vista);
                        cy += RIGA;
                    }
                    if let Some((k, d)) = sw {
                        cy += GAP;
                        let on = self.casa.valori().get(d) != 0;
                        p.text(d.title, ix, cy, iw - 60.0, RIGA, 13.5, Weight::Semibold, q.ink, Align::Left);
                        self.interruttore_cedro(p, ix + iw - 38.0, cy + RIGA / 2.0, on, c);
                        self.colpo((ix, cy - 4.0, iw, RIGA + 8.0), Azione::Casa(CasaAz::Interruttore(k)), vista);
                        cy += RIGA;
                    }
                    let n = self.altre_n();
                    if n > 0 {
                        cy += GAP;
                        self.riga_scelta(p, ix, cy, iw, "Altre opzioni", &format!("{n}"), Azione::Casa(CasaAz::AltreOpzioni), vista);
                        cy += RIGA;
                    }
                }
                cy += GAP;
                let e = match self.esiste {
                    cedro::Exists::Replace => "sostituisco",
                    cedro::Exists::Skip => "lo salto",
                    _ => "aggiungo -2",
                };
                self.riga_scelta(p, ix, cy, iw, "Salva in", &format!("Documenti/Cedro · {e}"), Azione::Casa(CasaAz::Apri(Elenco::Esiste)), vista);
                cy += RIGA + GAP;
            }
        }
        // Il pulsante grande, sempre in fondo.
        let (testo, icona) = self.testo_cta();
        let pronto = self.cta_pronta();
        let b = (ix, cy, iw, CTA_H);
        let a = Azione::Casa(CasaAz::Vai);
        if pronto {
            p.shadow(b.0, b.1, b.2, b.3, 14.0, 10.0, 0.3, 6.0);
            let prem = self.premuto == Some(a);
            gradiente(p, b.0, b.1, b.2, b.3, 14.0, if prem { mix(c, (0, 0, 0), 0.2) } else { c }, if prem { c } else { mix(c, BIANCO, 0.18) });
            p.rrect_line(b.0, b.1, b.2, b.3, 14.0, BIANCO, 1.0, 0.2, None);
        } else {
            p.rrect(b.0, b.1, b.2, b.3, 14.0, Some(q.line), None);
        }
        let fg = if pronto { BIANCO } else { q.mut_ };
        let tw = p.measure(&testo, 16.0, Weight::Bold).min(b.2 - 60.0);
        let x0 = b.0 + (b.2 - tw - 30.0) / 2.0;
        p.icon(icona, x0, b.1 + (b.3 - 22.0) / 2.0, 22.0, fg);
        p.text(&testo, x0 + 30.0, b.1, tw + 4.0, b.3, 16.0, Weight::Bold, fg, Align::Left);
        if pronto {
            self.colpo(b, a, vista);
        }
        self.pannello = None;
    }

    /// Semplice | Completa, in alto a destra del pannello.
    fn seg_vista(&mut self, p: &mut Painter, destra: f32, y: f32, completa: bool, vista: R) {
        let q = tinte::pannello(self.tema);
        let voci = ["Semplice", "Completa"];
        let larghe: Vec<f32> = voci.iter().map(|v| p.measure(v, 12.0, Weight::Bold) + 20.0).collect();
        let tot = larghe.iter().sum::<f32>() + 6.0;
        let x0 = destra - tot;
        p.rrect(x0, y + 1.0, tot, 28.0, 9.0, Some(q.alt), Some((q.line, 1.0)));
        let mut x = x0 + 3.0;
        for (i, v) in voci.iter().enumerate() {
            let on = (i == 1) == completa;
            if on {
                p.shadow(x, y + 4.0, larghe[i], 22.0, 7.0, 3.0, 0.2, 1.0);
                p.rrect(x, y + 4.0, larghe[i], 22.0, 7.0, Some(q.sf), None);
            }
            p.text(v, x, y + 4.0, larghe[i], 22.0, 12.0, Weight::Bold, if on { q.ink } else { q.mut_ }, Align::Center);
            self.colpo((x - 3.0, y - 6.0, larghe[i] + 6.0, 44.0), Azione::Casa(CasaAz::Vista(i == 1)), vista);
            x += larghe[i];
        }
    }

    /// Una tessera di formato del pannello: l'icona colorata e il nome. Scelta: fondo tinto e bordo del colore.
    #[allow(clippy::too_many_arguments)]
    fn tessera_formato(&mut self, p: &mut Painter, rc: R, tratto: &str, pieno: &str, col: Rgb, col_b: Rgb, nome: &str, on: bool, c: Rgb, cb: Rgb) {
        let q = tinte::pannello(self.tema);
        let (x, y, w, h) = rc;
        p.rrect(x, y, w, h, 11.0, Some(if on { mix(q.sf, cb, 0.2) } else { q.alt }), None);
        p.rrect(x, y, w, h, 11.0, None, Some(if on { (c, 2.0) } else { (q.line, 1.0) }));
        let s = 26.0;
        let (tx, ty) = (x + (w - s) / 2.0, y + 7.0);
        p.rrect_vgrad(tx, ty, s, s, 8.0, col_b, col);
        if !pieno.is_empty() {
            p.icon_fill(pieno, tx + 5.0, ty + 5.0, 16.0, BIANCO);
        }
        p.icon(tratto, tx + 5.0, ty + 5.0, 16.0, BIANCO);
        let nome = p.ellipsize(nome, 12.5, Weight::Bold, w - 6.0);
        p.text(&nome, x, y + 36.0, w, 20.0, 12.5, Weight::Bold, q.ink, Align::Center);
        let _ = self;
    }

    /// Una riga «etichetta ... valore ▾» che apre un elenco.
    #[allow(clippy::too_many_arguments)]
    fn riga_scelta(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, etichetta: &str, valore: &str, az: Azione, vista: R) {
        let q = tinte::pannello(self.tema);
        let ew = p.measure(etichetta, 13.5, Weight::Semibold) + 6.0;
        p.text(etichetta, x, y, ew, RIGA, 13.5, Weight::Semibold, q.mut_, Align::Left);
        let bx = x + ew + 8.0;
        let bw = w - ew - 8.0;
        let prem = self.premuto == Some(az);
        p.rrect(bx, y + 2.0, bw, RIGA - 4.0, 8.0, Some(if prem { q.line } else { q.alt }), Some((q.line, 1.0)));
        let t = p.ellipsize(valore, 13.0, Weight::Bold, bw - 36.0);
        p.text(&t, bx + 10.0, y + 2.0, bw - 30.0, RIGA - 4.0, 13.0, Weight::Bold, q.ink, Align::Left);
        p.icon(I_GIU, bx + bw - 24.0, y + RIGA / 2.0 - 7.0, 14.0, q.mut_);
        self.colpo((x, y - 3.0, w, RIGA + 6.0), az, vista);
    }

    /// «Precisione»: Veloce | Buona | Massima (il modello di trascrizione).
    fn riga_precisione(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, vista: R) {
        let q = tinte::pannello(self.tema);
        let (c, _) = page_colors(Lab::Trascrivi);
        let ew = p.measure("Precisione", 13.5, Weight::Semibold) + 10.0;
        p.text("Precisione", x, y, ew, RIGA, 13.5, Weight::Semibold, q.mut_, Align::Left);
        let nomi = ["Veloce", "Buona", "Massima"];
        let bx = x + ew;
        let bw = w - ew;
        p.rrect(bx, y + 2.0, bw, RIGA - 4.0, 9.0, Some(q.alt), Some((q.line, 1.0)));
        let sw = (bw - 6.0) / 3.0;
        for (k, n) in nomi.iter().enumerate() {
            let on = self.modello.min(2) == k;
            let rx = bx + 3.0 + k as f32 * sw;
            if on {
                p.rrect(rx, y + 5.0, sw, RIGA - 10.0, 7.0, Some(c), None);
            }
            p.text(n, rx, y + 5.0, sw, RIGA - 10.0, 12.0, Weight::Bold, if on { BIANCO } else { q.mut_ }, Align::Center);
            self.colpo((rx, y - 4.0, sw, RIGA + 8.0), Azione::Casa(CasaAz::Modello(k)), vista);
        }
    }

    fn interruttore_cedro(&self, p: &mut Painter, x: f32, cy: f32, on: bool, c: Rgb) {
        let q = tinte::pannello(self.tema);
        p.rrect(x, cy - 11.0, 38.0, 22.0, 11.0, Some(if on { c } else { q.line }), None);
        p.circle(if on { x + 27.0 } else { x + 11.0 }, cy, 8.0, Some(BIANCO), None);
    }
}

/// «2,8 MB» senza spazi superflui.
fn peso_corto(b: u64) -> String {
    super::super::peso(b)
}
