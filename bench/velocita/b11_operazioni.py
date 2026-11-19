#!/usr/bin/env python3
"""B11: tempi e memoria delle operazioni di tutti i giorni con la riga di comando di Cedro, per confrontare due versioni
(prima/dopo una release di prestazioni). Ogni operazione gira su una copia fresca del file, `--rip` volte (mediana).
Uso: b11_operazioni.py CORPUS [--cedro BIN]... [--rip 3] [--solo foto,pdf] [--json uscita.json]
CORPUS contiene: foto.jpg (12 MP), foto.png, documento.pdf, documento.docx, foglio.xlsx, audio.wav, audio.mp3, video.mp4, video.webm.
"""
import json, os, shutil, statistics, sys, tempfile
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import esegui

OPS = [
    ("foto", "foto.jpg", "png"), ("foto", "foto.jpg", "webp"), ("foto", "foto.jpg", "pdf"), ("foto", "foto.jpg", "compress"),
    ("foto", "foto.jpg", "half"), ("foto", "foto.png", "jpeg"), ("foto", "foto.jpg", "rot90"),
    ("pdf", "documento.pdf", "docx"), ("pdf", "documento.pdf", "png"), ("pdf", "documento.pdf", "jpg"), ("pdf", "documento.pdf", "txt"),
    ("pdf", "documento.pdf", "compress"), ("pdf", "documento.pdf", "xlsx"),
    ("ufficio", "documento.docx", "pdf"), ("ufficio", "documento.docx", "odt"), ("ufficio", "foglio.xlsx", "pdf"), ("ufficio", "foglio.xlsx", "csv"),
    ("audio", "audio.wav", "mp3"), ("audio", "audio.mp3", "wav"),
    ("video", "video.mp4", "mp3"), ("video", "video.mp4", "mp4"), ("video", "video.webm", "mp4"),
]


def args(flag, default=None, multi=False):
    vals = [sys.argv[i + 1] for i, a in enumerate(sys.argv) if a == flag and i + 1 < len(sys.argv)]
    return vals if multi else (vals[-1] if vals else default)


def main():
    corpus = Path(sys.argv[1]).resolve()
    bins = args("--cedro", multi=True) or [str(Path(__file__).resolve().parents[2] / "target/release/cedro")]
    rip = int(args("--rip", "3"))
    solo = set(args("--solo", "").split(",")) - {""}
    righe = []
    for area, nome, op in OPS:
        if solo and area not in solo:
            continue
        src = corpus / nome
        if not src.exists():
            continue
        riga = {"area": area, "file": nome, "op": op}
        for b in bins:
            ms = []
            for _ in range(rip):
                lav = Path(tempfile.mkdtemp(prefix="b11-"))
                shutil.copy(src, lav / nome)
                env = dict(os.environ, HOME=str(lav / "casa"), XDG_CONFIG_HOME=str(lav / "conf"), XDG_DATA_HOME=str(lav / "dati"))
                m = esegui([b, "convert", str(lav / nome), op, "--senza-caso"], timeout=1800, env=env)
                shutil.rmtree(lav, ignore_errors=True)
                ms.append(m)
            ok = [m for m in ms if m["rc"] == 0]
            chiave = Path(b).name
            if not ok:
                riga[chiave] = {"errore": (ms[0]["err"] or ms[0]["out"])[-200:]}
            else:
                riga[chiave] = {"s": round(statistics.median(m["s"] for m in ok), 3), "rss_mb": round(statistics.median(m["rss_mb"] for m in ok), 1)}
        righe.append(riga)
        print(json.dumps(riga, ensure_ascii=False), flush=True)
    uscita = args("--json")
    if uscita:
        Path(uscita).write_text(json.dumps({"carico": os.getloadavg()[0], "righe": righe}, indent=1, ensure_ascii=False))


if __name__ == "__main__":
    main()
