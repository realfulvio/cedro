//! «Video dal web» in Rust puro, senza programmi esterni: un link diretto a un file audio o video, una pagina che lo
//! incorpora (`og:video`, `<video>`, `<source>`, JSON-LD) o una pagina di Internet Archive. Niente estrattori di
//! siti che cambiano ogni settimana: se il link non porta a un file che si può scaricare con una richiesta normale,
//! si dice chiaramente perché (YouTube, flussi HLS/DASH, video protetti).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Cosa vuole l'utente da un link.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Want {
    /// Il video com'è.
    Video,
    /// Il video più piccolo che c'è (o ricompresso dopo lo scarico).
    Light,
    /// Solo l'audio (MP3).
    Audio,
}

/// Il file da scaricare, una volta capito il link.
#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub url: String,
    pub title: String,
    /// Estensione senza punto (mp4, webm, mp3…).
    pub ext: String,
    pub size: Option<u64>,
    /// Vero se è già solo audio.
    pub audio: bool,
}

const AUDIO_EXT: &[&str] = &["mp3", "m4a", "aac", "ogg", "oga", "opus", "flac", "wav"];
const VIDEO_EXT: &[&str] = &["mp4", "webm", "mov", "mkv", "m4v", "avi", "ogv"];

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .user_agent(&format!("cedro/{}", crate::update::CURRENT))
        .build()
}

/// Il primo link http(s) in un testo (quello che arriva da «Condividi» di un browser o dagli appunti).
pub fn find_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| matches!(c, '<' | '>' | '"' | '\'' | ',' | ';' | '(' | ')')))
        .find(|w| crate::youtube::is_video_url(w))
        .map(str::to_string)
}

fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let h = rest.split(['/', '?', '#']).next().unwrap_or("");
    h.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("").to_ascii_lowercase()
}

fn ext_of(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    let name = path.rsplit('/').next()?;
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    (AUDIO_EXT.contains(&ext.as_str()) || VIDEO_EXT.contains(&ext.as_str())).then_some(ext)
}

/// Un indirizzo (anche relativo) trovato in una pagina, reso assoluto rispetto alla pagina.
pub fn join(base: &str, link: &str) -> String {
    let link = link.trim().replace("&amp;", "&").replace("\\/", "/");
    if link.starts_with("http://") || link.starts_with("https://") {
        return link;
    }
    let (scheme, rest) = base.split_once("://").unwrap_or(("https", base));
    if let Some(l) = link.strip_prefix("//") {
        return format!("{scheme}://{l}");
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if link.starts_with('/') {
        return format!("{scheme}://{host}{link}");
    }
    let dir = rest.split(['?', '#']).next().unwrap_or("");
    let dir = dir.rsplit_once('/').map(|(d, _)| d).unwrap_or(host);
    format!("{scheme}://{dir}/{link}")
}

fn clean_title(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { ' ' } else { c }).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(80).collect();
    t.trim().trim_matches('.').to_string()
}

/// Il valore di `attr` (`content`, `src`…) del primo tag che contiene `marker`.
fn tag_attr(html: &str, marker: &str, attr: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = html[from..].find(marker) {
        let at = from + i;
        let start = html[..at].rfind('<').unwrap_or(0);
        let end = html[at..].find('>').map(|e| at + e).unwrap_or(html.len());
        let tag = &html[start..end];
        for quote in ['"', '\''] {
            let key = format!("{attr}={quote}");
            if let Some(k) = tag.find(&key) {
                let v = &tag[k + key.len()..];
                if let Some(e) = v.find(quote) {
                    return Some(v[..e].to_string());
                }
            }
        }
        from = at + marker.len();
    }
    None
}

/// Cerca nella pagina il file multimediale: `og:video`, `<video src>`, `<source src>` e `contentUrl` (JSON-LD).
pub fn media_in_page(html: &str, page_url: &str) -> Option<Media> {
    let title = tag_attr(html, "property=\"og:title\"", "content")
        .or_else(|| html.find("<title>").and_then(|i| html[i + 7..].find("</title>").map(|e| html[i + 7..i + 7 + e].to_string())))
        .map(|t| clean_title(&t.replace("&amp;", "&").replace("&#39;", "'").replace("&quot;", "\"")))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| host_of(page_url));
    let mut candidates: Vec<String> = Vec::new();
    for marker in ["property=\"og:video:secure_url\"", "property=\"og:video\"", "property=\"og:video:url\"", "name=\"twitter:player:stream\"", "property=\"og:audio\""] {
        candidates.extend(tag_attr(html, marker, "content"));
    }
    for tag in ["<video", "<source", "<audio"] {
        let mut from = 0;
        while let Some(i) = html[from..].find(tag) {
            let at = from + i;
            // Un tag mai chiuso (pagina tagliata) arriva fino in fondo, senza uscire dal testo.
            let end = html[at..].find('>').map_or(html.len(), |e| at + e + 1);
            if let Some(v) = tag_attr(&html[at..end], tag, "src") {
                candidates.push(v);
            }
            from = at + tag.len();
        }
    }
    if let Some(i) = html.find("\"contentUrl\"") {
        let rest = &html[i + 12..];
        if let Some(q) = rest.find('"') {
            let r = &rest[q + 1..];
            if let Some(e) = r.find('"') {
                candidates.push(r[..e].to_string());
            }
        }
    }
    for c in candidates {
        let url = join(page_url, &c);
        if let Some(ext) = ext_of(&url) {
            let audio = AUDIO_EXT.contains(&ext.as_str());
            return Some(Media { url, title, ext, size: None, audio });
        }
    }
    None
}

/// Internet Archive: dalla pagina `archive.org/details/<id>` sceglie il file giusto tra quelli dell'oggetto.
pub fn archive_media(meta: &serde_json::Value, id: &str, want: Want) -> Option<Media> {
    let files = meta["files"].as_array()?;
    let title = clean_title(meta["metadata"]["title"].as_str().unwrap_or(id));
    let size = |f: &serde_json::Value| f["size"].as_str().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let name = |f: &serde_json::Value| f["name"].as_str().unwrap_or("").to_string();
    let original = |f: &serde_json::Value| f["source"].as_str() == Some("original");
    let pick = |exts: &[&str], smallest: bool| {
        let mut v: Vec<&serde_json::Value> = files
            .iter()
            .filter(|f| {
                let n = name(f).to_ascii_lowercase();
                exts.iter().any(|e| n.ends_with(&format!(".{e}"))) && !n.contains("_thumb") && !n.ends_with("_itemimage.jpg") && size(f) > 0
            })
            .collect();
        if smallest {
            v.sort_by_key(|f| size(f));
        } else {
            // Il migliore: l'originale se è un formato che si legge, altrimenti il più grande.
            v.sort_by_key(|f| (!original(f), std::cmp::Reverse(size(f))));
        }
        v.first().map(|f| (name(f), size(f)))
    };
    let (file, bytes, audio) = match want {
        Want::Audio => pick(&["mp3", "m4a", "ogg", "flac", "opus"], false).map(|(n, s)| (n, s, true)).or_else(|| pick(&["mp4", "m4v", "webm", "ogv"], true).map(|(n, s)| (n, s, false)))?,
        Want::Light => pick(&["mp4", "m4v", "webm", "ogv"], true).map(|(n, s)| (n, s, false))?,
        Want::Video => pick(&["mp4", "m4v", "webm", "mkv", "mov", "ogv"], false).map(|(n, s)| (n, s, false)).or_else(|| pick(&["mp3", "m4a", "ogg", "flac"], false).map(|(n, s)| (n, s, true)))?,
    };
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let encoded: String = file.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    Some(Media { url: format!("https://archive.org/download/{id}/{encoded}"), title, ext, size: Some(bytes), audio })
}

/// Perché non si può scaricare: i siti che servono flussi a pezzi o con chiavi che cambiano.
fn unsupported(url: &str) -> Option<&'static str> {
    let h = host_of(url);
    let is = |d: &str| h == d || h.ends_with(&format!(".{d}"));
    if is("youtube.com") || is("youtu.be") || is("youtube-nocookie.com") {
        Some("YouTube non è ancora supportato su Android.")
    } else if is("netflix.com") || is("primevideo.com") || is("disneyplus.com") || is("raiplay.it") {
        Some("Questo sito usa video protetti: Cedro non li scarica.")
    } else {
        None
    }
}

/// Capisce cosa scaricare da `url` (rete: una o due richieste).
pub fn resolve(url: &str, want: Want) -> Result<Media, String> {
    if let Some(why) = unsupported(url) {
        return Err(why.into());
    }
    let h = host_of(url);
    if h == "archive.org" || h.ends_with(".archive.org") {
        let path = url.split(['?', '#']).next().unwrap_or("");
        let id = path.split("/details/").nth(1).or_else(|| path.split("/download/").nth(1)).and_then(|r| r.split('/').next()).filter(|i| !i.is_empty()).ok_or("Questo link di Internet Archive non porta a un oggetto.")?;
        let text = agent().get(&format!("https://archive.org/metadata/{id}")).call().map_err(|e| format!("Internet Archive non risponde: {e}"))?.into_string().map_err(|e| e.to_string())?;
        let meta: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        return archive_media(&meta, id, want).ok_or_else(|| "In questa pagina di Internet Archive non c'è un video o un audio da scaricare.".into());
    }
    if let Some(ext) = ext_of(url) {
        let title = clean_title(url.split(['?', '#']).next().unwrap_or("").rsplit('/').next().unwrap_or("").rsplit_once('.').map(|(n, _)| n).unwrap_or("video"));
        let audio = AUDIO_EXT.contains(&ext.as_str());
        return Ok(Media { url: url.to_string(), title: if title.is_empty() { "video".into() } else { title }, ext, size: None, audio });
    }
    let resp = agent().get(url).call().map_err(|e| format!("Il sito non risponde: {e}"))?;
    let ctype = resp.content_type().to_ascii_lowercase();
    if ctype.starts_with("video/") || ctype.starts_with("audio/") {
        let ext = match ctype.split(';').next().unwrap_or("") {
            "video/mp4" => "mp4",
            "video/webm" => "webm",
            "video/quicktime" => "mov",
            "audio/mpeg" => "mp3",
            "audio/mp4" | "audio/x-m4a" => "m4a",
            "audio/ogg" => "ogg",
            _ => "mp4",
        };
        return Ok(Media { url: url.to_string(), title: host_of(url), ext: ext.into(), size: None, audio: ctype.starts_with("audio/") });
    }
    // Il taglio a 3 MB può cadere a metà di un carattere, e non tutte le pagine sono in UTF-8.
    let mut raw = Vec::new();
    resp.into_reader().take(3_000_000).read_to_end(&mut raw).map_err(|_| "La pagina non si legge come testo.")?;
    let html = String::from_utf8_lossy(&raw);
    media_in_page(&html, url).ok_or_else(|| "In questa pagina non trovo un video o un audio da scaricare (se è un flusso a pezzi o protetto, Cedro non lo prende).".into())
}

fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    p
}

/// Scarica `media` in `dir` (file `.part` poi rinominato); `progress` riceve 0..1 quando la dimensione si conosce.
pub fn download(media: &Media, dir: &Path, progress: &dyn Fn(f32), cancel: &AtomicBool) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("la cartella non è scrivibile: {e}"))?;
    let stem = if media.title.is_empty() { "video".to_string() } else { media.title.clone() };
    let dest = unique(dir, &stem, &media.ext);
    let part = dest.with_extension(format!("{}.part", media.ext));
    let resp = agent().get(&media.url).call().map_err(|e| format!("scarico non riuscito: {e}"))?;
    let total = resp.header("content-length").and_then(|v| v.parse::<u64>().ok()).or(media.size);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let result = (|| loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("annullato".to_string());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("scarico interrotto: {e}"))?;
        if n == 0 {
            return Ok(());
        }
        file.write_all(&buf[..n]).map_err(|e| format!("disco pieno o non scrivibile: {e}"))?;
        done += n as u64;
        if let Some(t) = total.filter(|t| *t > 0) {
            progress((done as f32 / t as f32).min(1.0));
        }
    })();
    drop(file);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if done == 0 {
        let _ = std::fs::remove_file(&part);
        return Err("il sito ha risposto con un file vuoto".into());
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_link_in_shared_text() {
        assert_eq!(find_url("Guarda qui https://example.com/v.mp4, bello").as_deref(), Some("https://example.com/v.mp4"));
        assert_eq!(find_url("niente"), None);
    }

    #[test]
    fn joins_relative_links() {
        assert_eq!(join("https://a.it/x/y.html", "/m/v.mp4"), "https://a.it/m/v.mp4");
        assert_eq!(join("https://a.it/x/y.html", "v.mp4"), "https://a.it/x/v.mp4");
        assert_eq!(join("https://a.it/x", "//cdn.b.it/v.webm?a=1&amp;b=2"), "https://cdn.b.it/v.webm?a=1&b=2");
    }

    #[test]
    fn page_with_og_video_and_title() {
        let html = r#"<html><head><meta property="og:title" content="Il mio &amp; video"><meta property="og:video" content="/media/clip.mp4"></head></html>"#;
        let m = media_in_page(html, "https://a.it/p/1").unwrap();
        assert_eq!((m.url.as_str(), m.title.as_str(), m.ext.as_str(), m.audio), ("https://a.it/media/clip.mp4", "Il mio & video", "mp4", false));
    }

    #[test]
    fn page_with_video_tag_and_json_ld() {
        let m = media_in_page(r#"<video controls><source src="a.webm" type="video/webm"></video>"#, "https://a.it/d/").unwrap();
        assert_eq!(m.url, "https://a.it/d/a.webm");
        let m = media_in_page(r#"<script>{"contentUrl":"https:\/\/c.it\/v.mp4"}</script>"#, "https://a.it/").unwrap();
        assert_eq!(m.url, "https://c.it/v.mp4");
        assert!(media_in_page("<p>niente</p>", "https://a.it/").is_none());
    }

    /// Una pagina tagliata dentro un tag `<source` (mai chiuso): niente panic.
    #[test]
    fn page_cut_inside_a_tag() {
        assert!(media_in_page("<p>x</p><video", "https://a.it/").is_none());
        let m = media_in_page(r#"<video><source src="b.mp4" type="vid"#, "https://a.it/").unwrap();
        assert_eq!(m.url, "https://a.it/b.mp4");
    }

    #[test]
    fn archive_picks_original_video_smallest_light_and_audio() {
        let meta = serde_json::json!({"metadata": {"title": "Prova"}, "files": [
            {"name": "prova.mp4", "source": "derivative", "size": "5000000"},
            {"name": "prova.512kb.mp4", "source": "derivative", "size": "1000000"},
            {"name": "prova.avi", "source": "original", "size": "90000000"},
            {"name": "prova.mp3", "source": "derivative", "size": "300000"},
        ]});
        assert_eq!(archive_media(&meta, "p1", Want::Video).unwrap().url, "https://archive.org/download/p1/prova.mp4");
        assert_eq!(archive_media(&meta, "p1", Want::Light).unwrap().url, "https://archive.org/download/p1/prova.512kb.mp4");
        let a = archive_media(&meta, "p1", Want::Audio).unwrap();
        assert!(a.audio && a.ext == "mp3");
    }

    #[test]
    fn youtube_is_said_plainly() {
        assert!(resolve("https://youtu.be/abc", Want::Video).unwrap_err().contains("YouTube"));
    }
}
