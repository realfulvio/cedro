#!/usr/bin/env python3
"""B6: «Più piccolo» (compress) di Cedro contro Ghostscript /ebook e /screen: peso e SSIM (72 dpi) rispetto all'originale.
Ingressi generati con seme 42: scansioni (pagine di testo come immagine JPEG), PDF con foto (texture 300 dpi), PDF misto testo+foto.
Uso: b6_comprimi.py   Scrive bench/risultati/b6_comprimi.json"""
import random, shutil, statistics, sys, tempfile
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageFilter
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import cedro, esegui, pagine_gri, ssim_pagina, scrivi_json, SEED
random.seed(SEED); np.random.seed(SEED)
lav = Path(tempfile.mkdtemp(prefix="b6-"))
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf"
TXT = "Il preparato mostra una proliferazione neoplastica epiteliale con atipie nucleari di grado intermedio e indice mitotico elevato. "

def pagina_testo(dpi, q_rumore):
    w, h = int(8.27 * dpi), int(11.69 * dpi); f = ImageFont.truetype(FONT, int(11 * dpi / 72)); img = Image.new("L", (w, h), 250); d = ImageDraw.Draw(img); y = int(dpi * 0.8)
    while y < h - dpi:
        d.text((dpi * 0.8, y), (TXT * 3)[:random.randint(60, 95)], font=f, fill=25); y += int(dpi * 0.2)
    a = np.array(img).astype(np.float32) + np.random.normal(0, q_rumore, (h, w)); return Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))

def foto(dpi):
    w, h = int(8.27 * dpi), int(11.69 * dpi); rng = np.random.default_rng(random.randint(0, 9999))
    base = rng.random((h // 32 + 1, w // 32 + 1, 3)); img = Image.fromarray((base * 255).astype(np.uint8)).resize((w, h), Image.BICUBIC).filter(ImageFilter.GaussianBlur(6))
    a = np.array(img).astype(np.float32) + rng.normal(0, 7, (h, w, 3)); return Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))

def pdf_da_immagini(imgs, dst, q):
    imgs = [i.convert('RGB') for i in imgs]   # RGB: Pillow scrive DCTDecode (JPEG) come fanno gli scanner, non Flate
    imgs[0].save(dst, save_all=True, append_images=imgs[1:], quality=q, resolution=imgs[0].width / 8.27)

ingressi = {}
for i in range(3):
    p = lav / f"scansione{i}.pdf"; pdf_da_immagini([pagina_testo(200, 6) for _ in range(5)], p, 92); ingressi[p.name] = p
    p = lav / f"foto{i}.pdf"; pdf_da_immagini([foto(300) for _ in range(3)], p, 95); ingressi[p.name] = p
    p = lav / f"scansione-scura{i}.pdf"; pdf_da_immagini([pagina_testo(300, 12) for _ in range(3)], p, 85); ingressi[p.name] = p

righe = []
for nome, p in ingressi.items():
    orig = p.stat().st_size; ref = pagine_gri(p)
    c = cedro("tool", p, "compress", timeout=300)
    cp = Path(c["out"].strip().splitlines()[-1]) if c["rc"] == 0 and c["out"].strip() else None
    riga = {"file": nome, "originale": orig, "cedro": None, "rc": c["rc"], "s": c["s"], "rss_mb": c["rss_mb"], "err": c["err"][:200]}
    if cp and cp.exists():
        pg = pagine_gri(cp)
        riga["cedro"] = {"byte": cp.stat().st_size, "ssim": round(statistics.mean(ssim_pagina(a, b) for a, b in zip(ref, pg)), 4), "pagine_ok": len(pg) == len(ref)}
    for liv in ("ebook", "screen"):
        o = lav / f"{p.stem}-gs-{liv}.pdf"
        g = esegui(["gs", "-q", "-dNOPAUSE", "-dBATCH", "-sDEVICE=pdfwrite", f"-dPDFSETTINGS=/{liv}", f"-sOutputFile={o}", str(p)], timeout=300)
        if o.exists():
            pg = pagine_gri(o); riga[f"gs_{liv}"] = {"byte": o.stat().st_size, "ssim": round(statistics.mean(ssim_pagina(a, b) for a, b in zip(ref, pg)), 4), "s": g["s"]}
    righe.append(riga)

def ok(r):
    c, g = r["cedro"], r.get("gs_ebook")
    return None if not c or not g else (c["byte"] <= g["byte"] * 1.10 and c["ssim"] >= 0.98) or (c["ssim"] >= g["ssim"] and c["byte"] <= g["byte"] * 1.10)
res = {"righe": righe, "esito_per_file": {r["file"]: ok(r) for r in righe}}
scrivi_json("b6_comprimi.json", res)
for r in righe:
    print(r["file"], r["originale"], "cedro", r["cedro"], "gs_ebook", r.get("gs_ebook"), "gs_screen", r.get("gs_screen"), "->", ok(r))
shutil.rmtree(lav, ignore_errors=True)
