#!/usr/bin/env python3
"""B8: cosa conservano «Dividi» e «Più piccolo» di Cedro di un PDF ricco (segnalibri, modulo, link, allegato, metadati Info e XMP, cifratura).
0 perdite silenziose: o si conserva o si avvisa (il messaggio stampato lo dice). Uso: b8_perdite.py   Scrive bench/risultati/b8_perdite.json
Unisci non si può chiamare dalla riga di comando con più file: coperto solo dal test di libreria (pdfpages.rs), non da questo script."""
import sys, tempfile, shutil
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import cedro, scrivi_json
import pymupdf as fitz, pikepdf
lav = Path(tempfile.mkdtemp(prefix="b8-"))
d = fitz.open()
for i in range(4):
    p = d.new_page(); p.insert_text((72, 72), f"Pagina {i+1} testo di prova")
    if i == 1:
        w = fitz.Widget(); w.field_type = fitz.PDF_WIDGET_TYPE_TEXT; w.field_name = "nome"; w.rect = fitz.Rect(72, 100, 300, 130); w.field_value = "Rossi"; p.add_widget(w)
    if i == 2: p.insert_link({"kind": fitz.LINK_URI, "from": fitz.Rect(72, 60, 200, 80), "uri": "https://example.org"})
d.set_toc([[1, "Intro", 1], [1, "Metodi", 2], [2, "Sotto", 3], [1, "Fine", 4]])
d.set_metadata({"title": "Titolo prova", "author": "Autore", "subject": "S", "keywords": "k"})
d.embfile_add("allegato.txt", b"dati allegati", filename="allegato.txt"); d.save(lav / "ricco.pdf")
with pikepdf.open(lav / "ricco.pdf") as p:
    with p.open_metadata() as m: m["dc:title"] = "Titolo XMP"
    p.save(lav / "ricco-xmp.pdf")

def stato(f):
    x = fitz.open(f)
    with pikepdf.open(f) as p: xmp = p.Root.get("/Metadata") is not None
    return {"pagine": len(x), "titolo": x.metadata.get("title") or "", "autore": x.metadata.get("author") or "", "xmp": xmp,
            "campi_modulo": sum(len(list(pg.widgets())) for pg in x), "link": sum(len(pg.get_links()) for pg in x), "allegati": len(x.embfile_names()), "segnalibri": len(x.get_toc())}

src = lav / "ricco-xmp.pdf"; res = {"originale": stato(src), "operazioni": {}}
for az in ("split", "compress", "strip"):
    r = cedro("tool", src, az)
    righe = [l for l in r["out"].splitlines() if l.strip()]
    usc = [Path(l) for l in righe if l.startswith("./") or l.startswith("/")]
    nuovi = sorted(lav.glob("ricco-xmp-*.pdf"), key=lambda p: p.name)
    risul = [n for n in nuovi if az == "split" and "-parte-" in n.name or az == "compress" and "compresso" in n.name or az == "strip" and "anonimo" in n.name]
    res["operazioni"][az] = {"messaggio": righe[0] if righe else r["err"][:200], "rc": r["rc"], "uscite": {n.name: stato(n) for n in risul}}
cif = lav / "cifrato.pdf"
with pikepdf.open(src) as p: p.save(cif, encryption=pikepdf.Encryption(user="u", owner="o"))
r = cedro("tool", cif, "split"); res["operazioni"]["split_cifrato"] = {"rc": r["rc"], "messaggio": (r["err"] or r["out"]).strip()[:200]}
scrivi_json("b8_perdite.json", res); shutil.rmtree(lav, ignore_errors=True)
import json; print(json.dumps({k: (v if k == "originale" else {a: {"messaggio": b["messaggio"], "uscite": list(b.get("uscite", {}).values())[:2]} for a, b in v.items()}) for k, v in res.items()}, indent=1, ensure_ascii=False)[:3500])
