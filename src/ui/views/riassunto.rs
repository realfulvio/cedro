//! Rivedi, scheda «Riassunto»: i quattro momenti dell'anteprima approvata. La prima volta (cosa serve e
//! «Fai il riassunto»), al lavoro (i passi, la barra, la memoria del PC e Annulla), poca memoria (non
//! parte e lo dice) e pronto (in breve, chi ha detto cosa, decisioni, da fare, Copia e Rifai).
//! Lo stato sta in `ui/summary.rs`, il riassunto vero in `crate::riassunto`.

use super::revisione::chip_colors;
use super::*;
use crate::riassunto::{self, Failure, Step};
use crate::ui::gfx::Rgb;
use crate::ui::review::Review;
use crate::ui::summary::Job;

/// Il testo del modello non va tradotto: si mostra com'è.
fn lit(s: &str) -> String {
    crate::i18n::lit(s)
}

pub fn pane(f: &mut Frame, app: &App, rv: &Review, r: R) {
    super::scrolled(f, r, Scroll::ReviewSummary, |f| {
        let (x, w) = (r.0 + 19.0, r.2 - 38.0);
        let y = r.1 + 4.0;
        let h = if let Some(job) = app.summary_job_for(&rv.source) {
            working(f, job, x, y, w)
        } else if let Some(free) = still_low(app.summary_failure_for(&rv.source)) {
            low_memory(f, free, x, y, w)
        } else if let Some(sections) = rv.summary_sections() {
            ready(f, rv, &sections, x, y, w)
        } else {
            intro(f, app, rv, x, y, w)
        };
        h + 18.0
    });
}

/// La memoria libera di adesso, se l'ultimo tentativo si è fermato per poca memoria e manca ancora:
/// chiusi abbastanza programmi, l'avviso sparisce e torna «Fai il riassunto».
fn still_low(failure: Option<&Failure>) -> Option<u64> {
    let Some(Failure::LowMemory(then)) = failure else { return None };
    let now = crate::resources::available_memory().unwrap_or(*then);
    (now < riassunto::NEED_BYTES).then_some(now)
}

/// Su cosa lavorerà: la scheda video se le Impostazioni la vogliono e c'è, se no il processore.
fn runs_on() -> String {
    let settings = crate::read_settings();
    let gpu = &crate::sysinfo::info().gpu;
    if settings.gpu && crate::trascrizione::real_gpu_present() && !gpu.is_empty() {
        crate::accel::gpu_model(gpu)
    } else {
        "processore".to_string()
    }
}

/// Il riquadro con quello che serve: memoria, modello, dove lavora, niente internet.
fn facts(f: &mut Frame, free: Option<u64>, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let enough = free.is_none_or(|b| b >= riassunto::NEED_BYTES);
    let free_text = match free {
        Some(b) => format!("{} · ne servono 4 GB", riassunto::gb(b)),
        None => "ne servono 4 GB".to_string(),
    };
    let model = if riassunto::model_ready() {
        ("Modello", format!("{} già sul PC", riassunto::MODEL_NAME))
    } else {
        ("Prima volta: scarica il modello", crate::trascrizione::size_text(riassunto::MODEL_MB))
    };
    let rows: [(&str, &str, String, Option<Rgb>); 4] = [
        (ic::MEMORY, "Memoria libera", free_text, Some(if enough { pal.ok } else { pal.bad })),
        (ic::DOWNLOAD, model.0, model.1, None),
        (ic::CHIP, "Lavora su", runs_on(), None),
        (ic::LOCK, "Tutto sul tuo PC", "niente internet".to_string(), None),
    ];
    let rh = 40.0;
    let h = rh * rows.len() as f32;
    f.p.rrect(x, y, w, h, 10.0, Some(pal.surface), Some((pal.border, 1.0)));
    for (i, (icon, label, value, color)) in rows.iter().enumerate() {
        let ry = y + i as f32 * rh;
        if i > 0 {
            f.hline(x, ry, w);
        }
        let bad = !enough && i == 0;
        let fg = if bad { pal.bad } else { pal.text };
        f.icon(icon, x + 12.0, ry + (rh - 18.0) / 2.0, 18.0, fg);
        let size = 13.5;
        let vw = f.measure(value, size, SEMI).min(w * 0.58);
        f.text(label, x + 38.0, ry, w - 38.0 - 12.0 - vw - 6.0, rh, size, SANS, fg);
        f.text_r(value, (x + w - 12.0 - vw - 2.0, ry, vw + 2.0, rh), size, SEMI, color.unwrap_or(pal.text));
    }
    h
}

/// La prima volta (o finché non lo si fa): cosa fa, cosa serve e «Fai il riassunto».
fn intro(f: &mut Frame, app: &App, rv: &Review, x: f32, y0: f32, w: f32) -> f32 {
    let pal = f.pal;
    let mut y = y0 + 14.0;
    let tint = mix(pal.primary, pal.surface, if f.theme.dark() { 0.82 } else { 0.92 });
    f.p.rrect(x, y, 56.0, 56.0, 14.0, Some(tint), None);
    f.icon(ic::SUMMARY, x + 13.0, y + 13.0, 30.0, pal.primary);
    y += 70.0;
    f.text("Riassunto della riunione", x, y, w, 28.0, 19.0, SEMI, pal.text);
    y += 36.0;
    let about = if rv.names.is_empty() {
        "Cedro legge la trascrizione e scrive in breve, chi ha detto cosa, le decisioni e le cose da fare. Per un'ora di riunione ci mette qualche minuto."
    } else {
        "Cedro legge la trascrizione con i nomi di «Chi parla» e scrive in breve, chi ha detto cosa, le decisioni e le cose da fare. Per un'ora di riunione ci mette qualche minuto."
    };
    y += f.para(about, x, y, w, 14.5, SANS, pal.muted, 1.5, 0) + 14.0;
    if let Some(Failure::Other(e)) = app.summary_failure_for(&rv.source) {
        y += f.para(&format!("L'ultima volta non è riuscito: {}", lit(e)), x, y, w, 14.0, SANS, pal.bad, 1.45, 4) + 12.0;
    }
    y += facts(f, crate::resources::available_memory(), x, y, w) + 14.0;
    f.primary((x, y, w, 48.0), "Fai il riassunto", Some(ic::SUMMARY), 16.5, Act::SummaryStart, true);
    y + 48.0 - y0
}

/// Un passo: fatto (spunta verde), in corso (rotellina, in grassetto) o da fare (cerchio vuoto, grigio).
fn step_row(f: &mut Frame, state: u8, label: &str, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let (cx, cy) = (x + 10.0, y + 15.0);
    match state {
        2 => {
            f.p.circle(cx, cy, 10.0, Some(pal.ok), None);
            f.icon(ic::CHECK, cx - 6.5, cy - 6.5, 13.0, (255, 255, 255));
        }
        1 => f.spinner(cx, cy, 8.0, pal.primary),
        _ => f.p.circle(cx, cy, 9.5, None, Some((pal.border, 1.5))),
    }
    let (weight, color) = match state {
        1 => (SEMI, pal.text),
        2 => (SANS, pal.text),
        _ => (SANS, pal.muted),
    };
    f.text(label, x + 30.0, y, w - 30.0, 30.0, 14.5, weight, color);
    30.0
}

/// Al lavoro: i passi, la barra, quanto manca, la memoria del PC e Annulla.
fn working(f: &mut Frame, job: &Job, x: f32, y0: f32, w: f32) -> f32 {
    let pal = f.pal;
    let mut y = y0 + 12.0;
    f.text("Sto facendo il riassunto…", x, y, w, 28.0, 19.0, SEMI, pal.text);
    y += 40.0;
    let free = job.free.map(|b| format!("Memoria libera: {}", riassunto::gb(b))).unwrap_or_else(|| "Memoria controllata".to_string());
    // Il secondo passo racconta cosa fa prima di leggere: aspetta, scarica o accende il motore.
    let (model_state, model_label) = match &job.step {
        Step::Waiting => (1, "Aspetto che finisca l'altro lavoro".to_string()),
        Step::Download(text) => (1, format!("Scarico il modello: {}", text.trim_start_matches("Scaricati ").trim())),
        Step::Loading if !job.ready => (1, "Carico il modello".to_string()),
        _ => (2, "Modello pronto".to_string()),
    };
    let read_label = match job.step {
        Step::Reading(i, n) if n > 1 => format!("Leggo la riunione: parte {} di {n}", i + 1),
        Step::Merging => "Leggo la riunione: metto insieme gli appunti".to_string(),
        _ => "Leggo la riunione".to_string(),
    };
    let read_state = match job.step {
        Step::Reading(..) | Step::Merging => 1,
        Step::Writing => 2,
        _ => 0,
    };
    let write_state = if job.step == Step::Writing { 1 } else { 0 };
    y += step_row(f, 2, &free, x, y, w) + 4.0;
    y += step_row(f, model_state, &model_label, x, y, w) + 4.0;
    y += step_row(f, read_state, &read_label, x, y, w) + 4.0;
    y += step_row(f, write_state, "Scrivo il riassunto", x, y, w) + 14.0;
    // La barra.
    f.p.rrect(x, y, w, 10.0, 5.0, Some(pal.alt), None);
    f.p.rrect(x, y, (w * job.percent as f32 / 100.0).max(10.0), 10.0, 5.0, Some(pal.primary), None);
    y += 20.0;
    let left = match job.left_text() {
        Some(t) => format!("{t} · si può continuare a lavorare"),
        None => "Si può continuare a lavorare".to_string(),
    };
    y += f.para(&left, x, y, w, 13.5, SANS, pal.muted, 1.4, 2) + 14.0;
    y += memory(f, job, x, y, w) + 14.0;
    f.button((x, y, w, 42.0), "Annulla", None, 15.5, Act::SummaryCancel);
    y + 42.0 - y0
}

/// La memoria del PC: gli altri programmi, il riassunto e quanto resta, con la promessa che torna libera.
fn memory(f: &mut Frame, job: &Job, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    let Some(total) = job.total.filter(|t| *t > 0) else { return 0.0 };
    let used = riassunto::USE_BYTES;
    let other = total.saturating_sub(job.free.unwrap_or(total / 2));
    let (ix, iw) = (x + 12.0, w - 24.0);
    let note = format!("A fine riassunto il modello si chiude e i {} tornano liberi.", riassunto::gb(used));
    let note_h = f.p.wrap_height(&note, 13.0, SANS, iw, 18.5, 0);
    let h = 12.0 + 22.0 + 16.0 + 24.0 + note_h + 12.0;
    f.p.rrect(x, y, w, h, 10.0, Some(pal.alt), None);
    let mut cy = y + 12.0;
    f.text("Memoria del PC", ix, cy, iw - 80.0, 20.0, 13.5, SEMI, pal.text);
    f.text_r(&format!("{} GB", (total as f64 / (1u64 << 30) as f64).round() as u64), (ix + iw - 80.0, cy, 80.0, 20.0), 13.5, SANS, pal.text);
    cy += 26.0;
    f.p.rrect(ix, cy, iw, 8.0, 4.0, Some(pal.surface), None);
    let fo = (iw * other as f32 / total as f32).min(iw);
    let fu = (iw * used as f32 / total as f32).min(iw - fo);
    f.p.rrect(ix, cy, fo, 8.0, 4.0, Some(pal.muted), None);
    f.p.fill(ix + fo, cy, fu, 8.0, pal.primary);
    cy += 16.0;
    let legend = |f: &mut Frame, lx: f32, c: Rgb, label: &str| -> f32 {
        f.p.rrect(lx, cy + 8.0, 9.0, 9.0, 2.0, Some(c), None);
        let tw = f.measure(label, 13.0, SANS);
        f.text(label, lx + 15.0, cy, tw + 4.0, 24.0, 13.0, SANS, pal.muted);
        15.0 + tw + 16.0
    };
    let lx = ix + legend(f, ix, pal.muted, &format!("Altri programmi {}", riassunto::gb(other)));
    legend(f, lx, pal.primary, &format!("Riassunto {}", riassunto::gb(used)));
    cy += 26.0;
    f.para(&note, ix, cy, iw, 13.0, SANS, pal.muted, 1.42, 0);
    h
}

/// Poca memoria: non parte, dice quanta ce n'è e cosa fare.
fn low_memory(f: &mut Frame, free: u64, x: f32, y0: f32, w: f32) -> f32 {
    let pal = f.pal;
    let mut y = y0 + 12.0;
    f.text("Riassunto della riunione", x, y, w, 28.0, 19.0, SEMI, pal.text);
    y += 40.0;
    let body = format!(
        "Per il riassunto servono 4 GB liberi, adesso ce ne sono {}. Chiudi qualche programma, per esempio il browser o Outlook, e riprova. Cedro non parte per non bloccare il PC.",
        riassunto::gb(free).trim_end_matches(" GB")
    );
    let tw = w - 50.0 - 14.0;
    let h = 38.0 + f.p.wrap_height(&body, 14.0, SANS, tw, 14.0 * 1.45, 0) + 14.0;
    f.p.rrect(x, y, w, h, 10.0, Some(pal.surface), Some((pal.bad, 1.5)));
    f.icon(ic::WARNING, x + 14.0, y + 14.0, 24.0, pal.bad);
    f.text("Poca memoria libera", x + 50.0, y + 12.0, tw, 24.0, 16.0, SEMI, pal.text);
    f.para(&body, x + 50.0, y + 38.0, tw, 14.0, SANS, pal.muted, 1.45, 0);
    y += h + 14.0;
    y += facts(f, Some(free), x, y, w) + 14.0;
    f.primary((x, y, w, 46.0), "Riprova", None, 16.0, Act::SummaryStart, true);
    y + 46.0 - y0
}

/// Il titolo di una parte: maiuscoletto grigio.
fn label(f: &mut Frame, s: &str, x: f32, y: f32, w: f32) -> f32 {
    let pal = f.pal;
    f.text(&crate::i18n::tr(s).to_uppercase(), x, y, w, 20.0, 12.5, SEMI, pal.muted);
    26.0
}

/// Il chip di una voce (o solo il nome, se non è una delle voci). Ritorna la larghezza.
fn chip(f: &mut Frame, voice: Option<usize>, name: &str, x: f32, y: f32) -> f32 {
    let pal = f.pal;
    match voice {
        Some(v) => {
            let (bg, fg) = chip_colors(f, v);
            let cw = (f.measure(name, 12.5, SEMI) + 30.0).min(260.0);
            f.p.rrect(x, y, cw, 22.0, 11.0, Some(bg), None);
            f.p.circle(x + 11.0, y + 11.0, 4.0, Some(f.voice(v)), None);
            f.p.text(&lit(name), x + 20.0, y, cw - 26.0, 22.0, 12.5, SEMI, fg, Align::Left);
            cw
        }
        None => {
            let tw = f.measure(name, 13.5, SEMI);
            f.text(&lit(name), x, y, tw + 4.0, 22.0, 13.5, SEMI, pal.text);
            tw + 4.0
        }
    }
}

/// Pronto: le quattro parti, Copia e Rifai, e l'avviso di controllare.
fn ready(f: &mut Frame, rv: &Review, s: &riassunto::Sections, x: f32, y0: f32, w: f32) -> f32 {
    let pal = f.pal;
    let mut y = y0 + 10.0;
    let size = 14.5;
    y += label(f, "In breve", x, y, w);
    y += f.para(&lit(&s.brief), x, y, w, size, SANS, pal.text, 1.5, 0) + 14.0;
    if !s.who.is_empty() {
        y += label(f, "Chi ha detto cosa", x, y, w);
        for said in &s.who {
            if !said.name.is_empty() {
                chip(f, said.voice, &said.name, x, y);
                y += 26.0;
            }
            y += f.para(&lit(&said.text), x, y, w, size, SANS, pal.text, 1.45, 0) + 8.0;
        }
        y += 6.0;
    }
    y += label(f, "Decisioni", x, y, w);
    if s.decisions.is_empty() {
        y += f.para("Nessuna decisione presa.", x, y, w, size, SANS, pal.muted, 1.45, 0) + 8.0;
    }
    for d in &s.decisions {
        f.p.rrect(x, y + 2.0, 18.0, 18.0, 5.0, Some(pal.ok), None);
        f.icon(ic::CHECK, x + 3.0, y + 5.0, 12.0, (255, 255, 255));
        y += f.para(&lit(d), x + 26.0, y, w - 26.0, size, SANS, pal.text, 1.45, 0) + 8.0;
    }
    y += 6.0;
    y += label(f, "Da fare", x, y, w);
    if s.todo.is_empty() {
        y += f.para("Nessuna cosa da fare.", x, y, w, size, SANS, pal.muted, 1.45, 0) + 8.0;
    }
    let done = rv.summary.as_ref().map(|st| st.done.clone()).unwrap_or_default();
    for (k, t) in s.todo.iter().enumerate() {
        let on = done.contains(&k);
        let act = Act::SummaryTodo(k);
        if on {
            f.p.rrect(x, y + 3.0, 16.0, 16.0, 4.0, Some(pal.primary), None);
            f.icon(ic::CHECK, x + 2.5, y + 5.5, 11.0, pal.on_primary);
        } else {
            f.p.rrect(x, y + 3.0, 16.0, 16.0, 4.0, Some(pal.surface), Some((if f.hovered(&act) { pal.primary } else { pal.border }, 1.5)));
        }
        f.hit((x - 4.0, y - 1.0, 24.0, 24.0), act);
        let tw = w - 26.0;
        let text = lit(&t.what);
        let lines = f.p.wrap_lines(&text, size, SANS, tw, 0);
        let h = f.para(&text, x + 26.0, y, tw, size, SANS, if on { pal.muted } else { pal.text }, 1.45, 0);
        // Chi lo fa: il chip dopo l'ultima riga, se ci sta; se no sotto.
        if let Some(owner) = &t.owner {
            let last = lines.last().map(|l| f.measure(l, size, SANS)).unwrap_or(0.0);
            let cw = f.measure(owner, 12.5, SEMI) + 30.0;
            let sep = f.measure(" · ", size, SANS);
            let lh = h / lines.len().max(1) as f32;
            if last + sep + cw <= tw {
                let ly = y + h - lh;
                f.text(" · ", x + 26.0 + last, ly, sep + 2.0, lh, size, SANS, pal.text);
                chip(f, t.voice, owner, x + 26.0 + last + sep, ly + (lh - 22.0) / 2.0);
                y += h + 8.0;
            } else {
                chip(f, t.voice, owner, x + 26.0, y + h + 2.0);
                y += h + 30.0;
            }
        } else {
            y += h + 8.0;
        }
    }
    y += 10.0;
    let bw = (w - 10.0) / 2.0;
    f.button((x, y, bw, 40.0), if rv.summary_copied { "Copiato" } else { "Copia" }, Some(if rv.summary_copied { ic::CHECK } else { ic::COPY }), 14.5, Act::SummaryCopy);
    f.button((x + bw + 10.0, y, bw, 40.0), "Rifai", None, 14.5, Act::SummaryStart);
    y += 52.0;
    f.hline(x, y, w);
    y += 10.0;
    let stored = rv.summary.as_ref();
    let took = stored.map(|st| st.took()).unwrap_or_default();
    let note = format!("Scritto da {} sul tuo PC in {took}. Controlla nomi, cifre e date prima di usarlo.", riassunto::MODEL_NAME);
    y += f.para(&note, x, y, w, 13.0, SANS, pal.muted, 1.45, 0);
    y - y0
}
