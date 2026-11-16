#!/usr/bin/env python3
"""B4: PDF -> Word, andata e ritorno. Uso: b4_pdf_word.py <cartella-b2> [--max N]
<cartella-b2> contiene in/ (DOCX originali) e ref/ (PDF di LibreOffice) come lasciati da b2_word_pdf.py.
Per ogni DOCX con almeno un titolo o una tabella e PDF di 1-10 pagine: PDF -> DOCX con Cedro, pdf2docx e LibreOffice (writer_pdf_import);
metriche rispetto al DOCX originale: F1 del testo, F1 dei testi di cella delle tabelle, differenza nel numero di tabelle e di titoli,
e SSIM della resa (DOCX convertito -> PDF con LibreOffice, contro il PDF di partenza). Scrive bench/risultati/b4_pdf_word.json."""
import shutil, statistics, sys, tempfile
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import cedro, esegui, f1_parole, mediana, normalizza, pagine_gri, scrivi_json, ssim_pagina

import docx, fitz
b2 = Path(sys.argv[1]).resolve()
mx = int(sys.argv[sys.argv.index("--max") + 1]) if "--max" in sys.argv else 10**9
prof = Path(tempfile.gettempdir()) / "lo-bench-profile"
base = ["soffice", f"-env:UserInstallation=file://{prof}", "--headless"]


def struttura(path):
    d = docx.Document(str(path))
    testo = [p.text for p in d.paragraphs]
    celle = Counter()
    for t in d.tables:
        for r in t.rows:
            for c in r.cells:
                n = normalizza(c.text)
                if n: celle[n] += 1
                testo.append(c.text)
    titoli = sum(1 for p in d.paragraphs if p.style is not None and p.style.name.lower().startswith(("heading", "titolo", "title")))
    return "\n".join(testo), len(d.tables), titoli, celle


def f1_celle(a: Counter, b: Counter):
    if not a and not b: return 1.0
    com = sum((a & b).values()); p = com / max(1, sum(b.values())); r = com / max(1, sum(a.values()))
    return 0.0 if p + r == 0 else round(2 * p * r / (p + r), 4)


cand = []
for d in sorted((b2 / "in").glob("*.docx")):
    ref = b2 / "ref" / (d.stem + ".pdf")
    if not ref.exists(): continue
    try:
        with fitz.open(ref) as pdf: n = len(pdf)
        t, nt, nh, _ = struttura(d)
    except Exception:
        continue
    if 1 <= n <= 10 and (nt or nh) and len(t) > 200: cand.append((d, ref))
cand = cand[:mx]
lav = Path(tempfile.mkdtemp(prefix="b4-"))
motori = ["cedro", "pdf2docx", "libreoffice"]
righe = []
for d, ref in cand:
    tv, nt0, nh0, celle0 = struttura(d)
    riga = {"file": d.name, "tabelle_orig": nt0, "titoli_orig": nh0}
    for m in motori:
        o = lav / m; o.mkdir(exist_ok=True)
        src = o / (d.stem + ".pdf"); shutil.copy(ref, src)
        out = o / (d.stem + ".docx")
        try:
            if m == "cedro":
                r = cedro("convert", src, "docx", timeout=300)
                if r["rc"] == 0 and r["out"].strip(): out = Path(r["out"].strip().splitlines()[-1])
                else: raise RuntimeError(r["err"][:100])
            elif m == "pdf2docx":
                from pdf2docx import Converter
                import time; t0 = time.perf_counter(); c = Converter(str(src)); c.convert(str(out)); c.close(); r = {"s": round(time.perf_counter() - t0, 3), "rss_mb": None}
            else:
                r = esegui(base + ["--infilter=writer_pdf_import", "--convert-to", "docx:MS Word 2007 XML", "--outdir", str(o), str(src)], timeout=300)
            t, nt, nh, celle = struttura(out)
            res = {"s": r["s"], "f1_testo": f1_parole(tv, t)[0], "f1_celle": f1_celle(celle0, celle), "d_tabelle": nt - nt0, "d_titoli": nh - nh0}
            # resa: DOCX -> PDF (LibreOffice) contro il PDF di partenza
            rr = lav / "rend" / m; rr.mkdir(parents=True, exist_ok=True)
            esegui(base + ["--convert-to", "pdf", "--outdir", str(rr), str(out)], timeout=300)
            pdfr = rr / (out.stem + ".pdf")
            if pdfr.exists():
                a, b = pagine_gri(ref), pagine_gri(pdfr)
                res["pagine"] = [len(a), len(b)]
                ss = [ssim_pagina(a[i], b[i]) for i in range(min(len(a), len(b)))]
                res["ssim"] = round(statistics.median(ss), 4) if ss else None
            riga[m] = res
        except Exception as e:
            riga[m] = {"errore": str(e)[:120]}
    righe.append(riga)

def agg(m, k):
    v = [x[m][k] for x in righe if k in x.get(m, {}) and x[m][k] is not None]
    return round(statistics.median(v), 4) if v else None

res = {"file": len(righe), "motori": {m: {"f1_testo": agg(m, "f1_testo"), "f1_celle": agg(m, "f1_celle"), "ssim": agg(m, "ssim"), "s_mediano": agg(m, "s"),
       "falliti": sum("errore" in x.get(m, {}) for x in righe)} for m in motori}, "righe": righe}
scrivi_json("b4_pdf_word.json", res)
print(res["motori"])
