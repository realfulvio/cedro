#!/usr/bin/env python3
"""C1-C5: genera il corpus di firme da produttori INDIPENDENTI da Cedro (OpenSSL, pyHanko, EU DSS) e le sue manomissioni.

Uso: genera_corpus.py <cartella-uscita>
Scrive: <uscita>/pki/*.pem|cer, <uscita>/file/*, <uscita>/attesi.json
attesi.json: per ogni file {"classe": "integra"|"manomessa", "fiducia": "ok"|"non_ok", "nota": ...} deciso PER COSTRUZIONE,
cioè senza chiedere a nessun validatore: è la verità con cui si misurano Cedro e DSS.
Seme fisso: le chiavi cambiano a ogni giro (cryptography non ha seme), ma le classi no.
"""
import asyncio, datetime as dt, io, json, os, subprocess, sys
from pathlib import Path

from asn1crypto import cms as acms
# asn1crypto 1.5.1 (PyPI) non conosce ancora alcuni OID CAdES che pyHanko usa: si registrano a mano (sono solo nomi, dalla RFC 5035/3161).
for _oid, _nome in {"1.2.840.113549.1.9.16.2.12": "signing_certificate", "1.2.840.113549.1.9.16.2.47": "signing_certificate_v2",
                    "1.2.840.113549.1.9.16.2.20": "content_time_stamp", "1.2.840.113549.1.9.16.2.14": "signature_time_stamp_token"}.items():
    acms.CMSAttributeType._map.setdefault(_oid, _nome)
acms.ContentType._map.setdefault("1.2.840.113549.1.9.16.1.4", "tst_info")
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, rsa
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

OUT = Path(sys.argv[1]).resolve()
PKI, FILE = OUT / "pki", OUT / "file"
for d in (PKI, FILE):
    d.mkdir(parents=True, exist_ok=True)
UTC = dt.timezone.utc
NOW = dt.datetime.now(UTC)
attesi = {}


def nome(cn, o="Banco Cedro"):
    return x509.Name([x509.NameAttribute(NameOID.COUNTRY_NAME, "IT"), x509.NameAttribute(NameOID.ORGANIZATION_NAME, o), x509.NameAttribute(NameOID.COMMON_NAME, cn)])


def chiave(tipo):
    if tipo == "rsa": return rsa.generate_private_key(65537, 2048)
    if tipo == "p256": return ec.generate_private_key(ec.SECP256R1())
    if tipo == "p384": return ec.generate_private_key(ec.SECP384R1())
    raise ValueError(tipo)


def cert(cn, k, issuer_name, issuer_key, ca=False, da=NOW - dt.timedelta(days=400), a=NOW + dt.timedelta(days=3000), nonrep=True, h=hashes.SHA256(), tsa=False):
    b = (x509.CertificateBuilder().subject_name(nome(cn)).issuer_name(issuer_name).public_key(k.public_key())
         .serial_number(x509.random_serial_number()).not_valid_before(da).not_valid_after(a)
         .add_extension(x509.BasicConstraints(ca=ca, path_length=None), critical=True))
    if ca:
        b = b.add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), critical=True)
    elif tsa:
        b = b.add_extension(x509.KeyUsage(True, nonrep, False, False, False, False, False, False, False), critical=True).add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.TIME_STAMPING]), critical=True)
    else:
        b = b.add_extension(x509.KeyUsage(True, nonrep, False, False, False, False, False, False, False), critical=True)
    b = b.add_extension(x509.SubjectKeyIdentifier.from_public_key(k.public_key()), critical=False)
    return b.sign(issuer_key, h)


def pem(path, c=None, k=None):
    if c is not None: Path(path).write_bytes(c.public_bytes(serialization.Encoding.PEM))
    if k is not None: Path(path).write_bytes(k.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))


# --- PKI: radice, emittente, firmatari ---
root_k = chiave("rsa"); root = cert("Banco Cedro Radice", root_k, nome("Banco Cedro Radice"), root_k, ca=True)
ca_k = chiave("rsa"); ca = cert("Banco Cedro CA Firma", ca_k, root.subject, root_k, ca=True)
PERSONE = {
    "rsa":      dict(tipo="rsa"),
    "p256":     dict(tipo="p256"),
    "p384":     dict(tipo="p384"),
    "sha1":     dict(tipo="rsa"),
    "scaduto":  dict(tipo="rsa", da=NOW - dt.timedelta(days=900), a=NOW - dt.timedelta(days=300)),
    "futuro":   dict(tipo="rsa", da=NOW + dt.timedelta(days=300), a=NOW + dt.timedelta(days=900)),
    "senzanonrep": dict(tipo="rsa", nonrep=False),
}
persona = {}
for t, kw in PERSONE.items():
    k = chiave(kw.pop("tipo"))
    c = cert(f"Firmatario {t}", k, ca.subject, ca_k, **kw)
    persona[t] = (c, k)
    pem(PKI / f"{t}.pem", c, None); pem(PKI / f"{t}.key", None, k)
# firmatario con certificato di un'altra CA, mai vista (non fidata)
estr_root_k = chiave("rsa"); estr_root = cert("Radice Sconosciuta", estr_root_k, nome("Radice Sconosciuta"), estr_root_k, ca=True)
k = chiave("rsa"); persona["estraneo"] = (cert("Firmatario estraneo", k, estr_root.subject, estr_root_k), k)
pem(PKI / "estraneo.pem", persona["estraneo"][0], None); pem(PKI / "estraneo.key", None, k)
pem(PKI / "radice.pem", root); pem(PKI / "ca.pem", ca)
(PKI / "radice.cer").write_bytes(root.public_bytes(serialization.Encoding.DER))
(PKI / "ca.cer").write_bytes(ca.public_bytes(serialization.Encoding.DER))
catena = PKI / "catena.pem"; catena.write_bytes(ca.public_bytes(serialization.Encoding.PEM) + root.public_bytes(serialization.Encoding.PEM))
# TSA di prova
tsa_k = chiave("rsa"); tsa = cert("Banco Cedro TSA", tsa_k, ca.subject, ca_k, tsa=True, nonrep=False)
pem(PKI / "tsa.pem", tsa, None); pem(PKI / "tsa.key", None, tsa_k)

# CRL con il certificato «revocato»
rev_k = chiave("rsa"); rev_c = cert("Firmatario revocato", rev_k, ca.subject, ca_k)
persona["revocato"] = (rev_c, rev_k); pem(PKI / "revocato.pem", rev_c, None); pem(PKI / "revocato.key", None, rev_k)
crl = (x509.CertificateRevocationListBuilder().issuer_name(ca.subject).last_update(NOW - dt.timedelta(days=1)).next_update(NOW + dt.timedelta(days=30))
       .add_revoked_certificate(x509.RevokedCertificateBuilder().serial_number(rev_c.serial_number).revocation_date(NOW - dt.timedelta(days=60)).build())
       .sign(ca_k, hashes.SHA256()))
(PKI / "ca.crl").write_bytes(crl.public_bytes(serialization.Encoding.DER))

DOC = "Importo dovuto: 1000 euro. Convenzione di prova del banco Cedro.\n".encode() * 20
(FILE / "documento.txt").write_bytes(DOC)


def reg(nome_file, classe, fiducia="ok", nota="", produttore="", profilo=""):
    attesi[nome_file] = dict(classe=classe, fiducia=fiducia, nota=nota, produttore=produttore, profilo=profilo)


def openssl_sign(out, tags, doc=FILE / "documento.txt", detached=False, md="sha256", pss=False, cades=True):
    cmd = ["openssl", "cms", "-sign", "-binary", "-in", str(doc), "-outform", "DER", "-out", str(out), "-md", md, "-certfile", str(catena)]
    for t in tags:
        cmd += ["-signer", str(PKI / f"{t}.pem"), "-inkey", str(PKI / f"{t}.key")]
        if pss: cmd += ["-keyopt", "rsa_padding_mode:pss"]
    if cades: cmd += ["-cades"]
    if not detached: cmd += ["-nodetach"]
    subprocess.run(cmd, check=True, capture_output=True)


# --- A. CAdES con OpenSSL ---
casi = [
    ("ossl_rsa_sha256.p7m", ["rsa"], "sha256", False, "ok", "integra"),
    ("ossl_p256_sha256.p7m", ["p256"], "sha256", False, "ok", "integra"),
    ("ossl_p384_sha384.p7m", ["p384"], "sha384", False, "ok", "integra"),
    ("ossl_rsa_sha512.p7m", ["rsa"], "sha512", False, "ok", "integra"),
    ("ossl_rsa_sha1.p7m", ["sha1"], "sha1", False, "non_ok", "integra"),            # SHA-1: Cedro/DSS devono avvisare
    ("ossl_due_firme.p7m", ["rsa", "p256"], "sha256", False, "ok", "integra"),
    ("ossl_scaduto.p7m", ["scaduto"], "sha256", False, "non_ok", "integra"),        # certificato scaduto, nessuna marca
    ("ossl_futuro.p7m", ["futuro"], "sha256", False, "non_ok", "integra"),
    ("ossl_revocato.p7m", ["revocato"], "sha256", False, "non_ok", "integra"),
    ("ossl_senza_nonrep.p7m", ["senzanonrep"], "sha256", False, "ok", "integra"),
    ("ossl_estraneo.p7m", ["estraneo"], "sha256", False, "non_ok", "integra"),      # catena non fidata
    ("ossl_rsa_sha256_staccata.p7s", ["rsa"], "sha256", True, "ok", "integra"),
]
for f, tags, md, det, fid, cl in casi:
    openssl_sign(FILE / f, tags, detached=det, md=md)
    reg(f, cl, fid, produttore="openssl", profilo="CAdES-B-B")
openssl_sign(FILE / "ossl_rsa_pss.p7m", ["rsa"], pss=True); reg("ossl_rsa_pss.p7m", "integra", "ok", produttore="openssl", profilo="CAdES-B-B")
openssl_sign(FILE / "ossl_senza_cades.p7m", ["rsa"], cades=False); reg("ossl_senza_cades.p7m", "integra", "ok", "senza signingCertificateV2", "openssl", "CMS")
# la .p7s staccata va accanto al suo documento: copia con lo stesso nome
(FILE / "ossl_rsa_sha256_staccata").write_bytes(DOC)


# --- B. manomissioni delle buste (tutte con asn1crypto, un solo cambiamento alla volta) ---
def carica(f): return acms.ContentInfo.load((FILE / f).read_bytes())


def salva(ci, f): (FILE / f).write_bytes(ci.dump(force=True))


def mut(base, nuovo, cosa):
    """Un solo cambiamento alla volta, sui byte della busta (asn1crypto serve solo a LEGGERE dove stanno)."""
    raw = bytearray((FILE / base).read_bytes())
    sd = carica(base)["content"]
    si = sd["signer_infos"][0]

    def cerca(x):
        i = bytes(raw).find(bytes(x))
        assert i >= 0, cosa
        return i

    if cosa == "contenuto":       # stessa lunghezza
        i = cerca(b"1000 euro"); raw[i:i + 4] = b"9000"
    elif cosa == "firma":
        i = cerca(si["signature"].native) + 20; raw[i] ^= 0x01
    elif cosa == "digest":
        d = [a["values"][0].native for a in si["signed_attrs"] if a["type"].dotted == "1.2.840.113549.1.9.4"][0]
        i = cerca(d); raw[i] ^= 0xFF
    elif cosa == "tempo":          # cambia l'ora dichiarata senza rifirmare
        t = [a["values"][0] for a in si["signed_attrs"] if a["type"].dotted == "1.2.840.113549.1.9.5"][0]
        d = t.contents
        i = cerca(d); raw[i + 1] = ord("1") if raw[i + 1] != ord("1") else ord("2")
    elif cosa == "certificato":    # un byte nel certificato del firmatario (la chiave pubblica cambia)
        c = sd["certificates"][0].chosen.dump()
        i = cerca(c) + len(c) // 2; raw[i] ^= 0x01
    (FILE / nuovo).write_bytes(bytes(raw))
    reg(nuovo, "manomessa", "non_ok", cosa, "openssl", "CAdES-B-B")


for cosa in ["contenuto", "firma", "digest", "tempo", "certificato"]:
    mut("ossl_rsa_sha256.p7m", f"manom_{cosa}.p7m", cosa)
# staccata: documento cambiato
(FILE / "manom_staccata.p7s").write_bytes((FILE / "ossl_rsa_sha256_staccata.p7s").read_bytes())
(FILE / "manom_staccata").write_bytes(DOC.replace(b"1000 euro", b"9000 euro", 1)); reg("manom_staccata.p7s", "manomessa", "non_ok", "documento staccato cambiato", "openssl", "CAdES-B-B")
# troncata
b = (FILE / "ossl_rsa_sha256.p7m").read_bytes(); (FILE / "manom_troncata.p7m").write_bytes(b[: len(b) // 2]); reg("manom_troncata.p7m", "manomessa", "non_ok", "busta troncata", "openssl", "CAdES-B-B")

# --- C. PAdES con pyHanko ---
import fitz
from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
from pyhanko.sign import fields, signers
from pyhanko.sign.timestamps.dummy_client import DummyTimeStamper
from pyhanko_certvalidator.registry import SimpleCertificateStore
from asn1crypto import x509 as ax, keys as akeys


def base_pdf(path):
    d = fitz.open(); p = d.new_page()
    p.insert_text((72, 100), "Importo dovuto: 1000 euro", fontsize=14)
    p.insert_text((72, 140), "Contratto di prova del banco Cedro", fontsize=12)
    tmp = str(path) + ".tmp"
    d.save(tmp); d.close()
    import pikepdf   # flussi non compressi: serve per cambiare il testo di un byte senza rompere la struttura
    with pikepdf.open(tmp) as q:
        q.save(path, compress_streams=False, stream_decode_level=pikepdf.StreamDecodeLevel.generalized)
    os.remove(tmp)


def a_cert(c): return ax.Certificate.load(c.public_bytes(serialization.Encoding.DER))
def a_key(k): return akeys.PrivateKeyInfo.load(k.private_bytes(serialization.Encoding.DER, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))


reg_store = SimpleCertificateStore.from_certs([a_cert(ca), a_cert(root)])


def signer(tag): c, k = persona[tag]; return signers.SimpleSigner(a_cert(c), a_key(k), reg_store, embed_roots=False)


def pades(src, dst, tag, campo="Firma1", ts=False, md="sha256", certifica=False):
    with open(src, "rb") as f:
        w = IncrementalPdfFileWriter(f)
        fields.append_signature_field(w, fields.SigFieldSpec(campo, box=(50, 600, 250, 660), on_page=0)) if False else None
        meta = signers.PdfSignatureMetadata(field_name=campo, subfilter=fields.SigSeedSubFilter.PADES, md_algorithm=md, embed_validation_info=False)
        stamper = DummyTimeStamper(a_cert(tsa), a_key(tsa_k), certs_to_embed=SimpleCertificateStore.from_certs([a_cert(ca), a_cert(root)])) if ts else None
        out = signers.PdfSigner(meta, signer=signer(tag), timestamper=stamper).sign_pdf(w, in_place=False)
    Path(dst).write_bytes(out.getbuffer())


base_pdf(FILE / "base.pdf")
for f, tag, ts, fid, md in [("hanko_rsa_b.pdf", "rsa", False, "ok", "sha256"), ("hanko_rsa_t.pdf", "rsa", True, "ok", "sha256"), ("hanko_p256_b.pdf", "p256", False, "ok", "sha256"),
                            ("hanko_p384_b.pdf", "p384", False, "ok", "sha384"), ("hanko_scaduto_b.pdf", "scaduto", False, "non_ok", "sha256"),
                            ("hanko_revocato_b.pdf", "revocato", False, "non_ok", "sha256"), ("hanko_estraneo_b.pdf", "estraneo", False, "non_ok", "sha256")]:
    pades(FILE / "base.pdf", FILE / f, tag, ts=ts, md=md)
    reg(f, "integra", fid, produttore="pyhanko", profilo="PAdES-B-T" if ts else "PAdES-B-B")
# due firme in sequenza (la prima non deve invalidarsi)
pades(FILE / "hanko_rsa_b.pdf", FILE / "hanko_due_firme.pdf", "p256", campo="Firma2"); reg("hanko_due_firme.pdf", "integra", "ok", "due firme successive", "pyhanko", "PAdES-B-B")

# manomissioni PDF
raw = (FILE / "hanko_rsa_b.pdf").read_bytes()
HEX1000 = b"31303030206575726f"   # «1000 euro» in esadecimale nel flusso di contenuto
assert HEX1000 in raw
(FILE / "manom_pdf_testo.pdf").write_bytes(raw.replace(HEX1000, b"39303030206575726f", 1)); reg("manom_pdf_testo.pdf", "manomessa", "non_ok", "testo cambiato nel contenuto firmato", "pyhanko", "PAdES-B-B")
i = raw.index(b"/Contents <") + 20
bb = bytearray(raw); bb[i] = ord("0") if bb[i] != ord("0") else ord("1")
(FILE / "manom_pdf_firma.pdf").write_bytes(bytes(bb)); reg("manom_pdf_firma.pdf", "manomessa", "non_ok", "byte della firma cambiato", "pyhanko", "PAdES-B-B")
# aggiornamento incrementale dopo la firma: la firma resta integra sulla sua parte, ma il documento è stato modificato dopo
with open(FILE / "hanko_rsa_b.pdf", "rb") as f:
    w = IncrementalPdfFileWriter(f)
    w.root["/Lang"] = __import__("pyhanko.pdf_utils.generic", fromlist=["TextStringObject"]).TextStringObject("it-IT")
    w.update_root()
    o = io.BytesIO(); w.write(o)
(FILE / "agg_dopo_firma.pdf").write_bytes(o.getvalue()); reg("agg_dopo_firma.pdf", "integra", "non_ok", "modifica incrementale benigna dopo la firma: va segnalata", "pyhanko", "PAdES-B-B")
# ByteRange spostato: la firma non copre più tutto il file
i = raw.index(b"/ByteRange"); j = raw.index(b"]", i)
br = raw[i:j + 1].decode()
import re
nums = list(map(int, re.findall(r"\d+", br)))
nuovo = f"/ByteRange [{nums[0]} {nums[1]} {nums[2] + 1} {nums[3] - 1}]".ljust(len(br)).encode()
(FILE / "manom_pdf_byterange.pdf").write_bytes(raw[:i] + nuovo + raw[j + 1:]); reg("manom_pdf_byterange.pdf", "manomessa", "non_ok", "ByteRange alterato", "pyhanko", "PAdES-B-B")


# --- D. EU DSS come terzo produttore (CAdES-B incapsulata e staccata, PAdES-B), se DSS_LIB è impostata ---
if os.environ.get("DSS_LIB"):
    for tag in ("rsa", "p256"):
        subprocess.run(["openssl", "pkcs12", "-export", "-inkey", str(PKI / f"{tag}.key"), "-in", str(PKI / f"{tag}.pem"), "-certfile", str(catena), "-out", str(PKI / f"{tag}.p12"), "-passout", "pass:x"], check=True)
    shutil_src = FILE / "dss_rsa_pades_b.sorgente.pdf"
    (FILE / "base.pdf").replace(shutil_src) if False else shutil_src.write_bytes((FILE / "base.pdf").read_bytes())
    cp = f"{os.environ['DSS_LIB']}/*:{os.environ.get('DSS_BIN', '/tmp/dssbin')}"
    for tag, nome_f in (("rsa", "dss_rsa"), ("p256", "dss_p256")):
        pdf = str(FILE / f"{nome_f}_pades_b.pdf") if tag == "rsa" else "-"
        if tag == "rsa":
            subprocess.run(["java", "-cp", cp, "Dss", "firma", str(PKI / f"{tag}.p12"), "x", str(FILE / "documento.txt"), str(FILE / f"{nome_f}_cades_b.p7m"), str(FILE / "dss_rsa_pades_b.pdf")], check=True, capture_output=True)
            reg("dss_rsa_pades_b.pdf", "integra", "ok", produttore="dss", profilo="PAdES-B-B")
        else:
            subprocess.run(["java", "-cp", cp, "Dss", "firma", str(PKI / f"{tag}.p12"), "x", str(FILE / "documento.txt"), str(FILE / f"{nome_f}_cades_b.p7m"), "-"], check=True, capture_output=True)
        reg(f"{nome_f}_cades_b.p7m", "integra", "ok", produttore="dss", profilo="CAdES-B-B")
    subprocess.run(["java", "-cp", cp, "Dss", "firma", str(PKI / "rsa.p12"), "x", str(FILE / "documento.txt"), str(FILE / "dss_rsa_cades_staccata.p7s"), "-", "staccata"], check=True, capture_output=True)
    (FILE / "dss_rsa_cades_staccata").write_bytes(DOC); reg("dss_rsa_cades_staccata.p7s", "integra", "ok", produttore="dss", profilo="CAdES-B-B")
    # manomissione di un file di DSS: il testo del PDF
    raw2 = (FILE / "dss_rsa_pades_b.pdf").read_bytes()
    if HEX1000 in raw2:
        (FILE / "manom_dss_pdf_testo.pdf").write_bytes(raw2.replace(HEX1000, b"39303030206575726f", 1)); reg("manom_dss_pdf_testo.pdf", "manomessa", "non_ok", "testo cambiato", "dss", "PAdES-B-B")

(OUT / "attesi.json").write_text(json.dumps(attesi, indent=1, ensure_ascii=False, sort_keys=True))
print(f"{len(attesi)} file, {sum(1 for v in attesi.values() if v['classe'] == 'manomessa')} manomessi -> {OUT}")
