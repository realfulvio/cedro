//! PDF in Excel: le tabelle trovate da `pdflayout` diventano fogli di calcolo. I numeri scritti
//! all'italiana («1.234,56», «12%», «€ 50,00») e le date («03/11/2026») diventano valori veri,
//! che si sommano e si ordinano; i codici con gli zeri davanti (CAP, protocolli) restano testo.

use rust_xlsxwriter::{ExcelDateTime, Format, FormatAlign, Workbook, Worksheet, XlsxError};

use crate::pdflayout::{SheetTable, SheetText};

/// Come si scrive il foglio.
pub struct SheetOptions {
    /// Un foglio per tabella; spenta, tutte le tabelle in un foglio solo, una sotto l'altra.
    pub one_per_table: bool,
    /// Numeri e date diventano valori; spenta, tutto resta testo com'è nel PDF.
    pub values: bool,
    /// Il testo fuori dalle tabelle va in un foglio a parte.
    pub outside_text: bool,
}

/// Il valore di una cella.
#[derive(Debug, PartialEq)]
pub enum Value {
    Text,
    /// Numero e formato di Excel con cui mostrarlo.
    Number(f64, String),
    Date(u16, u8, u8),
}

fn digits_only(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Un numero scritto all'italiana: punto per le migliaia (solo a gruppi di tre), virgola per i decimali.
/// Restituisce il numero scritto all'inglese («1234.56», da leggere con `parse`) e quante cifre dopo la virgola.
fn italian_number(s: &str) -> Option<(String, usize)> {
    let (int, dec) = match s.split_once(',') {
        Some((i, d)) if digits_only(d) => (i, d),
        Some(_) => return None,
        None => (s, ""),
    };
    let groups: Vec<&str> = int.split('.').collect();
    let ok = match groups.len() {
        1 => digits_only(int),
        _ => (1..=3).contains(&groups[0].len()) && digits_only(groups[0]) && groups[1..].iter().all(|g| g.len() == 3 && digits_only(g)),
    };
    if !ok {
        return None;
    }
    let plain: String = groups.concat();
    // Zeri davanti (CAP, protocolli, codici) e numeri lunghi (IBAN, telefoni): restano testo.
    if (plain.len() > 1 && plain.starts_with('0') && dec.is_empty()) || plain.len() > 15 {
        return None;
    }
    Some((if dec.is_empty() { plain } else { format!("{plain}.{dec}") }, dec.len()))
}

fn number_format(decimals: usize, grouped: bool) -> String {
    let base = if grouped { "#,##0" } else { "0" };
    if decimals == 0 {
        base.to_string()
    } else {
        format!("{base}.{}", "0".repeat(decimals.min(10)))
    }
}

/// Una data gg/mm/aaaa (anche con - o .), o gg/mm/aa.
fn date(s: &str) -> Option<(u16, u8, u8)> {
    let sep = s.chars().find(|c| matches!(c, '/' | '-' | '.'))?;
    let parts: Vec<&str> = s.split(sep).collect();
    if parts.len() != 3 || !parts.iter().all(|p| digits_only(p)) || parts[0].len() > 2 || parts[1].len() > 2 {
        return None;
    }
    let d: u8 = parts[0].parse().ok()?;
    let m: u8 = parts[1].parse().ok()?;
    let y: u16 = match parts[2].len() {
        4 => parts[2].parse().ok()?,
        2 => 2000 + parts[2].parse::<u16>().ok()?,
        _ => return None,
    };
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => return None,
    };
    ((1900..=9999).contains(&y) && d >= 1 && d <= days).then_some((y, m, d))
}

/// Che cosa c'è scritto in una cella.
pub fn value(text: &str) -> Value {
    let t = text.trim();
    if t.is_empty() || t.contains('\n') {
        return Value::Text;
    }
    if let Some(d) = date(t) {
        return Value::Date(d.0, d.1, d.2);
    }
    let mut s: String = t.chars().filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}')).collect();
    let mut negative = false;
    if s.starts_with('(') && s.ends_with(')') && s.len() > 2 {
        s = s[1..s.len() - 1].to_string();
        negative = true;
    }
    let euro = s.starts_with('€') || s.ends_with('€') || s.starts_with("EUR") || s.ends_with("EUR");
    let s = s.trim_start_matches('€').trim_end_matches('€').trim_start_matches("EUR").trim_end_matches("EUR");
    let percent = s.ends_with('%');
    let s = s.trim_end_matches('%');
    let (sign, s) = match s.chars().next() {
        Some('-' | '−' | '–') => (-1.0, &s[s.chars().next().map_or(0, char::len_utf8)..]),
        Some('+') => (1.0, &s[1..]),
        _ => (1.0, s),
    };
    if negative && sign < 0.0 {
        return Value::Text;
    }
    let Some((plain, decimals)) = italian_number(s) else { return Value::Text };
    // La percentuale si sposta di due cifre nel testo, così 4,1% è proprio 0,041.
    let Ok(v) = (if percent { format!("{plain}e-2") } else { plain }).parse::<f64>() else { return Value::Text };
    let grouped = s.contains('.') || v.abs() >= 1000.0;
    let v = if negative { -v } else { sign * v };
    if percent {
        return Value::Number(v, format!("{}%", number_format(decimals, false)));
    }
    if euro {
        return Value::Number(v, format!("€ {}", number_format(decimals.max(2), true)));
    }
    Value::Number(v, number_format(decimals, grouped))
}

/// Il nome di un foglio: al massimo 31 caratteri, senza quelli che Excel non vuole.
fn sheet_name(name: &str, used: &mut Vec<String>) -> String {
    let clean: String = name.chars().filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\')).take(31).collect();
    let mut out = clean.clone();
    let mut n = 2;
    while used.iter().any(|u| u.eq_ignore_ascii_case(&out)) {
        let tail = format!(" ({n})");
        out = format!("{}{tail}", clean.chars().take(31 - tail.chars().count()).collect::<String>());
        n += 1;
    }
    used.push(out.clone());
    out
}

fn pages_label(t: &SheetTable) -> String {
    if t.pages.0 == t.pages.1 {
        format!("p. {}", t.pages.0 + 1)
    } else {
        format!("p. {}–{}", t.pages.0 + 1, t.pages.1 + 1)
    }
}

struct Styles {
    head: Format,
    wrap: Format,
    title: Format,
}

fn write_cell(ws: &mut Worksheet, row: u32, col: u16, text: &str, head: bool, styles: &Styles, values: bool) -> Result<(), XlsxError> {
    if text.is_empty() {
        return Ok(());
    }
    if head {
        ws.write_string_with_format(row, col, text, &styles.head)?;
        return Ok(());
    }
    match if values { value(text) } else { Value::Text } {
        Value::Number(v, fmt) => {
            ws.write_number_with_format(row, col, v, &Format::new().set_num_format(fmt))?;
        }
        Value::Date(y, m, d) => match ExcelDateTime::from_ymd(y, m, d) {
            Ok(date) => {
                ws.write_datetime_with_format(row, col, &date, &Format::new().set_num_format("dd/mm/yyyy"))?;
            }
            Err(_) => {
                ws.write_string(row, col, text)?;
            }
        },
        Value::Text if text.contains('\n') => {
            ws.write_string_with_format(row, col, text, &styles.wrap)?;
        }
        Value::Text => {
            ws.write_string(row, col, text)?;
        }
    }
    Ok(())
}

/// Scrive una tabella a partire dalla riga `top`; restituisce la prima riga libera dopo.
fn write_table(ws: &mut Worksheet, t: &SheetTable, top: u32, styles: &Styles, values: bool) -> Result<u32, XlsxError> {
    let mut row = top;
    for (r, cells) in t.rows.iter().enumerate() {
        let head = t.header && r == 0;
        let mut col: u16 = 0;
        for c in cells {
            let span = c.span.max(1) as u16;
            if span > 1 {
                let fmt = if head { &styles.head } else { &styles.wrap };
                ws.merge_range(row, col, row, col + span - 1, "", fmt)?;
            }
            write_cell(ws, row, col, &c.text, head, styles, values)?;
            col += span;
        }
        row += 1;
    }
    Ok(row)
}

/// Il file .xlsx con le tabelle e, se chiesto o se di tabelle non ce ne sono, il testo.
pub fn xlsx(tables: &[SheetTable], texts: &[SheetText], opts: &SheetOptions) -> Result<Vec<u8>, String> {
    let err = |e: XlsxError| format!("Excel non scritto: {e}");
    let mut book = Workbook::new();
    let styles = Styles {
        head: Format::new().set_bold().set_background_color("EAF2F9").set_text_wrap().set_align(FormatAlign::Top),
        wrap: Format::new().set_text_wrap().set_align(FormatAlign::Top),
        title: Format::new().set_bold().set_font_size(13),
    };
    let mut used = Vec::new();
    if opts.one_per_table {
        for (n, t) in tables.iter().enumerate() {
            let ws = book.add_worksheet();
            ws.set_name(sheet_name(&format!("Tabella {} · {}", n + 1, pages_label(t)), &mut used)).map_err(err)?;
            write_table(ws, t, 0, &styles, opts.values).map_err(err)?;
            if t.header {
                ws.set_freeze_panes(1, 0).map_err(err)?;
            }
            ws.set_autofit_max_width(400);
            ws.autofit();
        }
    } else if !tables.is_empty() {
        let ws = book.add_worksheet();
        ws.set_name(sheet_name("Tabelle", &mut used)).map_err(err)?;
        let mut row = 0;
        for (n, t) in tables.iter().enumerate() {
            let title = format!("Tabella {} · {}", n + 1, if t.pages.0 == t.pages.1 { format!("pagina {}", t.pages.0 + 1) } else { format!("pagine {}–{}", t.pages.0 + 1, t.pages.1 + 1) });
            ws.write_string_with_format(row, 0, &title, &styles.title).map_err(err)?;
            row = write_table(ws, t, row + 1, &styles, opts.values).map_err(err)? + 1;
        }
        ws.set_autofit_max_width(400);
        ws.autofit();
    }
    if (opts.outside_text || tables.is_empty()) && !texts.is_empty() {
        let ws = book.add_worksheet();
        ws.set_name(sheet_name("Testo", &mut used)).map_err(err)?;
        ws.write_string_with_format(0, 0, "Pagina", &styles.head).map_err(err)?;
        ws.write_string_with_format(0, 1, "Testo", &styles.head).map_err(err)?;
        let mut row = 1;
        for t in texts {
            for line in &t.lines {
                ws.write_number(row, 0, (t.page + 1) as f64).map_err(err)?;
                ws.write_string(row, 1, line).map_err(err)?;
                row += 1;
            }
        }
        ws.set_freeze_panes(1, 0).map_err(err)?;
        ws.set_column_width(0, 8).map_err(err)?;
        ws.set_column_width(1, 100).map_err(err)?;
    }
    if used.is_empty() {
        return Err("nel PDF non ci sono né tabelle né testo da mettere nel foglio".into());
    }
    book.save_to_buffer().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(text: &str) -> (f64, String) {
        match value(text) {
            Value::Number(v, f) => (v, f),
            other => panic!("{text}: {other:?}"),
        }
    }

    #[test]
    fn italian_numbers_become_numbers() {
        assert_eq!(num("1.234,56"), (1234.56, "#,##0.00".into()));
        assert_eq!(num("642.180,50"), (642180.5, "#,##0.00".into()));
        assert_eq!(num("1.284.300,00").0, 1284300.0);
        assert_eq!(num("42"), (42.0, "0".into()));
        assert_eq!(num("3,5"), (3.5, "0.0".into()));
        assert_eq!(num("−7,6").0, -7.6);
        assert_eq!(num("(1.200,00)").0, -1200.0);
        assert_eq!(num("1 234,00").0, 1234.0);
    }

    #[test]
    fn percent_and_euro_keep_their_look() {
        assert_eq!(num("12%"), (0.12, "0%".into()));
        assert_eq!(num("4,1 %"), (0.041, "0.0%".into()));
        assert_eq!(num("−7,6%").0, -0.076);
        assert_eq!(num("€ 50,00"), (50.0, "€ #,##0.00".into()));
        assert_eq!(num("1.000 €").0, 1000.0);
        assert_eq!(num("EUR 32.000,00").0, 32000.0);
    }

    #[test]
    fn codes_and_odd_numbers_stay_text() {
        for t in ["004127", "44012", "IT60X0542811101000000123456", "3.5", "1.23", "12.34.56", "1,2,3", "Via Roma 3", "", "12345678901234567", "a-b"] {
            if t == "44012" {
                assert!(matches!(value(t), Value::Number(..)));
                continue;
            }
            assert_eq!(value(t), Value::Text, "{t}");
        }
    }

    #[test]
    fn dates_become_dates() {
        assert_eq!(value("03/11/2026"), Value::Date(2026, 11, 3));
        assert_eq!(value("14-02-2026"), Value::Date(2026, 2, 14));
        assert_eq!(value("1.4.2025"), Value::Date(2025, 4, 1));
        assert_eq!(value("29/02/24"), Value::Date(2024, 2, 29));
        assert_eq!(value("31/02/2026"), Value::Text);
        assert_eq!(value("12/2026"), Value::Text);
    }

    #[test]
    fn sheet_names_are_valid_and_unique() {
        let mut used = Vec::new();
        assert_eq!(sheet_name("Tabella 1 · p. 1", &mut used), "Tabella 1 · p. 1");
        assert_eq!(sheet_name("Tabella 1 · p. 1", &mut used), "Tabella 1 · p. 1 (2)");
        let long = sheet_name("Un nome molto lungo: con [caratteri] vietati/strani", &mut used);
        assert!(long.chars().count() <= 31 && !long.contains(['[', ':', '/']));
    }
}
