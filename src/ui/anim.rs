//! Le piccole animazioni dell'interfaccia: il fondo che sfuma sotto il mouse, il pulsante che si
//! abbassa quando si preme, le icone che si muovono, la linea delle schede che scivola.
//!
//! L'interfaccia si ridisegna tutta a ogni fotogramma: qui si tiene solo, per ogni comando, un valore
//! che va verso il suo obiettivo in un tempo dato. Mentre un valore si muove il fotogramma chiede il
//! successivo (`Frame::animating`); quando tutto è fermo la finestra smette di ridisegnarsi.
//! Con gli effetti di animazione spenti nel sistema (`UiState::calm`) i valori saltano subito alla fine.

use super::assets as ic;
use super::core::{Act, Frame, R};
use super::gfx::Rgb;
use super::theme::mix;

/// Che cosa di un comando si anima (lo stesso comando può avere più valori).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    /// Il mouse è sopra: il fondo.
    Hover,
    /// Il mouse è sopra: l'icona (più lenta, con il rimbalzo).
    Icon,
    /// Il comando è premuto.
    Press,
    /// La tendina aperta (la freccia gira).
    Open,
    /// Una posizione che scivola (la scheda scelta, il riquadro dell'attività).
    Glide,
    /// Il menu a comparsa che appare.
    Menu,
}

struct Tween {
    act: Act,
    slot: Slot,
    from: f32,
    to: f32,
    t0: u64,
    dur: u64,
    seen: u64,
}

impl Tween {
    fn progress(&self, now: u64) -> f32 {
        if self.dur == 0 {
            return 1.0;
        }
        (now.saturating_sub(self.t0) as f32 / self.dur as f32).min(1.0)
    }
}

/// I valori che si muovono, tenuti fra un fotogramma e l'altro.
#[derive(Default)]
pub struct Anims {
    items: Vec<Tween>,
    frame: u64,
}

impl Anims {
    /// A fine fotogramma: si dimentica quello che non è stato disegnato (al ritorno riparte fermo).
    pub fn end_frame(&mut self) {
        let f = self.frame;
        self.items.retain(|t| t.seen == f);
        self.frame += 1;
    }
}

pub fn ease_out(p: f32) -> f32 {
    1.0 - (1.0 - p).powi(3)
}

/// Arriva un poco oltre e torna indietro: il piccolo rimbalzo delle icone.
pub fn back_out(p: f32) -> f32 {
    let (c1, c3) = (1.70158f32, 2.70158f32);
    1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2)
}

pub const HOVER_MS: u64 = 120;
pub const ICON_MS: u64 = 220;
pub const PRESS_MS: u64 = 90;
pub const GLIDE_MS: u64 = 200;

impl<'a> Frame<'a> {
    /// Il valore di `slot` per `act`, che va verso `target` in `dur` millisecondi (con `ease`).
    /// La prima volta che un comando si disegna parte già fermo sull'obiettivo.
    pub fn tween(&mut self, act: &Act, slot: Slot, target: f32, dur: u64, ease: fn(f32) -> f32) -> f32 {
        let now = self.ms;
        let calm = self.ui.calm;
        let mut anims = self.ui.anim.borrow_mut();
        let frame = anims.frame;
        let (v, moving) = match anims.items.iter_mut().find(|t| t.slot == slot && t.act == *act) {
            Some(t) => {
                t.seen = frame;
                if (t.to - target).abs() > 1e-4 {
                    let p = t.progress(now);
                    let cur = t.from + (t.to - t.from) * ease_out(p);
                    t.from = cur;
                    t.to = target;
                    t.t0 = now;
                    t.dur = dur;
                }
                if calm {
                    t.from = t.to;
                }
                let p = t.progress(now);
                (t.from + (t.to - t.from) * ease(p), p < 1.0 && t.from != t.to)
            }
            None => {
                anims.items.push(Tween { act: act.clone(), slot, from: target, to: target, t0: now, dur, seen: frame });
                (target, false)
            }
        };
        drop(anims);
        if moving {
            self.animating = true;
        }
        v
    }

    /// Una posizione che scivola verso `target` (la scheda scelta, l'attività scelta).
    pub fn glide(&mut self, act: &Act, target: f32) -> f32 {
        self.tween(act, Slot::Glide, target, GLIDE_MS, ease_out)
    }

    /// Da 0 a 1: quanto il mouse è «arrivato» sul comando (il fondo sfuma).
    pub fn hover_t(&mut self, act: &Act, enabled: bool) -> f32 {
        let on = enabled && self.hovered(act);
        self.tween(act, Slot::Hover, on as u8 as f32, HOVER_MS, ease_out)
    }

    /// Da 0 a 1, senza curva: l'avanzamento dell'icona sotto il mouse (la curva la sceglie l'icona).
    pub fn icon_t(&mut self, act: &Act, enabled: bool) -> f32 {
        let on = enabled && self.hovered(act);
        self.tween(act, Slot::Icon, on as u8 as f32, ICON_MS, |p| p)
    }

    /// Da 0 a 1: quanto il comando è premuto (mouse giù e ancora sopra).
    pub fn press_t(&mut self, act: &Act, enabled: bool) -> f32 {
        let on = enabled && self.ui.pressed.as_ref() == Some(act) && self.hovered(act);
        self.tween(act, Slot::Press, on as u8 as f32, PRESS_MS, ease_out)
    }

    /// Il riquadro di un pulsante premuto: si stringe un poco verso il centro (fino al 97%).
    pub fn pressed_rect(r: R, press: f32) -> R {
        if press <= 0.0 {
            return r;
        }
        let k = press * 0.015;
        let (dx, dy) = ((r.2 * k).min(4.0), (r.3 * k * 2.0).clamp(0.5, 2.0) * press.min(1.0));
        (r.0 + dx, r.1 + dy, r.2 - 2.0 * dx, r.3 - 2.0 * dy)
    }

    /// Il fondo sotto il mouse che sfuma: `c` con l'opacità di `t`.
    pub fn hover_fill(&mut self, r: R, radius: f32, c: Rgb, t: f32) {
        if t > 0.01 {
            self.p.rrect_a(r.0, r.1, r.2, r.3, radius, c, t.min(1.0));
        }
    }

    /// Un'icona del kit che si muove sotto il mouse secondo quello che fa: il «+» gira, le frecce
    /// scattano avanti, il cestino oscilla, la freccia della tendina scende; le altre si alzano.
    /// `p` è l'avanzamento senza curva (`icon_t`).
    pub fn icon_live(&mut self, d: &str, x: f32, y: f32, size: f32, c: Rgb, p: f32) {
        if p <= 0.0 {
            self.icon(d, x, y, size, c);
            return;
        }
        let k = size / 24.0;
        let b = back_out(p);
        if d == ic::ADD {
            self.p.icon_turned(d, x, y, size, c, 90.0 * ease_out(p));
        } else if d == ic::TRASH {
            let a = (p * std::f32::consts::TAU).sin() * 9.0 * (1.0 - p * 0.6);
            self.p.icon_turned(d, x, y, size, c, a);
        } else if d == ic::CONVERT || d.starts_with("M5 12h14") || d.starts_with("M4 12h16") {
            self.icon(d, x + 3.0 * k * b, y, size, c);
        } else if d == ic::CHEVRON_DOWN {
            self.icon(d, x, y + 2.0 * k * b, size, c);
        } else {
            self.icon(d, x, y - 2.0 * k * b, size, c);
        }
    }

    /// I due colori della sfumatura di un pulsante pieno, fra riposo (0) e mouse sopra (1).
    pub fn glossy_t(&self, c: Rgb, t: f32) -> (Rgb, Rgb) {
        let (a0, b0) = self.glossy(c, false);
        let (a1, b1) = self.glossy(c, true);
        (mix(a0, a1, t), mix(b0, b1, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_start_and_end_in_place() {
        for f in [ease_out as fn(f32) -> f32, back_out] {
            assert!(f(0.0).abs() < 1e-5);
            assert!((f(1.0) - 1.0).abs() < 1e-5);
        }
        // Il rimbalzo va oltre la fine prima di tornare.
        assert!(back_out(0.7) > 1.0);
    }

    #[test]
    fn end_frame_forgets_what_was_not_drawn() {
        let mut a = Anims::default();
        a.items.push(Tween { act: Act::AddFiles, slot: Slot::Hover, from: 0.0, to: 1.0, t0: 0, dur: 100, seen: 0 });
        a.end_frame();
        assert_eq!(a.items.len(), 1);
        a.end_frame();
        assert!(a.items.is_empty());
    }
}
