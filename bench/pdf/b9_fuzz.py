#!/usr/bin/env python3
"""B9: robustezza su file rovinati. Parte da file buoni di tipi diversi, applica mutazioni a caso (seme 42): byte cambiati,
troncamento, intervallo azzerato, spazzatura inserita. Difetto = panico Rust (rc 101), segnale (rc<0 o 128+), timeout > 60 s,
memoria > 3 GB. Un errore pulito (rc 1 con messaggio) NON è un difetto.
Uso: b9_fuzz.py [--n 1000] [--semi CARTELLA...]   Scrive bench/risultati/b9_fuzz.json (i file che fanno male restano in /tmp/b9-crash/)"""
import random, shutil, sys, tempfile
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import esegui, scrivi_json, SEED, CEDRO
random.seed(SEED)
N = int(sys.argv[sys.argv.index("--n") + 1]) if "--n" in sys.argv else 1000
# (percorso, comando) -- il comando è una lista con {f} al posto del file
SEMI = []
def aggiungi(pattern, cmd, limite=6):
    for p in sorted(Path("/").glob(pattern.lstrip("/")))[:limite]: SEMI.append((p, cmd))
aggiungi("/tmp/corpus/word/*.docx", ["convert", "{f}", "pdf"]); aggiungi("/tmp/corpus/word/*.doc", ["convert", "{f}", "pdf"])
aggiungi("/tmp/corpus/word/*.rtf", ["convert", "{f}", "pdf"], 3); aggiungi("/tmp/corpus/word/*.odt", ["convert", "{f}", "pdf"], 3)
aggiungi("/tmp/corpus/word/*.xlsx", ["convert", "{f}", "pdf"], 4); aggiungi("/tmp/corpus/word/*.xls", ["convert", "{f}", "pdf"], 3)
aggiungi("/tmp/firmacorpus/file/*.p7m", ["firma", "{f}"], 8); aggiungi("/tmp/firmacorpus/file/*.pdf", ["firma", "{f}"], 8)
aggiungi("/tmp/dcm/*.dcm", ["convert", "{f}", "png"], 6); aggiungi("/tmp/dcm/*.dcm", ["tool", "{f}", "strip"], 4)
aggiungi("/tmp/b8y/ricco*.pdf", ["tool", "{f}", "split"], 2); aggiungi("/tmp/b8y/ricco*.pdf", ["convert", "{f}", "docx"], 2); aggiungi("/tmp/b8y/ricco*.pdf", ["tool", "{f}", "compress"], 2)
lav = Path(tempfile.mkdtemp(prefix="b9-")); crash = Path("/tmp/b9-crash"); crash.mkdir(exist_ok=True)
# immagini e audio generati
from PIL import Image
import numpy as np
rng = np.random.default_rng(SEED)
for i in range(3):
    im = Image.fromarray(rng.integers(0, 255, (120, 160, 3), dtype=np.uint8))
    for fmt in ("png", "jpeg", "webp", "tiff", "bmp"): 
        p = lav / f"seme{i}.{fmt}"; im.save(p); SEMI.append((p, ["convert", "{f}", "jpeg" if fmt != "jpeg" else "png"]))
if not SEMI: sys.exit("nessun file seme trovato (corpus mancanti)")

def muta(dati: bytes) -> bytes:
    b = bytearray(dati); k = random.choice(["byte", "tronca", "zero", "inserisci", "byte", "byte"])
    if k == "byte":
        for _ in range(random.randint(1, 20)): b[random.randrange(len(b))] = random.randrange(256)
    elif k == "tronca": b = b[: random.randint(0, len(b))]
    elif k == "zero":
        i = random.randrange(len(b)); b[i: i + random.randint(1, 512)] = bytes(min(512, len(b) - i))
    else:
        i = random.randrange(len(b)); b[i:i] = bytes(random.randrange(256) for _ in range(random.randint(1, 300)))
    return bytes(b)

difetti, esiti, tot = [], {"ok": 0, "errore_pulito": 0}, 0
for n in range(N):
    p, cmd = random.choice(SEMI)
    f = lav / f"m{n:04d}{p.suffix}"; f.write_bytes(muta(p.read_bytes()))
    r = esegui([CEDRO, *[str(f) if c == "{f}" else c for c in cmd]], timeout=60)
    tot += 1
    cat = None
    if r["rc"] == 124: cat = "timeout"
    elif r["rc"] < 0 or r["rc"] >= 128: cat = f"segnale({r['rc']})"
    elif r["rc"] == 101 or "panicked" in r["err"]: cat = "panico"
    elif r["rss_mb"] > 3000: cat = "memoria"
    if cat:
        shutil.copy(f, crash / f"{n:04d}-{cat}-{p.stem}{p.suffix}"); difetti.append({"seme": p.name, "cmd": cmd[0] + " " + " ".join(c for c in cmd[1:] if c != "{f}"), "tipo": cat, "err": r["err"][-300:], "file": f"{n:04d}"})
    else: esiti["ok" if r["rc"] == 0 else "errore_pulito"] += 1
    f.unlink()
scrivi_json("b9_fuzz.json", {"mutati": tot, "esiti": esiti, "difetti": len(difetti), "elenco_difetti": difetti, "semi": len(SEMI)})
print(f"B9: {tot} file rovinati: {esiti}, difetti {len(difetti)}")
for d in difetti[:20]: print(" ", d["tipo"], d["seme"], d["cmd"], d["err"][:100].replace("\n", " "))
