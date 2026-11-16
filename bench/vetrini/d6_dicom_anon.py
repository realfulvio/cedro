#!/usr/bin/env python3
"""D6: anonimizzazione DICOM contro il Basic Application Level Confidentiality Profile (DICOM PS3.15 Annex E, Tabella E.1-1).
Riferimento: l'elenco di attributi del pacchetto `dicom-anonymizer` (dicomfields_2026c.py, che trascrive la tabella).
Uso: d6_dicom_anon.py <cartella-con-.dcm>
Per ogni file: `cedro tool file strip` (Cedro: «Togli dati nascosti», DICOM -> DICOM anonimo), poi si cerca ciò che avrebbe dovuto sparire:
  - attributo dell'elenco (azioni D, Z, X, e combinazioni) che conserva ESATTAMENTE il valore originale non vuoto  -> perdita
  - attributo con azione U (UID) che conserva lo stesso UID                                                        -> perdita
  - attributi privati (gruppo dispari) ancora presenti                                                             -> perdita
Esito grave = qualunque perdita. Scrive bench/risultati/d6_dicom_anon.json."""
import shutil, sys, tempfile, warnings
from pathlib import Path
warnings.filterwarnings("ignore")
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import cedro, scrivi_json
import pydicom
from dicomanonymizer.dicom_anonymization_databases import dicomfields_2026c as F

TUTTI = {}
for nome in ("D_TAGS", "Z_TAGS", "X_TAGS", "Z_D_TAGS", "X_Z_TAGS", "X_D_TAGS", "X_Z_D_TAGS", "X_Z_U_STAR_TAGS"):
    for t in getattr(F, nome):
        TUTTI[tuple(t)] = nome
UID = {tuple(t) for t in F.U_TAGS}


def valori(ds, prefisso=()):
    """(percorso, tag, valore testuale) di tutti gli elementi non vuoti, sequenze comprese."""
    for el in ds:
        tag = (el.tag.group, el.tag.element)
        if el.VR == "SQ":
            for i, item in enumerate(el.value):
                yield from valori(item, prefisso + (tag, i))
        else:
            try:
                v = el.value
                s = str(v) if not isinstance(v, (bytes, bytearray)) else bytes(v).hex()
            except Exception:
                continue
            yield prefisso, tag, s, el.tag.is_private


def main():
    righe, tot_perdite = [], 0
    for f in sorted(Path(sys.argv[1]).glob("*.dcm")):
        lav = Path(tempfile.mkdtemp(prefix="d6-")); src = lav / f.name; shutil.copy(f, src)
        riga = {"file": f.name}
        try:
            orig = {(p, t): (s, priv) for p, t, s, priv in valori(pydicom.dcmread(src, force=True))}
        except Exception as e:
            riga["errore_lettura"] = str(e)[:80]; righe.append(riga); continue
        r = cedro("tool", src, "strip", timeout=120)
        out = next((p for p in lav.glob("*-anonimo*.dcm")), None)
        riga.update(rc=r["rc"], s=r["s"])
        if r["rc"] != 0 or out is None:
            riga["errore"] = (r["err"] or r["out"]).strip()[:140]; righe.append(riga); continue
        try:
            nuovo = {(p, t): (s, priv) for p, t, s, priv in valori(pydicom.dcmread(out, force=True))}
        except Exception as e:
            riga["errore_output"] = str(e)[:80]; righe.append(riga); continue
        perdite = []
        for (p, t), (s, priv) in orig.items():
            if not s.strip():
                continue
            n = nuovo.get((p, t))
            if n is None:
                continue
            if n[0] == s and (t in TUTTI or t in UID):
                perdite.append({"tag": f"({t[0]:04X},{t[1]:04X})", "azione": TUTTI.get(t, "U"), "valore": s[:40]})
        privati = [f"({t[0]:04X},{t[1]:04X})" for (p, t), (s, priv) in nuovo.items() if priv]
        riga["perdite"], riga["privati_rimasti"] = perdite, privati
        tot_perdite += len(perdite) + len(privati)
        righe.append(riga)
        shutil.rmtree(lav, ignore_errors=True)
    ok = [x for x in righe if "perdite" in x]
    # Categorie: le scelte DICHIARATE da Cedro (caratteristiche del paziente per la ricerca; date spostate ma orari lasciati) e il resto.
    CARATT = {(0x0010, 0x0040), (0x0010, 0x1010), (0x0010, 0x1030), (0x0010, 0x1020), (0x0010, 0x21C0), (0x0010, 0x2160), (0x0010, 0x2292), (0x0010, 0x2293)}
    ORARI = {(0x0008, 0x0030), (0x0008, 0x0031), (0x0008, 0x0032), (0x0008, 0x0033), (0x0008, 0x0013), (0x0008, 0x0201), (0x0040, 0x0245), (0x0040, 0x0251), (0x0008, 0x002A)}
    from collections import Counter
    cat = Counter()
    for x in ok:
        for pd in x["perdite"]:
            t = (int(pd["tag"][1:5], 16), int(pd["tag"][6:10], 16))
            cat["caratteristiche_paziente_dichiarate" if t in CARATT else "orari_non_spostati" if t in ORARI else "altro:" + pd["tag"]] += 1
        for pv in x["privati_rimasti"]:
            cat["privati"] += 1
    res = {"categorie": dict(cat), "file": len(righe), "anonimizzati": len(ok), "falliti": [{x["file"]: x.get("errore") or x.get("errore_output") or x.get("errore_lettura")} for x in righe if "perdite" not in x],
           "file_con_perdite": [x["file"] for x in ok if x["perdite"] or x["privati_rimasti"]], "perdite_totali": tot_perdite, "righe": righe}
    scrivi_json("d6_dicom_anon.json", res)
    print({k: v for k, v in res.items() if k != "righe"})


main()
