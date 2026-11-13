//! Trascrivi con la traccia audio a tutto schermo: l'onda sonora prende lo spazio libero del file scelto,
//! con sotto i comandi per riascoltarlo (play, ±10 secondi, posizione, velocità, volume). Tre viste, sempre
//! a un clic: solo onda, onda e testo (predefinita), solo testo. La parte già trascritta si colora mentre cresce.

use super::cedro::{paper_box, colors, STAGE_INK, STAGE_MUTED, WHITE};
use super::documento::play_glyph;
use super::*;
use crate::engine::Kind;
use crate::ui::core::HitKind;
use crate::ui::gfx::Rgb;
use crate::ui::model::{FileState, SPEEDS};

const HEAD_H: f32 = 66.0;
const TICK_H: f32 = 26.0;
const CTRL_H: f32 = 76.0;
const CARD: Rgb = (0x0B, 0x08, 0x10);
const TODO: Rgb = (0x5B, 0x43, 0x90);

/// Vero se per questo file si usa la disposizione con l'onda grande (audio con l'onda già letta).
pub(super) fn applies(app: &App, i: usize) -> bool {
    let file = &app.files[i];
    file.kind == Kind::Audio && app.media_of(&file.path).and_then(|m| m.about.as_ref()).and_then(|a| a.wave.as_ref()).is_some_and(|w| !w.is_empty())
}

/// Solo onda | Onda e testo | Solo testo, a destra nella barra.
pub(super) fn view_toggle(f: &mut Frame, app: &App, right: f32, y: f32) {
    let items = [("Solo onda", 0u8), ("Onda e testo", 1), ("Solo testo", 2)];
    let widths: Vec<f32> = items.iter().map(|(l, _)| f.measure(l, 14.0, SEMI) + 26.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let mut x = right - total;
    f.p.rrect_a(x, y, total, 38.0, 10.0, WHITE, 0.1);
    x += 3.0;
    for ((label, v), w) in items.iter().zip(widths) {
        let on = app.tr_view == *v;
        let b = (x, y + 3.0, w, 32.0);
        let act = Act::TrView(*v);
        if on {
            f.p.rrect_a(b.0, b.1, b.2, b.3, 8.0, WHITE, 0.92);
        } else {
            let h = f.hover_t(&act, true);
            if h > 0.01 {
                f.p.rrect_a(b.0, b.1, b.2, b.3, 8.0, WHITE, 0.12 * h);
            }
        }
        f.text_c(label, b, 14.0, SEMI, if on { (0x20, 0x17, 0x29) } else { (0xCF, 0xC5, 0xDD) });
        f.focus_ring(b, 8.0, &act);
        f.hit(b, act);
        x += w;
    }
}

/// La zona centrale di Trascrivi per un audio: scheda con l'onda e i comandi, sotto (se serve) il foglio del testo.
pub(super) fn stage(f: &mut Frame, app: &App, i: usize, c: R) {
    let mode = app.tr_view.min(2);
    let card_h = match mode {
        0 => c.3,
        1 => (c.3 * 0.6).clamp(330.0, (c.3 - 170.0).max(330.0)),
        _ => HEAD_H + CTRL_H + 6.0,
    };
    let card = (c.0, c.1, c.2, card_h.min(c.3));
    wave_card(f, app, i, card, mode != 2);
    if mode != 0 {
        paper_box(f, app, (c.0, c.1 + card.3 + 12.0, c.2, (c.3 - card.3 - 12.0).max(80.0)));
    }
}

/// Il valore dell'onda al punto `t` (0..1), interpolato tra i due campioni vicini.
fn sample(wave: &[f32], t: f32) -> f32 {
    let p = (t * wave.len() as f32 - 0.5).clamp(0.0, wave.len() as f32 - 1.0);
    let (a, b) = (p.floor() as usize, (p.ceil() as usize).min(wave.len() - 1));
    wave[a] + (wave[b] - wave[a]) * (p - a as f32)
}

fn wave_card(f: &mut Frame, app: &App, i: usize, r: R, with_wave: bool) {
    let file = &app.files[i];
    let Some(about) = app.media_of(&file.path).and_then(|m| m.about.clone()) else { return };
    let wave = about.wave.clone().unwrap_or_default();
    let (_, bright) = colors(app);
    let duration = about.duration.unwrap_or(0.0);
    let player = app.player.as_ref().filter(|p| p.path == file.path);
    let playing = player.is_some_and(|p| p.playing());
    let pos = match (app.media_scrub, player) {
        (Some(s), Some(_)) => s as f64 * duration,
        (_, Some(p)) => p.position(),
        _ => 0.0,
    };
    if playing {
        f.animating = true;
    }
    f.p.shadow(r.0, r.1, r.2, r.3, 16.0, 24.0, 0.45, 10.0);
    f.p.rrect(r.0, r.1, r.2, r.3, 16.0, Some(CARD), Some((mix(bright, CARD, 0.78), 1.0)));
    let (x, iw) = (r.0 + 22.0, r.2 - 44.0);

    // Intestazione: nome e durata; a destra la legenda dei colori mentre trascrive.
    let running = with_wave && file.state == FileState::Running;
    let legend = [("già trascritto", bright), ("da trascrivere", TODO)];
    let lw: f32 = if running { legend.iter().map(|(l, _)| f.measure(l, 13.0, SANS) + 32.0).sum() } else { 0.0 };
    f.text(&file.name(), x, r.1 + 12.0, iw - lw - 12.0, 28.0, 18.0, BOLD, STAGE_INK);
    let length = about.duration.map(|d| format!("{} {}", crate::i18n::tr("Durata"), crate::ui::preview::clock(d))).unwrap_or_default();
    f.text(&length, x, r.1 + 38.0, iw, 22.0, 13.5, SANS, STAGE_MUTED);
    if running {
        let mut lx = x + iw - lw;
        for (label, c) in legend {
            let w = f.measure(label, 13.0, SANS);
            f.p.rrect(lx, r.1 + 21.0, 10.0, 10.0, 3.0, Some(c), None);
            f.text(label, lx + 16.0, r.1 + 12.0, w + 4.0, 28.0, 13.0, SANS, STAGE_MUTED);
            lx += w + 32.0;
        }
    }

    // L'onda: la parte già ascoltata chiara, quella già trascritta del colore della pagina, il resto viola.
    if with_wave {
        let (wy, wh) = (r.1 + HEAD_H + 6.0, (r.3 - HEAD_H - 6.0 - TICK_H - CTRL_H).max(40.0));
        let done = match file.state {
            FileState::Done => 1.0,
            FileState::Running => app.transcribe.percent.min(100) as f32 / 100.0,
            _ => 0.0,
        };
        let heard = if duration > 0.0 && player.is_some() { (pos / duration) as f32 } else { -1.0 };
        let nb = ((iw / 5.0) as usize).clamp(40, 600);
        let step = iw / nb as f32;
        let bw = (step * 0.62).max(1.5);
        let mid = wy + wh / 2.0;
        for k in 0..nb {
            let t = (k as f32 + 0.5) / nb as f32;
            let h = (sample(&wave, t).max(0.03) * wh * 0.92).max(3.0);
            let c = if t <= heard { mix(bright, WHITE, 0.55) } else if t <= done { bright } else { TODO };
            f.p.rrect(x + k as f32 * step, mid - h / 2.0, bw, h, bw.min(2.0), Some(c), None);
        }
        if heard >= 0.0 {
            let px = x + iw * heard.clamp(0.0, 1.0);
            f.p.fill(px - 1.0, wy, 2.0, wh, WHITE);
            let label = crate::ui::preview::clock(pos);
            let tw = f.measure(&label, 12.5, BOLD) + 12.0;
            let bx = if px + 6.0 + tw > x + iw { px - 6.0 - tw } else { px + 6.0 };
            f.p.rrect(bx, wy, tw, 20.0, 6.0, Some(WHITE), None);
            f.text_c(&label, (bx, wy, tw, 20.0), 12.5, BOLD, (0x2B, 0x1F, 0x38));
        }
        // Un clic o un trascinamento sull'onda sposta l'ascolto (come la barra: zona allargata di 10 ai lati).
        f.hit_kind((x - 10.0, wy, iw + 20.0, wh), Act::MediaSeek, HitKind::Slider);
        let ty = wy + wh + 4.0;
        for k in 0..5 {
            let label = crate::ui::preview::clock(duration * k as f64 / 4.0);
            let w = f.measure(&label, 12.5, SANS) + 4.0;
            let lx = (x + iw * k as f32 / 4.0 - w / 2.0).clamp(x, x + iw - w);
            f.text(&label, lx, ty, w, 20.0, 12.5, SANS, STAGE_MUTED);
        }
    }
    controls(f, app, (r.0, r.1 + r.3 - CTRL_H, r.2, CTRL_H), playing, pos, duration, player.is_some_and(|p| p.loading()), bright);
}

/// Play/pausa, ±10 secondi, tempo, velocità e volume.
#[allow(clippy::too_many_arguments)]
fn controls(f: &mut Frame, app: &App, r: R, playing: bool, pos: f64, duration: f64, loading: bool, bright: Rgb) {
    let (x, iw) = (r.0 + 22.0, r.2 - 44.0);
    let cy = r.1 + r.3 / 2.0;
    f.hline(r.0 + 1.0, r.1, r.2 - 2.0);
    let mut cx = x;
    for (label, secs) in [("−10 s", -10), ("+10 s", 10)] {
        // Il tasto grande sta in mezzo ai due salti.
        if secs > 0 {
            let c = (cx + 27.0, cy);
            let act = Act::MediaPlay;
            if f.hovered(&act) {
                f.p.circle(c.0, c.1, 31.0, Some(mix(bright, CARD, 0.7)), None);
            }
            f.p.circle(c.0, c.1, 27.0, Some(bright), None);
            if loading && playing {
                f.spinner(c.0, c.1, 11.0, (0x20, 0x10, 0x08));
            } else {
                play_glyph(f, c.0, c.1, 10.0, (0x20, 0x10, 0x08), playing);
            }
            f.focus_ring((c.0 - 27.0, c.1 - 27.0, 54.0, 54.0), 27.0, &act);
            f.hit((c.0 - 30.0, c.1 - 30.0, 60.0, 60.0), act);
            cx += 54.0 + 10.0;
        }
        let b = (cx, cy - 21.0, 62.0, 42.0);
        let act = Act::MediaSkip(secs);
        let h = f.hover_t(&act, true);
        f.p.rrect_a(b.0, b.1, b.2, b.3, 12.0, WHITE, 0.1 + 0.08 * h);
        f.text_c(label, b, 14.0, BOLD, STAGE_INK);
        f.focus_ring(b, 12.0, &act);
        f.hit(b, act);
        cx += 62.0 + 8.0;
    }
    // Alla frase prima o dopo (se il file ha già delle frasi con gli orari).
    for dir in [-1, 1] {
        let b = (cx, cy - 21.0, 42.0, 42.0);
        let act = Act::MediaPhrase(dir);
        let h = f.hover_t(&act, true);
        f.p.rrect_a(b.0, b.1, b.2, b.3, 12.0, WHITE, 0.1 + 0.08 * h);
        let (gx, gy, g) = (b.0 + 21.0, cy, 7.0);
        if dir < 0 {
            f.p.fill(gx - g - 1.0, gy - g, 2.5, 2.0 * g, STAGE_INK);
            f.p.poly(&[(gx + g, gy - g), (gx - g + 3.0, gy), (gx + g, gy + g)], STAGE_INK, 1.0);
        } else {
            f.p.fill(gx + g - 1.5, gy - g, 2.5, 2.0 * g, STAGE_INK);
            f.p.poly(&[(gx - g, gy - g), (gx + g - 3.0, gy), (gx - g, gy + g)], STAGE_INK, 1.0);
        }
        f.focus_ring(b, 12.0, &act);
        f.hit(b, act);
        cx += 42.0 + 8.0;
    }
    cx += 6.0;
    let now = crate::ui::preview::clock(pos);
    let total = crate::ui::preview::clock(duration);
    let time = format!("{now} / {total}");
    let tw = f.measure(&time, 15.0, BOLD) + 6.0;
    f.text(&time, cx, r.1, tw, r.3, 15.0, BOLD, STAGE_INK);
    cx += tw + 14.0;

    // A destra, dal fondo: il volume (se c'è posto) e le velocità.
    let mut rx = x + iw;
    let room = rx - cx;
    let speeds: Vec<f32> = SPEEDS.iter().map(|(l, _)| f.measure(l, 13.5, BOLD) + 18.0).collect();
    let speeds_w: f32 = speeds.iter().sum::<f32>() + 4.0 * (SPEEDS.len() as f32 - 1.0);
    if room >= speeds_w + 12.0 + 96.0 + 48.0 {
        let track = (rx - 96.0, cy - 2.0, 96.0, 4.0);
        f.p.rrect(track.0, track.1, track.2, track.3, 2.0, Some(mix(WHITE, CARD, 0.78)), None);
        f.p.rrect(track.0, track.1, track.2 * app.play_vol, track.3, 2.0, Some(bright), None);
        f.p.circle(track.0 + track.2 * app.play_vol, cy, 7.0, Some(bright), None);
        f.hit_kind((track.0 - 10.0, cy - 14.0, track.2 + 20.0, 28.0), Act::MediaVolume, HitKind::Slider);
        f.text_r("Vol.", (track.0 - 40.0, r.1, 34.0, r.3), 12.5, SANS, STAGE_MUTED);
        rx = track.0 - 48.0;
    }
    if rx - cx >= speeds_w {
        let mut sx = rx - speeds_w;
        for (k, ((label, _), w)) in SPEEDS.iter().zip(speeds).enumerate() {
            let b = (sx, cy - 17.0, w, 34.0);
            let on = app.play_speed as usize == k;
            let act = Act::MediaSpeed(k as u8);
            if on {
                f.p.rrect_a(b.0, b.1, b.2, b.3, 9.0, WHITE, 0.92);
            } else {
                let h = f.hover_t(&act, true);
                f.p.rrect(b.0, b.1, b.2, b.3, 9.0, None, Some((mix(WHITE, CARD, 0.8 - 0.2 * h), 1.0)));
            }
            f.text_c(label, b, 13.5, BOLD, if on { (0x20, 0x17, 0x29) } else { STAGE_MUTED });
            f.focus_ring(b, 9.0, &act);
            f.hit(b, act);
            sx += w + 4.0;
        }
    }
}
