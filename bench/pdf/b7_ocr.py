#!/usr/bin/env python3
"""B7: OCR su pagine italiane di testo NOTO. Cedro (ocrs) contro Tesseract (lingua ita). Seme 42.
Uso: b7_ocr.py [--pagine N]   Scrive bench/risultati/b7_ocr.json
Le pagine si generano da testo vero (README e docs del repository + paragrafi di laboratorio scritti per il banco) con font diversi,
corpo 9-12 pt a 150 dpi, rotazione ±2°, rumore gaussiano, JPEG q50 su metà delle pagine."""
import random, re, statistics, sys, tempfile
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageFilter
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import RADICE, cedro, esegui, normalizza, scrivi_json, SEED
import jiwer
random.seed(SEED); np.random.seed(SEED)
N = int(sys.argv[sys.argv.index("--pagine") + 1]) if "--pagine" in sys.argv else 60
DIFF = "--difficile" in sys.argv   # scansione brutta: 105 dpi, sfocata, rumorosa, storta fino a 4°, JPEG q35

LAB = ["Il preparato mostra una proliferazione neoplastica epiteliale a architettura tubulo-papillare, con atipie nucleari di grado intermedio e indice mitotico di 6 mitosi per 10 campi ad alto ingrandimento. L'immunoistochimica evidenzia positività diffusa per citocheratina 7 e negatività per TTF-1; il Ki-67 è pari al 25%.",
       "Campione bovino, femmina di 7 anni: biopsia cutanea con infiltrato linfoplasmocitario perivascolare e iperplasia epidermica. Colorazione con ematossilina-eosina, ingrandimento 20x. Non si osservano granulomi né strutture fungine alla colorazione PAS. Si consiglia il confronto con la coltura microbiologica.",
       "Protocollo: i vetrini vengono scansionati a 40x con risoluzione di 0,25 micrometri per pixel, salvati in formato SVS e archiviati con impronta SHA-256. L'etichetta del vetrino viene rimossa prima dell'invio al centro di consulto, così da non trasmettere nome del paziente o codice interno."]
def paragrafi():
    out = []
    for f in [RADICE / "README.md", RADICE / "docs/RILASCI.md"]:
        t = f.read_text(errors="replace")
        t = re.sub(r"<[^>]+>|!\[[^\]]*\]\([^)]*\)|\[([^\]]*)\]\([^)]*\)", r"\1", t)
        for p in re.split(r"\n\s*\n", t):
            p = re.sub(r"[#*`|>_]", "", p).strip()
            p = re.sub(r"\s+", " ", p)
            if 200 < len(p) < 1200 and sum(c.isalpha() for c in p) / len(p) > 0.75: out.append(p)
    return out + LAB * 5
P = paragrafi(); random.shuffle(P)
FONTI = [f for f in ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf", "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf", "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf"] if Path(f).exists()]
assert FONTI, "nessun font trovato"
def wrap(d, text, font, w):
    righe, cur = [], ""
    for parola in text.split():
        t = (cur + " " + parola).strip()
        if d.textlength(t, font=font) <= w: cur = t
        else: righe.append(cur); cur = parola
    return righe + [cur]
lav = Path(tempfile.mkdtemp(prefix="b7-")); righe_out = []
for i in range(N):
    pt = random.choice([9, 10, 11, 12]); px = int(pt * 150 / 72)
    font = ImageFont.truetype(random.choice(FONTI), px)
    img = Image.new("L", (1240, 1754), 255); d = ImageDraw.Draw(img)
    testo, y = [], 120
    while y < 1600 and P:
        p = P[(i * 7 + len(testo)) % len(P)]
        for r in wrap(d, p, font, 1000):
            if y > 1600: break
            d.text((120, y), r, font=font, fill=20); y += int(px * 1.35); testo.append(r)
        y += px
    img = img.rotate(random.uniform(-4, 4) if DIFF else random.uniform(-2, 2), fillcolor=255, resample=Image.BICUBIC)
    if DIFF: img = img.resize((868, 1228), Image.BILINEAR).filter(ImageFilter.GaussianBlur(1.0))
    a = np.array(img).astype(np.float32) + np.random.normal(0, 16 if DIFF else random.choice([2, 6, 12]), (img.height, img.width)); img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
    if random.random() < 0.5: img = img.filter(ImageFilter.GaussianBlur(0.6))
    p = lav / f"p{i:03d}.jpg"; img.save(p, quality=(35 if DIFF else 50) if i % 2 else (60 if DIFF else 90))
    verita = " ".join(testo)
    c = cedro("tool", p, "ocr", timeout=300)
    cedro_txt = ""
    if c["rc"] == 0 and c["out"].strip():
        o = Path(c["out"].strip().splitlines()[-1])
        if o.exists(): cedro_txt = o.read_text(errors="replace")
    t = esegui(["tesseract", str(p), "stdout", "-l", "ita", "--psm", "3"], timeout=300)
    righe_out.append({"pagina": p.name, "corpo_pt": pt, "jpeg_q": (35 if DIFF else 50) if i % 2 else (60 if DIFF else 90),
                      "cedro": {"cer": round(jiwer.cer(normalizza(verita), normalizza(cedro_txt)), 4), "wer": round(jiwer.wer(normalizza(verita), normalizza(cedro_txt)), 4), "s": c["s"], "rss_mb": c["rss_mb"]},
                      "tesseract": {"cer": round(jiwer.cer(normalizza(verita), normalizza(t["out"])), 4), "wer": round(jiwer.wer(normalizza(verita), normalizza(t["out"])), 4), "s": t["s"], "rss_mb": t["rss_mb"]}})
def agg(m, k): return round(statistics.median(x[m][k] for x in righe_out), 4)
res = {"pagine": N, "cedro": {k: agg("cedro", k) for k in ("cer", "wer", "s")}, "tesseract": {k: agg("tesseract", k) for k in ("cer", "wer", "s")},
       "per_corpo": {pt: {m: round(statistics.median(x[m]["cer"] for x in righe_out if x["corpo_pt"] == pt), 4) for m in ("cedro", "tesseract")} for pt in (9, 10, 11, 12)}, "righe": righe_out}
scrivi_json("b7_ocr_difficile.json" if DIFF else "b7_ocr.json", res)
print({k: v for k, v in res.items() if k != "righe"})
