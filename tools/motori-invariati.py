#!/usr/bin/env python3
"""Dice se i motori (trascrivi/ e riassumi/) sono uguali a quelli del tag precedente.

Uso: tools/motori-invariati.py <tag-precedente>   (dalla radice del repository, con HEAD = ciò che si pubblica)
Esce 0 se le due cartelle sono identiche a parte la versione del loro stesso pacchetto (che cambia a ogni
release senza cambiare il codice), 1 se qualcosa è cambiato, 2 se non si può dire (nel dubbio: si ricompila).
"""
import re
import subprocess
import sys

DIRS = ("trascrivi", "riassumi")
PACCHETTI = {"trascrivi": "cedro-trascrivi", "riassumi": "cedro-riassumi"}


def git(*args):
    r = subprocess.run(("git",) + args, capture_output=True)
    if r.returncode != 0:
        raise RuntimeError(r.stderr.decode(errors="replace").strip() or "git " + " ".join(args))
    return r.stdout


def normalizza(cartella, nome, dati):
    testo = dati.decode("utf-8", errors="surrogateescape").replace("\r\n", "\n")
    if nome == "Cargo.toml":
        return re.sub(r'(?m)^version = "[^"]*"', 'version = "X"', testo, count=1)
    if nome == "Cargo.lock":
        pacchetto = PACCHETTI[cartella]
        return re.sub(r'(name = "%s"\nversion = )"[^"]*"' % re.escape(pacchetto), r'\1"X"', testo)
    return testo


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    prec = sys.argv[1]
    try:
        git("rev-parse", "--verify", "-q", prec + "^{commit}")
    except RuntimeError:
        try:
            git("fetch", "--no-tags", "--depth=1", "origin", "refs/tags/%s:refs/tags/%s" % (prec, prec))
        except RuntimeError as e:
            print("tag precedente non raggiungibile:", e)
            return 2
    try:
        prima = {l.split("\t", 1)[1]: l.split()[2] for l in git("ls-tree", "-r", prec, "--", *DIRS).decode().splitlines()}
        ora = {l.split("\t", 1)[1]: l.split()[2] for l in git("ls-tree", "-r", "HEAD", "--", *DIRS).decode().splitlines()}
    except RuntimeError as e:
        print("elenco dei file non riuscito:", e)
        return 2
    diverso = []
    for f in sorted(set(prima) | set(ora)):
        if prima.get(f) == ora.get(f):
            continue
        cartella, _, nome = f.partition("/")
        if f in prima and f in ora and nome in ("Cargo.toml", "Cargo.lock"):
            a = normalizza(cartella, nome, git("cat-file", "blob", prima[f]))
            b = normalizza(cartella, nome, git("cat-file", "blob", ora[f]))
            if a == b:
                continue
        diverso.append(f)
    if diverso:
        print("cambiati:", ", ".join(diverso[:10]))
        return 1
    print("invariati rispetto a", prec)
    return 0


if __name__ == "__main__":
    sys.exit(main())
