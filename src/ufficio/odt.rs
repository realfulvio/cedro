//! I documenti di LibreOffice Writer e OpenOffice (.odt) letti nel modello del documento: stili dei
//! paragrafi e del testo con le loro eredità, elenchi con i loro numeri, tabelle con celle unite,
//! immagini e caselle, note, campi, collegamenti e le pagine (carta, margini, intestazioni e piè).

use std::collections::HashMap;
use std::io::Cursor;

use super::documento::*;
use super::foglio::{attr, parse, voce, voce_bytes};
use super::ods::{border, color, length};
use super::pagina::Target;

/// Le proprietà di uno stile, per gruppo: «p» paragrafo, «t» testo, «c» cella, «tb» tabella,
/// «col» colonna, «row» riga, «g» grafica, «sec» sezione.
#[derive(Clone, Default, Debug)]
struct Props {
    map: HashMap<String, String>,
    tabs: Option<Vec<Tab>>,
    columns: Option<(u8, f32)>,
}

impl Props {
    fn get(&self, k: &str) -> Option<&str> {
        self.map.get(k).map(String::as_str)
    }

    fn extend(&mut self, o: &Props) {
        // Proprietà che si escludono: quella del figlio cancella le sorelle ereditate.
        const ESCLUSIVE: [&str; 3] = ["line-height", "line-height-at-least", "line-spacing"];
        for (k, _) in &o.map {
            let Some((g, name)) = k.split_once(':') else { continue };
            if ESCLUSIVE.contains(&name) {
                for e in ESCLUSIVE {
                    self.map.remove(&format!("{g}:{e}"));
                }
            }
            if matches!(name, "margin" | "padding" | "border") {
                for side in ["top", "bottom", "left", "right"] {
                    self.map.remove(&format!("{g}:{name}-{side}"));
                }
            }
        }
        for (k, v) in &o.map {
            self.map.insert(k.clone(), v.clone());
        }
        if o.tabs.is_some() {
            self.tabs = o.tabs.clone();
        }
        if o.columns.is_some() {
            self.columns = o.columns;
        }
    }
}

fn group_of(element: &str) -> &'static str {
    match element {
        "paragraph-properties" => "p",
        "text-properties" => "t",
        "table-cell-properties" => "c",
        "table-properties" => "tb",
        "table-column-properties" => "col",
        "table-row-properties" => "row",
        "graphic-properties" => "g",
        "section-properties" => "sec",
        "page-layout-properties" => "pl",
        "header-footer-properties" => "hf",
        _ => "x",
    }
}

fn props_of(n: roxmltree::Node) -> Props {
    let mut p = Props::default();
    for c in n.children().filter(|c| c.is_element()) {
        let g = group_of(c.tag_name().name());
        for a in c.attributes() {
            p.map.insert(format!("{g}:{}", a.name()), a.value().to_string());
        }
        if let Some(ts) = c.children().find(|x| x.tag_name().name() == "tab-stops") {
            let tabs = ts
                .children()
                .filter(|t| t.tag_name().name() == "tab-stop")
                .filter_map(|t| {
                    let pos = length(attr(t, "position")?)?;
                    let tipo = match attr(t, "type") {
                        Some("center") => TipoTab::Centro,
                        Some("right") => TipoTab::Destra,
                        Some("char") => TipoTab::Decimale,
                        _ => TipoTab::Sinistra,
                    };
                    let leader = attr(t, "leader-text").and_then(|l| l.chars().next()).filter(|c| *c != ' ').or_else(|| match attr(t, "leader-style") {
                        Some("dotted") => Some('.'),
                        Some("solid") => Some('_'),
                        Some("dash" | "long-dash") => Some('-'),
                        _ => None,
                    });
                    Some(Tab { pos, tipo, leader })
                })
                .collect();
            p.tabs = Some(tabs);
        }
        if let Some(cols) = c.children().find(|x| x.tag_name().name() == "columns") {
            let n = attr(cols, "column-count").and_then(|v| v.parse::<u8>().ok()).unwrap_or(1).max(1);
            let gap = attr(cols, "column-gap").and_then(length).unwrap_or(18.0);
            p.columns = Some((n, gap));
        }
    }
    p
}

#[derive(Clone, Debug)]
struct ListLevel {
    bullet: Option<String>,
    format: String,
    prefix: String,
    suffix: String,
    display: usize,
    start: u32,
    margin: f32,
    indent: f32,
    tab: bool,
    tab_pos: Option<f32>,
    font: Option<String>,
}

#[derive(Default)]
struct Styles {
    by_name: HashMap<(String, String), (Option<String>, Props, Option<String>)>,
    defaults: HashMap<String, Props>,
    fonts: HashMap<String, String>,
    lists: HashMap<String, Vec<ListLevel>>,
    outline: Vec<ListLevel>,
    tab_default: f32,
    /// Stile di pagina → layout.
    masters: HashMap<String, String>,
    layouts: HashMap<String, (Props, Props, Props)>,
}

fn list_levels(n: roxmltree::Node) -> Vec<ListLevel> {
    let mut levels: Vec<ListLevel> = Vec::new();
    for l in n.children().filter(|c| c.is_element()) {
        let kind = l.tag_name().name();
        if !kind.starts_with("list-level-style") && kind != "outline-level-style" {
            continue;
        }
        let lvl = attr(l, "level").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).clamp(1, 10);
        let lp = l.children().find(|c| c.tag_name().name() == "list-level-properties");
        let align = lp.and_then(|p| p.children().find(|c| c.tag_name().name() == "list-level-label-alignment"));
        let (margin, indent, tab, tab_pos) = if let Some(a) = align {
            (
                attr(a, "margin-left").and_then(length).unwrap_or(0.0),
                attr(a, "text-indent").and_then(length).unwrap_or(0.0),
                attr(a, "label-followed-by").is_none_or(|v| v == "listtab"),
                attr(a, "list-tab-stop-position").and_then(length),
            )
        } else {
            let space = lp.and_then(|p| attr(p, "space-before")).and_then(length).unwrap_or(0.0);
            let width = lp.and_then(|p| attr(p, "min-label-width")).and_then(length).unwrap_or(0.0);
            (space + width, -width, true, None)
        };
        let tp = l.children().find(|c| c.tag_name().name() == "text-properties");
        let level = ListLevel {
            bullet: (kind == "list-level-style-bullet").then(|| attr(l, "bullet-char").unwrap_or("•").to_string()),
            format: attr(l, "num-format").unwrap_or(if kind == "list-level-style-bullet" { "" } else { "1" }).to_string(),
            prefix: attr(l, "num-prefix").unwrap_or("").to_string(),
            suffix: attr(l, "num-suffix").unwrap_or("").to_string(),
            display: attr(l, "display-levels").and_then(|v| v.parse().ok()).unwrap_or(1),
            start: attr(l, "start-value").and_then(|v| v.parse().ok()).unwrap_or(1),
            margin,
            indent,
            tab,
            tab_pos,
            font: tp.and_then(|t| attr(t, "font-name").or_else(|| attr(t, "font-family"))).map(str::to_string),
        };
        if levels.len() < lvl {
            levels.resize(lvl, level.clone());
        }
        levels[lvl - 1] = level;
    }
    levels
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
                        props.map.insert("master".into(), m.into());
                    }
                    if let Some(l) = attr(n, "list-style-name") {
                        props.map.insert("list".into(), l.into());
                    }
                    if n.parent().is_some_and(|p| p.tag_name().name() == "automatic-styles") {
                        props.map.insert("\u{0}auto".into(), "1".into());
                    }
                    let level = attr(n, "default-outline-level").map(str::to_string);
                    self.by_name.insert((fam.to_string(), name.to_string()), (parent, props, level));
                }
                "default-style" => {
                    let Some(fam) = attr(n, "family") else { continue };
                    let p = props_of(n);
                    if fam == "paragraph" {
                        if let Some(d) = p.get("p:tab-stop-distance").and_then(length) {
                            self.tab_default = d;
                        }
                    }
                    self.defaults.entry(fam.to_string()).or_default().extend(&p);
                }
                "font-face" => {
                    if let (Some(name), Some(family)) = (attr(n, "name"), attr(n, "font-family")) {
                        self.fonts.insert(name.to_string(), family.trim_matches(['\'', '"']).to_string());
                    }
                }
                "list-style" => {
                    if let Some(name) = attr(n, "name") {
                        self.lists.insert(name.to_string(), list_levels(n));
                    }
                }
                "outline-style" => self.outline = list_levels(n),
                "master-page" => {
                    if let (Some(name), Some(layout)) = (attr(n, "name"), attr(n, "page-layout-name")) {
                        self.masters.insert(name.to_string(), layout.to_string());
                    }
                }
                "page-layout" => {
                    let Some(name) = attr(n, "name") else { continue };
                    let pl = n.children().find(|c| c.tag_name().name() == "page-layout-properties").map(|c| {
                        let mut p = Props::default();
                        for a in c.attributes() {
                            p.map.insert(a.name().to_string(), a.value().to_string());
                        }
                        if let Some(cols) = c.children().find(|x| x.tag_name().name() == "columns") {
                            let k = attr(cols, "column-count").and_then(|v| v.parse::<u8>().ok()).unwrap_or(1).max(1);
                            p.columns = Some((k, attr(cols, "column-gap").and_then(length).unwrap_or(18.0)));
                        }
                        p
                    });
                    let hf = |tag: &str| -> Props {
                        let mut p = Props::default();
                        if let Some(h) = n.children().find(|c| c.tag_name().name() == tag).and_then(|h| h.children().find(|c| c.tag_name().name() == "header-footer-properties")) {
                            for a in h.attributes() {
                                p.map.insert(a.name().to_string(), a.value().to_string());
                            }
                        }
                        p
                    };
                    self.layouts.insert(name.to_string(), (pl.unwrap_or_default(), hf("header-style"), hf("footer-style")));
                }
                _ => {}
            }
        }
    }

    fn resolve(&self, family: &str, name: &str) -> Props {
        let mut chain = Vec::new();
        let mut cur = Some(name.to_string());
        while let Some(n) = cur.take() {
            if chain.len() > 16 {
                break;
            }
            let Some((parent, props, _)) = self.by_name.get(&(family.to_string(), n)) else { break };
            chain.push(props);
            cur = parent.clone();
        }
        let mut out = self.defaults.get(family).cloned().unwrap_or_default();
        if family == "paragraph" {
            // Le proprietà del testo predefinite valgono anche per i paragrafi.
            if let Some(t) = self.defaults.get("text") {
                out.extend(t);
            }
        }
        for p in chain.into_iter().rev() {
            out.extend(p);
        }
        // Il master e l'elenco non si ereditano dal genitore se lo stile li toglie: già fatto dall'ordine.
        out
    }

    /// Le proprietà scritte nello stile automatico (le «dirette» del paragrafo), senza i genitori.
    fn own(&self, family: &str, name: &str) -> Option<&Props> {
        let (_, props, _) = self.by_name.get(&(family.to_string(), name.to_string()))?;
        props.get("\u{0}auto").is_some().then_some(props)
    }

    /// Lo stile con nome che sta dietro a uno automatico.
    fn named(&self, family: &str, name: &str) -> String {
        match self.by_name.get(&(family.to_string(), name.to_string())) {
            Some((Some(parent), props, _)) if props.get("\u{0}auto").is_some() => parent.clone(),
            _ => name.to_string(),
        }
    }

    fn outline_level(&self, name: &str) -> Option<u8> {
        let mut cur = Some(name.to_string());
        for _ in 0..16 {
            let n = cur.take()?;
            let (parent, _, level) = self.by_name.get(&("paragraph".to_string(), n))?;
            if let Some(l) = level.as_deref().and_then(|v| v.parse::<u8>().ok()).filter(|v| *v > 0) {
                return Some(l.min(9));
            }
            cur = parent.clone();
        }
        None
    }
}

fn font_of(p: &Props, fonts: &HashMap<String, String>) -> Font {
    let g = |k: &str| p.get(k);
    let mut f = Font::default();
    if let Some(n) = g("t:font-name") {
        f.family = fonts.get(n).cloned().unwrap_or_else(|| n.to_string());
    } else if let Some(fam) = g("t:font-family") {
        f.family = fam.trim_matches(['\'', '"']).to_string();
    } else {
        f.family = "Liberation Serif".into();
    }
    f.size = g("t:font-size").and_then(length).unwrap_or(12.0);
    f.bold = g("t:font-weight").is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|v| v >= 600));
    f.italic = g("t:font-style").is_some_and(|s| s == "italic" || s == "oblique");
    f.underline = g("t:text-underline-style").is_some_and(|s| s != "none");
    f.double_underline = f.underline && g("t:text-underline-type") == Some("double");
    f.strike = g("t:text-line-through-style").is_some_and(|s| s != "none");
    if let Some(c) = g("t:color").and_then(color) {
        f.color = c;
    }
    f.highlight = g("t:background-color").and_then(color);
    if let Some(pos) = g("t:text-position") {
        let first = pos.split_whitespace().next().unwrap_or("");
        f.vert = match first {
            "super" => 1,
            "sub" => -1,
            v => {
                let n: f32 = v.trim_end_matches('%').parse().unwrap_or(0.0);
                if n > 0.0 {
                    1
                } else if n < 0.0 {
                    -1
                } else {
                    0
                }
            }
        };
    }
    f.caps = g("t:text-transform") == Some("uppercase");
    f.small_caps = g("t:font-variant") == Some("small-caps");
    f.spacing = g("t:letter-spacing").and_then(length).unwrap_or(0.0);
    f
}

/// Una dimensione del testo relativa («120%») rispetto al carattere del genitore.
fn font_in(p: &Props, parent: &Font, fonts: &HashMap<String, String>) -> Font {
    let mut f = parent.clone();
    let own = font_of(p, fonts);
    if p.get("t:font-name").is_some() || p.get("t:font-family").is_some() {
        f.family = own.family;
    }
    if let Some(s) = p.get("t:font-size") {
        f.size = if let Some(pc) = s.strip_suffix('%') { parent.size * pc.parse::<f32>().unwrap_or(100.0) / 100.0 } else { length(s).unwrap_or(parent.size) };
    }
    if p.get("t:font-weight").is_some() {
        f.bold = own.bold;
    }
    if p.get("t:font-style").is_some() {
        f.italic = own.italic;
    }
    if p.get("t:text-underline-style").is_some() {
        f.underline = own.underline;
        f.double_underline = own.double_underline;
    }
    if p.get("t:text-line-through-style").is_some() {
        f.strike = own.strike;
    }
    if p.get("t:color").is_some() {
        f.color = own.color;
    }
    if p.get("t:background-color").is_some() {
        f.highlight = own.highlight;
    }
    if p.get("t:text-position").is_some() {
        f.vert = own.vert;
    }
    if p.get("t:text-transform").is_some() {
        f.caps = own.caps;
    }
    if p.get("t:font-variant").is_some() {
        f.small_caps = own.small_caps;
    }
    if p.get("t:letter-spacing").is_some() {
        f.spacing = own.spacing;
    }
    f
}

fn linea(s: Option<&str>, padding: f32) -> Option<Linea> {
    let b = border(s?)?;
    Some(Linea { width: b.width, color: b.color, dash: b.dash, double: b.double, space: padding })
}

struct Ctx<'z> {
    zip: zip::ZipArchive<Cursor<&'z [u8]>>,
    st: Styles,
    doc: Documento,
    /// Contatori degli elenchi: stile → numeri per livello.
    counters: HashMap<String, Vec<Option<u32>>>,
    images: HashMap<String, Option<usize>>,
    sections: Vec<Sezione>,
    current: Vec<Blocco>,
    master: String,
    styles_xml: Option<std::rc::Rc<String>>,
    /// La prossima sezione comincia su una pagina nuova.
    next_new_page: bool,
}

struct ListCtx {
    style: String,
    level: usize,
    /// Primo paragrafo dell'elemento (porta il numero) o elemento senza numero (list-header).
    label: bool,
}

impl<'z> Ctx<'z> {
    fn image(&mut self, href: &str, w: f32, h: f32) -> Option<usize> {
        let key = format!("{href}@{w:.1}x{h:.1}");
        if let Some(v) = self.images.get(&key) {
            return *v;
        }
        let bytes = voce_bytes(&mut self.zip, href.trim_start_matches("./"));
        let img = bytes.and_then(|b| crate::pdfdoc::DocImage::from_bytes(&b, Some((w, h)), 0));
        let idx = img.map(|i| {
            self.doc.immagini.push(i);
            self.doc.immagini.len() - 1
        });
        self.images.insert(key, idx);
        idx
    }

    fn label(&mut self, l: &ListCtx) -> Option<(String, ListLevel)> {
        let levels = if l.style == "\u{0}outline" { self.st.outline.clone() } else { self.st.lists.get(&l.style)?.clone() };
        let lv = levels.get(l.level.min(levels.len().saturating_sub(1)))?.clone();
        if !l.label {
            return Some((String::new(), lv));
        }
        let counters = self.counters.entry(l.style.clone()).or_default();
        if counters.len() <= l.level {
            counters.resize(l.level + 1, None);
        }
        counters[l.level] = Some(match counters[l.level] {
            None => lv.start,
            Some(n) => n.saturating_add(1),
        });
        for c in counters.iter_mut().skip(l.level + 1) {
            *c = None;
        }
        if let Some(b) = &lv.bullet {
            return Some((super::docx::bullet_char(b.chars().next().unwrap_or('•')).to_string(), lv));
        }
        if lv.format.is_empty() {
            return Some((format!("{}{}", lv.prefix, lv.suffix), lv));
        }
        let mut parts = Vec::new();
        let from = (l.level + 1).saturating_sub(lv.display.max(1));
        for i in from..=l.level {
            let fmt = levels.get(i).map(|x| x.format.clone()).unwrap_or_else(|| "1".into());
            let v = counters.get(i).copied().flatten().unwrap_or(1);
            parts.push(number_text(v, &fmt));
        }
        Some((format!("{}{}{}", lv.prefix, parts.join("."), lv.suffix), lv))
    }

    fn blocks(&mut self, parent: roxmltree::Node, list: Option<&ListCtx>, out: &mut Vec<Blocco>, body: bool) {
        for c in parent.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "p" | "h" => self.paragraph(c, list, out, body),
                "list" => {
                    let style = attr(c, "style-name").map(str::to_string).or_else(|| list.map(|l| l.style.clone())).unwrap_or_default();
                    let level = list.map_or(0, |l| l.level + 1);
                    if level == 0 && attr(c, "continue-numbering") != Some("true") && attr(c, "continue-list").is_none() {
                        self.counters.remove(&style);
                    }
                    for item in c.children().filter(|x| x.is_element()) {
                        let header = item.tag_name().name() == "list-header";
                        if let Some(sv) = attr(item, "start-value").and_then(|v| v.parse::<u32>().ok()) {
                            let cs = self.counters.entry(style.clone()).or_default();
                            if cs.len() <= level {
                                cs.resize(level + 1, None);
                            }
                            // Il prossimo numero sarà `sv`.
                            cs[level] = sv.checked_sub(1);
                        }
                        let mut first = !header;
                        for x in item.children().filter(|x| x.is_element()) {
                            let lc = ListCtx { style: style.clone(), level, label: first };
                            match x.tag_name().name() {
                                "p" | "h" => {
                                    self.paragraph(x, Some(&lc), out, body);
                                    first = false;
                                }
                                "list" => {
                                    let mut tmp = Vec::new();
                                    let inner = ListCtx { style: style.clone(), level, label: false };
                                    let wrapper = x;
                                    self.blocks_list(wrapper, &inner, &mut tmp, body);
                                    out.append(&mut tmp);
                                }
                                _ => {
                                    let mut tmp = Vec::new();
                                    self.blocks(x, None, &mut tmp, body);
                                    out.append(&mut tmp);
                                }
                            }
                        }
                    }
                }
                "table" => {
                    if let Some(t) = self.table(c) {
                        out.push(Blocco::Tab(t));
                    }
                }
                "section" => {
                    let cols = attr(c, "style-name").map(|s| self.st.resolve("section", s)).and_then(|p| p.columns).filter(|(n, _)| *n > 1);
                    if let (Some((n, gap)), true) = (cols, body) {
                        // Una sezione a colonne: diventa una sezione continua del documento.
                        self.close_section(false);
                        self.blocks(c, list, out, body);
                        self.current.append(out);
                        self.close_section_cols(n, gap);
                    } else {
                        self.blocks(c, list, out, body);
                    }
                }
                "table-of-content" | "alphabetical-index" | "illustration-index" | "table-index" | "object-index" | "user-index" | "bibliography" => {
                    if let Some(b) = c.children().find(|x| x.tag_name().name() == "index-body") {
                        self.blocks(b, list, out, body);
                    }
                }
                "frame" => {
                    // Una cornice ancorata alla pagina, fuori dai paragrafi.
                    if let Some(m) = self.frame(c, &Font::default()) {
                        out.push(Blocco::Mobile(m));
                    }
                }
                "soft-page-break" | "sequence-decls" | "variable-decls" | "user-field-decls" | "tracked-changes" | "forms" => {}
                _ => {}
            }
        }
    }

    /// Un elenco dentro un elemento di elenco: un livello più giù, stesso stile se non ne dice un altro.
    fn blocks_list(&mut self, list: roxmltree::Node, outer: &ListCtx, out: &mut Vec<Blocco>, body: bool) {
        let fake = ListCtx { style: attr(list, "style-name").map(str::to_string).unwrap_or_else(|| outer.style.clone()), level: outer.level, label: false };
        // Si riusa il codice di «list» con il livello di partenza giusto.
        let parent_level = fake.level;
        let style = fake.style.clone();
        let level = parent_level + 1;
        for item in list.children().filter(|x| x.is_element()) {
            let header = item.tag_name().name() == "list-header";
            let mut first = !header;
            for x in item.children().filter(|x| x.is_element()) {
                let lc = ListCtx { style: style.clone(), level, label: first };
                match x.tag_name().name() {
                    "p" | "h" => {
                        self.paragraph(x, Some(&lc), out, body);
                        first = false;
                    }
                    "list" => {
                        let inner = ListCtx { style: style.clone(), level, label: false };
                        self.blocks_list(x, &inner, out, body);
                    }
                    _ => self.blocks(x, None, out, body),
                }
            }
        }
    }

    fn paragraph(&mut self, n: roxmltree::Node, list: Option<&ListCtx>, out: &mut Vec<Blocco>, body: bool) {
        let sname = attr(n, "style-name").unwrap_or("Standard").to_string();
        let p = self.st.resolve("paragraph", &sname);
        // Un cambio di stile di pagina: comincia una sezione nuova.
        if body {
            if let Some(m) = p.get("master").filter(|m| !m.is_empty()) {
                let m = m.to_string();
                if !(self.current.is_empty() && out.is_empty() && self.sections.is_empty()) {
                    self.current.append(out);
                    self.close_section(true);
                }
                self.master = m;
            }
        }
        let font = font_of(&p, &self.st.fonts);
        let g = |k: &str| p.get(k);
        let len = |k: &str| g(k).and_then(length).unwrap_or(0.0);
        let mut par = Paragrafo {
            allinea: match g("p:text-align") {
                Some("center") => Allinea::Centro,
                Some("end" | "right") => Allinea::Destra,
                Some("justify") => Allinea::Giustifica,
                _ => Allinea::Sinistra,
            },
            rientro_sx: len("p:margin-left"),
            rientro_dx: len("p:margin-right"),
            prima_riga: len("p:text-indent"),
            prima: len("p:margin-top"),
            dopo: len("p:margin-bottom"),
            insieme_al_seguente: g("p:keep-with-next") == Some("always"),
            righe_insieme: g("p:keep-together") == Some("always"),
            salto_prima: g("p:break-before") == Some("page"),
            vedove: g("p:widows").or(g("p:orphans")).is_some_and(|v| v != "0"),
            sfondo: g("p:background-color").and_then(color),
            contestuale: g("p:contextual-spacing") == Some("true"),
            stile: self.st.named("paragraph", &sname),
            font_fine: font.clone(),
            interlinea_testo: true,
            ..Default::default()
        };
        par.interlinea = match g("p:line-height") {
            Some(v) if v.ends_with('%') => Interlinea::Multipla(v.trim_end_matches('%').parse::<f32>().unwrap_or(100.0) / 100.0),
            Some(v) if v != "normal" => length(v).map_or(Interlinea::default(), Interlinea::Esatta),
            _ => match (g("p:line-height-at-least").and_then(length), g("p:line-spacing").and_then(length)) {
                (Some(v), _) => Interlinea::Minima(v),
                // Interlinea «iniziale»: spazio in più tra le righe, in proporzione alla riga singola.
                (None, Some(v)) => Interlinea::Multipla(1.0 + v / (font.size * 1.15).max(1.0)),
                _ => Interlinea::default(),
            },
        };
        // Le tabulazioni di Writer partono dal rientro del paragrafo.
        if let Some(tabs) = &p.tabs {
            par.tabs = tabs.iter().map(|t| Tab { pos: t.pos + par.rientro_sx, ..*t }).collect();
            par.tabs.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        }
        let pad = |side: &str| g(&format!("p:padding-{side}")).or(g("p:padding")).and_then(length).unwrap_or(0.0);
        let bd = |side: &str| linea(g(&format!("p:border-{side}")).or(g("p:border")), pad(side));
        par.bordi = [bd("top"), bd("bottom"), bd("left"), bd("right"), None];
        // Titoli: livello dallo stile o dall'elemento.
        let is_h = n.tag_name().name() == "h";
        let level = attr(n, "outline-level").and_then(|v| v.parse::<u8>().ok()).or_else(|| self.st.outline_level(&sname));
        if is_h || level.is_some() {
            par.titolo = level.map(|l| l.clamp(1, 9));
        }
        // Il numero dell'elenco o del titolo.
        let own_list = p.get("list").map(str::to_string);
        let lc_own;
        let lc = match (list, &own_list) {
            (Some(l), _) => Some(l),
            (None, Some(s)) if !s.is_empty() => {
                lc_own = ListCtx { style: s.clone(), level: 0, label: true };
                Some(&lc_own)
            }
            _ if is_h && !self.st.outline.is_empty() => {
                lc_own = ListCtx { style: "\u{0}outline".into(), level: level.unwrap_or(1).saturating_sub(1) as usize, label: true };
                Some(&lc_own)
            }
            _ => None,
        };
        if let Some(l) = lc {
            let l = ListCtx { style: if list.is_some() && own_list.as_ref().is_some_and(|s| !s.is_empty()) && !self.st.lists.contains_key(&l.style) { own_list.clone().unwrap() } else { l.style.clone() }, level: l.level, label: l.label };
            if let Some((text, lv)) = self.label(&l) {
                // Il rientro dell'elenco vince su quello dello stile del paragrafo, non su quello diretto.
                let own = self.st.own("paragraph", &sname);
                if own.and_then(|o| o.get("p:margin-left").or(o.get("p:margin"))).is_none() {
                    let delta = lv.margin - par.rientro_sx;
                    par.rientro_sx = lv.margin;
                    par.tabs.iter_mut().for_each(|t| t.pos += delta);
                }
                if own.and_then(|o| o.get("p:text-indent")).is_none() {
                    par.prima_riga = lv.indent;
                }
                if !text.is_empty() {
                    let mut lf = font.clone();
                    if let Some(fname) = &lv.font {
                        lf.family = self.st.fonts.get(fname).cloned().unwrap_or_else(|| fname.clone());
                    }
                    lf.underline = false;
                    par.etichetta = Some((text, lf));
                    par.etichetta_tab = lv.tab;
                    if let Some(tp) = lv.tab_pos {
                        if !par.tabs.iter().any(|t| (t.pos - tp).abs() < 0.5) && tp > par.rientro_sx + par.prima_riga {
                            par.tabs.push(Tab { pos: tp, tipo: TipoTab::Sinistra, leader: None });
                            par.tabs.sort_by(|a, b| a.pos.total_cmp(&b.pos));
                        }
                    }
                }
            }
        }
        let mut anchored = Vec::new();
        self.inline(n, &font, None, &mut par, &mut anchored);
        for m in anchored {
            out.push(Blocco::Mobile(m));
        }
        if g("p:break-after") == Some("page") {
            par.runs.push(Run { pezzo: Pezzo::SaltoPagina, font: font.clone(), link: None });
        }
        out.push(Blocco::Par(par));
    }

    fn inline(&mut self, n: roxmltree::Node, font: &Font, link: Option<&Target>, par: &mut Paragrafo, anchored: &mut Vec<Mobile>) {
        for c in n.children() {
            if c.is_text() {
                let t = c.text().unwrap_or("");
                // Gli spazi in fila valgono uno solo (gli altri sono text:s).
                let mut s = String::with_capacity(t.len());
                let mut prev_space = par.runs.last().is_some_and(|r| matches!(&r.pezzo, Pezzo::Testo(x) if x.ends_with(' ')));
                for ch in t.chars() {
                    let ch = if ch == '\n' || ch == '\t' || ch == '\r' { ' ' } else { ch };
                    if ch == ' ' {
                        if prev_space {
                            continue;
                        }
                        prev_space = true;
                    } else {
                        prev_space = false;
                    }
                    s.push(ch);
                }
                if !s.is_empty() {
                    push_text(par, s, font, link);
                }
                continue;
            }
            if !c.is_element() {
                continue;
            }
            match c.tag_name().name() {
                "span" => {
                    let f = attr(c, "style-name").map_or(font.clone(), |s| font_in(&self.st.resolve("text", s), font, &self.st.fonts));
                    self.inline(c, &f, link, par, anchored);
                }
                "a" => {
                    let href = c.attributes().find(|a| a.name() == "href").map(|a| a.value().to_string());
                    let t = href.map(|h| if let Some(a) = h.strip_prefix('#') { Target::Anchor(a.to_string()) } else { Target::Uri(h) });
                    let f = attr(c, "style-name").map_or(font.clone(), |s| font_in(&self.st.resolve("text", s), font, &self.st.fonts));
                    self.inline(c, &f, t.as_ref().or(link), par, anchored);
                }
                "s" => {
                    let k = attr(c, "c").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(200);
                    push_text(par, " ".repeat(k), font, link);
                }
                "tab" => par.runs.push(Run { pezzo: Pezzo::Tab, font: font.clone(), link: link.cloned() }),
                "line-break" => par.runs.push(Run { pezzo: Pezzo::ACapo, font: font.clone(), link: None }),
                "page-number" => par.runs.push(Run { pezzo: Pezzo::NumPagina, font: font.clone(), link: None }),
                "page-count" => par.runs.push(Run { pezzo: Pezzo::NumPagine, font: font.clone(), link: None }),
                "bookmark" | "bookmark-start" => {
                    if let Some(nm) = attr(c, "name") {
                        par.segnalibri.push(nm.to_string());
                    }
                }
                "note" => {
                    let end = attr(c, "note-class") == Some("endnote");
                    let cit = c.children().find(|x| x.tag_name().name() == "note-citation").and_then(|x| x.text()).unwrap_or("").to_string();
                    let mut blocks = Vec::new();
                    if let Some(b) = c.children().find(|x| x.tag_name().name() == "note-body") {
                        self.blocks(b, None, &mut blocks, false);
                    }
                    let mut f = font.clone();
                    f.vert = 1;
                    // Il numero della nota anche all'inizio del suo testo.
                    if let Some(Blocco::Par(first)) = blocks.iter_mut().find(|b| matches!(b, Blocco::Par(_))) {
                        let mut nf = first.font_fine.clone();
                        nf.vert = 1;
                        first.runs.insert(0, Run { pezzo: Pezzo::Testo(format!("{cit} ")), font: nf, link: None });
                    }
                    if end {
                        self.doc.note_fine.push((cit, blocks));
                        par.runs.push(Run { pezzo: Pezzo::NotaFine(self.doc.note_fine.len() - 1), font: f, link: None });
                    } else {
                        self.doc.note.push((cit, blocks));
                        par.runs.push(Run { pezzo: Pezzo::Nota(self.doc.note.len() - 1), font: f, link: None });
                    }
                }
                "frame" => {
                    if let Some(m) = self.frame(c, font) {
                        let inline = attr(c, "anchor-type") == Some("as-char");
                        match (&m.contenuto, inline) {
                            (MobileContenuto::Immagine(i), true) => par.runs.push(Run { pezzo: Pezzo::Immagine { img: *i, w: m.w, h: m.h }, font: font.clone(), link: link.cloned() }),
                            _ => anchored.push(m),
                        }
                    }
                }
                "soft-page-break" | "bookmark-end" | "reference-mark-start" | "reference-mark-end" | "annotation" | "annotation-end" | "change-start" | "change-end" | "change" => {}
                "sequence" | "date" | "time" | "chapter" | "title" | "subject" | "author-name" | "file-name" | "reference-ref" | "bookmark-ref" | "sequence-ref" | "note-ref" | "user-field-get" | "variable-get" | "expression" | "meta" | "ruby" | "ruby-base" | "text-input" | "drop-down" | "hidden-text" | "placeholder" | "initial-creator" | "creation-date" | "description" | "keywords" | "word-count" | "character-count" | "paragraph-count" | "table-count" | "image-count" => {
                    if c.tag_name().name() == "hidden-text" {
                        continue;
                    }
                    self.inline(c, font, link, par, anchored);
                }
                _ => self.inline(c, font, link, par, anchored),
            }
        }
    }

    fn frame(&mut self, f: roxmltree::Node, font: &Font) -> Option<Mobile> {
        let w = attr(f, "width").and_then(length).unwrap_or(0.0);
        let h = attr(f, "height").or_else(|| attr(f, "min-height")).and_then(length).unwrap_or(0.0);
        let g = attr(f, "style-name").map(|s| self.st.resolve("graphic", s)).unwrap_or_default();
        let content = if let Some(img) = f.children().find(|c| c.tag_name().name() == "image") {
            let href = img.attributes().find(|a| a.name() == "href").map(|a| a.value().to_string())?;
            MobileContenuto::Immagine(self.image(&href, w, h)?)
        } else if let Some(tb) = f.children().find(|c| c.tag_name().name() == "text-box") {
            let mut blocks = Vec::new();
            self.blocks(tb, None, &mut blocks, false);
            let fill = g.get("g:background-color").and_then(color).or_else(|| (g.get("g:fill") == Some("solid")).then(|| g.get("g:fill-color").and_then(color)).flatten());
            let line = linea(g.get("g:border"), 0.0).or_else(|| (g.get("g:stroke").is_some_and(|s| s != "none")).then(|| Linea { width: g.get("g:stroke-width").and_then(length).unwrap_or(0.5).max(0.5), color: g.get("g:stroke-color").and_then(color).unwrap_or((0, 0, 0)), dash: super::pagina::Dash::Solid, double: false, space: 0.0 }));
            let _ = font;
            if w <= 0.0 {
                // Larga quanto il testo (fo:min-width): una cornice che si misura da sé.
                MobileContenuto::Cornice(blocks)
            } else {
                MobileContenuto::Testo(blocks, fill, line)
            }
        } else {
            return None;
        };
        let anchor = attr(f, "anchor-type").unwrap_or("paragraph");
        let base_of = |rel: Option<&str>, vertical: bool| match rel {
            Some("page" | "page-content" | "page-start-margin" | "page-end-margin") if !vertical || anchor == "page" => {
                if rel == Some("page-content") {
                    Base::Margine
                } else {
                    Base::Pagina
                }
            }
            Some("page" | "page-content") => Base::Pagina,
            _ if anchor == "page" => Base::Pagina,
            _ => Base::Paragrafo,
        };
        let hb = base_of(g.get("g:horizontal-rel"), false);
        let vb = base_of(g.get("g:vertical-rel"), true);
        let x = match g.get("g:horizontal-pos") {
            Some("center") => Riferimento::Allineato(hb, 1),
            Some("right" | "outside") => Riferimento::Allineato(hb, 2),
            Some("left" | "inside") => Riferimento::Allineato(hb, 0),
            _ => Riferimento::Spostato(hb, attr(f, "x").and_then(length).unwrap_or(0.0)),
        };
        let y = match g.get("g:vertical-pos") {
            Some("middle" | "center") => Riferimento::Allineato(vb, 1),
            Some("bottom") => Riferimento::Allineato(vb, 2),
            Some("top") if vb != Base::Paragrafo => Riferimento::Allineato(vb, 0),
            _ => Riferimento::Spostato(vb, attr(f, "y").and_then(length).unwrap_or(0.0)),
        };
        let avvolge = match g.get("g:wrap") {
            Some("none") => Avvolge::SopraSotto,
            Some("run-through") | None => {
                if anchor == "as-char" {
                    Avvolge::Nessuno
                } else if g.get("g:wrap").is_none() {
                    Avvolge::SopraSotto
                } else {
                    Avvolge::Nessuno
                }
            }
            _ => Avvolge::Lati,
        };
        let dietro = g.get("g:run-through") == Some("background");
        Some(Mobile { contenuto: content, w, h, x, y, avvolge, dietro })
    }

    fn table(&mut self, t: roxmltree::Node) -> Option<Tabella> {
        let tp = attr(t, "style-name").map(|s| self.st.resolve("table", s)).unwrap_or_default();
        let mut griglia = Vec::new();
        let mut rows: Vec<(roxmltree::Node, bool)> = Vec::new();
        fn collect<'a>(n: roxmltree::Node<'a, 'a>, header: bool, cols: &mut Vec<roxmltree::Node<'a, 'a>>, rows: &mut Vec<(roxmltree::Node<'a, 'a>, bool)>) {
            for c in n.children().filter(|c| c.is_element()) {
                match c.tag_name().name() {
                    "table-column" => cols.push(c),
                    "table-columns" | "table-column-group" | "table-header-columns" => collect(c, header, cols, rows),
                    "table-header-rows" => collect(c, true, cols, rows),
                    "table-rows" | "table-row-group" => collect(c, header, cols, rows),
                    "table-row" => rows.push((c, header)),
                    _ => {}
                }
            }
        }
        let mut cols = Vec::new();
        collect(t, false, &mut cols, &mut rows);
        // Larghezze delle colonne: assolute, oppure relative («1234*») da riportare alla tabella.
        let mut rel = Vec::new();
        for c in &cols {
            let rep = attr(*c, "number-columns-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(1024);
            let cp = attr(*c, "style-name").map(|s| self.st.resolve("table-column", s)).unwrap_or_default();
            let w = cp.get("col:column-width").and_then(length).unwrap_or(0.0);
            let r = cp.get("col:rel-column-width").and_then(|v| v.trim_end_matches('*').trim().parse::<f32>().ok()).filter(|r| *r > 0.0);
            for _ in 0..rep {
                griglia.push(w);
                rel.push(r);
            }
        }
        let mut relativa = None;
        if !rel.is_empty() && rel.iter().all(|r| r.is_some()) && (griglia.iter().any(|w| *w <= 0.0) || tp.get("tb:rel-width").is_some()) {
            let sum: f32 = rel.iter().map(|r| r.unwrap()).sum();
            if let Some(tw) = tp.get("tb:width").and_then(length).filter(|_| tp.get("tb:rel-width").is_none()) {
                griglia = rel.iter().map(|r| r.unwrap() / sum * tw).collect();
            } else {
                // In parte della larghezza disponibile: la scala la sa solo l'impaginazione.
                let pct = tp.get("tb:rel-width").and_then(|v| v.trim_end_matches('%').trim().parse::<f32>().ok()).map_or(1.0, |v| v / 100.0);
                relativa = Some(pct.clamp(0.01, 1.0));
                griglia = rel.iter().map(|r| r.unwrap() / sum * 100.0).collect();
            }
        }
        // Celle unite in verticale: quante righe restano ancora per colonna.
        // colonna d'inizio → (righe che restano, colonne occupate)
        let mut pending: HashMap<usize, (usize, usize)> = HashMap::new();
        let mut righe = Vec::new();
        for (r, header) in &rows {
            let rp = attr(*r, "style-name").map(|s| self.st.resolve("table-row", s)).unwrap_or_default();
            let altezza = rp.get("row:row-height").and_then(length).map(|v| -v).or_else(|| rp.get("row:min-row-height").and_then(length)).unwrap_or(0.0);
            let rep = attr(*r, "number-rows-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(512);
            for _ in 0..rep {
                let mut celle = Vec::new();
                let mut col = 0;
                for c in r.children().filter(|c| c.is_element()) {
                    let name = c.tag_name().name();
                    if name != "table-cell" && name != "covered-table-cell" {
                        continue;
                    }
                    let crep = attr(c, "number-columns-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(1024);
                    for _ in 0..crep {
                        if name == "covered-table-cell" {
                            // Coperta da un'unione: in verticale continua quella di sopra, larga come lei.
                            if let Some((left, span)) = pending.get_mut(&col) {
                                if *left > 0 {
                                    *left -= 1;
                                    let mut cont = self.cell(c, col);
                                    cont.vmerge = Some(false);
                                    cont.span = *span;
                                    cont.blocchi.clear();
                                    celle.push(cont);
                                }
                            }
                            col += 1;
                            continue;
                        }
                        let mut cell = self.cell(c, col);
                        let rows_span = attr(c, "number-rows-spanned").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
                        if rows_span > 1 {
                            cell.vmerge = Some(true);
                            pending.insert(col, (rows_span - 1, cell.span));
                        }
                        col += cell.span;
                        celle.push(cell);
                    }
                }
                righe.push(Riga { celle, altezza, intestazione: *header, non_dividere: rp.get("row:keep-together") == Some("always") });
            }
        }
        if righe.is_empty() {
            return None;
        }
        let ncols = righe.iter().map(|r| r.celle.iter().map(|c| c.span).sum::<usize>()).max().unwrap_or(1);
        // Le colonne senza misura: in parti uguali della larghezza della tabella.
        let width = tp.get("tb:width").and_then(length);
        if griglia.len() < ncols {
            griglia.resize(ncols, 0.0);
        }
        let known: f32 = griglia.iter().sum();
        let missing = griglia.iter().filter(|w| **w <= 0.0).count();
        if missing > 0 {
            let each = ((width.unwrap_or(450.0) - known) / missing as f32).max(20.0);
            for g in griglia.iter_mut().filter(|w| **w <= 0.0) {
                *g = each;
            }
        }
        let allinea = match tp.get("tb:align") {
            Some("center") => 1,
            Some("right") => 2,
            _ => 0,
        };
        // In Writer il bordo della tabella sta sul margine: il testo della cella è rientrato.
        let rientro = tp.get("tb:margin-left").and_then(length).unwrap_or(0.0) + righe.first().and_then(|r| r.celle.first()).map_or(0.0, |c| c.margini[0]);
        Some(Tabella { griglia, righe, allinea, rientro, bordo_h: None, bordo_v: None, relativa })
    }

    fn cell(&mut self, c: roxmltree::Node, _col: usize) -> Cella {
        let cp = attr(c, "style-name").map(|s| self.st.resolve("table-cell", s)).unwrap_or_default();
        let g = |k: &str| cp.get(k);
        let pad = |side: &str| g(&format!("c:padding-{side}")).or(g("c:padding")).and_then(length).unwrap_or(1.4);
        let bd = |side: &str| linea(g(&format!("c:border-{side}")).or(g("c:border")), 0.0);
        let mut blocchi = Vec::new();
        self.blocks(c, None, &mut blocchi, false);
        Cella {
            blocchi,
            span: attr(c, "number-columns-spanned").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1),
            vmerge: None,
            sfondo: g("c:background-color").and_then(color),
            bordi: [bd("top"), bd("bottom"), bd("left"), bd("right")],
            valign: match g("c:vertical-align") {
                Some("middle") => 1,
                Some("bottom") => 2,
                _ => 0,
            },
            margini: [pad("left"), pad("right"), pad("top"), pad("bottom")],
            larghezza: None,
        }
    }

    /// La pagina di uno stile di pagina: carta, margini, colonne, intestazioni e piè.
    fn page(&mut self, master: &str, styles_doc: Option<&roxmltree::Document>) -> Sezione {
        let mut s = Sezione::default();
        let layout = self.st.masters.get(master).or_else(|| self.st.masters.get("Standard")).cloned().unwrap_or_default();
        let (pl, hs, fs) = self.st.layouts.get(&layout).cloned().unwrap_or_default();
        let g = |k: &str| pl.get(k).and_then(length);
        s.w = g("page-width").unwrap_or(595.3);
        s.h = g("page-height").unwrap_or(841.9);
        let mt = g("margin-top").unwrap_or(56.7);
        let mb = g("margin-bottom").unwrap_or(56.7);
        s.margini = [g("margin-left").unwrap_or(56.7), g("margin-right").unwrap_or(56.7), mt, mb];
        if let Some((n, gap)) = pl.columns {
            s.colonne = n;
            s.spazio_colonne = gap;
        }
        // Le intestazioni stanno dentro il margine di Writer: il testo comincia sotto di loro.
        let hf_room = |p: &Props| -> f32 {
            let h = p.get("min-height").or(p.get("height")).and_then(length).unwrap_or(0.0);
            h.max(p.get("margin-bottom").or(p.get("margin-top")).and_then(length).unwrap_or(0.0))
        };
        s.dist_testa = mt;
        s.dist_piede = mb;
        let Some(sd) = styles_doc else { return s };
        let Some(mp) = sd.descendants().find(|n| n.tag_name().name() == "master-page" && attr(*n, "name") == Some(master)).or_else(|| sd.descendants().find(|n| n.tag_name().name() == "master-page")) else { return s };
        let part = |me: &mut Self, tag: &str| -> Option<Vec<Blocco>> {
            let n = mp.children().find(|c| c.tag_name().name() == tag)?;
            if attr(n, "display") == Some("false") {
                return None;
            }
            let mut b = Vec::new();
            me.blocks(n, None, &mut b, false);
            Some(b)
        };
        s.testa.normale = part(self, "header");
        s.testa.pari = part(self, "header-left");
        s.testa.prima = part(self, "header-first");
        s.piede.normale = part(self, "footer");
        s.piede.pari = part(self, "footer-left");
        s.piede.prima = part(self, "footer-first");
        if s.testa.pari.is_some() || s.piede.pari.is_some() {
            self.doc.pari_dispari = true;
            s.testa.pari = s.testa.pari.take().or(s.testa.normale.clone());
            s.piede.pari = s.piede.pari.take().or(s.piede.normale.clone());
        }
        if s.testa.prima.is_some() || s.piede.prima.is_some() {
            s.prima_diversa = true;
            s.testa.prima = s.testa.prima.take().or(s.testa.normale.clone());
            s.piede.prima = s.piede.prima.take().or(s.piede.normale.clone());
        }
        // In Writer l'altezza dell'intestazione comprende già la distanza dal testo; con la distanza
        // dinamica la distanza si consuma quando il contenuto cresce, senza si aggiunge al contenuto.
        let spacing = |p: &Props, k: &str| p.get(k).and_then(length).unwrap_or(0.0);
        let dynamic = |p: &Props| p.get("dynamic-spacing") == Some("true");
        if s.testa.normale.is_some() {
            s.margini[2] = mt + hf_room(&hs);
            let gap = spacing(&hs, "margin-bottom");
            if !dynamic(&hs) && gap > 0.0 {
                for v in [&mut s.testa.normale, &mut s.testa.prima, &mut s.testa.pari] {
                    if let Some(Blocco::Par(p)) = v.as_mut().and_then(|b| b.last_mut()) {
                        p.dopo += gap;
                    }
                }
            }
        }
        if s.piede.normale.is_some() {
            s.margini[3] = mb + hf_room(&fs);
            let gap = spacing(&fs, "margin-top");
            if !dynamic(&fs) && gap > 0.0 {
                for v in [&mut s.piede.normale, &mut s.piede.prima, &mut s.piede.pari] {
                    if let Some(Blocco::Par(p)) = v.as_mut().and_then(|b| b.first_mut()) {
                        p.prima += gap;
                    }
                }
            }
        }
        s
    }

    fn close_section(&mut self, new_page: bool) {
        if self.current.is_empty() && !new_page {
            return;
        }
        let mut s = self.page_cache(&self.master.clone());
        s.blocchi = std::mem::take(&mut self.current);
        s.pagina_nuova = self.sections.is_empty() || self.next_new_page;
        self.next_new_page = new_page;
        self.sections.push(s);
    }

    fn close_section_cols(&mut self, n: u8, gap: f32) {
        let mut s = self.page_cache(&self.master.clone());
        s.blocchi = std::mem::take(&mut self.current);
        s.colonne = n;
        s.spazio_colonne = gap;
        s.pagina_nuova = self.sections.is_empty() || self.next_new_page;
        self.next_new_page = false;
        self.sections.push(s);
    }
}

fn number_text(v: u32, fmt: &str) -> String {
    match fmt {
        "a" => super::docx::number_text(v, "lowerLetter"),
        "A" => super::docx::number_text(v, "upperLetter"),
        "i" => super::docx::number_text(v, "lowerRoman"),
        "I" => super::docx::number_text(v, "upperRoman"),
        "" => String::new(),
        _ => v.to_string(),
    }
}

fn push_text(par: &mut Paragrafo, s: String, font: &Font, link: Option<&Target>) {
    if let Some(last) = par.runs.last_mut() {
        if last.font == *font && link.is_none() && last.link.is_none() {
            if let Pezzo::Testo(t) = &mut last.pezzo {
                t.push_str(&s);
                return;
            }
        }
    }
    par.runs.push(Run { pezzo: Pezzo::Testo(s), font: font.clone(), link: link.cloned() });
}

// Il contesto ha bisogno di due campi in più per le sezioni: messi qui per leggibilità.
impl<'z> Ctx<'z> {
    fn page_cache(&mut self, master: &str) -> Sezione {
        let styles = self.styles_xml.clone();
        let sd = styles.as_deref().and_then(|x| roxmltree::Document::parse(x).ok());
        self.page(master, sd.as_ref())
    }
}

/// Legge un .odt.
pub fn leggi_odt(bytes: &[u8]) -> Result<Documento, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "il file non è un documento di Writer leggibile".to_string())?;
    let content = voce(&mut zip, "content.xml").ok_or("nel file non c'è il testo del documento")?;
    let styles_xml = voce(&mut zip, "styles.xml");
    let mut st = Styles { tab_default: 35.4, ..Default::default() };
    let sdoc = styles_xml.as_deref().and_then(|x| roxmltree::Document::parse(x).ok());
    if let Some(d) = &sdoc {
        st.add(d);
    }
    let cdoc = parse(&content)?;
    st.add(&cdoc);
    let title = voce(&mut zip, "meta.xml").and_then(|m| {
        let d = roxmltree::Document::parse(&m).ok()?;
        d.descendants().find(|n| n.tag_name().name() == "title").and_then(|n| n.text()).map(str::to_string)
    });
    let tab = st.tab_default;
    let mut ctx = Ctx { zip, st, doc: Documento { tab_predefinita: tab, titolo: title.unwrap_or_default(), ..Default::default() }, counters: HashMap::new(), images: HashMap::new(), sections: Vec::new(), current: Vec::new(), master: "Standard".into(), styles_xml: styles_xml.map(std::rc::Rc::new), next_new_page: true };
    // Il primo stile di pagina: quello del primo paragrafo, se ne dice uno.
    let text = cdoc.descendants().find(|n| n.tag_name().name() == "text" && n.parent().is_some_and(|p| p.tag_name().name() == "body")).ok_or("documento senza testo")?;
    let mut out = Vec::new();
    ctx.blocks(text, None, &mut out, true);
    ctx.current.append(&mut out);
    let last_master = ctx.master.clone();
    let mut s = ctx.page_cache(&last_master);
    s.blocchi = std::mem::take(&mut ctx.current);
    s.pagina_nuova = ctx.sections.is_empty() || ctx.next_new_page;
    ctx.sections.push(s);
    ctx.sections.retain(|s| !s.blocchi.is_empty() || s.pagina_nuova);
    ctx.doc.sezioni = std::mem::take(&mut ctx.sections);
    Ok(ctx.doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_and_numbers() {
        assert_eq!(number_text(3, "a"), "c");
        assert_eq!(number_text(4, "I"), "IV");
        let f = font_in(&Props { map: [("t:font-size".to_string(), "150%".to_string())].into_iter().collect(), ..Default::default() }, &Font { size: 10.0, ..Default::default() }, &HashMap::new());
        assert_eq!(f.size, 15.0);
        assert!(linea(Some("0.5pt solid #ff0000"), 2.0).is_some_and(|l| l.color == (255, 0, 0) && l.space == 2.0));
    }
}
