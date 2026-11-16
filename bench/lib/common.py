"""Funzioni comuni del banco di prova di Cedro. Solo libreria standard + numpy/pymupdf/jiwer dove serve."""
import json, os, re, resource, shutil, subprocess, sys, tempfile, time, unicodedata
from pathlib import Path

SEED = 42
RADICE = Path(__file__).resolve().parents[2]
CEDRO = os.environ.get("CEDRO", str(RADICE / "target/release/cedro"))


def esegui(cmd, timeout=300, cwd=None, env=None):
    """Lancia un comando misurando tempo e RSS di picco DI QUEL processo (e dei discendenti già attesi), tramite il lanciatore
    piccolo `rss_helper.py` (ru_maxrss di un figlio parte dal picco del genitore: vedi il commento in quel file).
    Ritorna dict con rc, s, rss_mb, out, err."""
    helper = str(Path(__file__).with_name("rss_helper.py"))
    r = subprocess.run([sys.executable, "-I", "-S", helper, json.dumps({"cmd": [str(c) for c in cmd], "cwd": str(cwd) if cwd else None, "env": env, "timeout": timeout})],
                       capture_output=True, text=True, errors="replace")
    try:
        return json.loads(r.stdout)
    except ValueError:
        return {"rc": 125, "s": 0.0, "rss_mb": 0.0, "out": "", "err": "lanciatore: " + r.stderr[-300:]}


def cedro(*args, timeout=300, cwd=None):
    return esegui([CEDRO, *map(str, args)], timeout=timeout, cwd=cwd)


def normalizza(t: str) -> str:
    """Normalizzazione del testo del protocollo (§2): NFKC, minuscolo, punteggiatura via (apostrofo -> spazio), spazi collassati."""
    t = unicodedata.normalize("NFKC", t).lower()
    t = re.sub(r"[’'`´]", " ", t)
    t = re.sub(r"[^\w\s]|_", " ", t)
    return re.sub(r"\s+", " ", t).strip()


def parole(t: str):
    return normalizza(t).split()


def f1_parole(rif: str, ipo: str):
    """F1 su multiinsieme di parole (ordine ignorato): misura la perdita di testo senza punire l'impaginazione."""
    from collections import Counter
    a, b = Counter(parole(rif)), Counter(parole(ipo))
    if not a and not b:
        return 1.0, 1.0, 1.0
    comuni = sum((a & b).values())
    p = comuni / max(1, sum(b.values()))
    r = comuni / max(1, sum(a.values()))
    f = 0.0 if p + r == 0 else 2 * p * r / (p + r)
    return round(f, 4), round(p, 4), round(r, 4)


def testo_pdf(path) -> str:
    import fitz
    with fitz.open(path) as d:
        return "\n".join(p.get_text() for p in d)


def pagine_gri(path, dpi=72):
    """Pagine PDF come array numpy in scala di grigi."""
    import fitz, numpy as np
    out = []
    with fitz.open(path) as d:
        for p in d:
            pix = p.get_pixmap(dpi=dpi, colorspace=fitz.csGRAY, alpha=False)
            out.append(np.frombuffer(pix.samples, dtype=np.uint8).reshape(pix.height, pix.width).copy())
    return out


def ssim_pagina(a, b):
    """SSIM tra due pagine; se le dimensioni differiscono si riempie di bianco (differenza di formato = punteggio basso)."""
    import numpy as np
    from skimage.metrics import structural_similarity as ssim
    h, w = max(a.shape[0], b.shape[0]), max(a.shape[1], b.shape[1])
    def pad(x):
        y = np.full((h, w), 255, np.uint8)
        y[: x.shape[0], : x.shape[1]] = x
        return y
    return float(ssim(pad(a), pad(b), data_range=255))


def mediana(v):
    v = sorted(v)
    n = len(v)
    return None if n == 0 else (v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2)


def scrivi_json(nome, dati):
    d = RADICE / "bench/risultati"
    d.mkdir(parents=True, exist_ok=True)
    (d / nome).write_text(json.dumps(dati, indent=1, ensure_ascii=False, sort_keys=True), encoding="utf-8")


def soffice_pdf(src, outdir, timeout=120):
    """Riferimento LibreOffice: file -> PDF. Profilo separato per non litigare con altre istanze."""
    prof = Path(tempfile.gettempdir()) / "lo-bench-profile"
    return esegui(["soffice", f"-env:UserInstallation=file://{prof}", "--headless", "--convert-to", "pdf", "--outdir", str(outdir), str(src)], timeout=timeout)
