#!/usr/bin/env python3
"""Banco di prova della trascrizione (Area A del protocollo). Si lancia sul PC di collaudo (rete, scheda video, Cedro compilato).

  bench.py prepara [--n 200]           scarica FLEURS it_it (test) e MLS italiano (test), seme 42, li porta a WAV 16 kHz mono
  bench.py wer --sistemi a,b,c [--set fleurs|mls] [--cpu]        A1/A2: WER e CER per file, tempo e memoria
  bench.py riassumi [--set fleurs]     tabella WER/CER con IC 95 % pareggiato contro il miglior concorrente
  bench.py allucinazioni --sistemi ... A3: silenzio, rumori, tono musicale, parlato lontano
  bench.py prestazioni --sistemi ...   A4/A5: file di 10 min, CPU e scheda video, 5 ripetizioni
  bench.py robustezza --sistemi ...    A6: contenitori/codec/file rotti (+ tutto ciò che trovi in DATI/reali/)
  bench.py lungo --sistemi ...         A8: 3 h di audio, deriva dei tempi, frasi in loop
  bench.py voci --sistemi cedro-buona  A7: miscele di 2/3/5 parlanti noti, DER e numero di voci
  bench.py prova                       autoprova con un sistema finto (non serve rete)

Dati e risultati: DATI (variabile BENCH_DATI, predefinita ~/bench-dati) e bench/risultati/a_*.jsonl.
I sistemi si scelgono tra quelli di adapters.ELENCO; versioni e modelli vanno scritti in bench/VERSIONI.md PRIMA di misurare.
"""
import argparse, csv, json, os, random, re, shutil, subprocess, sys, tempfile, time
from pathlib import Path

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))
import wer as W
from adapters import ELENCO

SEME = 42
DATI = Path(os.environ.get("BENCH_DATI", str(Path.home() / "bench-dati")))
RIS = AQUI.parents[0] / "risultati"
RIS.mkdir(parents=True, exist_ok=True)


# ----------------------------------------------------------------------------- utilità
def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def ffmpeg(*args):
    r = sh(["ffmpeg", "-y", "-loglevel", "error", *map(str, args)])
    if r.returncode:
        raise RuntimeError(f"ffmpeg {' '.join(map(str, args))}: {r.stderr[:300]}")


def durata(p):
    r = sh(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(p)])
    try:
        return float(r.stdout.strip())
    except ValueError:
        return 0.0


def misura(argv, timeout=3600, env=None):
    """Lancia il comando misurando tempo vero e RSS di picco (wait4: include i discendenti già attesi, niente GNU time)."""
    import threading
    t0 = time.perf_counter()
    p = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    cattura = {}
    def leggi(f, k): cattura[k] = f.read()
    th = [threading.Thread(target=leggi, args=(p.stdout, "out")), threading.Thread(target=leggi, args=(p.stderr, "err"))]
    for t in th: t.start()
    try:
        _, stato, ru = os.wait4(p.pid, 0) if timeout is None else _attendi(p, timeout)
        rc = os.waitstatus_to_exitcode(stato)
    except TimeoutError:
        p.kill(); _, stato, ru = os.wait4(p.pid, 0); rc = 124; cattura["err"] = "TIMEOUT"
    for t in th: t.join()
    return {"rc": rc, "s": round(time.perf_counter() - t0, 2), "rss_mb": round(ru.ru_maxrss / 1024, 1), "out": cattura.get("out", ""), "err": (cattura.get("err") or "")[-600:]}


def _attendi(p, timeout):
    fine = time.time() + timeout
    while time.time() < fine:
        r = os.wait4(p.pid, os.WNOHANG)
        if r[0]: return r
        time.sleep(0.05)
    raise TimeoutError


def lancia(sistema, wav, gpu=True, timeout=3600):
    """Esegue un sistema su un file; ritorna (testo, misura)."""
    a = ELENCO[sistema]() if isinstance(sistema, str) else sistema
    out = Path(tempfile.mkdtemp(prefix="bench-"))
    try:
        m = misura(a.comando(Path(wav), out, gpu), timeout=timeout)
        testo = a.leggi(Path(wav), out, m["out"]) if m["rc"] == 0 else ""
        return testo, m
    finally:
        shutil.rmtree(out, ignore_errors=True)


def scrivi_riga(nome, d):
    with open(RIS / nome, "a") as f:
        f.write(json.dumps(d, ensure_ascii=False) + "\n")


def leggi_righe(nome):
    p = RIS / nome
    return [json.loads(r) for r in p.read_text().splitlines() if r.strip()] if p.exists() else []


def parole_n(t):
    return len(W.normalizza(t).split())


def cer(rif, ipo):
    import jiwer
    r, h = W.normalizza(rif), W.normalizza(ipo)
    if not r: return float(len(h) > 0)
    return jiwer.cer(r, h if h else "x")


# ----------------------------------------------------------------------------- dati
def cmd_prepara(a):
    from huggingface_hub import hf_hub_download, HfApi
    import tarfile
    (DATI / "fleurs").mkdir(parents=True, exist_ok=True)
    tsv = hf_hub_download("google/fleurs", "data/it_it/test.tsv", repo_type="dataset")
    tar = hf_hub_download("google/fleurs", "data/it_it/audio/test.tar.gz", repo_type="dataset")
    righe = list(csv.reader(open(tsv, encoding="utf-8"), delimiter="\t", quoting=csv.QUOTE_NONE))
    rif = {r[1]: r[2] for r in righe if len(r) >= 3}          # file_name -> trascrizione grezza (numeri come arrivano)
    with tarfile.open(tar) as t:
        t.extractall(DATI / "fleurs" / "orig", filter="data")
    nomi = sorted(rif)
    random.Random(SEME).shuffle(nomi)
    ok = []
    for n in nomi[: a.n]:
        src = next((DATI / "fleurs" / "orig").rglob(n), None)
        if not src: continue
        wav = DATI / "fleurs" / (Path(n).stem + ".wav")
        ffmpeg("-i", src, "-ar", 16000, "-ac", 1, wav)
        ok.append({"id": wav.stem, "wav": str(wav), "rif": rif[n]})
    (DATI / "fleurs" / "rif.json").write_text(json.dumps(ok, ensure_ascii=False, indent=1))
    print(f"FLEURS: {len(ok)} clip (seme {SEME}) in {DATI/'fleurs'}")
    # tutto il test, in ordine, serve per il file lungo (A8) e per i 10 minuti (A4)
    tutti = []
    for n in sorted(rif):
        src = next((DATI / "fleurs" / "orig").rglob(n), None)
        if src: tutti.append((n, src))
    (DATI / "fleurs" / "tutti.json").write_text(json.dumps([{"nome": n, "src": str(s), "rif": rif[n]} for n, s in tutti], ensure_ascii=False))
    # MLS italiano: parquet convertito, con speaker_id (serve anche per A7)
    try:
        import pandas as pd, io
        api = HfApi()
        parts = sorted(f for f in api.list_repo_files("facebook/multilingual_librispeech", repo_type="dataset") if f.startswith("italian/") and "test" in f and f.endswith(".parquet"))
        (DATI / "mls").mkdir(parents=True, exist_ok=True)
        dfs = [pd.read_parquet(hf_hub_download("facebook/multilingual_librispeech", f, repo_type="dataset")) for f in parts]
        df = pd.concat(dfs, ignore_index=True)
        idx = list(range(len(df))); random.Random(SEME).shuffle(idx)
        mls, parl = [], {}
        for i in idx:
            r = df.iloc[i]
            wav = DATI / "mls" / f"{r['id'] if 'id' in r else i}.wav"
            raw = DATI / "mls" / f"{i}.src"
            raw.write_bytes(r["audio"]["bytes"])
            ffmpeg("-i", raw, "-ar", 16000, "-ac", 1, wav); raw.unlink()
            d = {"id": wav.stem, "wav": str(wav), "rif": r["transcript"], "parlante": str(r["speaker_id"]), "durata": durata(wav)}
            if len(mls) < min(a.n, 200): mls.append(d)
            parl.setdefault(d["parlante"], []).append(d)
            if len(mls) >= min(a.n, 200) and sum(len(v) >= 4 for v in parl.values()) >= 8: break
        (DATI / "mls" / "rif.json").write_text(json.dumps(mls, ensure_ascii=False, indent=1))
        (DATI / "mls" / "parlanti.json").write_text(json.dumps({k: v for k, v in parl.items() if len(v) >= 4}, ensure_ascii=False))
        print(f"MLS: {len(mls)} clip, {sum(len(v) >= 4 for v in parl.values())} parlanti con ≥4 clip")
    except Exception as e:
        print("MLS non scaricato:", e, "- A7 e il set MLS di A1 restano da fare (BENCH: vedi README)")


def carica_set(nome):
    p = DATI / nome / "rif.json"
    if not p.exists():
        sys.exit(f"manca {p}: lancia prima `bench.py prepara`")
    return json.loads(p.read_text())


# ----------------------------------------------------------------------------- A1/A2
def cmd_wer(a):
    clip = carica_set(a.set)[: a.n] if a.n else carica_set(a.set)
    gia = {(r["sistema"], r["id"], r["gpu"]) for r in leggi_righe(f"a1_wer_{a.set}.jsonl")}
    for s in a.sistemi:
        for c in clip:
            if (s, c["id"], not a.cpu) in gia: continue
            testo, m = lancia(s, c["wav"], gpu=not a.cpu)
            e, n = W.errori(c["rif"], testo)
            scrivi_riga(f"a1_wer_{a.set}.jsonl", {"sistema": s, "id": c["id"], "gpu": not a.cpu, "rc": m["rc"], "err": e, "parole": n,
                                                  "cer": round(cer(c["rif"], testo), 4), "s": m["s"], "rss_mb": m["rss_mb"], "ipo": testo[:2000], "stderr": m["err"] if m["rc"] else ""})
        print(f"{s}: fatto")


def cmd_riassumi(a):
    righe = leggi_righe(f"a1_wer_{a.set}.jsonl")
    per = {}
    for r in righe:
        per.setdefault(r["sistema"], {})[r["id"]] = r
    comuni = sorted(set.intersection(*[set(v) for v in per.values()])) if per else []
    print(f"set {a.set}: {len(comuni)} clip in comune tra {len(per)} sistemi")
    tab = {s: [(per[s][i]["err"], per[s][i]["parole"]) for i in comuni] for s in per}
    wer_s = {s: W.wer(v) for s, v in tab.items()}
    cer_s = {s: sum(per[s][i]["cer"] for i in comuni) / max(1, len(comuni)) for s in per}
    cedri = [s for s in per if s.startswith("cedro")]
    altri = [s for s in per if not s.startswith("cedro")]
    migliore = min(altri, key=lambda s: wer_s[s]) if altri else None
    for s in sorted(per, key=lambda s: wer_s[s]):
        riga = f"{s:22s} WER {wer_s[s]*100:5.2f}%  CER {cer_s[s]*100:5.2f}%  fallimenti {sum(per[s][i]['rc'] != 0 for i in comuni)}"
        if s in cedri and migliore:
            d, lo, hi = W.bootstrap_diff(tab[s], tab[migliore])
            riga += f"   vs {migliore}: {d*100:+.2f} pt [{lo*100:+.2f}, {hi*100:+.2f}] -> " + ("PARI/MEGLIO" if d * 100 <= 1.0 else "SOTTO SOGLIA (+1 pt)")
        print(riga)
    (RIS / f"a1_riassunto_{a.set}.json").write_text(json.dumps({"clip": len(comuni), "wer": wer_s, "cer": cer_s, "migliore_concorrente": migliore}, indent=1))


# ----------------------------------------------------------------------------- A3
def genera_allucinazioni():
    d = DATI / "allucinazioni"; d.mkdir(parents=True, exist_ok=True)
    ffmpeg("-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono", "-t", 60, d / "silenzio.wav")
    ffmpeg("-f", "lavfi", "-i", "anoisesrc=color=white:amplitude=0.1:r=16000", "-t", 60, d / "rumore-bianco.wav")
    ffmpeg("-f", "lavfi", "-i", "anoisesrc=color=pink:amplitude=0.05:r=16000", "-t", 60, d / "rumore-rosa.wav")
    ffmpeg("-f", "lavfi", "-i", "aevalsrc=0.3*sin(2*PI*440*t)*gt(sin(2*PI*0.5*t)\\,0)+0.25*sin(2*PI*660*t)*lt(sin(2*PI*0.5*t)\\,0):s=16000:d=60", d / "tono-musicale.wav")
    ffmpeg("-f", "lavfi", "-i", "anoisesrc=color=brown:amplitude=0.02:r=16000", "-t", 60, d / "ronzio-ambiente.wav")
    # parlato lontano: 60 s di clip FLEURS molto piano, con eco e fruscio
    clip = carica_set("fleurs")[:8]
    lista = d / "lista.txt"; lista.write_text("".join(f"file '{c['wav']}'\n" for c in clip))
    ffmpeg("-f", "concat", "-safe", 0, "-i", lista, "-t", 60, "-af", "volume=0.04,aecho=0.8:0.7:120|240:0.5|0.3", d / "parlato-lontano.wav")
    return sorted(d.glob("*.wav"))


def cmd_allucinazioni(a):
    for f in genera_allucinazioni():
        for s in a.sistemi:
            testo, m = lancia(s, f)
            scrivi_riga("a3_allucinazioni.jsonl", {"sistema": s, "file": f.name, "rc": m["rc"], "parole": parole_n(testo), "testo": testo[:400], "stderr": m["err"] if m["rc"] else ""})
            print(f"{s:20s} {f.name:22s} parole={parole_n(testo)} rc={m['rc']}")


# ----------------------------------------------------------------------------- A4/A5
def file_lungo(minuti, nome):
    """Concatena clip FLEURS in ordine fino a `minuti`; ritorna (wav, riferimento, [(inizio_s, testo)])."""
    out = DATI / "lunghi"; out.mkdir(parents=True, exist_ok=True)
    wav = out / f"{nome}.wav"; meta = out / f"{nome}.json"
    if wav.exists() and meta.exists():
        m = json.loads(meta.read_text()); return wav, m["rif"], m["tempi"]
    tutti = json.loads((DATI / "fleurs" / "tutti.json").read_text())
    scelti, t, tempi = [], 0.0, []
    for c in tutti:
        w = DATI / "fleurs" / "orig-wav" / (Path(c["nome"]).stem + ".wav"); w.parent.mkdir(exist_ok=True)
        if not w.exists(): ffmpeg("-i", c["src"], "-ar", 16000, "-ac", 1, w)
        d = durata(w)
        scelti.append(w); tempi.append((round(t, 2), c["rif"])); t += d + 0.5
        if t >= minuti * 60: break
    lista = out / f"{nome}.txt"
    lista.write_text("".join(f"file '{w}'\nfile '{DATI/'fleurs'/'pausa.wav'}'\n" for w in scelti))
    ffmpeg("-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono", "-t", 0.5, DATI / "fleurs" / "pausa.wav")
    ffmpeg("-f", "concat", "-safe", 0, "-i", lista, "-ar", 16000, "-ac", 1, wav)
    rif = " ".join(x[1] for x in tempi)
    meta.write_text(json.dumps({"rif": rif, "tempi": tempi}, ensure_ascii=False))
    return wav, rif, tempi


def cmd_prestazioni(a):
    wav, rif, _ = file_lungo(10, "dieci-minuti")
    dur = durata(wav)
    for s in a.sistemi:
        for gpu in ([True, False] if not a.cpu else [False]):
            for rep in range(a.ripetizioni):
                testo, m = lancia(s, wav, gpu=gpu)
                e, n = W.errori(rif, testo)
                d = {"sistema": s, "gpu": gpu, "rep": rep, "durata_s": round(dur, 1), "s": m["s"], "rtf": round(m["s"] / dur, 3), "rss_mb": m["rss_mb"], "rc": m["rc"], "wer": round(e / n, 4)}
                scrivi_riga("a4_prestazioni.jsonl", d); print(d)


# ----------------------------------------------------------------------------- A6
def genera_robustezza():
    d = DATI / "robustezza"; d.mkdir(parents=True, exist_ok=True)
    clip = carica_set("fleurs")[:1][0]; src = clip["wav"]
    casi = {}   # nome -> (argomenti ffmpeg di uscita, atteso: 'ok' | 'errore')
    spec = {
        "mp3.mp3": ["-c:a", "libmp3lame", "-b:a", "96k"], "m4a.m4a": ["-c:a", "aac", "-b:a", "64k"], "ogg.ogg": ["-c:a", "libvorbis"],
        "opus.opus": ["-c:a", "libopus", "-b:a", "24k"], "flac.flac": ["-c:a", "flac"], "wav8k.wav": ["-ar", "8000"],
        "wav44-stereo.wav": ["-ar", "44100", "-ac", "2"], "wav-24bit.wav": ["-c:a", "pcm_s24le", "-ar", "48000"], "amr.amr": ["-ar", "8000", "-c:a", "libopencore_amrnb", "-b:a", "12.2k"],
        "webm.webm": ["-c:a", "libopus"], "mkv.mkv": ["-c:a", "aac"], "aiff.aiff": [], "3gp.3gp": ["-ar", "8000", "-c:a", "aac"], "wma.wma": ["-c:a", "wmav2"],
        "whatsapp.opus": ["-c:a", "libopus", "-b:a", "16k", "-ar", "16000"], "volume-basso.wav": ["-af", "volume=0.05"], "con-rumore.mp3": ["-c:a", "libmp3lame", "-b:a", "32k", "-ar", "22050"],
    }
    for n, o in spec.items():
        try: ffmpeg("-i", src, *o, d / n); casi[n] = "ok"
        except RuntimeError: pass        # codec non disponibile in questo ffmpeg: caso saltato
    for n, cont in {"video-con-audio.mp4": "mp4", "video-con-audio.mov": "mov"}.items():
        try: ffmpeg("-f", "lavfi", "-i", "color=c=black:s=320x240:r=10", "-i", src, "-shortest", "-c:v", "libx264", "-c:a", "aac", d / n); casi[n] = "ok"
        except RuntimeError: pass
    try: ffmpeg("-f", "lavfi", "-i", "color=c=black:s=320x240:r=10:d=5", "-c:v", "libx264", d / "senza-audio.mp4"); casi["senza-audio.mp4"] = "errore"
    except RuntimeError: pass
    (d / "vuoto.wav").write_bytes(b""); casi["vuoto.wav"] = "errore"
    dati = Path(src).read_bytes(); (d / "troncato.wav").write_bytes(dati[: len(dati) // 3]); casi["troncato.wav"] = "ok?"
    (d / "testo-finto.mp3").write_text("questo non è audio\n" * 50); casi["testo-finto.mp3"] = "errore"
    (d / "nome con spazi e àccènti.wav").write_bytes(dati); casi["nome con spazi e àccènti.wav"] = "ok"
    for p in sorted((DATI / "reali").glob("*")) if (DATI / "reali").exists() else []:
        casi[str(p)] = "ok"
    return d, clip["rif"], casi


def cmd_robustezza(a):
    d, rif, casi = genera_robustezza()
    for s in a.sistemi:
        tot = ok = 0
        for n, atteso in casi.items():
            f = Path(n) if os.path.isabs(n) else d / n
            testo, m = lancia(s, f, timeout=600)
            e, k = W.errori(rif, testo) if atteso != "errore" and not os.path.isabs(n) else (0, 1)
            fine = m["rc"] == 0
            if atteso == "errore": passa = (not fine) and len(m["err"].strip() + m["out"]) > 0   # deve fallire e dirlo
            elif os.path.isabs(n): passa = fine
            else: passa = fine and e / k < 0.5
            tot += 1; ok += passa
            scrivi_riga("a6_robustezza.jsonl", {"sistema": s, "caso": f.name, "atteso": atteso, "rc": m["rc"], "passa": passa, "wer": round(e / k, 3), "messaggio": (m["err"] or m["out"])[-300:]})
            print(f"{s:20s} {f.name:34s} {'ok' if passa else 'DIFETTO'} rc={m['rc']}")
        print(f"{s}: {ok}/{tot}")


# ----------------------------------------------------------------------------- A8
def cmd_lungo(a):
    wav, rif, tempi = file_lungo(180, "tre-ore")
    dur = durata(wav)
    for s in a.sistemi:
        testo, m = lancia(s, wav, timeout=6 * 3600)
        righe = re.findall(r"^\[(\d+):(\d\d)(?::(\d\d))?\]\s*(?:Interlocutore \d+:\s*)?(.*)$", m["out"], re.M)
        seg = [((int(h) * 60 + int(mm)) if ss == "" else int(h) * 3600 + int(mm) * 60 + int(ss), t) for h, mm, ss, t in righe]
        # frasi identiche di fila (loop)
        frasi = [W.normalizza(x) for x in re.split(r"(?<=[.!?])\s+|\n", testo) if W.normalizza(x)]
        loop = max((len(list(g)) for k, g in __import__("itertools").groupby(frasi)), default=0)
        # deriva: scarto tra il tempo dato al segmento e l'inizio vero della clip che contiene quella frase
        scarti = []
        for t0, tx in seg:
            best = max(tempi, key=lambda c: _f1(tx, c[1]))
            if _f1(tx, best[1]) >= 0.6: scarti.append((t0, t0 - best[0]))
        ini = [x[1] for x in scarti[:20]]; fin = [x[1] for x in scarti[-20:]]
        e, n = W.errori(rif, testo)
        d = {"sistema": s, "durata_s": round(dur), "s": m["s"], "rss_mb": m["rss_mb"], "rc": m["rc"], "wer": round(e / n, 4), "frasi_uguali_di_fila_max": loop,
             "segmenti_con_tempo": len(seg), "scarto_inizio_s": round(sum(ini) / len(ini), 2) if ini else None, "scarto_fine_s": round(sum(fin) / len(fin), 2) if fin else None}
        scrivi_riga("a8_lungo.jsonl", d); print(d)


def _f1(a, b):
    from collections import Counter
    x, y = Counter(W.normalizza(a).split()), Counter(W.normalizza(b).split())
    c = sum((x & y).values())
    if not c: return 0.0
    p, r = c / sum(y.values()), c / sum(x.values())
    return 2 * p * r / (p + r)


# ----------------------------------------------------------------------------- A7
def der(rif, ipo, passo=0.1):
    """DER a trame (senza sovrapposizioni): rif/ipo = [(inizio, fine, etichetta)]; etichette mappate in modo ottimo (scipy)."""
    import numpy as np
    from scipy.optimize import linear_sum_assignment
    fine = max(x[1] for x in rif + ipo); T = int(fine / passo) + 1
    def trame(seg):
        v = [None] * T
        for i, f, e in seg:
            for k in range(int(i / passo), min(T, int(f / passo))): v[k] = e
        return v
    r, h = trame(rif), trame(ipo)
    er, eh = sorted({x for x in r if x is not None}), sorted({x for x in h if x is not None})
    if not eh: return 1.0
    M = np.zeros((len(er), len(eh)))
    for a_, b_ in zip(r, h):
        if a_ is not None and b_ is not None: M[er.index(a_), eh.index(b_)] += 1
    ri, ci = linear_sum_assignment(-M); mappa = {eh[c]: er[r_] for r_, c in zip(ri, ci)}
    parlato = sum(x is not None for x in r); err = 0
    for a_, b_ in zip(r, h):
        if a_ is None and b_ is None: continue
        if a_ is None or b_ is None: err += 1
        elif mappa.get(b_) != a_: err += 1
    return err / max(1, parlato)


def cmd_voci(a):
    p = DATI / "mls" / "parlanti.json"
    if not p.exists(): sys.exit("serve MLS (parlanti noti): `bench.py prepara`")
    parl = json.loads(p.read_text()); rnd = random.Random(SEME)
    for k in (2, 3, 5):
        for m_ in range(7 if k else 0):               # 7 miscele per numero di voci ≈ 20 miscele in tutto
            ids = rnd.sample(sorted(parl), min(k, len(parl)))
            seq = []
            for turno in range(4 * k):
                par = ids[turno % k] if turno < k else rnd.choice(ids)
                seq.append((par, rnd.choice(parl[par])))
            dirm = DATI / "miscele"; dirm.mkdir(exist_ok=True); nome = f"m{k}_{m_}"
            lista = dirm / f"{nome}.txt"; lista.write_text("".join(f"file '{c['wav']}'\n" for _, c in seq))
            ffmpeg("-f", "concat", "-safe", 0, "-i", lista, "-ar", 16000, "-ac", 1, dirm / f"{nome}.wav")
            t, rif = 0.0, []
            for par, c in seq:
                rif.append((t, t + c["durata"], par)); t += c["durata"]
            for s in a.sistemi:
                # per misurare le voci Cedro deve essere lanciato con --voci auto
                cmd = [os.environ.get("CEDRO", "cedro"), "trascrivi", str(dirm / f"{nome}.wav"), "--testo", "--voci", "auto"]
                mm = misura(cmd)
                righe = re.findall(r"^\[(\d+):(\d\d)(?::(\d\d))?\]\s*Interlocutore (\d+):", mm["out"], re.M)
                ipo = []
                for i, (h, mi, se, v) in enumerate(righe):
                    t0 = (int(h) * 60 + int(mi)) if se == "" else int(h) * 3600 + int(mi) * 60 + int(se)
                    ipo.append((t0, None, v))
                ipo = [(x[0], (ipo[i + 1][0] if i + 1 < len(ipo) else t), x[2]) for i, x in enumerate(ipo)]
                voci = len({x[2] for x in ipo})
                d = {"sistema": s, "miscela": nome, "voci_vere": k, "voci_stimate": voci, "der": round(der(rif, ipo), 4) if ipo else None, "rc": mm["rc"]}
                scrivi_riga("a7_voci.jsonl", d); print(d)


# ----------------------------------------------------------------------------- autoprova
def cmd_prova(a):
    class Finto:
        nome = "finto"
        def __init__(self, p): self.p = p
        def comando(self, wav, outdir, gpu): return ["echo", "il gatto mangia il pesce"]
        def leggi(self, wav, outdir, stdout): return stdout
    t, m = lancia(Finto(0), "/dev/null")
    assert t.strip() == "il gatto mangia il pesce" and m["rc"] == 0 and m["rss_mb"] is not None, (t, m)
    e, n = W.errori("Il gatto, mangia il pesce!", t); assert e == 0 and n == 5
    r = [(0.0, 1.0, "a"), (1.0, 2.0, "b")]; assert der(r, [(0, 1.0, "x"), (1.0, 2.0, "y")]) == 0.0
    assert abs(der(r, [(0, 2.0, "x")]) - 0.5) < 0.02
    assert abs(_f1("a b c", "a b d") - 2 / 3) < 1e-6
    print("autoprova ok (wer, bootstrap, tempo/RSS, DER, F1)")


def main():
    p = argparse.ArgumentParser(); sp = p.add_subparsers(dest="cmd", required=True)
    for nome, f in [("prepara", cmd_prepara), ("wer", cmd_wer), ("riassumi", cmd_riassumi), ("allucinazioni", cmd_allucinazioni), ("prestazioni", cmd_prestazioni),
                    ("robustezza", cmd_robustezza), ("lungo", cmd_lungo), ("voci", cmd_voci), ("prova", cmd_prova)]:
        q = sp.add_parser(nome); q.set_defaults(f=f)
        q.add_argument("--sistemi", type=lambda x: x.split(","), default=["cedro-buona"]); q.add_argument("--set", default="fleurs")
        q.add_argument("--n", type=int, default=200); q.add_argument("--cpu", action="store_true"); q.add_argument("--ripetizioni", type=int, default=5)
    a = p.parse_args(); a.f(a)


if __name__ == "__main__":
    main()
