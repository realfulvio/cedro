//! Il foglio A usa le stesse azioni della ruota e conserva anche ZIP multipli, firme e link.
use super::*;
use super::casa::telefono::{altezza_a, testo_a};

impl App {
    pub(super) fn foglio_a(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let f = self.file.clone().unwrap();
        let top = h * 0.10;
        p.fill_a(0.0, 0.0, w, h, (0, 0, 0), 0.35);
        p.rrect(0.0, top, w, h - top + 24.0, 22.0, Some(pal.bg), Some((pal.border, 1.0)));
        p.rrect(w / 2.0 - 19.0, top + 9.0, 38.0, 4.0, 2.0, Some(pal.muted), None);
        let title = "Ricevuto. Scegli cosa farne.";
        let th = altezza_a(p, title, 23.0, Weight::Bold, w - 88.0, 27.0, 0).max(48.0);
        testo_a(p, title, 16.0, top + 26.0, w - 88.0, 23.0, Weight::Bold, pal.text, 27.0, 0);
        self.bottone_icona(p, (w - 64.0, top + 22.0, 48.0, 48.0), ic::CLOSE, Azione::Esci);
        let vy = top + 32.0 + th;
        let vista = (0.0, vy, w, (h - vy - 76.0).max(1.0));
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let x = 16.0;
        let cw = w - 32.0;
        let mut y = vy + 8.0 - self.scorri;
        let nh = altezza_a(p, &cedro::i18n::lit(&f.nome), 14.0, Weight::Semibold, cw - 112.0, 20.0, 0);
        let hh = (nh + 62.0).max(106.0);
        p.rrect(x, y, cw, hh, 15.0, Some(pal.surface), Some((pal.border, 1.0)));
        self.anteprima(p, (x + 12.0, y + 12.0, 76.0, 82.0), 8.0, false);
        testo_a(p, &cedro::i18n::lit(&f.nome), x + 100.0, y + 18.0, cw - 112.0, 14.0, Weight::Semibold, pal.text, 20.0, 0);
        p.text(&peso(f.dim), x + 100.0, y + 24.0 + nh, cw - 112.0, 24.0, 12.0, Weight::Regular, pal.muted, Align::Left);
        y += hh + 20.0;
        let cols = if w > 600.0 && !self.grandi { 4 } else { 2 };
        let tw = (cw - (cols - 1) as f32 * 8.0) / cols as f32;
        // Azioni che non fanno parte del catalogo dei formati: link, ZIP multiplo, firme.
        let speciali: Vec<_> = self.righe_ora().into_iter().enumerate().filter(|(_, r)| e_link(&f.path) || matches!(r.id, "zip-tutti" | "firme")).collect();
        for (i, r) in speciali {
            let hh = altezza_a(p, r.titolo, 14.0, Weight::Semibold, cw - 12.0, 20.0, 0).max(24.0) + 48.0;
            let rc = (x, y, cw, hh);
            let v = cedro::ui::voci::voce(self.kind, r.id);
            self.formato_a(p, rc, r.titolo, &v, self.foglio && self.tessera == i);
            self.colpo(rc, Azione::Tessera(i), vista);
            y += hh + 8.0;
        }
        if !e_link(&f.path) {
            let modo = self.modo;
            for m in 0..2 {
                self.modo = m;
                let all = self.fette();
                self.modo = modo;
                for lab in [false, true] {
                    if m == 0 && lab { continue; }
                    let mut voci: Vec<_> = all.iter().copied().enumerate().filter(|(_, f)| m == 0 || cedro::ui::voci::is_lab(&cedro::ui::voci::voce(self.kind, f.id)) == lab).collect();
                    if voci.is_empty() { continue; }
                    if m == 0 && self.kind == Kind::Image {
                        voci.sort_by_key(|(_, f)| ["pdf", "jpeg", "png", "webp", "avif", "jxl"].iter().position(|id| *id == f.id).unwrap_or(99));
                    }
                    let titolo = if m == 0 { "Converti in" } else if lab { "Laboratorio" } else { "Strumenti" };
                    y += testo_a(p, titolo, x, y, cw, 17.0, Weight::Bold, pal.text, 23.0, 0) + 12.0;
                    for row in voci.chunks(cols) {
                        let hh = row.iter().map(|(_, f)| altezza_a(p, f.label, 14.0, Weight::Semibold, tw - 12.0, 19.0, 0) + 46.0).fold(65.0_f32, f32::max);
                        for (ci, (i, f)) in row.iter().enumerate() {
                            let r = (x + ci as f32 * (tw + 8.0), y, tw, hh);
                            let v = cedro::ui::voci::voce(self.kind, f.id);
                            let on = if self.foglio { self.righe_ora().get(self.tessera).is_some_and(|s| s.id == f.id) } else { self.modo == m && self.scelta == Some(*i) };
                            self.formato_a(p, r, f.label, &v, on);
                            self.colpo(r, Azione::FoglioFormato(m, *i), vista);
                        }
                        y += hh + 8.0;
                    }
                    y += 16.0;
                }
            }
        }
        let scelto = if self.foglio { self.righe_ora().get(self.tessera).map(|r| r.id) } else { self.scelta.and_then(|i| self.fette().get(i).map(|r| r.id)) };
        if scelto.is_some_and(|id| super::opzioni::formati(self.kind).iter().any(|f| f.id == id)) {
            let r = (x, y, cw, 56.0);
            p.rrect(r.0, r.1, r.2, r.3, 12.0, Some(pal.surface), Some((pal.border, 1.0)));
            p.text("Altre opzioni", x + 12.0, y, cw - 24.0, 56.0, 16.0, Weight::Semibold, pal.primary, Align::Left);
            self.colpo(r, Azione::AltreOpzioni, vista);
            y += 68.0;
        }
        if self.qualita_serve() {
            self.slider(p, (x, y, cw, 48.0));
            y += 60.0;
        }
        p.pop_clip();
        self.scorri_max = (y + self.scorri - (vista.1 + vista.3)).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        // L'etichetta ed il dispatch restano quelli del lavoro realmente selezionato.
        let testo = if self.foglio {
            self.righe_ora().get(self.tessera).map(|r| if r.conv { format!("Crea {}", cedro::i18n::tr(r.titolo)) } else { cedro::i18n::tr(r.titolo).into_owned() })
        } else {
            self.scelta.and_then(|i| self.fette().get(i).copied()).map(|f| if self.modo == 0 && f.id != "trascrivi" { format!("Crea {}", cedro::i18n::tr(f.label)) } else { cedro::i18n::tr(f.label).into_owned() })
        };
        self.azione_fissa_a(p, w, h, testo.as_deref().unwrap_or("Scegli uno strumento"), testo.is_some(), Azione::Vai);
    }
}
