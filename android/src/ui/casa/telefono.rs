//! Variante A: attività nella home, tre destinazioni fisse, catalogo intero e azione sempre raggiungibile.
//! Il catalogo e le opzioni sono quelli del motore; nessun contenuto dimostrativo viene disegnato nell'app.
use super::*;

const TOP: f32 = 62.0;
const NAVB: f32 = 66.0;
const PAD: f32 = 16.0;
const GAP: f32 = 8.0;
const ICO_HOME: &str = "M4 11l8-7 8 7v9h-5v-6H9v6H4z";
const ICO_LAB: &str = "M9 3h6M10 3v7L4 20h16l-6-10V3M8 15h8";

/// Gli indici restano compatibili con `vista_nav` e con le azioni già salvate negli esempi.
const ATTIVITA: [(&str, &str, &str, Rgb, Rgb); 6] = [
    ("Converti", "Foto, documenti, audio", ic::CONVERT, (184, 233, 207), (20, 39, 31)),
    ("PDF", "Unisci, dividi, comprimi", I_PDF, (255, 185, 173), (75, 34, 28)),
    ("Trascrivi", "Ascolta e rivedi", ic::TRANSCRIBE, (255, 209, 160), (81, 45, 14)),
    ("Video dal web", "Link in video o audio", ic::WEB, (199, 216, 255), (37, 55, 90)),
    ("Firma", "Apri e verifica", ic::SHIELD, (213, 199, 250), (53, 33, 94)),
    ("Laboratorio", "Vetrini, DICOM, casi", ICO_LAB, (197, 229, 222), (24, 76, 64)),
];

impl App {
    pub(super) fn casa_cedro(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let nome = if self.casa.inizio { "Cedro" } else if self.casa.laboratorio { "Laboratorio" } else { self.casa.att.corto() };
        if self.casa.inizio {
            p.symbol(16.0, 12.0, 31.0);
        } else {
            self.bottone_icona(p, (4.0, 5.0, 48.0, 48.0), ic::ARROW_LEFT, Azione::Casa(CasaAz::Nav(0)));
        }
        p.text(nome, 56.0, 0.0, w - 124.0, TOP, if self.casa.inizio { 23.0 } else { 18.0 }, Weight::Bold, pal.text, Align::Left);
        if self.casa.inizio {
            p.rrect(w - 64.0, 5.0, 48.0, 48.0, 12.0, None, Some((pal.border, 1.0)));
            self.bottone_icona(p, (w - 64.0, 5.0, 48.0, 48.0), ic::ADD, Azione::Casa(CasaAz::Aggiungi));
        }
        p.fill(0.0, TOP - 1.0, w, 1.0, pal.border);
        let cta = !self.casa.inizio && self.casa.att != Att::Firma;
        let testo = self.testo_principale_a();
        let piede = if cta { misure_azione_a(p, w, &testo).1 } else { 0.0 };
        let famiglie_h=if !self.casa.inizio && !self.casa.laboratorio && self.casa.att==Att::Conv && self.casa.categoria==Kind::Image && !self.imp.simple {
            self.barra_famiglie_a(p,w)
        }else{0.0};
        self.casa.famiglie_y.clear();
        let cima=TOP+famiglie_h;
        let vista = (0.0, cima, w, (h - cima - NAVB - piede).max(1.0));
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let y = cima + 20.0 - self.scorri;
        let fine = if self.casa.inizio {
            self.inizio_a(p, w, y, vista)
        } else if self.casa.att == Att::Firma {
            y + self.corpo_firma(p, PAD, y, w - 2.0 * PAD, vista)
        } else {
            self.attivita_a(p, w, y, vista)
        };
        p.pop_clip();
        self.scorri_max = (fine + 24.0 + self.scorri - (vista.1 + vista.3)).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(vista);
        if cta {
            self.azione_fissa_a(p, w, h - NAVB, &testo, self.cta_pronta(), Azione::Casa(CasaAz::Vai));
        }
        self.barra_cedro(p, w, h, Some(0));
    }

    pub(in crate::ui) fn barra_cedro(&mut self, p: &mut Painter, w: f32, h: f32, scelta: Option<usize>) {
        let pal = self.pal();
        let (ml, _, mr, mb) = self.margini;
        let y = h - NAVB;
        p.fill(-ml, y, w + ml + mr, NAVB + mb, pal.bg);
        p.fill(0.0, y, w, 1.0, pal.border);
        let attiva = match self.sopra { Some(Scheda::Lavori) => 1, Some(Scheda::Aspetto) => 2, _ => scelta.unwrap_or(0) };
        let voci = [("Inizio", ICO_HOME, CasaAz::Nav(0)), ("Lavori", ic::CLOCK, CasaAz::Lavori), ("Impostazioni", ic::SETTINGS, CasaAz::Impostazioni)];
        let bw = (w - 10.0) / 3.0;
        for (i, (nome, icona, az)) in voci.into_iter().enumerate() {
            let x = 5.0 + i as f32 * bw;
            if i == attiva { p.rrect(x, y + 4.0, bw - 2.0, 56.0, 12.0, Some(pal.alt), None); }
            let c = if i == attiva { pal.primary } else { pal.muted };
            p.icon(icona, x + (bw - 21.0) / 2.0, y + 10.0, 21.0, c);
            p.text(nome, x + 2.0, y + 35.0, bw - 6.0, 23.0, 11.0, Weight::Semibold, c, Align::Center);
            self.colpi.push((Forma::R((x, y + 2.0, bw, 60.0)), Azione::Casa(az)));
        }
    }

    fn inizio_a(&mut self, p: &mut Painter, w: f32, y: f32, vista: R) -> f32 {
        let pal = self.pal();
        let largo = w >= 600.0;
        let cw = if largo { (w - 3.0 * PAD) / 2.0 } else { w - 2.0 * PAD };
        let mut cy = y;
        cy += testo_a(p, "I tuoi file.", PAD, cy, cw, 28.0, Weight::Bold, pal.text, 30.0, 0);
        cy += testo_a(p, "Come ti servono.", PAD, cy, cw, 28.0, Weight::Bold, pal.primary, 30.0, 0);
        cy += 9.0;
        cy += testo_a(p, "Conversioni, PDF, voce e laboratorio.\nTutto sul tuo telefono.", PAD, cy, cw, 14.0, Weight::Regular, pal.muted, 20.0, 0) + 17.0;
        let bh = 56.0_f32.max(altezza_a(p, "Apri un file", 17.0, Weight::Bold, cw - 60.0, 22.0, 0) + 20.0);
        self.bottone_a(p, (PAD, cy, cw, bh), "Apri un file", ic::ADD, pal.primary, pal.on_primary, Azione::Casa(CasaAz::Aggiungi), vista);
        cy += bh + 20.0;
        let gx = if largo { 2.0 * PAD + cw } else { PAD };
        let mut gy = if largo { y } else { cy };
        let colonne = if self.grandi || p.text_scale > 1.25 { 1 } else { 2 };
        let tw = (cw - (colonne - 1) as f32 * 10.0) / colonne as f32;
        for row in ATTIVITA.chunks(colonne) {
            let th = row.iter().map(|(nome, sotto, _, _, _)| {
                26.0_f32.max(altezza_a(p, nome, p.fit_size(nome, 18.0, Weight::Bold, tw - 60.0), Weight::Bold, tw - 60.0, 23.0, 0)) + altezza_a(p, sotto, 12.0, Weight::Regular, tw - 26.0, 17.0, 0) + 34.0
            }).fold(96.0_f32, f32::max);
            for (j, (nome, sotto, icona, bg, ink)) in row.iter().enumerate() {
                let i = ATTIVITA.iter().position(|v| v.0 == *nome).unwrap();
                let x = gx + j as f32 * (tw + 10.0);
                p.rrect(x, gy, tw, th, 14.0, Some(*bg), None);
                p.icon(icona, x + 13.0, gy + 13.0, 26.0, *ink);
                let font = p.fit_size(nome, 18.0, Weight::Bold, tw - 60.0);
                let nh = testo_a(p, nome, x + 47.0, gy + 13.0, tw - 60.0, font, Weight::Bold, *ink, 23.0, 0);
                testo_a(p, sotto, x + 13.0, gy + 21.0 + nh.max(26.0), tw - 26.0, 12.0, Weight::Regular, *ink, 17.0, 0);
                self.colpo((x, gy, tw, th), Azione::Casa(CasaAz::Nav(i + 1)), vista);
            }
            gy += th + 10.0;
        }
        cy = cy.max(gy) + 6.0;
        if !self.recenti.is_empty() {
            cy += self.titolo_a(p, PAD, cy, w - 2.0 * PAD, "Riprendi da qui");
            for i in 0..self.recenti.len().min(4) {
                let r = self.recenti[i].clone();
                p.fill(PAD, cy, w - 2.0 * PAD, 1.0, pal.border);
                p.icon(icona_kind(r.kind), PAD + 8.0, cy + 19.0, 26.0, colore_kind(r.kind, self.scuro()));
                p.text(&cedro::i18n::lit(&r.nome), PAD + 48.0, cy + 10.0, w - 2.0 * PAD - 96.0, 44.0, 14.0, Weight::Semibold, pal.text, Align::Left);
                self.bottone_icona(p, (w - PAD - 48.0, cy + 8.0, 48.0, 48.0), I_FRECCIA, Azione::Casa(CasaAz::RecenteVai(i)));
                cy += 64.0;
            }
        }
        cy
    }

    /// Un componente condiviso per sezioni del catalogo, Labs e opzioni.
    pub(in crate::ui) fn titolo_a(&self, p: &mut Painter, x: f32, y: f32, w: f32, titolo: &str) -> f32 {
        testo_a(p, titolo, x, y, w, 17.0, Weight::Bold, self.pal().text, 23.0, 0) + 12.0
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ui) fn bottone_a(&mut self, p: &mut Painter, r: R, titolo: &str, ico: &str, bg: Rgb, fg: Rgb, az: Azione, vista: R) {
        p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(bg), None);
        if !ico.is_empty() { p.icon(ico, r.0 + 14.0, r.1 + (r.3 - 22.0) / 2.0, 22.0, fg); }
        if titolo == "Apri un file" {
            // Il pulsante della home centra testo ed icona come nell'HTML approvato.
            p.rrect(r.0, r.1, r.2, r.3, 13.0, Some(bg), None);
            let tw = p.measure(titolo, 17.0, Weight::Bold);
            let xx = r.0 + (r.2 - tw - 32.0) / 2.0;
            p.icon(ico, xx, r.1 + (r.3 - 22.0) / 2.0, 22.0, fg);
            p.text(titolo, xx + 32.0, r.1, tw + 2.0, r.3, 17.0, Weight::Bold, fg, Align::Left);
            self.colpo(r, az, vista);
            return;
        }
        let left = if ico.is_empty() { 12.0 } else { 46.0 };
        let hh = altezza_a(p, titolo, 15.0, Weight::Semibold, r.2 - left - 12.0, 20.0, 0);
        testo_a(p, titolo, r.0 + left, r.1 + (r.3 - hh) / 2.0, r.2 - left - 12.0, 15.0, Weight::Semibold, fg, 20.0, 0);
        self.colpo(r, az, vista);
    }

    fn testo_principale_a(&self) -> String {
        if self.casa.scelti().is_empty() {
            match self.casa.att { Att::Conv => return "Converti".into(), Att::Tras => return "Trascrivi".into(), _ => {} }
        }
        let (mut testo, _) = self.testo_cta();
        if self.casa.att == Att::Conv && self.casa.scelti().len() == 1 {
            let nome = voci::voce(self.casa.categoria, self.casa.operazione()).label;
            testo = if conversioni::formats(self.casa.categoria).iter().any(|f| f.id == self.casa.operazione()) { format!("Crea {}", cedro::i18n::tr(nome)) } else { cedro::i18n::tr(nome).into_owned() };
        }
        testo
    }

    pub(in crate::ui) fn azione_fissa_a(&mut self, p: &mut Painter, w: f32, bottom: f32, testo: &str, pronto: bool, az: Azione) {
        let pal = self.pal();
        let (th, piede) = misure_azione_a(p, w, testo);
        let bh = piede - 24.0;
        p.fill(0.0, bottom - piede, w, piede, pal.bg);
        p.fill(0.0, bottom - piede, w, 1.0, pal.border);
        let r = (14.0, bottom - bh - 13.0, w - 28.0, bh);
        let att = if self.casa_aperta { self.casa.att } else { Att::Conv };
        let kind = if self.casa_aperta { self.casa.categoria } else { self.kind };
        let accent = match att {
            Att::Tras => (255, 189, 137),
            Att::Web | Att::Yt => (164, 193, 255),
            _ => match kind { Kind::Image => (133, 218, 176), Kind::Pdf => (255, 169, 159), Kind::Audio => (255, 189, 137), Kind::Archive => (239, 206, 119), _ => (166, 197, 255) },
        };
        let c = if pronto { if self.tema.tint().light { pal.primary } else { accent } } else { pal.surface };
        p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(c), None);
        let fg = if pronto { pal.on_primary } else { pal.muted };
        let tw = p.measure(testo, 15.0, Weight::Bold);
        if th <= 24.0 && tw + 48.0 < r.2 {
            let xx = r.0 + (r.2 - tw - 32.0) / 2.0;
            p.icon(ic::ARROW_RIGHT, xx, r.1 + (r.3 - 22.0) / 2.0, 22.0, fg);
            p.text(testo, xx + 32.0, r.1, tw + 2.0, r.3, 15.0, Weight::Bold, fg, Align::Left);
        } else {
            testo_centrato_a(p, testo, (r.0 + 12.0, r.1 + (r.3 - th) / 2.0, r.2 - 24.0, th), 15.0, Weight::Bold, fg, 20.0);
        }
        self.colpo(r, if pronto { az } else { Azione::Niente }, r);
    }

    fn attivita_a(&mut self, p: &mut Painter, w: f32, y: f32, vista: R) -> f32 {
        let pal = self.pal();
        let largo = w >= 600.0;
        let cw = if largo { (w - 3.0 * PAD) / 2.0 } else { w - 2.0 * PAD };
        let mut cy = y;
        if self.casa.laboratorio { cy=self.pannello_labs(p,PAD,cy,cw,vista); }
        if matches!(self.casa.att, Att::Web | Att::Yt) {
            cy += self.web_a(p, PAD, cy, cw, vista);
        } else {
            cy = self.file_a(p, PAD, cy, cw, vista);
        }
        if self.casa.att==Att::Tras {cy+=self.anteprima_trascrizione(p,(PAD,cy,cw,vista.3),vista);}
        let x = if largo { cw + 2.0 * PAD } else { PAD };
        let mut fy = if largo { y } else { cy + 14.0 };
        match self.casa.att {
            Att::Conv => {
                fy = self.catalogo_a(p, x, fy, cw, vista);
                fy += 12.0;
                let scelta=voci::voce(self.casa.categoria,self.casa.operazione());
                fy+=testo_a(p,scelta.label,x,fy,cw,18.0,Weight::Bold,schiarisci(scelta.color,self.scuro()),24.0,0)+4.0;
                fy+=testo_a(p,scelta.hint,x,fy,cw,14.0,Weight::Regular,pal.muted,20.0,0)+14.0;
                if self.casa.laboratorio {
                    fy+=testo_a(p,"Tessere AI, zona mitotica e consulto richiedono SVS, NDPI o MRXS. La copia anonima di CZI, ND2 e LIF non è supportata dal motore.",x,fy,cw,13.0,Weight::Regular,self.avviso(),19.0,0)+12.0;
                }
                fy += self.titolo_a(p, x, fy, cw, "Opzioni del risultato");
                for (k, d) in self.casa.defs().into_iter().enumerate() {
                    match d.control {
                        Control::Choice(_) | Control::Segments(_) => {
                            let valore = self.testo_scelta(d);
                            fy += self.opzione_a(p, x, fy, cw, d.title, &valore, Azione::Casa(CasaAz::Apri(Elenco::Opzione(k))), vista);
                        }
                        Control::Switch => {
                            let on = self.casa.valori().get(d) != 0;
                            let hh = altezza_a(p, d.title, 15.0, Weight::Regular, cw - 64.0, 21.0, 0).max(32.0) + 20.0;
                            testo_a(p, d.title, x, fy + 10.0, cw - 64.0, 15.0, Weight::Regular, pal.text, 21.0, 0);
                            self.interruttore_casa(p, x + cw - 46.0, fy + hh / 2.0, on);
                            self.colpo((x, fy, cw, hh), Azione::Casa(CasaAz::Interruttore(k)), vista);
                            fy += hh + 4.0;
                        }
                        _ => {}
                    }
                }
                // Numeri, testo e controlli speciali mantengono il loro editor completo.
                if self.casa.defs().iter().any(|d| !matches!(d.control, Control::Choice(_) | Control::Segments(_) | Control::Switch)) {
                    fy += self.opzione_a(p, x, fy, cw, "Altre opzioni", "", Azione::Casa(CasaAz::AltreOpzioni), vista);
                }
                fy += self.opzione_a(p, x, fy, cw, "Salva in", "Documenti/Cedro", Azione::Casa(CasaAz::Apri(Elenco::Esiste)), vista);
                if !self.casa.laboratorio {
                    fy += self.titolo_a(p, x, fy, cw, "Tipo di file");
                    // Tutte le categorie sono raggiungibili anche quando la coda è vuota.
                    for (i, cat) in Casa::categorie().iter().enumerate() {
                        let r = (x, fy, cw, 48.0);
                        self.bottone_a(p, r, cat.name, icona_kind(cat.kind), if cat.kind == self.casa.categoria { pal.alt } else { pal.surface }, pal.text, Azione::Casa(CasaAz::Categoria(i)), vista);
                        fy += 54.0;
                    }
                    self.bottone_a(p, (x, fy, cw, 48.0), "Cartella", ic::FOLDER, pal.surface, pal.primary, Azione::Casa(CasaAz::Cartella), vista);
                    fy += 56.0;
                }

            }
            Att::Tras => { fy += self.trascrizione_a(p, x, fy, cw, vista); }
            Att::Web | Att::Yt => {
                if self.casa.web == 3 { fy += self.trascrizione_a(p, x, fy, cw, vista); }
            }
            _ => {}
        }
        cy.max(fy)
    }

    fn file_a(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let vis = self.casa.visibili();
        for (j, i) in vis.iter().copied().enumerate() {
            let v = self.casa.coda[i].clone();
            let nh = altezza_a(p, &cedro::i18n::lit(&v.nome), 14.0, Weight::Semibold, w - 112.0, 19.0, 0);
            let hh = (nh + 62.0).max(106.0);
            p.rrect(x, y, w, hh, 15.0, Some(pal.surface), Some((pal.border, 1.0)));
            self.miniatura_voce(p, &v, (x + 12.0, y + 12.0, 76.0, 82.0), 8.0);
            p.text(if v.scelto { "FILE SELEZIONATO" } else { "File" }, x + 100.0, y + 10.0, w - 112.0, 20.0, 11.0, Weight::Bold, colore_kind(v.kind, self.scuro()), Align::Left);
            testo_a(p, &cedro::i18n::lit(&v.nome), x + 100.0, y + 34.0, w - 112.0, 14.0, Weight::Semibold, pal.text, 19.0, 0);
            let ext = std::path::Path::new(&v.nome).extension().and_then(|e| e.to_str()).unwrap_or("").to_uppercase();
            let peso = super::super::peso(v.dim);
            let meta = if ext.is_empty() { peso } else { format!("{ext} · {peso}") };
            p.text(&meta, x + 100.0, y + 36.0 + nh, w - 112.0, 24.0, 12.0, Weight::Regular, pal.muted, Align::Left);
            y += hh + 6.0;
            for (n, (ico, az)) in [(if v.scelto { ic::CHECK } else { ic::ADD }, CasaAz::Spunta(i)), (ic::TRASH, CasaAz::Togli(i))].into_iter().enumerate() {
                let r = (x + n as f32 * 56.0, y, 48.0, 48.0);
                p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(pal.surface), Some((pal.border, 1.0)));
                p.icon(ico, r.0 + 12.0, r.1 + 12.0, 24.0, pal.primary);
                self.colpo(r, Azione::Casa(az), vista);
            }
            self.bottone_a(p, (x + 112.0, y, w - 112.0, 48.0), "Aggiungi file", ic::ADD, pal.surface, pal.primary, Azione::Casa(CasaAz::Aggiungi), vista);
            y += 56.0;
            if self.casa.laboratorio && crate::labs::laboratorio(&v.path) {
                self.bottone_a(p,(x,y,w,48.0),"Anteprima",ic::IMAGE,pal.surface,pal.primary,Azione::Labs(super::super::labs::LabAz::File(i)),vista);
                y+=56.0;
            }
            if self.casa.att==Att::Tras {
                self.bottone_a(p,(x,y,w,48.0),"Onda e testo",ic::AUDIO,pal.surface,pal.primary,Azione::Casa(CasaAz::Ascolta(i)),vista);
                y+=56.0;
            }
            let _ = j;
        }
        if vis.is_empty() {
            y += testo_a(p, "Aggiungi i file, o condividili a Cedro da un'altra app", x, y, w, 15.0, Weight::Regular, pal.muted, 22.0, 0) + 12.0;
        }
        if vis.is_empty() {
            self.bottone_a(p, (x, y, w, 48.0), "Aggiungi file", ic::ADD, pal.surface, pal.primary, Azione::Casa(CasaAz::Aggiungi), vista);
            y += 56.0;
        }
        y
    }

    fn barra_famiglie_a(&mut self,p:&mut Painter,w:f32)->f32 {
        let famiglie=famiglie_immagine_a();
        // Le etichette tedesche e i font di sistema grandi determinano quante colonne entrano.
        let disponibili=w-2.0*PAD;
        let desiderata=famiglie.iter().map(|(nome,_)|p.measure(nome,12.0,Weight::Semibold)+16.0).fold(48.0,f32::max);
        let colonne=(((disponibili+GAP)/(desiderata+GAP)).floor() as usize).clamp(1,famiglie.len());
        let bw=(disponibili-(colonne-1) as f32*GAP)/colonne as f32;
        let bh=famiglie.iter().map(|(nome,_)|altezza_a(p,nome,12.0,Weight::Semibold,bw-8.0,17.0,0)+20.0).fold(48.0,f32::max);
        let rows=famiglie.len().div_ceil(colonne);
        let height=rows as f32*(bh+GAP)+4.0;
        let pal=self.pal();
        p.fill(0.0,TOP,w,height,pal.bg);
        for (i,(nome,col)) in famiglie.into_iter().enumerate() {
            let r=(PAD+(i%colonne) as f32*(bw+GAP),TOP+4.0+(i/colonne) as f32*(bh+GAP),bw,bh);
            let tinta=schiarisci(col,self.scuro());
            p.rrect(r.0,r.1,r.2,r.3,8.0,Some(pal.surface),Some((tinta,1.5)));
            p.fill(r.0+8.0,r.1+5.0,r.2-16.0,3.0,tinta);
            testo_a(p,nome,r.0+4.0,r.1+12.0,r.2-8.0,12.0,Weight::Semibold,pal.text,17.0,0);
            self.colpi.push((Forma::R(r),Azione::Casa(CasaAz::Famiglia(nome))));
        }
        height
    }

    fn catalogo_a(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let k = self.casa.categoria;
        let ops = self.operazioni();
        let nf = conversioni::formats(k).iter().filter(|s| s.id != "none").count();
        if !self.casa.laboratorio {
            y += testo_a(p, "Converti in", x, y, w, 28.0, Weight::Bold, self.pal().text, 32.0, 0) + 16.0;
        }
        if !self.casa.laboratorio {
            let bw=(w-8.0)/2.0;
            for (i,titolo) in ["Semplice","Completa"].into_iter().enumerate(){
                self.bottone_a(p,(x+i as f32*(bw+8.0),y,bw,48.0),titolo,"",if self.imp.simple==(i==0){self.pal().alt}else{self.pal().surface},self.pal().text,Azione::Casa(CasaAz::Vista(i==1)),vista);
            }
            y+=60.0;
        }
        let mut gruppi: Vec<(&'static str, Vec<usize>)> = Vec::new();
        for (i, (id, _)) in ops.iter().enumerate() {
            if !self.casa.laboratorio && self.imp.simple && i<nf && !conversioni::simple_formats(k).iter().any(|f|f.id==*id) {continue;}
            let v = voci::voce(k, id);
            let lab = voci::is_lab(&v);
            if self.casa.laboratorio && !lab && !matches!(*id,"png"|"jpeg"|"tiff"|"pdf"|"strip") { continue; }
            let titolo = if lab { "Laboratorio" } else if i < nf { famiglia_a(k, id) } else { voci::tools_heading(k) };
            if let Some(g) = gruppi.iter_mut().find(|g| g.0 == titolo) { g.1.push(i); } else { gruppi.push((titolo, vec![i])); }
        }
        for (titolo, mut idx) in gruppi {
            self.casa.famiglie_y.insert(titolo,y+self.scorri-vista.1);
            if titolo == "Uso quotidiano" {
                idx.sort_by_key(|i| ["jpeg", "png", "webp", "pdf"].iter().position(|id| *id == ops[*i].0).unwrap_or(99));
            }
            let tinta=idx.first().map(|i|schiarisci(voci::voce(k,ops[*i].0).color,self.scuro())).unwrap_or(self.pal().primary);
            y += testo_a(p, titolo, x, y, w, 12.0, Weight::Bold, tinta, 17.0, 0) + 12.0;
            let cols = if self.grandi || titolo == "Laboratorio" || titolo == voci::tools_heading(k) { 2 } else { 3 };
            let tw = (w - (cols - 1) as f32 * GAP) / cols as f32;
            for row in idx.chunks(cols) {
                let hh = row.iter().map(|i| altezza_a(p, if self.casa.laboratorio&&ops[*i].0=="strip"{"Copia anonima"}else{ops[*i].1}, 14.0, Weight::Semibold, tw - 12.0, 19.0, 0) + 46.0).fold(65.0_f32, f32::max);
                for (j, i) in row.iter().copied().enumerate() {
                    let (id, mut nome) = ops[i];
                    if self.casa.laboratorio && id=="strip" {nome="Copia anonima";}
                    let v = voci::voce(k, id);
                    let r = (x + j as f32 * (tw + GAP), y, tw, hh);
                    self.formato_a(p, r, nome, &v, id == self.casa.operazione());
                    self.colpo(r, Azione::Casa(CasaAz::Formato(i)), vista);
                }
                y += hh + GAP;
            }
            y += 16.0;
        }
        y
    }

    pub(in crate::ui) fn formato_a(&self, p: &mut Painter, r: R, nome: &str, v: &voci::Voce, on: bool) {
        let pal = self.pal();
        let col = schiarisci(v.color, self.scuro());
        p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(if on { pal.alt } else { pal.surface }), Some((if on { col } else { pal.border }, if on { 2.0 } else { 1.0 })));
        p.icon(v.glyph.stroke, r.0 + (r.2 - 21.0) / 2.0, r.1 + 12.0, 21.0, col);
        testo_centrato_a(p, nome, (r.0 + 6.0, r.1 + 39.0, r.2 - 12.0, r.3 - 39.0), 14.0, Weight::Semibold, pal.text, 19.0);
    }

    #[allow(clippy::too_many_arguments)]
    fn opzione_a(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, titolo: &str, valore: &str, az: Azione, vista: R) -> f32 {
        let pal = self.pal();
        let th = altezza_a(p, titolo, 14.0, Weight::Regular, w - 16.0, 20.0, 0);
        testo_a(p, titolo, x, y + 6.0, w - 16.0, 14.0, Weight::Regular, pal.muted, 20.0, 0);
        let bh = altezza_a(p, valore, 15.0, Weight::Semibold, w - 54.0, 21.0, 0).max(28.0) + 20.0;
        self.bottone_a(p, (x, y + th + 12.0, w, bh), if valore.is_empty() { titolo } else { valore }, I_GIU, pal.surface, pal.text, az, vista);
        th + bh + 24.0
    }

    fn trascrizione_a(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let start = y;
        let pal = self.pal();
        p.text("Referti",x,y,w-64.0,48.0,16.0,Weight::Semibold,pal.text,Align::Left);
        self.interruttore_casa(p,x+w-46.0,y+24.0,self.referto);
        self.colpo((x,y,w,48.0),Azione::Casa(CasaAz::Referto),vista);
        y+=56.0;
        y+=testo_a(p,"Numeri e parole decisive da riascoltare. Finché restano punti aperti, salvi una bozza.",x,y,w,13.0,Weight::Regular,pal.muted,19.0,0)+12.0;
        y += self.titolo_a(p, x, y, w, "Precisione");
        for (i, nome) in ["Veloce", "Buona", "Massima"].into_iter().enumerate() {
            self.bottone_a(p, (x, y, w, 52.0), nome, if self.modello == i { ic::CHECK } else { "" }, if self.modello == i { pal.alt } else { pal.surface }, pal.text, Azione::Casa(CasaAz::Modello(i)), vista);
            y += 60.0;
        }
        let v = match self.voci { 0 => "Automatico".into(), 1 => "Una persona sola".into(), n => format!("{n} persone") };
        y += self.opzione_a(p, x, y, w, "Chi parla", &v, Azione::Casa(CasaAz::Apri(Elenco::Voci)), vista);
        y - start
    }

    pub(in crate::ui) fn impostazioni_a(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        self.titolo_barra(p, 16.0, w - 32.0, "Impostazioni");
        let cw = (w - 32.0).min(600.0);
        let x = (w - cw) / 2.0;
        let vista = (0.0, TOP, w, (h - TOP).max(1.0));
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let mut y = TOP + 20.0 - self.scorri;
        y += testo_a(p, "A modo tuo.", x, y, cw, 28.0, Weight::Bold, pal.text, 32.0, 0) + 24.0;
        y += self.titolo_a(p, x, y, cw, "Tinta di Cedro");
        let tw = (cw - 9.0) / 2.0;
        for (i, tema) in crate::ui::tinte::CEDRO.into_iter().enumerate() {
            let r = (x + (i % 2) as f32 * (tw + 9.0), y + (i / 2) as f32 * 61.0, tw, 52.0);
            p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(pal.surface), Some((if self.tema == tema { pal.primary } else { pal.border }, if self.tema == tema { 2.0 } else { 1.0 })));
            p.circle(r.0 + 25.0, r.1 + 26.0, 12.5, Some(tema.tint().frame), Some((pal.muted, 1.0)));
            p.text(crate::ui::tinte::nome(tema), r.0 + 48.0, r.1, r.2 - 58.0, r.3, 14.0, Weight::Semibold, pal.text, Align::Left);
            self.colpo(r, Azione::Tema(i), vista);
        }
        y += 138.0;
        let lingua = super::super::LINGUE.iter().find(|(k, _)| *k == self.lingua).map(|(k, n)| if *k == "auto" { n.to_string() } else { cedro::i18n::lit(n) }).unwrap_or_default();
        y += self.opzione_a(p, x, y, cw, "Lingua", &lingua, Azione::Casa(CasaAz::Apri(Elenco::Lingua)), vista);
        y += 16.0;
        y += self.titolo_a(p, x, y, cw, "Leggibilità");
        for (i, nome) in ["Normali", "Grandi"].into_iter().enumerate() {
            self.bottone_a(p, (x, y, cw, 52.0), nome, if self.grandi == (i == 1) { ic::CHECK } else { "" }, pal.surface, pal.text, Azione::Caratteri(i == 1), vista);
            y += 60.0;
        }
        y += testo_a(p, "Animazioni: segue il telefono", x, y, cw, 14.0, Weight::Regular, pal.muted, 20.0, 0) + 24.0;
        y += self.opzione_a(p, x, y, cw, "Temi precedenti", self.tema.name(), Azione::Casa(CasaAz::Apri(Elenco::Tema)), vista);
        y += 16.0;
        y += self.impostazioni_rete(p,x,y,cw,vista)+16.0;
        for (nome, ico, az) in [("Nome dei file, qualità, OCR", ic::SETTINGS, Azione::Opz(OpzAz::ApriImpostazioni)), ("Informazioni", ic::INFO, Azione::Casa(CasaAz::Informazioni)), ("Guida", ic::HELP, Azione::Casa(CasaAz::Guida))] {
            let hh = altezza_a(p, nome, 15.0, Weight::Semibold, cw - 58.0, 21.0, 0).max(28.0) + 20.0;
            self.bottone_a(p, (x, y, cw, hh), nome, ico, pal.surface, pal.text, az, vista);
            y += hh + 8.0;
        }
        p.pop_clip();
        self.scorri_max = (y + 24.0 + self.scorri - h).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        if self.casa.elenco.is_some() { self.foglio_elenco(p, w, h); }
    }

    fn web_a(&mut self, p: &mut Painter, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let start = y;
        let pal = self.pal();
        let link = self.casa.link.clone();
        self.campo_testo(p, (x, y, w, 52.0), Campo::Link, &link, "Incolla il link", "", false, vista);
        y += 60.0;
        self.bottone_a(p, (x, y, w, 48.0), "Incolla", ic::COPY, pal.surface, pal.primary, Azione::Casa(CasaAz::Incolla), vista);
        y += 64.0;
        for (i, (id, nome, sotto)) in WEB.iter().enumerate() {
            let hh = altezza_a(p, sotto, 13.0, Weight::Regular, w - 24.0, 18.0, 0) + 60.0;
            self.bottone_a(p, (x, y, w, hh), nome, super::super::icona_di(id), if i == self.casa.web { pal.alt } else { pal.surface }, pal.text, Azione::Casa(CasaAz::Web(i)), vista);
            y += hh + 8.0;
        }
        y += self.pannello_web(p,x,y,w,vista);
        y - start
    }
}

#[allow(clippy::too_many_arguments)]
fn testo_centrato_a(p: &mut Painter, testo: &str, r: R, size: f32, weight: Weight, color: Rgb, line_h: f32) {
    let text = righe_a(p, testo, size, weight, r.2);
    let lines = p.wrap_lines(&text, size, weight, r.2, 0);
    let lh = line_h * if p.larger { cedro::ui::gfx::text_size(size) / size } else { 1.0 };
    for (i, line) in lines.into_iter().enumerate() {
        p.text(&cedro::i18n::lit(&line), r.0, r.1 + i as f32 * lh, r.2, lh, size, weight, color, Align::Center);
    }
}

/// Altezza del testo e dell'intero piede: identiche per layout, clip, scorrimento e disegno.
pub(in crate::ui) fn misure_azione_a(p: &Painter, w: f32, testo: &str) -> (f32, f32) {
    let th = altezza_a(p, testo, 15.0, Weight::Bold, w - 52.0, 20.0, 0);
    (th, th.max(32.0) + 44.0)
}

fn famiglia_a(kind: Kind, id: &str) -> &'static str {
    voci::family(kind,&voci::voce(kind,id)).unwrap_or("Formato")
}

/// Painter traduce già i testi. Qui si aggiunge il ritorno a capo dentro le parole lunghe,
/// indispensabile per tedesco, nomi di file e caratteri grandi nelle tessere strette.
fn righe_a(p: &Painter, s: &str, size: f32, weight: Weight, width: f32) -> String {
    let translated = cedro::i18n::tr(s);
    let mut out = String::new();
    for (n, line) in translated.split('\n').enumerate() {
        if n > 0 { out.push('\n'); }
        for (i, word) in line.split(' ').enumerate() {
            if i > 0 { out.push(' '); }
            let mut part = String::new();
            for c in word.chars() {
                let next = format!("{part}{c}");
                if !part.is_empty() && p.measure(&cedro::i18n::lit(&next), size, weight) > width {
                    out.push_str(&part);
                    out.push('\n');
                    part.clear();
                }
                part.push(c);
            }
            out.push_str(&part);
        }
    }
    cedro::i18n::lit(&out)
}

pub(in crate::ui) fn altezza_a(p: &Painter, s: &str, size: f32, weight: Weight, w: f32, lh: f32, max: usize) -> f32 {
    p.wrap_height(&righe_a(p, s, size, weight, w), size, weight, w, lh, max)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn testo_a(p: &mut Painter, s: &str, x: f32, y: f32, w: f32, size: f32, weight: Weight, c: Rgb, lh: f32, max: usize) -> f32 {
    p.text_wrap(&righe_a(p, s, size, weight, w), x, y, w, size, weight, c, lh, max)
}

/// Ordine, nomi e colori provengono dal catalogo condiviso, come i gruppi sottostanti.
fn famiglie_immagine_a()->Vec<(&'static str,Rgb)> {
    let mut out=Vec::new();
    for f in conversioni::formats(Kind::Image).iter().filter(|f|f.id!="none") {
        let voce=voci::voce(Kind::Image,f.id);
        if voci::is_lab(&voce){continue;}
        if let Some(nome)=voci::family(Kind::Image,&voce) {
            if !out.iter().any(|(n,_)|*n==nome){out.push((nome,voce.color));}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cedro::ui::gfx::{Fonts, GlyphCache};
    use std::collections::BTreeSet;

    fn frame(a: &mut App, w: f32, h: f32, cache: &mut GlyphCache) {
        let mut p = Painter::new(w as u32, h as u32, 1.0, Fonts::for_theme(a.tema), cache);
        p.larger = a.grandi;
        a.disegna(&mut p, w, h);
    }

    #[test]
    fn cinque_famiglie_restano_visibili_e_saltano_ai_formati() {
        let mut cache=GlyphCache::new();
        for (w,h) in [(360.0,800.0),(360.0,640.0),(800.0,400.0)] {
            for grandi in [false,true] {
                let mut a=App::new(ThemeId::Cedro);a.anim.calmo=true;a.vai_casa();a.vista_nav(1);a.vista_completa(true);a.grandi=grandi;
                frame(&mut a,w,h,&mut cache);
                let famiglie=famiglie_immagine_a();assert_eq!(famiglie.len(),5);
                for (nome,col) in famiglie {
                    let r=a.colpi.iter().find_map(|(f,az)|if *az==Azione::Casa(CasaAz::Famiglia(nome)){if let Forma::R(r)=f{Some(*r)}else{None}}else{None}).unwrap();
                    assert!(r.2>=48.0&&r.3>=48.0&&r.0>=0.0&&r.0+r.2<=w&&r.1+r.3<h-66.0);
                    let target=a.casa.famiglie_y[nome];
                    a.tocca_giu(r.0+r.2/2.0,r.1+r.3/2.0);a.tocca_su(r.0+r.2/2.0,r.1+r.3/2.0);
                    assert!((a.scorri-target).abs()<0.001);
                    frame(&mut a,w,h,&mut cache);
                    let ops=a.operazioni();
                    assert!(a.colpi.iter().any(|(f,az)|{
                        let Azione::Casa(CasaAz::Formato(i))=az else{return false};
                        let voce=voci::voce(Kind::Image,ops[*i].0);
                        let Forma::R(r)=f else{return false};
                        voci::family(Kind::Image,&voce)==Some(nome)&&voce.color==col&&r.2>=48.0&&r.3>=48.0
                    }),"{nome}: {w}x{h}, grandi={grandi}");
                }
            }
        }
    }

    #[test]
    fn variante_a_catalogo_tocchi_e_navigazione() {
        let mut cache = GlyphCache::new();
        for (w, h) in [(360.0, 800.0), (360.0, 640.0), (800.0, 400.0)] {
            let mut a = App::new(ThemeId::Cedro);
            a.anim.calmo = true;
            a.vai_casa();
            a.imp.simple=false;
            frame(&mut a, w, h, &mut cache);
            let nav: Vec<_> = a.colpi.iter().filter(|(f, _)| matches!(f, Forma::R(r) if r.1 >= h - NAVB)).collect();
            assert_eq!(nav.len(), 3);
            for (f, _) in &nav { if let Forma::R(r) = f { assert!(r.2 >= 48.0 && r.3 >= 48.0); } }
            // I tocchi, non soltanto il dispatch, aprono le tre destinazioni.
            let x = w / 2.0;
            a.tocca_giu(x, h - 30.0); a.tocca_su(x, h - 30.0);
            assert_eq!(a.sopra, Some(Scheda::Lavori));
            frame(&mut a, w, h, &mut cache);
            a.tocca_giu(w * 5.0 / 6.0, h - 30.0); a.tocca_su(w * 5.0 / 6.0, h - 30.0);
            assert_eq!(a.sopra, Some(Scheda::Aspetto));
            frame(&mut a, w, h, &mut cache);
            a.tocca_giu(w / 6.0, h - 30.0); a.tocca_su(w / 6.0, h - 30.0);
            assert!(a.sopra.is_none() && a.casa.inizio);
            a.vista_nav(6);
            assert!(a.casa.laboratorio && a.casa.att == Att::Conv);
            a.vista_nav(1);
            assert!(!a.casa.laboratorio);
            for line in include_str!("../../../tests/azioni.txt").lines().filter(|l| !l.starts_with('#')) {
                let mut words = line.split_whitespace();
                let kind = match words.next().unwrap() { "Image" => Kind::Image, "Doc" => Kind::Doc, "Pdf" => Kind::Pdf, "Text" => Kind::Text, "Audio" => Kind::Audio, "Video" => Kind::Video, "Archive" => Kind::Archive, _ => unreachable!() };
                a.casa.categoria = kind;
                a.casa.coda.clear();
                a.aggiungi_coda(vec![(PathBuf::from("/tmp/cedro-ui-fixture"), "campione".into(), kind)]);
                a.casa.att = Att::Conv;
                a.casa.inizio = false;
                a.scorri = 0.0;
                frame(&mut a, w, h, &mut cache);
                let max = a.scorri_max;
                let mut seen = BTreeSet::new();
                let mut scroll = 0.0;
                loop {
                    a.scorri = scroll;
                    frame(&mut a, w, h, &mut cache);
                    let ops = a.operazioni();
                    for (shape, az) in &a.colpi {
                        if let Azione::Casa(CasaAz::Formato(i)) = az { seen.insert(ops[*i].0); }
                        if let Azione::Casa(CasaAz::Vai) = az {
                            let Forma::R(r) = shape else { panic!("CTA non rettangolare") };
                            assert!(r.2 >= 48.0 && r.3 >= 48.0 && r.1 + r.3 <= h - NAVB);
                        }
                    }
                    if scroll >= max { break; }
                    scroll = (scroll + (h - TOP - NAVB - 76.0) * 0.7).min(max);
                }
                for id in words { assert!(seen.contains(id), "{kind:?} azione {id} non raggiungibile a {w}x{h}"); }
            }
        }
    }

    #[test]
    fn foglio_con_azione_multilinea_non_copre_opzioni_o_tocchi() {
        // Larghezza effettiva ridotta: esercita il wrapping come un ingrandimento del testo.
        let (w, h) = (180.0, 640.0);
        let mut a = App::new(ThemeId::Cedro);
        a.grandi = true;
        a.anim.calmo = true;
        a.imposta_file(Archivio { path: "/tmp/campioni.zip".into(), nome: "campioni.zip".into(), dim: 100 }, Kind::Archive, true);
        let i = a.fette().iter().position(|f| f.id == "z-pdf").unwrap();
        a.attiva(Azione::FoglioFormato(0, i));
        let mut cache = GlyphCache::new();
        frame(&mut a, w, h, &mut cache);
        assert!(a.scorri_max > 0.0);
        a.scorri = a.scorri_max;
        frame(&mut a, w, h, &mut cache);
        let mut p = Painter::new(w as u32, h as u32, 1.0, Fonts::for_theme(a.tema), &mut cache);
        p.larger = true;
        let label = format!("Crea {}", cedro::i18n::tr("Documenti in PDF"));
        let (testo_h, piede) = misure_azione_a(&p, w, &label);
        assert!(testo_h > 32.0 && piede > 76.0, "il caso deve esercitare la CTA multilinea");
        let limite = h - piede;
        let mut opzioni_visibili = false;
        let mut cta = None;
        for (forma, az) in &a.colpi {
            let Forma::R(r) = forma else { continue };
            match az {
                Azione::AltreOpzioni => {
                    // L’intersezione usa f32: 56 dp può essere rappresentato come 55.99997 dopo le sottrazioni.
                    assert!((r.3 - 56.0).abs() < 0.001, "l'ultima opzione deve essere interamente raggiungibile: {}", r.3);
                    assert!(r.1 + r.3 <= limite);
                    opzioni_visibili = true;
                }
                Azione::FoglioFormato(_, _) => assert!(r.1 + r.3 <= limite),
                Azione::Vai => {
                    assert!(r.1 >= limite && r.1 + r.3 <= h && r.2 >= 48.0 && r.3 >= 48.0);
                    cta = Some(*r);
                }
                _ => {}
            }
        }
        assert!(opzioni_visibili);
        let r = cta.expect("CTA raggiungibile dopo lo scorrimento");
        let (tx, ty) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
        a.tocca_giu(tx, ty);
        assert!(matches!(a.tocca_su(tx, ty), Effetto::Converti { azione: "z-pdf", .. }));
    }

    #[test]
    fn strumenti_con_opzioni_non_diventano_formati() {
        let mut a = App::new(ThemeId::Cedro);
        a.vai_casa();
        a.casa.operazione.insert(ck(Kind::Image), "mitosi");
        a.azione_casa(CasaAz::AltreOpzioni);
        let o = a.opzioni.as_ref().unwrap();
        assert_eq!(crate::ui::opzioni::formati(Kind::Image)[o.formato].id, "mitosi");
        a.azione_opzioni(OpzAz::Indietro);
        assert_eq!(a.casa.operazione(), "mitosi");
        a.imposta_file(Archivio { path: "/tmp/foto.heic".into(), nome: "foto.heic".into(), dim: 100 }, Kind::Image, true);
        assert!(a.fette().iter().any(|f| f.id == "avif"));
        let avif = a.fette().iter().position(|f| f.id == "avif").unwrap();
        a.attiva(Azione::FoglioFormato(0, avif));
        assert!(!a.foglio && a.dal_foglio);
        let result = a.attiva(Azione::Vai);
        assert!(matches!(result, Effetto::Converti { azione: "avif", .. }));
        // Regressione 0.45: PDF scelto nella ruota ma foglio ancora attivo avviava JPEG.
        a.imposta_file(Archivio { path: "/tmp/foto.heic".into(), nome: "foto.heic".into(), dim: 100 }, Kind::Image, true);
        let pdf = a.fette().iter().position(|f| f.id == "pdf").unwrap();
        a.attiva(Azione::FoglioFormato(0, pdf));
        assert!(matches!(a.attiva(Azione::Vai), Effetto::Converti { azione: "pdf", .. }));
        a.imposta_file(Archivio { path: "/tmp/foto.heic".into(), nome: "foto.heic".into(), dim: 100 }, Kind::Image, true);
        a.apri_opzioni(Some("avif"));
        a.azione_opzioni(OpzAz::Indietro);
        assert!(!a.foglio);
        assert!(matches!(a.attiva(Azione::Vai), Effetto::Converti { azione: "avif", opts: Some(_), .. }));
        a.vai_casa();
        a.vista_nav(4);
        a.vista_nav(0);
        a.aggiungi_coda(vec![("/tmp/nuova.png".into(), "nuova.png".into(), Kind::Image)]);
        assert_eq!(a.casa.att, Att::Conv);
    }
}
