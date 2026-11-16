#!/usr/bin/env python3
"""B2: Word/ODT/RTF/DOC -> PDF. Cedro contro il riferimento LibreOffice e contro il testo vero del documento.
Uso: b2_word_pdf.py <cartella-documenti> [--max N]
Metriche per file: pagine uguali al riferimento (sì/no), SSIM mediana a 72 dpi, F1 delle parole del PDF contro il testo del documento
(python-docx per i .docx, esportazione testo di LibreOffice per gli altri), tempo, RSS.
Scrive bench/risultati/b2_word_pdf.json.
"""
import shutil, statistics, subprocess, sys, tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import cedro, esegui, f1_parole, mediana, pagine_gri, scrivi_json, ssim_pagina, testo_pdf

sorg = Path(sys.argv[1]).resolve()
mx = int(sys.argv[sys.argv.index("--max") + 1]) if "--max" in sys.argv else 10**9
EST = {".doc", ".docx", ".odt", ".rtf", ".dotx", ".docm", ".ott"}
files = sorted(p for p in sorg.iterdir() if p.suffix.lower() in EST)[:mx]
lav = Path(tempfile.mkdtemp(prefix="b2-"))
(lav / "in").mkdir(); (lav / "ref").mkdir(); (lav / "txt").mkdir()
for f in files:
    shutil.copy(f, lav / "in" / f.name)
prof = Path(tempfile.gettempdir()) / "lo-bench-profile"
base = ["soffice", f"-env:UserInstallation=file://{prof}", "--headless"]
# riferimento: PDF e testo in blocco
esegui(base + ["--convert-to", "pdf", "--outdir", str(lav / "ref")] + [str(lav / "in" / f.name) for f in files], timeout=1800)
esegui(base + ["--convert-to", "txt:Text (encoded):UTF8", "--outdir", str(lav / "txt")] + [str(lav / "in" / f.name) for f in files], timeout=1800)


def testo_vero(f):
    if f.suffix.lower() in (".docx", ".docm"):
        try:
            import docx
            d = docx.Document(str(f))
            t = [p.text for p in d.paragraphs]
            for tb in d.tables:
                for r in tb.rows:
                    t += [c.text for c in r.cells]
            return "\n".join(t)
        except Exception:
            pass
    p = lav / "txt" / (f.stem + ".txt")
    return p.read_text(errors="replace") if p.exists() else None


righe = []
for f in files:
    src = lav / "in" / f.name
    ref = lav / "ref" / (f.stem + ".pdf")
    r = cedro("convert", src, "pdf", timeout=180)
    out = None
    if r["rc"] == 0 and r["out"].strip():
        out = Path(r["out"].strip().splitlines()[-1])
    riga = {"file": f.name, "rc": r["rc"], "s": r["s"], "rss_mb": r["rss_mb"], "errore": r["err"].strip()[:200] if r["rc"] else ""}
    if out and out.exists() and ref.exists():
        try:
            pc, pr = pagine_gri(out), pagine_gri(ref)
            riga["pagine_cedro"], riga["pagine_rif"] = len(pc), len(pr)
            n = min(len(pc), len(pr))
            ss = [ssim_pagina(pc[i], pr[i]) for i in range(n)]
            riga["ssim_mediana"] = round(statistics.median(ss), 4) if ss else None
            tv = testo_vero(src)
            if tv is not None:
                riga["f1_testo_cedro"] = f1_parole(tv, testo_pdf(out))[0]
                riga["f1_testo_libreoffice"] = f1_parole(tv, testo_pdf(ref))[0]
        except Exception as e:
            riga["errore"] = f"misura: {e}"[:200]
    elif r["rc"] == 0 and not ref.exists():
        riga["nota"] = "il riferimento LibreOffice non ha prodotto il PDF"
    righe.append(riga)

ok = [x for x in righe if x["rc"] == 0 and "pagine_rif" in x]
res = {
    "file": len(files), "convertiti_cedro": sum(x["rc"] == 0 for x in righe), "falliti_cedro": [x["file"] for x in righe if x["rc"] != 0],
    "con_riferimento": len(ok),
    "pagine_uguali_pct": round(100 * sum(x["pagine_cedro"] == x["pagine_rif"] for x in ok) / max(1, len(ok)), 1),
    "ssim_mediana_globale": mediana([x["ssim_mediana"] for x in ok if x.get("ssim_mediana") is not None]),
    "f1_testo_cedro_mediano": mediana([x["f1_testo_cedro"] for x in ok if "f1_testo_cedro" in x]),
    "f1_testo_libreoffice_mediano": mediana([x["f1_testo_libreoffice"] for x in ok if "f1_testo_libreoffice" in x]),
    "tempo_mediano_s": mediana([x["s"] for x in righe]), "rss_max_mb": max((x["rss_mb"] for x in righe), default=0),
    "righe": righe,
}
scrivi_json("b2_word_pdf.json", res)
print({k: v for k, v in res.items() if k != "righe"})
print("PDF di Cedro in", lav / "in")
