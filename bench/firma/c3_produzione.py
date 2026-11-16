#!/usr/bin/env python3
"""C3/C4: i file FIRMATI DA CEDRO (CAdES .p7m e PAdES .pdf, firmati con chiave dentro SoftHSM via PKCS#11) li accettano i validatori indipendenti?
Uso: c3_produzione.py <cartella con softhsm.pdf e softhsm.txt.p7m> <cert.der del firmatario> [--dss-lib JAR] [--dss-bin CLASSI]
Come si ottengono i file: source tools/prova-softhsm.sh; CEDRO_SOFTHSM_OUT=<cartella> cargo test --lib firma::pkcs11 -- --nocapture
Il certificato di prova è autofirmato: per DSS e OpenSSL è l'àncora di fiducia, così si misura la firma e non la catena."""
import json, logging, os, subprocess, sys
logging.disable(logging.CRITICAL)
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import esegui, scrivi_json
cart, cert = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
a = sys.argv[3:]
lib = a[a.index("--dss-lib") + 1] if "--dss-lib" in a else os.environ.get("DSS_LIB", "")
binc = a[a.index("--dss-bin") + 1] if "--dss-bin" in a else os.environ.get("DSS_BIN", "/tmp/dssbin")
pem = cart / "cert.pem"
subprocess.run(["openssl", "x509", "-inform", "DER", "-in", str(cert), "-out", str(pem)], check=True)
res = {}
for f in ("softhsm.txt.p7m", "softhsm.pdf"):
    p = cart / f
    r = {}
    if lib:
        o = esegui(["java", "-cp", f"{lib}/*:{binc}", "Dss", "valida", str(cert), "-", "-", str(p)], timeout=120)
        r["dss"] = [l.split("\t")[1:] for l in o["out"].splitlines() if l.startswith(f)]
    if f.endswith(".p7m"):
        o = esegui(["openssl", "cms", "-verify", "-binary", "-inform", "DER", "-in", str(p), "-CAfile", str(pem), "-purpose", "any", "-out", os.devnull], timeout=30)
        r["openssl_rc"] = o["rc"]; r["openssl"] = (o["err"] or o["out"]).strip()[:120]
    else:
        o = esegui(["pdfsig", str(p)], timeout=30)
        r["pdfsig"] = [l.strip() for l in (o["out"] + o["err"]).splitlines() if "Validation" in l or "Signer" in l or "Digest" in l][:4]
        try:
            from pyhanko.pdf_utils.reader import PdfFileReader
            from pyhanko.sign.validation import validate_pdf_signature
            with open(p, "rb") as fh:
                rd = PdfFileReader(fh)
                r["pyhanko"] = [{"intact": s.intact if hasattr(s, "intact") else None, "valid": s.valid} for s in (validate_pdf_signature(x) for x in rd.embedded_signatures)]
        except Exception as e:
            r["pyhanko"] = str(e)[:100]
    res[f] = r
scrivi_json("c3_produzione.json", res)
print(json.dumps(res, indent=1, ensure_ascii=False))
