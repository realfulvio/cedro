#!/usr/bin/env python3
"""Aggiorna versione, nomi dei file, dimensioni e data nella pagina «Scarica» del sito.

Uso: aggiorna-sito.py PAGINA.html [release.json]
Senza il secondo argomento legge l'ultima release del repository con `gh api` (serve GH_TOKEN): è sempre
quella desktop, perché le release Android hanno tag «android-v…» e non sono mai «latest».
Se qualcosa non torna (rete, tag Android, file mancanti) lascia la pagina com'è e esce con 0.
"""
import json, os, re, subprocess, sys
from datetime import datetime
from zoneinfo import ZoneInfo

MESI_IT = ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"]
MESI_EN = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]


def carica(arg):
    if arg:
        return json.load(open(arg))
    repo = os.environ.get("GITHUB_REPOSITORY", "realfulvio/cedro")
    out = subprocess.run(["gh", "api", f"repos/{repo}/releases/latest"], capture_output=True, text=True, check=True).stdout
    return json.loads(out)


def main():
    pagina = sys.argv[1]
    try:
        rel = carica(sys.argv[2] if len(sys.argv) > 2 else None)
    except Exception as e:  # rete o permessi: la pagina resta com'è
        print("release non letta, pagina invariata:", e)
        return
    tag = rel.get("tag_name", "")
    if not re.fullmatch(r"v\d+\.\d+\.\d+", tag):
        print("tag non desktop, pagina invariata:", tag)
        return
    nuova = tag[1:]
    asset = {a["name"]: a["size"] for a in rel.get("assets", [])}
    modelli = {
        "setup": f"Cedro-Setup-{nuova}.exe",
        "portable": f"Cedro-Portable-{nuova}-x64.zip",
        "linux": f"cedro-{nuova}-linux-x64.tar.gz",
    }
    if any(n not in asset for n in modelli.values()):
        print("mancano file nella release, pagina invariata:", [n for n in modelli.values() if n not in asset])
        return
    html = open(pagina, encoding="utf-8").read()
    m = re.search(r"Cedro-Setup-(\d+\.\d+\.\d+)\.exe", html)
    if not m:
        print("versione attuale non trovata nella pagina")
        sys.exit(1)
    vecchia = m.group(1)
    html = html.replace(vecchia, nuova)
    for n in modelli.values():
        html = re.sub(re.escape(n) + r" · \d+ MB", f"{n} · {round(asset[n] / 1048576)} MB", html)
    d = datetime.fromisoformat(rel["published_at"].replace("Z", "+00:00")).astimezone(ZoneInfo("Europe/Rome"))
    html = re.sub(r"(Ultima versione: [\d.]+, )\d{1,2} \w+ \d{4}", lambda x: f"{x.group(1)}{d.day} {MESI_IT[d.month - 1]} {d.year}", html)
    html = re.sub(r"(Latest version: [\d.]+, )\d{1,2} \w+ \d{4}", lambda x: f"{x.group(1)}{d.day} {MESI_EN[d.month - 1]} {d.year}", html)
    open(pagina, "w", encoding="utf-8").write(html)
    print(f"sito: {vecchia} -> {nuova}")


main()
