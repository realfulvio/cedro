#!/usr/bin/env python3
"""B1: validità dei PDF. Uso: b1_validita.py <cartella-o-file.pdf>...
qpdf --check (struttura ISO 32000-1) + veraPDF -f 2b (si legge? struttura 6.1.x; vedi emendamento 1 del protocollo) + font non incorporati / senza ToUnicode (pikepdf)."""
import sys, json, subprocess, os
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import esegui, scrivi_json
VERA = os.environ.get("VERAPDF_LIB", "/tmp/claude-0/-home-user/5e2c466c-99f5-553e-aa36-5bd8c1ce695b/scratchpad/vera/lib")

def vera(pdf):
    cp = f"{VERA}/*"
    # -f 2b: con -f 0 veraPDF sceglie PDF/A-1b e boccia ogni PDF che non lo dichiara; qui conta solo se il file SI LEGGE
    # (failedToParse) e quali clausole di struttura (6.1.x) sono violate; le altre clausole PDF/A sono informative.
    return esegui(["java", "-cp", cp, "org.verapdf.apps.GreenfieldCliWrapper", "-f", "2b", "--format", "xml", str(pdf)], timeout=120)

def font_info(pdf):
    import pikepdf
    nonincorporati, senza_tounicode, n = 0, 0, 0
    try:
        with pikepdf.open(pdf) as d:
            for pg in d.pages:
                res = pg.get("/Resources", {})
                for _, f in (res.get("/Font", {}) or {}).items():
                    n += 1
                    desc = f.get("/FontDescriptor")
                    if f.get("/Subtype") == "/Type0":
                        desc = f["/DescendantFonts"][0].get("/FontDescriptor")
                    base14 = str(f.get("/BaseFont", "")).lstrip("/") in ("Helvetica", "Times-Roman", "Courier", "Helvetica-Bold", "Times-Bold", "Courier-Bold")
                    if desc is None or not any(k in desc for k in ("/FontFile", "/FontFile2", "/FontFile3")):
                        if not base14: nonincorporati += 1
                    if "/ToUnicode" not in f and f.get("/Subtype") != "/Type3" and not base14 and "/Encoding" not in f:
                        senza_tounicode += 1
    except Exception as e:
        return {"errore": str(e)[:120]}
    return {"font": n, "non_incorporati": nonincorporati, "senza_tounicode": senza_tounicode}

def main():
    files = []
    for a in sys.argv[1:]:
        p = Path(a)
        files += sorted(p.rglob("*.pdf")) if p.is_dir() else [p]
    righe, ko = [], 0
    for f in files:
        q = esegui(["qpdf", "--check", str(f)], timeout=60)
        v = vera(f)
        import re
        x = v["out"]
        parse_ko = bool(re.search(r'failedToParse="[1-9]', x)) or "<batchSummary" not in x
        struct = sorted(set(re.findall(r'clause="(6\.1\.[0-9.]+)"', x)))
        v_ok = not parse_ko
        riga = {"file": f.name, "qpdf_rc": q["rc"], "qpdf": (q["out"] + q["err"]).strip().splitlines()[-1:][0] if (q["out"] + q["err"]).strip() else "", "vera_letto": v_ok, "vera_clausole_struttura": struct, **font_info(f)}
        riga["ok"] = q["rc"] in (0, 3) and v_ok   # qpdf 3 = solo avvisi
        ko += not riga["ok"]
        righe.append(riga)
    r = {"B1": {"file": len(files), "non_validi": ko, "righe": righe}}
    scrivi_json("b1_validita.json", r)
    print(f"B1: {len(files)} PDF, {ko} non validi")
    for x in righe:
        if not x["ok"]: print("  ✗", x["file"], x["qpdf"], x["vera_letto"])

if __name__ == "__main__":
    main()
