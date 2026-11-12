//! Test di revisione: ogni test asserisce il comportamento CORRETTO; quelli che falliscono sono difetti.
use crate::pacchetto::*;
use crate::tabella::*;
use crate::textconv::{parse_csv, write_csv};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;

fn xlsx_con_foglio(sheet_xml: &str, styles: Option<&str>) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("xl/workbook.xml", o).unwrap();
        z.write_all(br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#).unwrap();
        z.start_file("xl/_rels/workbook.xml.rels", o).unwrap();
        z.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="x" Target="worksheets/sheet1.xml"/></Relationships>"#).unwrap();
        if let Some(s) = styles {
            z.start_file("xl/styles.xml", o).unwrap();
            z.write_all(s.as_bytes()).unwrap();
        }
        z.start_file("xl/worksheets/sheet1.xml", o).unwrap();
        z.write_all(format!(r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{sheet_xml}</sheetData></worksheet>"#).as_bytes()).unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

// ---------------------------------------------------------------- tabella

/// T1: CSV italiano senza intestazione, solo numeri con la virgola: il separatore deve restare ';'.
#[test]
fn t01_separatore_csv_italiano_senza_intestazione() {
    let csv = "0,512;0,733\n0,123;0,456\n1,5;2,5\n";
    assert_eq!(separatore(csv, "csv"), ';');
}

/// T2: CSV USA con migliaia tra virgolette e nessun decimale: 1,234 NON deve diventare 1.234.
#[test]
fn t02_migliaia_usa_non_diventano_decimali() {
    let csv = "Count,Sample\n\"1,234\",S1\n\"5,678\",S2\n\"12,000\",S3\n";
    let rows = parse_csv(csv, separatore(csv, "csv"));
    let virgola = decimali_con_virgola(&rows);
    let c = celle(&rows, virgola);
    assert_ne!(c[1][0], Cella::Numero(1.234), "1,234 (milleduecento) letto come 1,234 decimale");
}

/// T3: una cella più lunga di 32767 caratteri (sequenze, note) non deve far fallire tutto l'Excel.
#[test]
fn t03_cella_oltre_32767_caratteri() {
    let rows = vec![vec!["seq".to_string()], vec!["ACGT".repeat(9000)]];
    let r = in_excel(&rows, true);
    assert!(r.is_ok(), "in_excel fallisce: {:?}", r.err());
}

/// T4: formato di durata «[h]:mm» (tempi di incubazione) letto come numero, e 36 ore diventano 12:00.
#[test]
fn t04_durate_oltre_24_ore() {
    let styles = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="[h]:mm"/></numFmts><cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="164"/><xf numFmtId="46"/></cellXfs></styleSheet>"#;
    let x = xlsx_con_foglio(r#"<row r="1"><c r="A1" s="1"><v>1.5</v></c><c r="B1" s="2"><v>1.5</v></c></row>"#, Some(styles));
    let f = &leggi("xlsx", &x).unwrap()[0];
    let a = f.righe[0][0].testo(false);
    let b = f.righe[0][1].testo(false);
    assert_eq!((a.as_str(), b.as_str()), ("36:00", "36:00:00"), "[h]:mm → {a:?}, [h]:mm:ss (46) → {b:?}");
}

/// T5: xlsx → CSV: il rumore binario dei float (0.1+0.2) deve uscire come lo mostra Excel (15 cifre).
#[test]
fn t05_float_come_in_excel() {
    let x = xlsx_con_foglio(r#"<row r="1"><c r="A1"><v>0.30000000000000004</v></c><c r="B1"><v>1.0000000000000002</v></c></row>"#, None);
    let f = &leggi("xlsx", &x).unwrap()[0];
    assert_eq!(f.testo(true)[0], vec!["0,3".to_string(), "1".to_string()]);
}

/// T6: CSV → Excel: «1,50» (cifre significative di una misura) si vede con due decimali in Excel.
#[test]
fn t06_cifre_significative_tornano() {
    let rows = vec![vec!["Ct".to_string()], vec!["1,50".to_string()], vec!["23,10".to_string()]];
    let (x, _) = in_excel(&rows, true).unwrap();
    let mut z = zip::ZipArchive::new(Cursor::new(x)).unwrap();
    let mut stili = String::new();
    z.by_name("xl/styles.xml").unwrap().read_to_string(&mut stili).unwrap();
    assert!(stili.contains("formatCode=\"0.00\""), "{stili}");
}

/// T7: il «+» davanti (telefoni, codici «+1») si perde: Excel non lo vede più.
#[test]
fn t07_piu_davanti_resta() {
    assert_eq!(numero("+393331234567", false), None);
}

/// T8: xlsx → CSV: un testo che inizia con = + - @ esce nudo e Excel, riaprendo il CSV, lo esegue come formula.
#[test]
fn t08_csv_injection_in_uscita() {
    let x = xlsx_con_foglio(r#"<row r="1"><c r="A1" t="inlineStr"><is><t>=HYPERLINK("http://x","clic")</t></is></c><c r="B1" t="inlineStr"><is><t>-ACTB</t></is></c></row>"#, None);
    let f = &leggi("xlsx", &x).unwrap()[0];
    let out = write_csv(&f.testo(true), ';');
    // Excel toglie le virgolette e poi interpreta comunque =… e -… come formule.
    let campi = parse_csv(&out, ';');
    // = e @ vanno protetti; «-ACTB» resta com'è (un trattino davanti è comune nei dati).
    assert!(!campi[0].iter().any(|c| c.starts_with(['=', '@'])), "CSV scritto: {out:?}");
}

/// T9: una formula senza valore in cache (file scritti da script/librerie) sparisce senza avviso.
#[test]
fn t09_formula_senza_valore_non_sparisce() {
    let x = xlsx_con_foglio(r#"<row r="1"><c r="A1"><v>2</v></c><c r="B1"><f>A1*2</f></c></row>"#, None);
    let f = &leggi("xlsx", &x).unwrap()[0];
    assert!(f.righe[0].len() == 2 && f.righe[0][1] != Cella::Vuota, "{:?}", f.righe[0]);
}

/// T11: numeri lunghi con decimali (> 15 cifre intere) perdono precisione invece di restare testo.
#[test]
fn t11_numero_lungo_con_decimali() {
    assert_eq!(numero("12345678901234567,5", true), None);
}

/// T12: ODS: l'a capo dentro la cella (text:line-break) si perde.
#[test]
fn t12_ods_a_capo() {
    let content = r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet><table:table table:name="T"><table:table-row><table:table-cell office:value-type="string"><text:p>riga1<text:line-break/>riga2</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#;
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        z.start_file("content.xml", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(content.as_bytes()).unwrap();
        z.finish().unwrap();
    }
    let f = leggi("ods", buf.get_ref()).unwrap();
    assert_eq!(f[0].righe[0][0], Cella::Testo("riga1\nriga2".into()));
}

/// T13: UTF-16 LE senza BOM (alcuni strumenti) diventa testo con NUL e lettere sparse.
#[test]
fn t13_utf16_senza_bom() {
    let bytes: Vec<u8> = "a;b\n1;2\n".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    assert_eq!(crate::engine::decode_text(&bytes), "a;b\n1;2\n");
}

/// T14: campi tra virgolette su più righe confondono il separatore (righe di continuazione contate da sole).
#[test]
fn t14_separatore_con_campi_multiriga() {
    let csv = "1,\"nota\ncon; punto; e virgola\"\n2,\"altra\nnota; lunga; qui\"\n";
    assert_eq!(separatore(csv, "csv"), ',');
}

/// T15 (controllo, deve passare): righe oltre il limite di Excel vanno su più fogli con l'intestazione ripetuta e nulla si perde.
#[test]
fn t15_oltre_un_milione_di_righe() {
    let n = 1_048_576 + 10;
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(n + 1);
    rows.push(vec!["id".into()]);
    for i in 0..n {
        rows.push(vec![format!("{}", i + 1)]);
    }
    let (x, frase) = in_excel(&rows, true).unwrap();
    let fogli = leggi("xlsx", &x).unwrap();
    assert_eq!(fogli.len(), 2, "{frase}");
    let dati: usize = fogli.iter().map(|f| f.righe.len() - 1).sum();
    assert_eq!(dati, n);
    assert_eq!(fogli[1].righe[0][0], Cella::Testo("id".into()));
}

/// T16 (controllo): geni, codici con zeri, ID lunghi restano testo e tornano uguali.
#[test]
fn t16_geni_codici_round_trip() {
    let rows: Vec<Vec<String>> = vec![
        vec!["Gene".into(), "ID".into(), "Barcode".into(), "Exp".into()],
        vec!["SEPT2".into(), "007".into(), "1234567890123456789".into(), "1,5E-03".into()],
        vec!["MARCH1".into(), "0012".into(), "9780306406157".into(), "2E-05".into()],
    ];
    let (x, _) = in_excel(&rows, true).unwrap();
    let f = &leggi("xlsx", &x).unwrap()[0];
    let t = f.testo(true);
    assert_eq!(t[1][0], "SEPT2");
    assert_eq!(t[1][1], "007");
    assert_eq!(t[1][2], "1234567890123456789");
    assert_eq!(t[2][1], "0012");
}

// ---------------------------------------------------------------- pacchetto

fn cartella(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rev-pacchetto-{nome}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// P1: un bit cambiato DENTRO lo ZIP (copia rovinata) deve dare «Pacchetto NON integro: Cambiati …»,
/// non un errore tecnico che ferma la verifica degli altri file.
#[test]
fn p01_bit_cambiato_nello_zip() {
    let d = cartella("bit");
    fs::write(d.join("a.png"), b"PNGDATA-UNICO-0123456789").unwrap();
    fs::write(d.join("b.png"), b"altro file").unwrap();
    let z = d.join("p.zip");
    crea(&[d.join("a.png"), d.join("b.png")], &z, "p", &|_| {}).unwrap();
    let mut bytes = fs::read(&z).unwrap();
    let pos = bytes.windows(13).position(|w| w == b"PNGDATA-UNICO").unwrap();
    bytes[pos] ^= 1;
    fs::write(&z, &bytes).unwrap();
    let v = verifica(&z, &|_| {});
    match v {
        Ok(v) => assert!(!v.tutto_bene() && v.diversi == vec!["data/a.png"] && v.integri == 1, "{v:?}"),
        Err(e) => panic!("verifica interrotta con errore: {e}"),
    }
}

/// P2: il manifesto può nominare file fuori da data/ o lo stesso file due volte: vengono contati come «integri».
#[test]
fn p02_manifesto_fuori_da_data_o_doppio() {
    use sha2::{Digest, Sha256};
    let d = cartella("fuori");
    let z = d.join("x.zip");
    let h = |b: &[u8]| Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect::<String>();
    {
        let mut zw = zip::ZipWriter::new(fs::File::create(&z).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        zw.start_file("p/bagit.txt", o).unwrap();
        zw.write_all(b"BagIt-Version: 1.0\n").unwrap();
        zw.start_file("p/manifest-sha256.txt", o).unwrap();
        zw.write_all(format!("{}  data/a.txt\n{}  data/a.txt\n{}  bagit.txt\n", h(b"a"), h(b"a"), h(b"BagIt-Version: 1.0\n")).as_bytes()).unwrap();
        zw.start_file("p/data/a.txt", o).unwrap();
        zw.write_all(b"a").unwrap();
        zw.finish().unwrap();
    }
    let v = verifica(&z, &|_| {}).unwrap();
    assert_eq!(v.integri, 1, "{}", v.messaggio());
}

/// P3: il tagmanifest non viene controllato: manifest e dato cambiati insieme passano come «integro».
#[test]
fn p03_tagmanifest_ignorato() {
    let d = cartella("tag");
    fs::write(d.join("m.csv"), "A1;0,52\n").unwrap();
    let z = d.join("p.zip");
    crea(&[d.join("m.csv")], &z, "p", &|_| {}).unwrap();
    // Ricostruisce lo ZIP cambiando m.csv e la sua riga nel manifesto; tagmanifest e bag-info restano.
    use sha2::{Digest, Sha256};
    let mut src = zip::ZipArchive::new(fs::File::open(&z).unwrap()).unwrap();
    let z2 = d.join("p2.zip");
    let mut zw = zip::ZipWriter::new(fs::File::create(&z2).unwrap());
    let nuovo = b"A1;9,99\n";
    let hn: String = Sha256::digest(nuovo).iter().map(|x| format!("{x:02x}")).collect();
    for i in 0..src.len() {
        let mut e = src.by_index(i).unwrap();
        let name = e.name().unwrap().to_string();
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).unwrap();
        if name == "p/data/m.csv" {
            buf = nuovo.to_vec();
        } else if name == "p/manifest-sha256.txt" {
            buf = format!("{hn}  data/m.csv\n").into_bytes();
        }
        zw.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(&buf).unwrap();
    }
    zw.finish().unwrap();
    let v = verifica(&z2, &|_| {}).unwrap();
    assert!(!v.tutto_bene(), "manomissione non vista: {}", v.messaggio());
}

/// P4: un collegamento simbolico a una cartella viene saltato in silenzio (i dati non entrano nel pacchetto).
#[cfg(unix)]
#[test]
fn p04_symlink_a_cartella_saltato() {
    let d = cartella("link");
    fs::create_dir_all(d.join("vere")).unwrap();
    fs::write(d.join("vere/x.csv"), "1").unwrap();
    fs::create_dir_all(d.join("esp")).unwrap();
    fs::write(d.join("esp/y.csv"), "2").unwrap();
    std::os::unix::fs::symlink(d.join("vere"), d.join("esp/misure")).unwrap();
    let z = d.join("p.zip");
    let r = crea(&[d.join("esp")], &z, "p", &|_| {});
    match r {
        Ok(f) => assert_eq!(f.file, 2, "x.csv (dietro il link) mancante e nessun avviso"),
        Err(_) => {} // un errore chiaro andrebbe bene
    }
}

/// P5 (controllo): nomi con %, a capo, \r, spazi e byte non UTF-8 fanno il giro crea → verifica.
#[cfg(unix)]
#[test]
fn p05_nomi_strani_round_trip() {
    use std::os::unix::ffi::OsStrExt;
    let d = cartella("nomi");
    let dir = d.join("dati");
    fs::create_dir_all(&dir).unwrap();
    for n in ["100%.csv", "a\nb.txt", "c\rd.txt", " spazio .txt", "%0A.txt", "x\\y.txt"] {
        fs::write(dir.join(n), n).unwrap();
    }
    fs::write(dir.join(std::ffi::OsStr::from_bytes(b"lat\xe0.txt")), "x").unwrap();
    fs::write(dir.join(std::ffi::OsStr::from_bytes(b"lat\xe8.txt")), "y").unwrap();
    let z = d.join("p.zip");
    let f = crea(&[dir], &z, "p", &|_| {}).unwrap();
    assert_eq!(f.file, 8);
    let v = verifica(&z, &|_| {}).unwrap();
    assert!(v.tutto_bene(), "{}", v.messaggio());
}

/// P6: un file «dati» e una cartella «dati» danno nello ZIP «data/dati» e «data/dati/x»: non estraibile.
#[test]
fn p06_file_e_cartella_con_lo_stesso_nome() {
    let d = cartella("conflitto");
    fs::create_dir_all(d.join("a/dati")).unwrap();
    fs::write(d.join("a/dati/x.csv"), "1").unwrap();
    fs::create_dir_all(d.join("b")).unwrap();
    fs::write(d.join("b/dati"), "file").unwrap();
    let z = d.join("p.zip");
    crea(&[d.join("a/dati"), d.join("b/dati")], &z, "p", &|_| {}).unwrap();
    let za = zip::ZipArchive::new(fs::File::open(&z).unwrap()).unwrap();
    let nomi: Vec<String> = za.file_names().map(|n| n.unwrap().into_owned()).collect();
    let file = nomi.iter().any(|n| n == "p/data/dati");
    let dentro = nomi.iter().any(|n| n.starts_with("p/data/dati/"));
    assert!(!(file && dentro), "{nomi:?}");
}

/// Prova dal vero: in un CSV americano con decimali a punto, «1,234» e «2,000» sono migliaia.
#[test]
fn t17_migliaia_con_decimali_a_punto_sicuri() {
    let csv = "Count,OD\n\"1,234\",0.5\n\"2,000\",-3.25\n";
    let rows = parse_csv(csv, separatore(csv, "csv"));
    let (x, _) = in_excel(&rows, true).unwrap();
    let f = &leggi("xlsx", &x).unwrap()[0];
    assert_eq!(f.righe[1][0], Cella::Numero(1234.0));
    assert_eq!(f.righe[2][0], Cella::Numero(2000.0));
    // Senza decimali nel file resta ambiguo: testo.
    let solo = vec![vec!["n".to_string()], vec!["1,234".to_string()]];
    let (x, _) = in_excel(&solo, true).unwrap();
    assert_eq!(leggi("xlsx", &x).unwrap()[0].righe[1][0], Cella::Testo("1,234".into()));
}
