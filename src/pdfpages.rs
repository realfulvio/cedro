//! Unire e dividere i PDF. Le pagine scelte di uno o più PDF diventano un PDF nuovo: si copia solo
//! quello che le pagine usano (caratteri, immagini, moduli), con i riferimenti rinumerati, e i pezzi
//! uguali arrivati da file diversi (lo stesso carattere incorporato due volte) restano una volta sola.
//! Segnalibri e link verso pagine che non ci sono più si tolgono; quelli per nome diventano espliciti.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use sha2::{Digest, Sha256};

/// Come si costruisce il PDF nuovo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Settings {
    /// Un segnalibro per ogni file di partenza, con il suo nome.
    pub file_bookmarks: bool,
    /// I segnalibri dei file di partenza (sotto quello del loro file).
    pub keep_bookmarks: bool,
    /// Campi compilabili e link interni.
    pub keep_forms: bool,
    /// Una pagina bianca dopo i file con le pagine dispari, per stampare fronte-retro.
    pub duplex_blank: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { file_bookmarks: true, keep_bookmarks: true, keep_forms: true, duplex_blank: false }
    }
}

/// Un PDF letto, con la password se serve.
pub fn open(bytes: &[u8], password: Option<&str>) -> Result<Document, String> {
    let mut doc = Document::load_mem(bytes).map_err(|e| format!("PDF non letto: {e}"))?;
    if doc.is_encrypted() {
        // Senza password lopdf ci ha già provato: resta cifrato solo se ne serve una. Da lopdf 0.46 un PDF
        // cifrato aperto senza la sua password resta vuoto: la password si dà all'apertura.
        let Some(pw) = password.filter(|p| !p.is_empty()) else {
            return Err("il PDF è protetto: scrivi la password in «Password per aprire»".into());
        };
        doc = Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(pw))
            .map_err(|_| "password sbagliata per questo PDF".to_string())?;
    }
    if doc.get_pages().is_empty() {
        return Err("il PDF non ha pagine".into());
    }
    Ok(doc)
}

pub fn page_count(doc: &Document) -> usize {
    doc.get_pages().len()
}

/// Un pezzo del PDF nuovo: alcune pagine (da 1) di un documento, con il titolo del suo segnalibro.
pub struct Part<'a> {
    pub doc: &'a Document,
    pub pages: Vec<u32>,
    pub title: String,
}

/// Il PDF nuovo con le pagine dei pezzi, in ordine.
pub fn build(parts: &[Part], s: &Settings) -> Result<Vec<u8>, String> {
    if parts.iter().all(|p| p.pages.is_empty()) {
        return Err("nessuna pagina scelta".into());
    }
    let mut b = Builder { objects: Default::default(), next: 1 };
    let pages_id = b.reserve();
    let mut kids: Vec<Object> = Vec::new();
    let mut outline: Vec<Node> = Vec::new();
    let mut fields: Vec<Object> = Vec::new();
    let mut field_names: HashSet<Vec<u8>> = HashSet::new();
    let mut acro: Option<Dictionary> = None;
    let mut version = "1.4".to_string();
    for (pi, part) in parts.iter().enumerate() {
        if part.pages.is_empty() {
            continue;
        }
        let doc = part.doc;
        if doc.version > version {
            version = doc.version.clone();
        }
        let all = doc.get_pages();
        let mut copier = Copier::new(doc);
        // Le pagine scelte prendono subito il loro numero nuovo: chi le cita (link, campi) le ritrova.
        let mut chosen: Vec<(ObjectId, ObjectId)> = Vec::new();
        for n in &part.pages {
            let old = *all.get(n).ok_or_else(|| format!("la pagina {n} non c'è: il PDF ne ha {}", all.len()))?;
            let new = b.reserve();
            copier.map.entry(old).or_insert(new);
            chosen.push((old, new));
        }
        let kept_annots: HashSet<ObjectId> = chosen.iter().flat_map(|(old, _)| annot_ids(doc, *old)).collect();
        let first_new = chosen[0].1;
        let mut last_box = None;
        for (old, new) in &chosen {
            let dict = page_dict(doc, *old, s.keep_forms, &mut copier, &mut b, pages_id)?;
            last_box = dict.get(b"MediaBox").ok().cloned();
            b.objects.insert(*new, Object::Dictionary(dict));
            kids.push(Object::Reference(*new));
        }
        if s.duplex_blank && pi + 1 < parts.len() && chosen.len() % 2 == 1 {
            let mut blank = Dictionary::new();
            blank.set("Type", Object::Name(b"Page".to_vec()));
            blank.set("Parent", Object::Reference(pages_id));
            blank.set("MediaBox", last_box.unwrap_or_else(|| a4_box()));
            blank.set("Resources", Object::Dictionary(Dictionary::new()));
            let id = b.add(Object::Dictionary(blank));
            kids.push(Object::Reference(id));
        }
        // Segnalibri: quello del file e, sotto, quelli che il file aveva.
        let children = if s.keep_bookmarks { read_outline(doc, &copier.map) } else { Vec::new() };
        if s.file_bookmarks {
            outline.push(Node { title: pdf_text(&part.title), dest: Some(fit_dest(first_new)), children });
        } else {
            outline.extend(children);
        }
        if s.keep_forms {
            if let Ok(form) = catalog_dict(doc, b"AcroForm") {
                let list = form.get(b"Fields").and_then(|f| deref(doc, f).as_array().map(|a| a.clone())).unwrap_or_default();
                for f in list {
                    let Ok(fid) = f.as_reference() else { continue };
                    if !field_on_pages(doc, fid, &kept_annots, 0) {
                        continue;
                    }
                    let new = copier.copy_ref(fid, &mut b);
                    if let Some(Object::Dictionary(d)) = b.objects.get_mut(&new) {
                        // Due file con un campo dallo stesso nome: il secondo prende «_2», altrimenti i valori si mescolano.
                        if let Ok(Object::String(t, _)) = d.get(b"T") {
                            let mut name = t.clone();
                            let mut i = 2;
                            while field_names.contains(&name) {
                                name = [t.as_slice(), format!("_{i}").as_bytes()].concat();
                                i += 1;
                            }
                            field_names.insert(name.clone());
                            d.set("T", Object::String(name, StringFormat::Literal));
                        }
                    }
                    fields.push(Object::Reference(new));
                }
                if acro.is_none() {
                    let mut a = Dictionary::new();
                    for key in [b"DA".as_slice(), b"Q", b"NeedAppearances"] {
                        if let Ok(v) = form.get(key) {
                            a.set(key.to_vec(), copier.copy_obj(v, &mut b));
                        }
                    }
                    if let Ok(dr) = form.get(b"DR") {
                        a.set("DR", copier.copy_obj(dr, &mut b));
                    }
                    acro = Some(a);
                } else if let (Some(a), Ok(dr)) = (acro.as_mut(), form.get(b"DR")) {
                    // Le risorse dei campi degli altri file si aggiungono a quelle del primo.
                    let dr = copier.copy_obj(dr, &mut b);
                    merge_dr(a, dr, &b);
                }
            }
        }
    }
    let count = kids.len() as i64;
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", Object::Array(kids));
    pages.set("Count", Object::Integer(count));
    b.objects.insert(pages_id, Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    if !outline.is_empty() {
        let id = write_outline(&outline, &mut b);
        catalog.set("Outlines", Object::Reference(id));
        catalog.set("PageMode", Object::Name(b"UseOutlines".to_vec()));
    }
    if let Some(mut a) = acro.filter(|_| !fields.is_empty()) {
        a.set("Fields", Object::Array(fields));
        let id = b.add(Object::Dictionary(a));
        catalog.set("AcroForm", Object::Reference(id));
    }
    let catalog_id = b.add(Object::Dictionary(catalog));
    let mut info = Dictionary::new();
    info.set("Producer", Object::String(b"Cedro".to_vec(), StringFormat::Literal));
    let info_id = b.add(Object::Dictionary(info));

    dedupe(&mut b.objects);
    let mut doc = Document::with_version(version);
    doc.objects = b.objects;
    doc.max_id = b.next - 1;
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc.trailer.set("Info", Object::Reference(info_id));
    doc.prune_objects();
    doc.compress();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| format!("PDF non scritto: {e}"))?;
    Ok(out)
}

fn a4_box() -> Object {
    Object::Array(vec![0.into(), 0.into(), Object::Real(595.28), Object::Real(841.89)])
}

struct Builder {
    objects: std::collections::BTreeMap<ObjectId, Object>,
    next: u32,
}

impl Builder {
    fn reserve(&mut self) -> ObjectId {
        let id = (self.next, 0);
        self.next += 1;
        id
    }

    fn add(&mut self, o: Object) -> ObjectId {
        let id = self.reserve();
        self.objects.insert(id, o);
        id
    }
}

/// Copia gli oggetti di un documento nel PDF nuovo, uno per numero vecchio.
struct Copier<'a> {
    doc: &'a Document,
    map: HashMap<ObjectId, ObjectId>,
}

impl<'a> Copier<'a> {
    fn new(doc: &'a Document) -> Self {
        Copier { doc, map: HashMap::new() }
    }

    /// Il numero nuovo di un oggetto (copiato la prima volta). Pagine non scelte, l'albero delle pagine e
    /// il catalogo non si copiano: il riferimento diventa `null`, altrimenti arriverebbe tutto il file.
    fn copy_ref(&mut self, id: ObjectId, b: &mut Builder) -> ObjectId {
        if let Some(n) = self.map.get(&id) {
            return *n;
        }
        let new = b.reserve();
        self.map.insert(id, new);
        let copied = match self.doc.objects.get(&id) {
            Some(o) => self.copy_obj(o, b),
            None => Object::Null,
        };
        b.objects.insert(new, copied);
        new
    }

    fn skipped(&self, id: ObjectId) -> bool {
        if self.map.contains_key(&id) {
            return false;
        }
        let t = match self.doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => d.get(b"Type").ok().and_then(|t| t.as_name().ok()).map(|n| n.to_vec()),
            _ => None,
        };
        matches!(t.as_deref(), Some(b"Page" | b"Pages" | b"Catalog" | b"Outlines"))
    }

    fn copy_obj(&mut self, o: &Object, b: &mut Builder) -> Object {
        match o {
            Object::Reference(id) => {
                if self.skipped(*id) {
                    Object::Null
                } else {
                    Object::Reference(self.copy_ref(*id, b))
                }
            }
            Object::Array(a) => Object::Array(a.iter().map(|x| self.copy_obj(x, b)).collect()),
            Object::Dictionary(d) => Object::Dictionary(self.copy_dict(d, b)),
            Object::Stream(s) => {
                let mut st = Stream::new(self.copy_dict(&s.dict, b), s.content.clone());
                st.allows_compression = s.allows_compression;
                Object::Stream(st)
            }
            other => other.clone(),
        }
    }

    fn copy_dict(&mut self, d: &Dictionary, b: &mut Builder) -> Dictionary {
        let mut out = Dictionary::new();
        for (k, v) in d.iter() {
            // Il genitore di un campo o di un segnalibro si ricollega da sé; quello delle pagine lo mette `page_dict`.
            if k == b"Parent" {
                if let Ok(id) = v.as_reference() {
                    if self.skipped(id) {
                        continue;
                    }
                }
            }
            out.set(k.clone(), self.copy_obj(v, b));
        }
        out
    }
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map(|(_, o)| o).unwrap_or(o)
}

fn catalog_dict<'a>(doc: &'a Document, key: &[u8]) -> Result<&'a Dictionary, ()> {
    let cat = doc.catalog().map_err(|_| ())?;
    deref(doc, cat.get(key).map_err(|_| ())?).as_dict().map_err(|_| ())
}

fn annot_ids(doc: &Document, page: ObjectId) -> Vec<ObjectId> {
    let Ok(d) = doc.get_dictionary(page) else { return Vec::new() };
    let Ok(a) = d.get(b"Annots") else { return Vec::new() };
    deref(doc, a).as_array().map(|a| a.iter().filter_map(|x| x.as_reference().ok()).collect()).unwrap_or_default()
}

/// Vero se il campo (o uno dei suoi figli) ha un widget su una delle pagine tenute.
fn field_on_pages(doc: &Document, id: ObjectId, kept: &HashSet<ObjectId>, depth: u8) -> bool {
    if kept.contains(&id) {
        return true;
    }
    if depth > 16 {
        return false;
    }
    let Ok(d) = doc.get_dictionary(id) else { return false };
    let kids = d.get(b"Kids").map(|k| deref(doc, k)).and_then(|k| k.as_array()).cloned().unwrap_or_default();
    kids.iter().filter_map(|k| k.as_reference().ok()).any(|k| field_on_pages(doc, k, kept, depth + 1))
}

/// Il dizionario della pagina pronto per il PDF nuovo: con gli attributi ereditati dai nodi sopra
/// (risorse, formato, rotazione), le annotazioni sistemate e il nuovo genitore.
fn page_dict(doc: &Document, old: ObjectId, keep_forms: bool, c: &mut Copier, b: &mut Builder, parent: ObjectId) -> Result<Dictionary, String> {
    let src = doc.get_dictionary(old).map_err(|e| format!("pagina non letta: {e}"))?;
    let mut d = Dictionary::new();
    for (k, v) in src.iter() {
        if matches!(k.as_slice(), b"Parent" | b"Annots" | b"B" | b"StructParents" | b"Thumb") {
            continue;
        }
        d.set(k.clone(), c.copy_obj(v, b));
    }
    for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
        if d.get(key).is_ok() {
            continue;
        }
        if let Some(v) = inherited(doc, src, key) {
            d.set(key.to_vec(), c.copy_obj(&v, b));
        }
    }
    if d.get(b"MediaBox").is_err() {
        d.set("MediaBox", a4_box());
    }
    d.set("Parent", Object::Reference(parent));
    let mut annots = Vec::new();
    for aid in annot_ids(doc, old) {
        let Ok(a) = doc.get_dictionary(aid) else { continue };
        let sub = a.get(b"Subtype").ok().and_then(|s| s.as_name().ok()).unwrap_or(b"");
        if matches!(sub, b"Link" | b"Widget") && !keep_forms {
            continue;
        }
        if sub == b"Popup" {
            continue;
        }
        if sub == b"Link" {
            match link_target(doc, a) {
                // Un link verso una pagina che non c'è più si toglie.
                LinkTarget::Page(p) if !c.map.contains_key(&p) => continue,
                LinkTarget::Page(p) => {
                    let mut a = c.copy_dict(a, b);
                    a.remove(b"A");
                    a.set("Dest", fit_dest(c.map[&p]));
                    a.remove(b"P");
                    annots.push(Object::Reference(b.add(Object::Dictionary(a))));
                    continue;
                }
                LinkTarget::Broken => continue,
                LinkTarget::Other => {}
            }
        }
        annots.push(Object::Reference(c.copy_ref(aid, b)));
    }
    if !annots.is_empty() {
        d.set("Annots", Object::Array(annots));
    }
    Ok(d)
}

fn inherited(doc: &Document, page: &Dictionary, key: &[u8]) -> Option<Object> {
    let mut node = page.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    for _ in 0..32 {
        let d = doc.get_dictionary(node?).ok()?;
        if let Ok(v) = d.get(key) {
            return Some(v.clone());
        }
        node = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    None
}

enum LinkTarget {
    Page(ObjectId),
    /// Un link interno che non porta da nessuna parte.
    Broken,
    /// Un indirizzo web, un altro file: resta com'è.
    Other,
}

fn link_target(doc: &Document, a: &Dictionary) -> LinkTarget {
    let dest = match a.get(b"Dest") {
        Ok(d) => Some(d.clone()),
        Err(_) => match a.get(b"A").map(|x| deref(doc, x)).and_then(|x| x.as_dict()) {
            Ok(act) if act.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"GoTo") => act.get(b"D").ok().cloned(),
            _ => return LinkTarget::Other,
        },
    };
    match dest.and_then(|d| dest_page(doc, &d)) {
        Some(p) => LinkTarget::Page(p),
        None => LinkTarget::Broken,
    }
}

/// La pagina a cui porta una destinazione: esplicita (`[pagina /XYZ …]`) o per nome.
fn dest_page(doc: &Document, d: &Object) -> Option<ObjectId> {
    match deref(doc, d) {
        Object::Array(a) => a.first().and_then(|p| p.as_reference().ok()),
        Object::Dictionary(dd) => dd.get(b"D").ok().and_then(|x| dest_page(doc, x)),
        Object::Name(n) => named_dest(doc, n).and_then(|x| dest_page(doc, &x)),
        Object::String(s, _) => named_dest(doc, s).and_then(|x| dest_page(doc, &x)),
        _ => None,
    }
}

fn named_dest(doc: &Document, name: &[u8]) -> Option<Object> {
    if let Ok(dests) = catalog_dict(doc, b"Dests") {
        if let Ok(v) = dests.get(name) {
            return Some(v.clone());
        }
    }
    let names = catalog_dict(doc, b"Names").ok()?;
    let tree = deref(doc, names.get(b"Dests").ok()?).as_dict().ok()?;
    name_tree_find(doc, tree, name, 0)
}

fn name_tree_find(doc: &Document, node: &Dictionary, name: &[u8], depth: u8) -> Option<Object> {
    if depth > 32 {
        return None;
    }
    if let Ok(list) = node.get(b"Names").map(|x| deref(doc, x)).and_then(|x| x.as_array()) {
        for pair in list.chunks(2) {
            if let [Object::String(k, _), v] = pair {
                if k.as_slice() == name {
                    return Some(v.clone());
                }
            }
        }
    }
    let kids = node.get(b"Kids").map(|x| deref(doc, x)).and_then(|x| x.as_array()).ok()?;
    kids.iter().find_map(|k| deref(doc, k).as_dict().ok().and_then(|kd| name_tree_find(doc, kd, name, depth + 1)))
}

fn fit_dest(page: ObjectId) -> Object {
    Object::Array(vec![Object::Reference(page), Object::Name(b"Fit".to_vec())])
}

/// Un segnalibro da scrivere: titolo (già codificato), pagina, figli.
struct Node {
    title: Vec<u8>,
    dest: Option<Object>,
    children: Vec<Node>,
}

/// I segnalibri di un documento, con le pagine già nel PDF nuovo. Quelli che portano a pagine non
/// tenute si tolgono (i loro figli salgono di un livello).
fn read_outline(doc: &Document, map: &HashMap<ObjectId, ObjectId>) -> Vec<Node> {
    let Ok(root) = catalog_dict(doc, b"Outlines") else { return Vec::new() };
    let first = root.get(b"First").ok().and_then(|f| f.as_reference().ok());
    let mut seen = HashSet::new();
    outline_chain(doc, first, map, &mut seen, 0)
}

fn outline_chain(doc: &Document, mut item: Option<ObjectId>, map: &HashMap<ObjectId, ObjectId>, seen: &mut HashSet<ObjectId>, depth: u8) -> Vec<Node> {
    let mut out = Vec::new();
    while let Some(id) = item {
        if !seen.insert(id) || depth > 32 {
            break;
        }
        let Ok(d) = doc.get_dictionary(id) else { break };
        let title = match d.get(b"Title").map(|t| deref(doc, t)) {
            Ok(Object::String(s, _)) => s.clone(),
            _ => Vec::new(),
        };
        let dest = d.get(b"Dest").ok().cloned().or_else(|| {
            let act = deref(doc, d.get(b"A").ok()?).as_dict().ok()?;
            (act.get(b"S").ok()?.as_name().ok()? == b"GoTo").then(|| act.get(b"D").ok().cloned()).flatten()
        });
        let page = dest.as_ref().and_then(|x| dest_page(doc, x)).and_then(|p| map.get(&p).copied());
        let first = d.get(b"First").ok().and_then(|f| f.as_reference().ok());
        let children = outline_chain(doc, first, map, seen, depth + 1);
        match page {
            Some(p) => out.push(Node { title, dest: Some(fit_dest(p)), children }),
            None if dest.is_none() && !children.is_empty() => out.push(Node { title, dest: None, children }),
            None => out.extend(children),
        }
        item = d.get(b"Next").ok().and_then(|f| f.as_reference().ok());
    }
    out
}

fn write_outline(nodes: &[Node], b: &mut Builder) -> ObjectId {
    let root = b.reserve();
    let (first, last, count) = write_level(nodes, root, b);
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Outlines".to_vec()));
    d.set("First", Object::Reference(first));
    d.set("Last", Object::Reference(last));
    d.set("Count", Object::Integer(count));
    b.objects.insert(root, Object::Dictionary(d));
    root
}

fn write_level(nodes: &[Node], parent: ObjectId, b: &mut Builder) -> (ObjectId, ObjectId, i64) {
    let ids: Vec<ObjectId> = nodes.iter().map(|_| b.reserve()).collect();
    let mut total = 0;
    for (i, n) in nodes.iter().enumerate() {
        let mut d = Dictionary::new();
        d.set("Title", Object::String(n.title.clone(), StringFormat::Literal));
        d.set("Parent", Object::Reference(parent));
        if let Some(dest) = &n.dest {
            d.set("Dest", dest.clone());
        }
        if i > 0 {
            d.set("Prev", Object::Reference(ids[i - 1]));
        }
        if i + 1 < ids.len() {
            d.set("Next", Object::Reference(ids[i + 1]));
        }
        if !n.children.is_empty() {
            let (f, l, c) = write_level(&n.children, ids[i], b);
            d.set("First", Object::Reference(f));
            d.set("Last", Object::Reference(l));
            // Negativo = chiuso: i segnalibri dei file restano raccolti sotto il nome del file.
            d.set("Count", Object::Integer(-c));
        }
        total += 1;
        b.objects.insert(ids[i], Object::Dictionary(d));
    }
    (ids[0], ids[ids.len() - 1], total)
}

/// Aggiunge al dizionario delle risorse dei campi (`DR`) le voci che mancano.
fn merge_dr(acro: &mut Dictionary, extra: Object, b: &Builder) {
    let resolve = |o: &Object| match o {
        Object::Reference(id) => b.objects.get(id).and_then(|x| x.as_dict().ok()).cloned(),
        Object::Dictionary(d) => Some(d.clone()),
        _ => None,
    };
    let Some(extra) = resolve(&extra) else { return };
    let mut dr = acro.get(b"DR").ok().and_then(resolve).unwrap_or_default();
    for (cat, v) in extra.iter() {
        let Some(add) = resolve(v) else { continue };
        let mut have = dr.get(cat).ok().and_then(resolve).unwrap_or_default();
        for (k, x) in add.iter() {
            if have.get(k).is_err() {
                have.set(k.clone(), x.clone());
            }
        }
        dr.set(cat.clone(), Object::Dictionary(have));
    }
    acro.set("DR", Object::Dictionary(dr));
}

/// Gli oggetti identici (lo stesso carattere incorporato da due file) diventano uno: si rifà finché
/// cambia qualcosa, così anche i dizionari che li citano diventano uguali e si uniscono.
fn dedupe(objects: &mut std::collections::BTreeMap<ObjectId, Object>) {
    for _ in 0..4 {
        let mut first: HashMap<[u8; 32], ObjectId> = HashMap::new();
        let mut replace: HashMap<ObjectId, ObjectId> = HashMap::new();
        for (id, o) in objects.iter() {
            let worth = match o {
                Object::Stream(_) => true,
                Object::Dictionary(d) => !matches!(d.get(b"Type").ok().and_then(|t| t.as_name().ok()), Some(b"Page" | b"Pages" | b"Catalog" | b"Outlines" | b"Annot")) && [b"Parent".as_slice(), b"Rect", b"FT", b"T"].iter().all(|k| d.get(k).is_err()),
                _ => false,
            };
            if !worth {
                continue;
            }
            let mut h = Sha256::new();
            h.update(format!("{o:?}").as_bytes());
            if let Object::Stream(s) = o {
                h.update(&s.content);
            }
            let key: [u8; 32] = h.finalize().into();
            match first.get(&key) {
                Some(keep) => {
                    replace.insert(*id, *keep);
                }
                None => {
                    first.insert(key, *id);
                }
            }
        }
        if replace.is_empty() {
            return;
        }
        for id in replace.keys() {
            objects.remove(id);
        }
        for o in objects.values_mut() {
            relink(o, &replace);
        }
    }
}

fn relink(o: &mut Object, r: &HashMap<ObjectId, ObjectId>) {
    match o {
        Object::Reference(id) => {
            if let Some(n) = r.get(id) {
                *id = *n;
            }
        }
        Object::Array(a) => a.iter_mut().for_each(|x| relink(x, r)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, x)| relink(x, r)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, x)| relink(x, r)),
        _ => {}
    }
}

/// Un testo per il PDF: ASCII così com'è, altrimenti UTF-16 con il BOM.
fn pdf_text(s: &str) -> Vec<u8> {
    if s.is_ascii() {
        return s.as_bytes().to_vec();
    }
    let mut out = vec![0xFE, 0xFF];
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out
}

/// Un testo del PDF in chiaro (UTF-16 con BOM o PDFDocEncoding, trattata come Latin-1).
fn text_of(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&u);
    }
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    b.iter().map(|&c| c as char).collect()
}

/// Dove inizia ogni segnalibro di primo livello: (pagina da 1, titolo), in ordine di pagina.
pub fn top_bookmarks(doc: &Document) -> Vec<(u32, String)> {
    let pages = doc.get_pages();
    let number: HashMap<ObjectId, u32> = pages.iter().map(|(n, id)| (*id, *n)).collect();
    let Ok(root) = catalog_dict(doc, b"Outlines") else { return Vec::new() };
    let mut item = root.get(b"First").ok().and_then(|f| f.as_reference().ok());
    let mut seen = HashSet::new();
    let mut out: Vec<(u32, String)> = Vec::new();
    while let Some(id) = item {
        if !seen.insert(id) {
            break;
        }
        let Ok(d) = doc.get_dictionary(id) else { break };
        let dest = d.get(b"Dest").ok().cloned().or_else(|| {
            let act = deref(doc, d.get(b"A").ok()?).as_dict().ok()?;
            act.get(b"D").ok().cloned()
        });
        if let Some(n) = dest.and_then(|x| dest_page(doc, &x)).and_then(|p| number.get(&p).copied()) {
            let title = match d.get(b"Title").map(|t| deref(doc, t)) {
                Ok(Object::String(s, _)) => text_of(s),
                _ => String::new(),
            };
            out.push((n, title.trim().to_string()));
        }
        item = d.get(b"Next").ok().and_then(|f| f.as_reference().ok());
    }
    out.sort_by_key(|(n, _)| *n);
    out.dedup_by_key(|(n, _)| *n);
    out
}

/// Come dividere un PDF.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Plan {
    /// Un file per pagina.
    EveryPage,
    /// Un file ogni N pagine.
    Every(u32),
    /// Un file per gruppo, come scritto: `1-2, 3-4, 5-`.
    Ranges(String),
    /// Un file per segnalibro di primo livello.
    Bookmarks,
    /// Un file solo, senza queste pagine.
    Remove(String),
    /// I tagli scelti sulla striscia: un file nuovo dopo ciascuna di queste pagine (da 1).
    Cuts(Vec<u32>),
}

/// Un gruppo del PDF diviso: le pagine (da 1) e, per i segnalibri, il titolo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Group {
    pub pages: Vec<u32>,
    pub title: Option<String>,
}

/// I gruppi di pagine per un documento di `total` pagine.
pub fn groups(plan: &Plan, total: u32, doc: Option<&Document>) -> Result<Vec<Group>, String> {
    let g = |pages: Vec<u32>| Group { pages, title: None };
    let out: Vec<Group> = match plan {
        Plan::EveryPage => (1..=total).map(|p| g(vec![p])).collect(),
        Plan::Every(n) => {
            let n = (*n).max(1);
            (1..=total).collect::<Vec<_>>().chunks(n as usize).map(|c| g(c.to_vec())).collect()
        }
        Plan::Ranges(text) => {
            let items: Vec<&str> = text.split([',', ';']).map(str::trim).filter(|t| !t.is_empty()).collect();
            if items.is_empty() {
                return Err("scrivi gli intervalli, per esempio 1-3, 4-6, 7-".into());
            }
            let mut out = Vec::new();
            for it in items {
                let sel = crate::engine::PageSel::parse(it)?.ok_or_else(|| format!("intervallo «{it}» non valido"))?;
                if let Some(max) = sel.max_page() {
                    if max as u32 > total {
                        return Err(format!("il PDF ha {total} pagine: la pagina {max} non c'è"));
                    }
                }
                let pages: Vec<u32> = (0..total as usize).filter(|i| sel.contains(*i)).map(|i| i as u32 + 1).collect();
                if pages.is_empty() {
                    return Err(format!("l'intervallo «{it}» non ha pagine"));
                }
                out.push(g(pages));
            }
            out
        }
        Plan::Remove(text) => {
            let sel = crate::engine::PageSel::parse(text)?.ok_or("scrivi le pagine da togliere, per esempio 2,5-6")?;
            let pages: Vec<u32> = (0..total as usize).filter(|i| !sel.contains(*i)).map(|i| i as u32 + 1).collect();
            if pages.is_empty() {
                return Err("così non resta nessuna pagina".into());
            }
            vec![g(pages)]
        }
        Plan::Bookmarks => {
            let marks = doc.map(top_bookmarks).unwrap_or_default();
            if marks.is_empty() {
                return Err("il PDF non ha segnalibri: scegli un altro modo di dividerlo".into());
            }
            let mut out = Vec::new();
            if marks[0].0 > 1 {
                out.push(Group { pages: (1..marks[0].0).collect(), title: Some("Inizio".into()) });
            }
            for (i, (start, title)) in marks.iter().enumerate() {
                let end = marks.get(i + 1).map(|m| m.0 - 1).unwrap_or(total);
                out.push(Group { pages: (*start..=end).collect(), title: Some(title.clone()).filter(|t| !t.is_empty()) });
            }
            out
        }
        Plan::Cuts(cuts) => {
            let mut cuts: Vec<u32> = cuts.iter().copied().filter(|c| *c >= 1 && *c < total).collect();
            cuts.sort_unstable();
            cuts.dedup();
            let mut out = Vec::new();
            let mut start = 1;
            for c in cuts.into_iter().chain(std::iter::once(total)) {
                out.push(g((start..=c).collect()));
                start = c + 1;
            }
            out
        }
    };
    Ok(out)
}

/// Il testo «1-2, 3-4» che descrive i tagli dopo le pagine `cuts` in un PDF di `total` pagine.
pub fn cuts_text(cuts: &[u32], total: u32) -> String {
    groups(&Plan::Cuts(cuts.to_vec()), total, None)
        .unwrap_or_default()
        .iter()
        .map(|g| match (g.pages.first(), g.pages.last()) {
            (Some(a), Some(b)) if a == b => a.to_string(),
            (Some(a), Some(b)) => format!("{a}-{b}"),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Un nome di file pulito da un titolo (niente caratteri che Windows non vuole).
pub fn safe_name(title: &str) -> String {
    let s: String = title.chars().map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '-' } else { c }).collect();
    let s = s.trim().trim_end_matches('.').trim();
    s.chars().take(80).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::dictionary;

    /// Un PDF di prova con `n` pagine e un carattere uguale per tutti; `marks` aggiunge segnalibri.
    pub(crate) fn sample(n: u32, label: &str, marks: bool) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_file = doc.add_object(Stream::new(dictionary! {}, vec![7u8; 4096]));
        let descriptor = doc.add_object(dictionary! { "Type" => "FontDescriptor", "FontName" => "Prova", "FontFile2" => font_file });
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Prova", "FontDescriptor" => descriptor });
        let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
        let mut kids = Vec::new();
        for i in 1..=n {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![72.into(), 700.into()]),
                    Operation::new("Tj", vec![Object::string_literal(format!("{label} {i}"))]),
                    Operation::new("ET", vec![]),
                ],
            };
            let c = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => c });
            kids.push(Object::Reference(page));
        }
        // Risorse e formato ereditati dal nodo delle pagine: vanno ricopiati su ogni pagina.
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids.clone(), "Count" => n as i64, "Resources" => resources, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()] }),
        );
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
        if marks {
            let root = doc.new_object_id();
            let a = doc.new_object_id();
            let b = doc.new_object_id();
            let p1 = kids[0].as_reference().unwrap();
            let p3 = kids[2].as_reference().unwrap();
            doc.objects.insert(a, Object::Dictionary(dictionary! { "Title" => Object::string_literal("Premessa"), "Parent" => root, "Next" => b, "Dest" => vec![p1.into(), "Fit".into()] }));
            doc.objects.insert(b, Object::Dictionary(dictionary! { "Title" => Object::string_literal("Allegati"), "Parent" => root, "Prev" => a, "Dest" => vec![p3.into(), "Fit".into()] }));
            doc.objects.insert(root, Object::Dictionary(dictionary! { "Type" => "Outlines", "First" => a, "Last" => b, "Count" => 2 }));
            catalog.set("Outlines", root);
        }
        let catalog_id = doc.add_object(catalog);
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn texts(bytes: &[u8]) -> Vec<String> {
        let doc = Document::load_mem(bytes).unwrap();
        doc.get_pages().keys().map(|n| doc.extract_text(&[*n]).unwrap().trim().to_string()).collect()
    }

    #[test]
    fn merge_keeps_order_pages_and_inherited_resources() {
        let a = open(&sample(3, "A", true), None).unwrap();
        let b = open(&sample(2, "B", false), None).unwrap();
        let parts = [Part { doc: &b, pages: vec![2], title: "B".into() }, Part { doc: &a, pages: vec![1, 3], title: "Delibera è".into() }];
        let out = build(&parts, &Settings::default()).unwrap();
        assert_eq!(texts(&out), vec!["B 2", "A 1", "A 3"]);
        let doc = Document::load_mem(&out).unwrap();
        // Il carattere uguale dei due file c'è una volta sola.
        let font_files = doc.objects.values().filter(|o| matches!(o, Object::Dictionary(d) if d.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"FontDescriptor"))).count();
        assert_eq!(font_files, 1, "descrittore del carattere duplicato");
        // Segnalibri: uno per file, e sotto quello di A solo «Premessa» (la pagina 3 c'è, quindi anche «Allegati»).
        let toc = top_titles(&doc);
        assert_eq!(toc, vec!["B".to_string(), "Delibera è".to_string()]);
    }

    fn top_titles(doc: &Document) -> Vec<String> {
        let root = catalog_dict(doc, b"Outlines").unwrap();
        let mut item = root.get(b"First").ok().and_then(|f| f.as_reference().ok());
        let mut out = Vec::new();
        while let Some(id) = item {
            let d = doc.get_dictionary(id).unwrap();
            if let Ok(Object::String(s, _)) = d.get(b"Title") {
                out.push(text_of(s));
            }
            item = d.get(b"Next").ok().and_then(|f| f.as_reference().ok());
        }
        out
    }

    #[test]
    fn duplex_blank_after_odd_files() {
        let a = open(&sample(3, "A", false), None).unwrap();
        let b = open(&sample(2, "B", false), None).unwrap();
        let parts = [Part { doc: &a, pages: vec![1, 2, 3], title: "A".into() }, Part { doc: &b, pages: vec![1, 2], title: "B".into() }];
        let s = Settings { duplex_blank: true, ..Settings::default() };
        let out = build(&parts, &s).unwrap();
        assert_eq!(page_count(&Document::load_mem(&out).unwrap()), 6);
    }

    #[test]
    fn split_plans() {
        let doc = open(&sample(5, "A", true), None).unwrap();
        let pages = |p: &Plan| groups(p, 5, Some(&doc)).unwrap().into_iter().map(|g| g.pages).collect::<Vec<_>>();
        assert_eq!(pages(&Plan::EveryPage).len(), 5);
        assert_eq!(pages(&Plan::Every(2)), vec![vec![1, 2], vec![3, 4], vec![5]]);
        assert_eq!(pages(&Plan::Ranges("1-2, 3-".into())), vec![vec![1, 2], vec![3, 4, 5]]);
        assert_eq!(pages(&Plan::Remove("2,4".into())), vec![vec![1, 3, 5]]);
        assert_eq!(pages(&Plan::Cuts(vec![2])), vec![vec![1, 2], vec![3, 4, 5]]);
        let marks = groups(&Plan::Bookmarks, 5, Some(&doc)).unwrap();
        assert_eq!(marks.iter().map(|g| g.pages.clone()).collect::<Vec<_>>(), vec![vec![1, 2], vec![3, 4, 5]]);
        assert_eq!(marks[1].title.as_deref(), Some("Allegati"));
        assert!(groups(&Plan::Ranges("1-9".into()), 5, None).is_err());
        assert!(groups(&Plan::Remove("1-5".into()), 5, None).is_err());
        assert_eq!(cuts_text(&[2], 4), "1-2, 3-4");
        assert_eq!(cuts_text(&[1, 3], 4), "1, 2-3, 4");
    }

    #[test]
    fn split_part_has_only_its_pages_and_bookmarks() {
        let doc = open(&sample(4, "A", true), None).unwrap();
        let out = build(&[Part { doc: &doc, pages: vec![3, 4], title: "A".into() }], &Settings { file_bookmarks: false, ..Settings::default() }).unwrap();
        assert_eq!(texts(&out), vec!["A 3", "A 4"]);
        let d = Document::load_mem(&out).unwrap();
        // «Premessa» porta alla pagina 1, che non c'è: resta solo «Allegati».
        assert_eq!(top_titles(&d), vec!["Allegati".to_string()]);
        // L'albero delle pagine vecchio non è arrivato.
        assert_eq!(d.objects.values().filter(|o| matches!(o, Object::Dictionary(x) if x.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"Pages"))).count(), 1);
    }

    #[test]
    fn protected_pdf_needs_the_password() {
        let plain = sample(2, "A", false);
        let locked = crate::pdf::protect(&plain, "segreta").unwrap();
        assert!(open(&locked, None).unwrap_err().contains("protetto"));
        assert!(open(&locked, Some("sbagliata")).is_err());
        let doc = open(&locked, Some("segreta")).unwrap();
        assert_eq!(page_count(&doc), 2);
        let out = build(&[Part { doc: &doc, pages: vec![1, 2], title: "A".into() }], &Settings::default()).unwrap();
        assert_eq!(texts(&out), vec!["A 1", "A 2"]);
    }
}
