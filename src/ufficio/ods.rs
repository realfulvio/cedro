//! I fogli di LibreOffice Calc e OpenOffice (.ods) letti per stamparli: Calc scrive nel file anche il
//! testo già formattato di ogni cella, le misure di righe e colonne e lo stile di pagina di ogni foglio
//! (carta, margini, scala, griglia, intestazione e piè), quindi si stampa com'è.

use std::collections::HashMap;
use std::io::Cursor;

use super::foglio::{attr, parse, truthy, voce, voce_bytes, Bordo, Carattere, Cartella, Cella, Foglio, HAlign, Stile, VAlign, Valore, MAX_COLS, MAX_ROWS};
use super::pagina::{Dash, Rgb};

/// Una lunghezza di ODF («2.258cm», «0.5in», «12pt», «4mm») in punti.
pub fn length(s: &str) -> Option<f32> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(s.len());
    let (n, unit) = s.split_at(split);
    let v: f32 = n.trim().parse().ok()?;
    Some(match unit {
        "pt" | "" => v,
        "cm" => v * 72.0 / 2.54,
        "mm" => v * 72.0 / 25.4,
        "in" | "inch" => v * 72.0,
        "pc" => v * 12.0,
        "px" => v * 0.75,
        _ => return None,
    })
}

fn color(s: &str) -> Option<Rgb> {
    if s == "transparent" {
        return None;
    }
    super::foglio::hex_color(s)
}

/// «0.74pt solid #000000» → bordo.
fn border(s: &str) -> Option<Bordo> {
    let mut width = 0.5;
    let mut dash = Dash::Solid;
    let mut double = false;
    let mut col = (0, 0, 0);
    let mut any = false;
    for part in s.split_whitespace() {
        match part {
            "none" | "hidden" => return None,
            "solid" => any = true,
            "dashed" | "dash-dot" | "dash-dot-dot" | "long-dash" => {
                dash = Dash::Dashed;
                any = true;
            }
            "dotted" | "fine-dashed" => {
                dash = Dash::Dotted;
                any = true;
            }
            "double" | "double-thin" => {
                double = true;
                any = true;
            }
            "thin" => width = 0.5,
            "medium" => width = 1.0,
            "thick" => width = 1.5,
            p if p.starts_with('#') => col = color(p).unwrap_or(col),
            p => {
                if let Some(w) = length(p) {
                    width = w;
                }
            }
        }
    }
    // Le linee di Calc sono spesso 0,06 pt («capello»): a stampa diventano la riga più sottile.
    (any && width > 0.0).then_some(Bordo { width: width.clamp(0.25, 3.0), color: col, dash, double })
}

/// Le proprietà di uno stile: le sue e quelle ereditate dal genitore, per nome con il prefisso.
type Props = HashMap<String, String>;

fn props_of(n: roxmltree::Node) -> Props {
    let mut p = Props::new();
    for c in n.children().filter(|c| c.is_element()) {
        for a in c.attributes() {
            let prefix = a.namespace().map(prefix_of).unwrap_or("");
            p.insert(format!("{prefix}:{}", a.name()), a.value().to_string());
        }
    }
    p
}

fn prefix_of(ns: &str) -> &'static str {
    match ns {
        "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" => "fo",
        "urn:oasis:names:tc:opendocument:xmlns:style:1.0" => "style",
        "urn:oasis:names:tc:opendocument:xmlns:table:1.0" => "table",
        "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" => "svg",
        "urn:oasis:names:tc:opendocument:xmlns:text:1.0" => "text",
        _ => "",
    }
}

struct Styles {
    /// (famiglia, nome) → (genitore, proprietà, stile dei numeri).
    by_name: HashMap<(String, String), (Option<String>, Props)>,
    /// Le proprietà predefinite per famiglia.
    defaults: HashMap<String, Props>,
    fonts: HashMap<String, String>,
}

impl Styles {
    fn add(&mut self, doc: &roxmltree::Document) {
        for n in doc.descendants().filter(|n| n.is_element()) {
            match n.tag_name().name() {
                "style" => {
                    let (Some(fam), Some(name)) = (attr(n, "family"), attr(n, "name")) else { continue };
                    let parent = attr(n, "parent-style-name").map(str::to_string);
                    let mut props = props_of(n);
                    if let Some(m) = attr(n, "master-page-name") {
                        props.insert("style:master-page-name".into(), m.into());
                    }
                    self.by_name.insert((fam.to_string(), name.to_string()), (parent, props));
                }
                "default-style" => {
                    let Some(fam) = attr(n, "family") else { continue };
                    self.defaults.entry(fam.to_string()).or_default().extend(props_of(n));
                }
                "font-face" => {
                    if let (Some(name), Some(family)) = (attr(n, "name"), attr(n, "font-family")) {
                        self.fonts.insert(name.to_string(), family.trim_matches(['\'', '"']).to_string());
                    }
                }
                _ => {}
            }
        }
    }

    /// Le proprietà complete di uno stile, dal predefinito della famiglia in giù.
    fn resolve(&self, family: &str, name: &str) -> Props {
        let mut chain = Vec::new();
        let mut cur = Some(name.to_string());
        while let Some(n) = cur.take() {
            if chain.len() > 16 {
                break;
            }
            let Some((parent, props)) = self.by_name.get(&(family.to_string(), n)) else { break };
            chain.push(props);
            cur = parent.clone();
        }
        let mut out = self.defaults.get(family).cloned().unwrap_or_default();
        // Lo stile «Default» delle celle vale come genitore di tutti.
        if family == "table-cell" && name != "Default" {
            if let Some((_, p)) = self.by_name.get(&("table-cell".to_string(), "Default".to_string())) {
                out.extend(p.clone());
            }
        }
        for p in chain.into_iter().rev() {
            out.extend(p.clone());
        }
        out
    }
}

fn cell_style(p: &Props, fonts: &HashMap<String, String>) -> Stile {
    let g = |k: &str| p.get(k).map(String::as_str);
    let font_name = g("style:font-name").map(|n| fonts.get(n).cloned().unwrap_or_else(|| n.to_string())).or_else(|| g("fo:font-family").map(|f| f.trim_matches(['\'', '"']).to_string()));
    let all = g("fo:border").and_then(border);
    let side = |k: &str| match g(k) {
        Some(v) => border(v),
        None => all,
    };
    Stile {
        font: Carattere {
            nome: font_name.unwrap_or_else(|| "Liberation Sans".into()),
            size: g("fo:font-size").and_then(length).unwrap_or(10.0),
            bold: g("fo:font-weight").is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)),
            italic: g("fo:font-style").is_some_and(|s| s == "italic" || s == "oblique"),
            underline: g("style:text-underline-style").is_some_and(|s| s != "none"),
            strike: g("style:text-line-through-style").is_some_and(|s| s != "none"),
            color: g("fo:color").and_then(color).unwrap_or((0, 0, 0)),
        },
        fill: g("fo:background-color").and_then(color),
        bordi: [side("fo:border-left"), side("fo:border-right"), side("fo:border-top"), side("fo:border-bottom")],
        h: if g("style:text-align-source") == Some("value-type") {
            HAlign::General
        } else {
            match g("fo:text-align") {
                Some("start" | "left") => HAlign::Left,
                Some("center") => HAlign::Center,
                Some("end" | "right") => HAlign::Right,
                Some("justify") => HAlign::Justify,
                _ => HAlign::General,
            }
        },
        v: match g("style:vertical-align") {
            Some("top") => VAlign::Top,
            Some("middle") => VAlign::Center,
            _ => VAlign::Bottom,
        },
        wrap: g("fo:wrap-option") == Some("wrap"),
        indent: g("fo:margin-left").and_then(length).map_or(0, |m| (m / 7.5).round().clamp(0.0, 250.0) as u8),
        shrink: truthy(g("style:shrink-to-fit")),
        rotation: g("style:rotation-angle").and_then(|a| a.trim_end_matches("deg").parse::<f32>().ok()).map_or(0, |a| {
            let a = a.rem_euclid(360.0);
            if a > 180.0 {
                (a - 360.0) as i16
            } else {
                a as i16
            }
        }),
        numfmt: String::new(),
    }
}

/// Una data ISO («2026-11-11T14:30:00») come numero di serie (1899-12-30 = 0).
fn iso_serial(s: &str) -> Option<f64> {
    let (d, t) = s.split_once('T').unwrap_or((s, ""));
    let mut it = d.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let day: i64 = it.next()?.parse().ok()?;
    // Giorni dal 1970 (algoritmo di Howard Hinnant), più 25569.
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let mut secs = 0.0;
    if !t.is_empty() {
        let mut p = t.split(':');
        let h: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let mi: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let se: f64 = p.next().and_then(|v| v.trim_end_matches('Z').parse().ok()).unwrap_or(0.0);
        secs = h * 3600.0 + mi * 60.0 + se;
    }
    Some(days as f64 + 25_569.0 + secs / 86_400.0)
}

/// «PT14H30M05S» come frazione di giorno.
fn duration(s: &str) -> Option<f64> {
    let s = s.strip_prefix('-').unwrap_or(s);
    let s = s.strip_prefix('P')?;
    let (days, time) = s.split_once('T').unwrap_or((s, ""));
    let d: f64 = days.trim_end_matches('D').parse().unwrap_or(0.0);
    let mut secs = 0.0;
    let mut num = String::new();
    for c in time.chars() {
        match c {
            'H' => secs += num.parse::<f64>().unwrap_or(0.0) * 3600.0,
            'M' => secs += num.parse::<f64>().unwrap_or(0.0) * 60.0,
            'S' => secs += num.parse::<f64>().unwrap_or(0.0),
            _ => {
                num.push(c);
                continue;
            }
        }
        num.clear();
    }
    Some(d + secs / 86_400.0)
}

/// Il testo dei paragrafi di una cella, con spazi, tabulazioni, a capo e l'eventuale collegamento.
fn cell_text(c: roxmltree::Node) -> (String, Option<String>) {
    let mut lines = Vec::new();
    let mut link = None;
    for p in c.children().filter(|n| n.tag_name().name() == "p") {
        let mut s = String::new();
        for d in p.descendants() {
            if d.is_text() {
                // Il testo delle note (annotation) non si stampa.
                if d.ancestors().any(|a| a.tag_name().name() == "annotation") {
                    continue;
                }
                s.push_str(d.text().unwrap_or(""));
            } else {
                match d.tag_name().name() {
                    "s" => s.push_str(&" ".repeat(attr(d, "c").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(1000))),
                    "tab" => s.push('\t'),
                    "line-break" => s.push('\n'),
                    "a" => {
                        if link.is_none() {
                            link = attr(d, "href").map(str::to_string);
                        }
                    }
                    _ => {}
                }
            }
        }
        lines.push(s);
    }
    (lines.join("\n"), link)
}

/// Un'area di stampa di Calc («Foglio1.A1:Foglio1.E30», «$'Spese 2026'.$A$1:.$E$30»).
fn print_area(s: &str) -> Option<super::foglio::Area> {
    // Le aree sono separate da spazi, ma i nomi dei fogli tra apici possono averne.
    let mut quoted = false;
    let end = s.char_indices().find(|(_, c)| {
        if *c == '\'' {
            quoted = !quoted;
        }
        c.is_whitespace() && !quoted
    });
    let first = &s[..end.map_or(s.len(), |e| e.0)];
    let (a, b) = first.split_once(':').unwrap_or((first, first));
    let cell = |x: &str| super::foglio::cell_ref(x.rsplit('.').next().unwrap_or(x));
    let (p, q) = (cell(a)?, cell(b)?);
    Some((p.0.min(q.0), p.1.min(q.1), p.0.max(q.0), p.1.max(q.1)))
}

/// Intestazione o piè di Calc nei codici di Excel che capisce l'impaginazione (&L &C &R &P &N &A…).
fn header_codes(n: roxmltree::Node) -> String {
    if attr(n, "display") == Some("false") {
        return String::new();
    }
    let region = |r: roxmltree::Node| -> String {
        let mut out = String::new();
        for (i, p) in r.children().filter(|x| x.tag_name().name() == "p").enumerate() {
            if i > 0 {
                out.push('\n');
            }
            for d in p.descendants() {
                if d.is_text() {
                    if d.parent().is_some_and(|x| matches!(x.tag_name().name(), "page-number" | "page-count" | "sheet-name" | "file-name" | "title" | "date" | "time")) {
                        continue;
                    }
                    out.push_str(&d.text().unwrap_or("").replace('&', "&&"));
                } else {
                    match d.tag_name().name() {
                        "page-number" => out.push_str("&P"),
                        "page-count" => out.push_str("&N"),
                        "sheet-name" => out.push_str("&A"),
                        "file-name" | "title" => out.push_str("&F"),
                        "date" => out.push_str("&D"),
                        "time" => out.push_str("&T"),
                        "s" => out.push(' '),
                        _ => {}
                    }
                }
            }
        }
        out
    };
    let mut out = String::new();
    let mut any_region = false;
    for (tag, code) in [("region-left", "&L"), ("region-center", "&C"), ("region-right", "&R")] {
        if let Some(r) = n.children().find(|c| c.tag_name().name() == tag) {
            any_region = true;
            let t = region(r);
            if !t.is_empty() {
                out.push_str(code);
                out.push_str(&t);
            }
        }
    }
    if !any_region {
        let t = region(n);
        if !t.is_empty() {
            out = format!("&C{t}");
        }
    }
    out
}

/// Le impostazioni di pagina dello stile di pagina «master» di un foglio.
fn page_setup(styles_doc: Option<&roxmltree::Document>, master: &str, f: &mut Foglio) {
    let Some(doc) = styles_doc else { return };
    let masters: Vec<_> = doc.descendants().filter(|n| n.tag_name().name() == "master-page").collect();
    let Some(mp) = masters.iter().find(|n| attr(**n, "name") == Some(master)).or_else(|| masters.iter().find(|n| attr(**n, "name") == Some("Default"))).or(masters.first()) else { return };
    let layout_name = attr(*mp, "page-layout-name").unwrap_or("");
    let s = &mut f.stampa;
    let mut header_h = 0.0;
    let mut footer_h = 0.0;
    let header = mp.children().find(|c| c.tag_name().name() == "header");
    let footer = mp.children().find(|c| c.tag_name().name() == "footer");
    s.testa = header.map(header_codes).unwrap_or_default();
    s.piede = footer.map(header_codes).unwrap_or_default();
    if let Some(pl) = doc.descendants().find(|n| n.tag_name().name() == "page-layout" && attr(*n, "name") == Some(layout_name)) {
        if let Some(pp) = pl.children().find(|c| c.tag_name().name() == "page-layout-properties") {
            let g = |k: &str| attr(pp, k);
            let w = g("page-width").and_then(length);
            let h = g("page-height").and_then(length);
            if let (Some(w), Some(h)) = (w, h) {
                s.carta = Some((w.min(h), w.max(h)));
                s.orizzontale = Some(w > h);
            }
            if let Some(o) = g("print-orientation") {
                s.orizzontale = Some(o == "landscape");
            }
            let m = |k: &str, d: f32| g(k).and_then(length).unwrap_or(d);
            let (ml, mr, mt, mb) = (m("margin-left", 56.7), m("margin-right", 56.7), m("margin-top", 56.7), m("margin-bottom", 56.7));
            s.margini = [ml, mr, mt, mb, mt, mb];
            if let Some(print) = g("print") {
                let parts: Vec<&str> = print.split_whitespace().collect();
                s.griglia = parts.contains(&"grid");
                s.intestazioni = parts.contains(&"headers");
            }
            s.giu_poi_destra = g("print-page-order") != Some("ltr");
            if let Some(sc) = g("scale-to").and_then(|v| v.trim_end_matches('%').parse::<f32>().ok()) {
                s.scala = sc.round().clamp(10.0, 400.0) as u32;
            }
            let x = g("scale-to-X").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            let y = g("scale-to-Y").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            if x > 0 || y > 0 {
                s.adatta = Some((x, y));
            } else if g("scale-to-pages").and_then(|v| v.parse::<u32>().ok()) == Some(1) {
                s.adatta = Some((1, 1));
            }
            match g("table-centering") {
                Some("both") => (s.centra_o, s.centra_v) = (true, true),
                Some("horizontal") => s.centra_o = true,
                Some("vertical") => s.centra_v = true,
                _ => {}
            }
            if let Some(n) = g("first-page-number").and_then(|v| v.parse::<u32>().ok()) {
                s.primo_numero = Some(n);
            }
        }
        let hf = |tag: &str, margin: &str| -> f32 {
            let Some(st) = pl.children().find(|c| c.tag_name().name() == tag) else { return 0.0 };
            let Some(p) = st.children().find(|c| c.tag_name().name() == "header-footer-properties") else { return 0.0 };
            // In LibreOffice l'altezza dell'intestazione comprende già la distanza dal foglio.
            let h = attr(p, "min-height").or_else(|| attr(p, "height")).and_then(length);
            h.unwrap_or_else(|| attr(p, margin).and_then(length).unwrap_or(0.0))
        };
        if header.is_some_and(|h| attr(h, "display") != Some("false")) {
            header_h = hf("header-style", "margin-bottom");
        }
        if footer.is_some_and(|h| attr(h, "display") != Some("false")) {
            footer_h = hf("footer-style", "margin-top");
        }
    }
    // In Calc l'intestazione sta dentro il margine: il foglio comincia sotto di lei.
    s.margini[2] += header_h;
    s.margini[3] += footer_h;
}

/// Il foglio attivo, da settings.xml.
fn active_sheet(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, names: &[String]) -> usize {
    let Some(xml) = voce(zip, "settings.xml") else { return 0 };
    let Ok(doc) = roxmltree::Document::parse(&xml) else { return 0 };
    doc.descendants()
        .find(|n| n.tag_name().name() == "config-item" && attr(*n, "name") == Some("ActiveTable"))
        .and_then(|n| n.text())
        .and_then(|t| names.iter().position(|x| x == t))
        .unwrap_or(0)
}

/// Legge un .ods per stamparlo.
pub fn leggi_ods(bytes: &[u8]) -> Result<Cartella, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("non è un foglio di Calc: {e}"))?;
    let content = voce(&mut zip, "content.xml").ok_or("contenuto del foglio non trovato")?;
    let styles_xml = voce(&mut zip, "styles.xml");
    let doc = parse(&content)?;
    let styles_doc = styles_xml.as_deref().and_then(|x| parse(x).ok());
    let mut st = Styles { by_name: HashMap::new(), defaults: HashMap::new(), fonts: HashMap::new() };
    if let Some(sd) = &styles_doc {
        st.add(sd);
    }
    st.add(&doc);
    let mut stili: Vec<Stile> = vec![cell_style(&st.resolve("table-cell", "Default"), &st.fonts)];
    let mut index: HashMap<String, u32> = HashMap::new();
    let mut style_index = |name: Option<&str>, stili: &mut Vec<Stile>| -> u32 {
        let Some(name) = name else { return 0 };
        if name == "Default" {
            return 0;
        }
        if let Some(i) = index.get(name) {
            return *i;
        }
        let s = cell_style(&st.resolve("table-cell", name), &st.fonts);
        let i = match stili.iter().position(|x| *x == s) {
            Some(i) => i as u32,
            None => {
                stili.push(s);
                (stili.len() - 1) as u32
            }
        };
        index.insert(name.to_string(), i);
        i
    };
    let size_of = |family: &str, name: Option<&str>, key: &str| -> Option<f32> { name.and_then(|n| st.resolve(family, n).get(key).and_then(|v| length(v))) };
    let breaks_before = |family: &str, name: Option<&str>| -> bool { name.is_some_and(|n| st.resolve(family, n).get("fo:break-before").is_some_and(|v| v == "page")) };

    let mut fogli = Vec::new();
    let mut names = Vec::new();
    let tables: Vec<_> = doc.descendants().filter(|n| n.tag_name().name() == "table" && n.parent().is_some_and(|p| p.tag_name().name() == "spreadsheet")).collect();
    for t in tables {
        let nome = attr(t, "name").unwrap_or("Foglio").to_string();
        names.push(nome.clone());
        let mut f = Foglio::new(&nome);
        f.col_default = 64.0;
        f.riga_default = 12.8;
        let tprops = attr(t, "style-name").map(|n| st.resolve("table", n)).unwrap_or_default();
        f.nascosto = tprops.get("table:display").is_some_and(|v| v == "false");
        if let Some(c) = tprops.get("table:tab-color").and_then(|c| color(c)) {
            f.colore = Some(c);
        }
        let master = tprops.get("style:master-page-name").cloned().unwrap_or_else(|| "Default".into());
        page_setup(styles_doc.as_ref(), &master, &mut f);
        if let Some(a) = attr(t, "print-ranges").and_then(print_area) {
            f.stampa.area = Some(a);
        }

        // Colonne.
        let mut c = 0u32;
        let mut head_cols: Option<(u32, u32)> = None;
        for col in t.descendants().filter(|n| n.tag_name().name() == "table-column") {
            let rep = attr(col, "number-columns-repeated").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).max(1);
            let w = size_of("table-column", attr(col, "style-name"), "style:column-width");
            let hidden = matches!(attr(col, "visibility"), Some("collapse" | "filter"));
            let def = attr(col, "default-cell-style-name").map(|n| style_index(Some(n), &mut stili)).unwrap_or(0);
            let brk = breaks_before("table-column", attr(col, "style-name"));
            let in_head = col.ancestors().any(|a| a.tag_name().name() == "table-header-columns");
            let end = c.saturating_add(rep).min(MAX_COLS);
            if brk && c > 0 {
                f.stampa.salti_colonne.push(c);
            }
            if in_head {
                head_cols = Some(head_cols.map_or((c, end - 1), |(a, _)| (a, end - 1)));
            }
            // Le colonne ripetute a migliaia (fino a XFD) non servono una per una se sono normali.
            if w.is_some() || hidden || def != 0 {
                for x in c..end {
                    f.colonne.insert(x, (w, hidden, def));
                }
            }
            c = c.saturating_add(rep);
            if c >= MAX_COLS {
                break;
            }
        }
        f.stampa.titoli_colonne = head_cols;
        // Larghezza normale: quella delle colonne più frequenti in fondo (Calc ripete l'ultima fino alla fine).
        if let Some(last) = t.descendants().filter(|n| n.tag_name().name() == "table-column").last() {
            if let Some(w) = size_of("table-column", attr(last, "style-name"), "style:column-width") {
                f.col_default = w;
            }
        }

        // Righe e celle.
        let mut r = 0u32;
        let mut head_rows: Option<(u32, u32)> = None;
        for row in t.descendants().filter(|n| n.tag_name().name() == "table-row") {
            if r >= MAX_ROWS {
                break;
            }
            let rep = attr(row, "number-rows-repeated").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).max(1);
            let h = size_of("table-row", attr(row, "style-name"), "style:row-height");
            let hidden = matches!(attr(row, "visibility"), Some("collapse" | "filter"));
            let row_def = attr(row, "default-cell-style-name").map(|n| style_index(Some(n), &mut stili));
            let brk = breaks_before("table-row", attr(row, "style-name"));
            let in_head = row.ancestors().any(|a| a.tag_name().name() == "table-header-rows");
            let has_content = row.children().any(|c| c.tag_name().name() == "table-cell" && (c.has_children() || attr(c, "value-type").is_some()));
            // Una riga vuota ripetuta migliaia di volte è il fondo del foglio: conta solo per l'altezza.
            let rep_eff = if rep > 1000 && !has_content { 1 } else { rep };
            let end = r.saturating_add(rep_eff).min(MAX_ROWS);
            if brk && r > 0 {
                f.stampa.salti_righe.push(r);
            }
            if in_head {
                head_rows = Some(head_rows.map_or((r, end - 1), |(a, _)| (a, end - 1)));
            }
            for y in r..end {
                if h.is_some() || hidden || row_def.is_some() {
                    f.righe.insert(y, (h, hidden, row_def));
                }
            }
            let mut c = 0u32;
            for cell in row.children().filter(|n| matches!(n.tag_name().name(), "table-cell" | "covered-table-cell")) {
                let crep = attr(cell, "number-columns-repeated").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).max(1);
                if c >= MAX_COLS {
                    break;
                }
                let covered = cell.tag_name().name() == "covered-table-cell";
                let style_name = attr(cell, "style-name").or_else(|| attr(row, "default-cell-style-name")).or_else(|| None);
                let stile = match style_name {
                    Some(n) => style_index(Some(n), &mut stili),
                    None => f.colonne.get(&c).map_or(0, |x| x.2),
                };
                let (text, link) = cell_text(cell);
                let vt = attr(cell, "value-type");
                let valore = match vt {
                    Some("float" | "currency" | "percentage") => attr(cell, "value").and_then(|v| v.parse().ok()).map_or(Valore::Testo(text.clone()), Valore::Numero),
                    Some("date") => attr(cell, "date-value").and_then(iso_serial).map_or(Valore::Testo(text.clone()), Valore::Numero),
                    Some("time") => attr(cell, "time-value").and_then(duration).map_or(Valore::Testo(text.clone()), Valore::Numero),
                    Some("boolean") => Valore::Bool(attr(cell, "boolean-value") == Some("true")),
                    _ if text.is_empty() => Valore::Vuoto,
                    _ => Valore::Testo(text.clone()),
                };
                let shown = (!text.is_empty() && !matches!(valore, Valore::Testo(_))).then(|| text.clone());
                let visible_style = stile != 0 && stili.get(stile as usize).is_some_and(|s| s.fill.is_some() || s.bordi.iter().any(Option::is_some));
                // Una cella vuota con uno stile suo copre quello della colonna (anche «Default» senza sfondo).
                let col_style = f.colonne.get(&c).map_or(0, |x| x.2);
                let overrides = style_name.is_some() && stile != col_style;
                let is_empty = matches!(valore, Valore::Vuoto);
                // Celle vuote ripetute a migliaia: formattazione di righe o colonne intere, non si stampa.
                let crep_eff = if is_empty && crep > 256 { 0 } else { crep };
                if !covered && (!is_empty || visible_style || overrides) {
                    for y in r..end {
                        for x in c..c.saturating_add(crep_eff).min(MAX_COLS) {
                            f.celle.insert((y, x), Cella { valore: valore.clone(), stile, mostra: shown.clone(), link: link.clone() });
                        }
                    }
                }
                let cs = attr(cell, "number-columns-spanned").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
                let rs = attr(cell, "number-rows-spanned").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
                if !covered && (cs > 1 || rs > 1) {
                    f.unioni.push((r, c, r + rs - 1, c + cs - 1));
                }
                // Le immagini dentro la cella.
                for frame in cell.children().filter(|n| n.tag_name().name() == "frame") {
                    add_frame(&mut zip, frame, r, c, &mut f);
                }
                c = c.saturating_add(crep);
            }
            r = r.saturating_add(rep_eff);
        }
        f.stampa.titoli_righe = head_rows;
        // Le immagini ancorate alla pagina: posizione assoluta dal bordo del foglio.
        for shapes in t.children().filter(|n| n.tag_name().name() == "shapes") {
            for frame in shapes.children().filter(|n| n.tag_name().name() == "frame") {
                add_frame(&mut zip, frame, 0, 0, &mut f);
            }
        }
        fogli.push(f);
    }
    if fogli.is_empty() {
        return Err("nessun foglio nel file".into());
    }
    let attivo = active_sheet(&mut zip, &names);
    Ok(Cartella { fogli, stili, attivo, date1904: false, testo_fuori: true })
}

/// Un'immagine di Calc (draw:frame con draw:image): posizione dal suo ancoraggio.
fn add_frame(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, frame: roxmltree::Node, r0: u32, c0: u32, f: &mut Foglio) {
    let Some(img) = frame.children().find(|n| n.tag_name().name() == "image") else { return };
    let Some(href) = attr(img, "href") else { return };
    let Some(bytes) = voce_bytes(zip, href.trim_start_matches("./")) else { return };
    let x = attr(frame, "x").and_then(length).unwrap_or(0.0);
    let y = attr(frame, "y").and_then(length).unwrap_or(0.0);
    let w = attr(frame, "width").and_then(length).unwrap_or(0.0);
    let h = attr(frame, "height").and_then(length).unwrap_or(0.0);
    if w <= 0.5 || h <= 0.5 {
        return;
    }
    // Da una posizione in punti alla cella che la contiene, sommando colonne e righe.
    let (mut c, mut dx) = (c0, x);
    while c < MAX_COLS && dx >= f.col_width(c) && f.col_width(c) > 0.0 {
        dx -= f.col_width(c);
        c += 1;
    }
    let row_h = |f: &Foglio, r: u32| if f.row_hidden(r) { 0.0 } else { f.righe.get(&r).and_then(|x| x.0).unwrap_or(f.riga_default) };
    let (mut r, mut dy) = (r0, y);
    while r < MAX_ROWS && dy >= row_h(f, r) && row_h(f, r) > 0.0 {
        dy -= row_h(f, r);
        r += 1;
    }
    if let Some(im) = crate::pdfdoc::DocImage::from_bytes(&bytes, Some((w, h)), 0) {
        f.immagini.push((r, c, dx, dy, w, h, im));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_borders_dates() {
        assert!((length("2.54cm").unwrap() - 72.0).abs() < 0.01);
        assert_eq!(length("12pt"), Some(12.0));
        assert!((length("0.5in").unwrap() - 36.0).abs() < 0.01);
        let b = border("0.74pt solid #ff0000").unwrap();
        assert_eq!(b.color, (255, 0, 0));
        assert!(border("none").is_none());
        assert!(border("2.01pt double #000000").unwrap().double);
        assert_eq!(iso_serial("2026-11-11"), Some(46337.0));
        assert!((iso_serial("2026-11-11T12:00:00").unwrap() - 46337.5).abs() < 1e-9);
        assert!((duration("PT36H00M00S").unwrap() - 1.5).abs() < 1e-9);
        assert_eq!(print_area("$'Spese 2026'.$A$1:.$E$30"), Some((0, 0, 29, 4)));
        assert_eq!(print_area("Foglio1.B2:Foglio1.C3"), Some((1, 1, 2, 2)));
    }
}
