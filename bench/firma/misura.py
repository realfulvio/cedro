#!/usr/bin/env python3
"""C1-C5: misura la VERIFICA di Cedro contro la verità costruita (attesi.json) e contro EU DSS, OpenSSL, pdfsig, pyHanko.

Uso: misura.py <cartella-corpus> [--dss-lib CARTELLA_JAR] [--dss-bin CARTELLA_CLASSI]
Il corpus si crea con genera_corpus.py. Scrive bench/risultati/c_firma_verifica.json.
Verdetti normalizzati: "ok" (✓), "avviso" (!), "no" (✗).
Regole del protocollo (§4):
  C2 falso positivo grave  = file MANOMESSO che Cedro non dà come ✗
  C1 concordanza          = Cedro ✗/non-✗ uguale a DSS; e verdetto a tre valori uguale
  fiducia "non_ok" (integra ma scaduta/revocata/estranea/SHA-1) = Cedro non deve dare ✓
"""
import base64, json, logging, os, re, shutil, subprocess, sys, tempfile
logging.disable(logging.CRITICAL)   # pyHanko scrive tracce lunghe per ogni catena non fidata
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import CEDRO, esegui, scrivi_json

corpus = Path(sys.argv[1]).resolve()
argv = sys.argv[2:]
dss_lib = argv[argv.index("--dss-lib") + 1] if "--dss-lib" in argv else os.environ.get("DSS_LIB", "")
dss_bin = argv[argv.index("--dss-bin") + 1] if "--dss-bin" in argv else os.environ.get("DSS_BIN", "/tmp/dssbin")
attesi = json.loads((corpus / "attesi.json").read_text())
FILE = corpus / "file"
PKI = corpus / "pki"

# --- 1. elenco di fiducia di Cedro con la CA del corpus (come se fosse un certificatore dell'elenco UE) ---
casa = Path(tempfile.mkdtemp(prefix="cedro-casa-"))
(casa / ".config/cedro/firma").mkdir(parents=True)
b64 = lambda p: base64.b64encode(p.read_bytes()).decode()
adesso = int(__import__("time").time())
(casa / ".config/cedro/firma/fiducia.txt").write_text(
    f"cedro-fiducia 1\t{adesso}\n"
    f"CA\tIT\tBanco Cedro\tCA Firma\tg1\t{b64(PKI / 'ca.cer')}\n"
    f"CA\tIT\tBanco Cedro\tRadice\tg1\t{b64(PKI / 'radice.cer')}\n"
)
env = {**os.environ, "HOME": str(casa), "XDG_CONFIG_HOME": str(casa / ".config")}


def cedro_verdetto(f):
    r = esegui([CEDRO, "firma", str(FILE / f)], timeout=120, env=env)
    righe = r["out"].splitlines()
    # prima riga = nome file; seconda = esito complessivo, che comincia con ✓ ! ✗
    esito = righe[1].strip() if len(righe) > 1 else ""
    v = {"✓": "ok", "!": "avviso", "✗": "no"}.get(esito[:1], "errore")
    return {"verdetto": v, "rc": r["rc"], "s": r["s"], "rss_mb": r["rss_mb"], "riga": esito[:160]}


# --- 2. EU DSS ---
def dss_verdetti(files):
    if not dss_lib:
        return {}
    cmd = ["java", "-cp", f"{dss_lib}/*:{dss_bin}", "Dss", "valida", str(PKI / "radice.cer"), str(PKI / "ca.cer"), str(PKI / "ca.crl")] + [str(FILE / f) for f in files]
    r = esegui(cmd, timeout=900)
    out = {}
    for riga in r["out"].splitlines():
        c = riga.split("\t")
        if len(c) >= 4:
            out.setdefault(c[0], []).append({"formato": c[1], "indicazione": c[2], "sotto": c[3], "messaggi": c[4] if len(c) > 4 else ""})
    return out


def dss_norm(lst):
    if not lst:
        return "errore"
    ind = [x["indicazione"] for x in lst]
    if any(i in ("TOTAL_FAILED", "ERRORE", "NESSUNA_FIRMA") for i in ind):
        return "no"
    if any(i == "INDETERMINATE" for i in ind):
        return "avviso"
    return "ok"


# --- 3. OpenSSL (solo CMS) e pdfsig (solo PDF): integrità ---
def openssl_integro(f):
    if not f.endswith((".p7m", ".p7s")):
        return None
    cmd = ["openssl", "cms", "-verify", "-binary", "-inform", "DER", "-in", str(FILE / f), "-noverify", "-out", os.devnull]
    if f.endswith(".p7s"):
        cmd += ["-content", str(FILE / f[:-4])]
    r = esegui(cmd, timeout=30)
    return r["rc"] == 0


def pdfsig_integro(f):
    if not f.endswith(".pdf") or f == "base.pdf":
        return None
    r = esegui(["pdfsig", str(FILE / f)], timeout=30)
    t = r["out"] + r["err"]
    if "Signature Validation: Signature is Valid" in t and "Signature is Invalid" not in t and "digest mismatch" not in t.lower():
        return True
    return False


def hanko_integro(f):
    if not f.endswith(".pdf") or f == "base.pdf":
        return None
    try:
        from pyhanko.pdf_utils.reader import PdfFileReader
        from pyhanko.sign.validation import validate_pdf_signature
        from pyhanko_certvalidator import ValidationContext
        with open(FILE / f, "rb") as fh:
            r = PdfFileReader(fh)
            sigs = r.embedded_signatures
            if not sigs:
                return False
            return all(validate_pdf_signature(s).intact for s in sigs)
    except Exception:
        return False


files = sorted(attesi)
dss = dss_verdetti(files)
righe, fp, fr, nok_fiducia, concord2, concord3, n_dss = [], [], [], [], 0, 0, 0
for f in files:
    a = attesi[f]
    c = cedro_verdetto(f)
    d = dss_norm(dss.get(f)) if dss else None
    riga = {"file": f, "atteso": a, "cedro": c, "dss": d, "dss_dettaglio": dss.get(f), "openssl_integro": openssl_integro(f), "pdfsig_integro": pdfsig_integro(f), "pyhanko_integro": hanko_integro(f)}
    # verità
    if a["classe"] == "manomessa" and c["verdetto"] != "no":
        fp.append(f)
    if a["classe"] == "integra" and a["fiducia"] == "ok" and c["verdetto"] == "no":
        fr.append(f)
    if a["classe"] == "integra" and a["fiducia"] == "non_ok" and c["verdetto"] == "ok":
        nok_fiducia.append(f)
    if d is not None:
        n_dss += 1
        concord2 += (c["verdetto"] == "no") == (d == "no")
        concord3 += c["verdetto"] == d
    righe.append(riga)

res = {
    "file": len(files), "manomessi": sum(a["classe"] == "manomessa" for a in attesi.values()),
    "C2_falsi_positivi_gravi": fp, "falsi_rifiuti_su_file_buoni": fr, "fiducia_ok_su_file_non_fidabili": nok_fiducia,
    "C1_concordanza_con_DSS_si_no": f"{concord2}/{n_dss}" if n_dss else "n/d", "C1_concordanza_con_DSS_3_valori": f"{concord3}/{n_dss}" if n_dss else "n/d",
    "tempo_mediano_s": sorted(r["cedro"]["s"] for r in righe)[len(righe) // 2], "rss_max_mb": max(r["cedro"]["rss_mb"] for r in righe),
    "righe": righe,
}
scrivi_json("c_firma_verifica.json", res)
shutil.rmtree(casa, ignore_errors=True)
print(f"file {res['file']} (manomessi {res['manomessi']}) | falsi positivi gravi: {len(fp)} {fp} | falsi rifiuti: {len(fr)} {fr}")
print(f"fiducia ✓ su file non fidabili: {nok_fiducia}")
print(f"concordanza con DSS: si/no {res['C1_concordanza_con_DSS_si_no']}, tre valori {res['C1_concordanza_con_DSS_3_valori']}")
for r in righe:
    if r["dss"] is not None and (r["cedro"]["verdetto"] == "no") != (r["dss"] == "no"):
        print("  DISCORDE:", r["file"], "cedro:", r["cedro"]["verdetto"], "dss:", r["dss"], r["dss_dettaglio"])
