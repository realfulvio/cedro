"""WER/CER con la normalizzazione del protocollo (§2) e intervallo di confidenza bootstrap pareggiato tra due sistemi."""
import random, re, unicodedata
import jiwer


def normalizza(t: str) -> str:
    t = unicodedata.normalize("NFKC", t).lower()
    t = re.sub(r"[’'`´]", " ", t)
    t = re.sub(r"[^\w\s]|_", " ", t)
    return re.sub(r"\s+", " ", t).strip()


def errori(rif: str, ipo: str):
    """(errori di parola, parole del riferimento) con la normalizzazione fissa; serve per sommare i file e non fare la media delle medie."""
    r, h = normalizza(rif), normalizza(ipo)
    if not r:
        return (len(h.split()), 1)
    o = jiwer.process_words(r, h if h else "x")
    return (o.substitutions + o.deletions + o.insertions, len(r.split()))


def wer(coppie):
    e = sum(c[0] for c in coppie); n = sum(c[1] for c in coppie)
    return e / max(1, n)


def bootstrap_diff(a, b, n=2000, seme=42):
    """Differenza di WER (a - b) con IC 95 % ricampionando i FILE in modo pareggiato. a, b: liste di (errori, parole) negli stessi file."""
    rnd = random.Random(seme); k = len(a); diffs = []
    for _ in range(n):
        idx = [rnd.randrange(k) for _ in range(k)]
        diffs.append(wer([a[i] for i in idx]) - wer([b[i] for i in idx]))
    diffs.sort()
    return wer(a) - wer(b), diffs[int(0.025 * n)], diffs[int(0.975 * n)]


if __name__ == "__main__":     # prova minima
    a = [errori("il gatto mangia il pesce", "il gatto mangia pesce"), errori("ciao a tutti", "ciao tutti")]
    b = [errori("il gatto mangia il pesce", "il gatto mangia il pesce"), errori("ciao a tutti", "ciao a tutti")]
    print(wer(a), wer(b), bootstrap_diff(a, b))
