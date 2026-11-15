#!/usr/bin/env python3
"""B5: i DOCX/XLSX che scrive Cedro rispettano gli schemi ECMA-376 e si riaprono negli altri programmi?
Uso: b5_ooxml.py <cartella-b2> [--poi-lib CARTELLA_JAR] [--poi-bin CLASSI]   (<cartella-b2>: in/ e ref/ come lasciati da b2_word_pdf.py)
Produce con Cedro: PDF -> DOCX, PDF -> XLSX (tabelle), CSV -> XLSX, DOCX -> DOCX/ODT; poi:
  1. validazione XSD ECMA-376 (XmlBeans/Apache POI, `Ooxml.java`)
  2. riapertura con python-docx / openpyxl
  3. riapertura con LibreOffice (conversione in PDF senza errori)
Scrive bench/risultati/b5_ooxml.json."""
import csv, os, shutil, sys, tempfile
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import cedro, esegui, scrivi_json, soffice_pdf
b2 = Path(sys.argv[1]).resolve(); a = sys.argv[2:]
lib = a[a.index("--poi-lib") + 1] if "--poi-lib" in a else os.environ.get("POI_LIB", "")
binc = a[a.index("--poi-bin") + 1] if "--poi-bin" in a else os.environ.get("POI_BIN", "/tmp/poibin")
lav = Path(tempfile.mkdtemp(prefix="b5-")); prodotti = []
for ref in sorted((b2 / "ref").glob("*.pdf"))[:60]:
    for az, est in (("docx", ".docx"), ("xlsx", ".xlsx")):
        d = lav / az; d.mkdir(exist_ok=True); s = d / ref.name; shutil.copy(ref, s)
        r = cedro("convert", s, az, timeout=300)
        if r["rc"] == 0 and r["out"].strip():
            o = Path(r["out"].strip().splitlines()[-1])
            if o.exists(): prodotti.append(("pdf->" + az, o))
# CSV -> XLSX con numeri, virgole decimali, accenti, date
c = lav / "csv"; c.mkdir()
with open(c / "dati.csv", "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f, delimiter=";"); w.writerow(["campione", "peso_g", "data", "nota"])
    for i in range(300): w.writerow([f"C{i:04d}", f"{i*1.37:.2f}".replace(".", ","), f"2026-11-{i%28+1:02d}", "perché però è così"])
r = cedro("convert", c / "dati.csv", "xlsx")
if r["rc"] == 0: prodotti.append(("csv->xlsx", Path(r["out"].strip().splitlines()[-1])))
righe = []
rows = {}
if lib:
    files = [str(p) for _, p in prodotti]
    o = esegui(["java", "-cp", f"{lib}/*:{binc}", "Ooxml"] + files, timeout=900)
    for l in o["out"].splitlines():
        t = l.split("\t")
        if len(t) == 4: rows[t[0]] = (int(t[1]), int(t[2]), t[3])
for tipo, p in prodotti:
    riga = {"tipo": tipo, "file": p.name}
    if p.name in rows: riga["parti_xsd"], riga["errori_xsd"], riga["primo_errore"] = rows[p.name]
    try:
        if p.suffix == ".docx":
            import docx; docx.Document(str(p)); riga["python_docx"] = True
        else:
            import openpyxl; openpyxl.load_workbook(str(p)); riga["openpyxl"] = True
    except Exception as e:
        riga["riapertura_errore"] = str(e)[:100]
    righe.append(riga)
# riapertura in LibreOffice su un campione
camp = prodotti[:: max(1, len(prodotti) // 20)]
lo_ok = 0; lo_ko = []
for _, p in camp:
    r = soffice_pdf(p, lav / "lo", timeout=120)
    if (lav / "lo" / (p.stem + ".pdf")).exists(): lo_ok += 1
    else: lo_ko.append(p.name)
res = {"file": len(righe), "errori_xsd_totali": sum(x.get("errori_xsd", 0) for x in righe if x.get("errori_xsd", 0) > 0), "file_non_validi_xsd": [x["file"] for x in righe if x.get("errori_xsd", 0) != 0 and "errori_xsd" in x],
       "riaperti_python": sum(1 for x in righe if x.get("python_docx") or x.get("openpyxl")), "libreoffice_campione": f"{lo_ok}/{len(camp)}", "libreoffice_falliti": lo_ko, "righe": righe}
scrivi_json("b5_ooxml.json", res)
print({k: v for k, v in res.items() if k != "righe"})
