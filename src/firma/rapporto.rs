//! Il rapporto di verifica in PDF (e in testo): uno per file o uno unico per tanti file.

use super::verify::{Level, Report};
use super::{date_time, title_case};
use crate::pdfdoc::{DocBlock, PdfOptions};

fn mark(l: Level) -> &'static str {
    match l {
        Level::Ok => "✓",
        Level::Warn => "!",
        Level::Bad => "✗",
    }
}

fn p(text: impl Into<String>) -> DocBlock {
    DocBlock { text: text.into(), heading: 0, image: None }
}

fn h(text: impl Into<String>, level: u8) -> DocBlock {
    DocBlock { text: text.into(), heading: level, image: None }
}

fn blocks_of(r: &Report, out: &mut Vec<DocBlock>, level: u8) {
    out.push(h(r.name.clone(), level));
    out.push(p(format!("{} {} — {}", mark(r.level), r.headline, r.detail)));
    if !r.doc_name.is_empty() && !r.doc.is_empty() {
        out.push(p(format!("Documento dentro: {} ({})", r.doc_name, size(r.doc.len()))));
    }
    for t in &r.timestamps {
        out.push(p(format!("{} Marca temporale: {} — {} ({})", mark(t.level), date_time(t.time), t.tsa, t.text)));
    }
    for i in r.ordered() {
        let s = &r.signers[i];
        out.push(h(format!("{} {} — {}", mark(s.level), title_case(&s.name), s.summary), level + 1));
        if !s.role.is_empty() {
            out.push(p(s.role.clone()));
        }
        out.push(p(s.why.clone()));
        for c in &s.checks {
            out.push(p(format!("{} {}: {}", mark(c.level), c.what, c.text)));
        }
        for (k, v) in &s.details {
            out.push(p(format!("{k}: {v}")));
        }
        if !s.chain.is_empty() {
            out.push(p(format!("Catena: {}", s.chain.join(" › "))));
        }
    }
    if r.layers.len() > 1 {
        out.push(p(format!("Com'è fatto il file: {}", r.layers.iter().map(|l| l.label.clone()).collect::<Vec<_>>().join(" › "))));
    }
}

fn size(n: usize) -> String {
    if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1 << 20) as f64).replace('.', ",")
    } else {
        format!("{} KB", n.div_ceil(1024))
    }
}

fn footer(reports: &[&Report]) -> DocBlock {
    let r0 = reports.first();
    let when = r0.map(|r| date_time(r.checked_at)).unwrap_or_default();
    let tl = r0.and_then(|r| r.trust_updated).map(|t| format!("elenco europeo dei certificatori aggiornato il {}", date_time(t))).unwrap_or_else(|| "elenco europeo dei certificatori non ancora scaricato".into());
    let online = if !reports.is_empty() && reports.iter().all(|r| r.revoca_online()) { "revoca controllata" } else { "revoca non controllata online" };
    p(format!("Controllato con Cedro {} il {when}; {tl}; {online}.", crate::update::CURRENT))
}

/// Il rapporto in PDF di uno o più file.
pub fn pdf(reports: &[&Report]) -> Result<Vec<u8>, String> {
    let mut b = vec![h("Rapporto di verifica delle firme digitali", 1)];
    if reports.len() == 1 {
        blocks_of(reports[0], &mut b, 2);
    } else {
        let ok = reports.iter().filter(|r| r.level == Level::Ok).count();
        b.push(p(format!("{} file controllati: {ok} senza problemi, {} da guardare.", reports.len(), reports.len() - ok)));
        for r in reports {
            blocks_of(r, &mut b, 2);
        }
    }
    b.push(footer(reports));
    // Il font del PDF non ha ✓ e ✗: si usano √ e ×, che ha.
    for x in &mut b {
        x.text = x.text.replace('✓', "√").replace('✗', "×");
    }
    let o = PdfOptions { size: crate::engine::PageSize::A4, embed: true, bookmarks: true, pdfa: false, title: "Rapporto di verifica".into() };
    crate::pdfdoc::build(&b, &o)
}

/// Il rapporto in testo semplice.
pub fn text(reports: &[&Report]) -> String {
    let mut b = Vec::new();
    for r in reports {
        blocks_of(r, &mut b, 1);
    }
    b.push(footer(reports));
    b.iter().map(|x| x.text.clone()).collect::<Vec<_>>().join("\n")
}
