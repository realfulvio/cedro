//! Conversioni di testo vere: CSV, JSON, tabelle Markdown e HTML, codifiche di uscita.

use crate::engine::Encoding;

/// Il testo nella codifica scelta; i caratteri che non ci stanno diventano `?`.
pub fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Latin1 => text.chars().map(|c| if (c as u32) < 0x100 { c as u32 as u8 } else { b'?' }).collect(),
        Encoding::Windows1252 => text
            .chars()
            .map(|c| match c as u32 {
                0..=0x7F | 0xA0..=0xFF => c as u32 as u8,
                _ => (0x80u8..=0x9F).find(|b| crate::doctext::cp1252(*b) == c).unwrap_or(b'?'),
            })
            .collect(),
    }
}

/// Il separatore più usato nella prima riga: `;`, `,` o tabulazione.
pub fn detect_separator(text: &str) -> char {
    let first = text.lines().next().unwrap_or("");
    let mut best = (';', 0usize);
    for sep in [';', ',', '\t'] {
        let mut quoted = false;
        let n = first
            .chars()
            .filter(|c| {
                if *c == '"' {
                    quoted = !quoted;
                }
                !quoted && c == &sep
            })
            .count();
        if n > best.1 {
            best = (sep, n);
        }
    }
    best.0
}

/// Le righe del CSV, con le virgolette (`""` dentro un campo è una virgoletta).
pub fn parse_csv(text: &str, sep: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
        } else if c == sep {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            any = false;
        } else {
            field.push(c);
        }
    }
    if any || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    rows
}

pub fn write_csv(rows: &[Vec<String>], sep: char) -> String {
    let mut out = String::new();
    for row in rows {
        let line: Vec<String> = row
            .iter()
            .map(|f| {
                if f.contains(sep) || f.contains('"') || f.contains('\n') || f.contains('\r') {
                    format!("\"{}\"", f.replace('"', "\"\""))
                } else {
                    f.clone()
                }
            })
            .collect();
        out.push_str(&line.join(&sep.to_string()));
        out.push('\n');
    }
    out
}

fn width(rows: &[Vec<String>]) -> usize {
    rows.iter().map(|r| r.len()).max().unwrap_or(0)
}

/// Tabella Markdown. Senza `header` la riga dei titoli resta vuota.
pub fn markdown_table(rows: &[Vec<String>], header: bool) -> String {
    let cols = width(rows).max(1);
    let cell = |r: &Vec<String>, i: usize| r.get(i).map(|f| f.replace('|', "\\|").replace('\n', " ")).unwrap_or_default();
    let line = |r: &Vec<String>| format!("| {} |\n", (0..cols).map(|i| cell(r, i)).collect::<Vec<_>>().join(" | "));
    let mut out = String::new();
    let (head, body): (Vec<String>, &[Vec<String>]) = if header && !rows.is_empty() {
        (rows[0].clone(), &rows[1..])
    } else {
        (Vec::new(), rows)
    };
    out.push_str(&line(&head));
    out.push_str(&format!("|{}|\n", vec![" --- "; cols].join("|")));
    for r in body {
        out.push_str(&line(r));
    }
    out
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn html_page(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"it\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape_html(title)
    )
}

pub fn html_table(rows: &[Vec<String>], header: bool) -> String {
    let mut out = String::from("<table border=\"1\" cellspacing=\"0\" cellpadding=\"4\">\n");
    for (i, r) in rows.iter().enumerate() {
        let tag = if header && i == 0 { "th" } else { "td" };
        out.push_str("<tr>");
        for f in r {
            out.push_str(&format!("<{tag}>{}</{tag}>", escape_html(f)));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

/// Paragrafi HTML da un testo semplice: le righe vuote separano i paragrafi.
pub fn html_paragraphs(text: &str) -> String {
    let mut out = String::new();
    for para in text.replace("\r\n", "\n").split("\n\n") {
        let para = para.trim_matches('\n');
        if !para.is_empty() {
            out.push_str(&format!("<p>{}</p>\n", escape_html(para).replace('\n', "<br>\n")));
        }
    }
    out
}

fn json_string(v: &str) -> String {
    let mut out = String::from("\"");
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// CSV in JSON: un array di oggetti con i titoli come chiavi, o un array di array senza titoli.
pub fn csv_to_json(rows: &[Vec<String>], header: bool, pretty: bool) -> String {
    let compact = if header && !rows.is_empty() {
        let keys = &rows[0];
        let objects: Vec<String> = rows[1..]
            .iter()
            .map(|r| {
                let fields: Vec<String> = keys
                    .iter()
                    .enumerate()
                    .map(|(i, k)| format!("{}:{}", json_string(k), json_string(r.get(i).map(String::as_str).unwrap_or(""))))
                    .collect();
                format!("{{{}}}", fields.join(","))
            })
            .collect();
        format!("[{}]", objects.join(","))
    } else {
        let arrays: Vec<String> =
            rows.iter().map(|r| format!("[{}]", r.iter().map(|f| json_string(f)).collect::<Vec<_>>().join(","))).collect();
        format!("[{}]", arrays.join(","))
    };
    if pretty {
        json_format(&compact, true).unwrap_or(compact)
    } else {
        compact
    }
}

/// Riscrive un JSON con i rientri (`pretty`) o su una riga, senza cambiarne il contenuto.
/// Ritorna `Err` se le parentesi o le virgolette non tornano.
pub fn json_format(text: &str, pretty: bool) -> Result<String, String> {
    let mut out = String::new();
    let mut depth = 0usize;
    let mut stack: Vec<char> = Vec::new();
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        out.push_str(&"  ".repeat(depth));
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                let mut closed = false;
                while let Some(s) = chars.next() {
                    out.push(s);
                    if s == '\\' {
                        if let Some(n) = chars.next() {
                            out.push(n);
                        }
                    } else if s == '"' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err("JSON non valido: virgolette aperte".into());
                }
            }
            '{' | '[' => {
                out.push(c);
                stack.push(c);
                let close = if c == '{' { '}' } else { ']' };
                while chars.peek().is_some_and(|n| n.is_whitespace()) {
                    chars.next();
                }
                if chars.peek() == Some(&close) {
                    out.push(chars.next().unwrap());
                    stack.pop();
                } else if pretty {
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            '}' | ']' => {
                let open = if c == '}' { '{' } else { '[' };
                if stack.pop() != Some(open) {
                    return Err("JSON non valido: parentesi che non tornano".into());
                }
                if pretty {
                    depth = depth.saturating_sub(1);
                    newline(&mut out, depth);
                }
                out.push(c);
            }
            ',' => {
                out.push(',');
                if pretty {
                    newline(&mut out, depth);
                }
            }
            ':' => {
                out.push(':');
                if pretty {
                    out.push(' ');
                }
            }
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    if !stack.is_empty() {
        return Err("JSON non valido: parentesi aperte".into());
    }
    if pretty {
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_chosen_charset() {
        assert_eq!(encode("è€", Encoding::Utf8), "è€".as_bytes());
        assert_eq!(encode("è€", Encoding::Windows1252), vec![0xE8, 0x80]);
        assert_eq!(encode("è€", Encoding::Latin1), vec![0xE8, b'?']);
    }

    #[test]
    fn csv_roundtrip_with_quotes_and_other_separators() {
        let text = "nome;nota\n\"Rossi; Luca\";\"dice \"\"ciao\"\"\"\nAnna;ok\n";
        assert_eq!(detect_separator(text), ';');
        let rows = parse_csv(text, ';');
        assert_eq!(rows[1], vec!["Rossi; Luca".to_string(), "dice \"ciao\"".to_string()]);
        let comma = write_csv(&rows, ',');
        assert!(comma.starts_with("nome,nota
Rossi; Luca,\"dice \"\"ciao\"\"\"
"), "{comma}");
        assert_eq!(parse_csv(&comma, ','), rows);
        assert_eq!(detect_separator("a\tb\tc\n"), '\t');
    }

    #[test]
    fn tables_follow_the_header_option() {
        let rows = parse_csv("a;b\n1;2\n", ';');
        assert_eq!(markdown_table(&rows, true), "| a | b |\n| --- | --- |\n| 1 | 2 |\n");
        assert_eq!(markdown_table(&rows, false), "|  |  |\n| --- | --- |\n| a | b |\n| 1 | 2 |\n");
        assert!(html_table(&rows, true).contains("<th>a</th>"));
        assert!(html_table(&rows, false).contains("<td>a</td>"));
    }

    #[test]
    fn json_is_pretty_or_compact() {
        let raw = "{\"a\": [1, 2, {\"b\": \"x,y\"}], \"c\": {}}";
        assert_eq!(json_format(raw, false).unwrap(), "{\"a\":[1,2,{\"b\":\"x,y\"}],\"c\":{}}");
        let pretty = json_format(raw, true).unwrap();
        assert!(pretty.contains("\n  \"a\": [\n    1,\n    2,"), "{pretty}");
        assert!(pretty.contains("\"c\": {}"), "{pretty}");
        assert!(json_format("{\"a\": [1}", true).is_err());
        assert!(json_format("{\"a\": \"x}", true).is_err());
    }

    #[test]
    fn csv_becomes_json_objects_or_arrays() {
        let rows = parse_csv("nome;eta\nLuca;40\n", ';');
        assert_eq!(csv_to_json(&rows, true, false), "[{\"nome\":\"Luca\",\"eta\":\"40\"}]");
        assert_eq!(csv_to_json(&rows, false, false), "[[\"nome\",\"eta\"],[\"Luca\",\"40\"]]");
    }
}
