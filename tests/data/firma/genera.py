#!/usr/bin/env python3
"""Genera i file firmati di prova per i test di src/firma (una PKI finta «Cedro Test»).

Serve pyhanko (pip install pyhanko). Si lancia dalla cartella del repository:
    python3 tests/data/firma/genera.py
I file sono già nel repository: rilanciarlo solo per cambiarli.
"""
import asyncio, base64, datetime as dt, io, os, sys
from asn1crypto import cms, keys, x509 as ax, core, tsp, algos, crl as acrl
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa, ec, padding
from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID
from pyhanko.sign import signers
from pyhanko.sign.signers.pdf_cms import PdfCMSSignedAttributes
from pyhanko.sign.timestamps.dummy_client import DummyTimeStamper
from pyhanko_certvalidator.registry import SimpleCertificateStore
from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
from pyhanko.sign.fields import SigFieldSpec

OUT = os.path.dirname(os.path.abspath(__file__))
UTC = dt.timezone.utc
D = lambda y, m, d, h=12: dt.datetime(y, m, d, h, tzinfo=UTC)


def rsa_key():
    return rsa.generate_private_key(public_exponent=65537, key_size=2048)


def name(cn, **kw):
    attrs = [x509.NameAttribute(NameOID.COUNTRY_NAME, "IT")]
    if "o" in kw:
        attrs.append(x509.NameAttribute(NameOID.ORGANIZATION_NAME, kw["o"]))
    if "gn" in kw:
        attrs.append(x509.NameAttribute(NameOID.GIVEN_NAME, kw["gn"]))
        attrs.append(x509.NameAttribute(NameOID.SURNAME, kw["sn"]))
        attrs.append(x509.NameAttribute(NameOID.SERIAL_NUMBER, kw["cf"]))
    if "t" in kw:
        attrs.append(x509.NameAttribute(NameOID.TITLE, kw["t"]))
    attrs.append(x509.NameAttribute(NameOID.COMMON_NAME, cn))
    return x509.Name(attrs)


# QcStatements: QcCompliance + QcSSCD + QcType esign


def qc_der():
    def seq(*parts):
        b = b"".join(parts)
        return b"\x30" + bytes([len(b)]) + b

    def oid(dotted):
        return core.ObjectIdentifier(dotted).dump()

    comp = seq(oid("0.4.0.1862.1.1"))
    sscd = seq(oid("0.4.0.1862.1.4"))
    qtype = seq(oid("0.4.0.1862.1.6"), seq(oid("0.4.0.1862.1.6.1")))
    return seq(comp, sscd, qtype)


def cert(subject, key, issuer_name, issuer_key, nb, na, ca=False, qc=False, eku=None, serial=None, ocsp=None, crl=None):
    b = (
        x509.CertificateBuilder()
        .subject_name(subject)
        .issuer_name(issuer_name)
        .public_key(key.public_key())
        .serial_number(serial or x509.random_serial_number())
        .not_valid_before(nb)
        .not_valid_after(na)
        .add_extension(x509.BasicConstraints(ca=ca, path_length=None), critical=True)
        .add_extension(x509.SubjectKeyIdentifier.from_public_key(key.public_key()), critical=False)
        .add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(issuer_key.public_key()), critical=False)
    )
    if ca:
        b = b.add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), critical=True)
    elif eku is None:
        b = b.add_extension(x509.KeyUsage(False, True, False, False, False, False, False, False, False), critical=True)
    if qc:
        b = b.add_extension(x509.UnrecognizedExtension(x509.ObjectIdentifier("1.3.6.1.5.5.7.1.3"), qc_der()), critical=False)
    if eku:
        b = b.add_extension(x509.ExtendedKeyUsage(eku), critical=True)
    if ocsp:
        b = b.add_extension(x509.AuthorityInformationAccess([x509.AccessDescription(x509.oid.AuthorityInformationAccessOID.OCSP, x509.UniformResourceIdentifier(ocsp))]), critical=False)
    if crl:
        b = b.add_extension(x509.CRLDistributionPoints([x509.DistributionPoint([x509.UniformResourceIdentifier(crl)], None, None, None)]), critical=False)
    return b.sign(issuer_key, hashes.SHA256())


def a_cert(c):
    return ax.Certificate.load(c.public_bytes(serialization.Encoding.DER))


def a_key(k):
    return keys.PrivateKeyInfo.load(k.private_bytes(serialization.Encoding.DER, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))


def write(fn, data):
    with open(os.path.join(OUT, fn), "wb") as f:
        f.write(data)


root_k = rsa_key()
root_n = name("Cedro Test Root", o="Cedro Test")
root = cert(root_n, root_k, root_n, root_k, D(2020, 1, 1), D(2040, 1, 1), ca=True)
ca_k = rsa_key()
ca_n = name("Cedro Test Firma Qualificata", o="Cedro Test S.p.A.")
ca = cert(ca_n, ca_k, root_n, root_k, D(2020, 1, 1), D(2040, 1, 1), ca=True)
tsa_k = rsa_key()
tsa_n = name("Cedro Test TSA", o="Cedro Test S.p.A.")
tsa = cert(tsa_n, tsa_k, root_n, root_k, D(2020, 1, 1), D(2040, 1, 1), eku=[ExtendedKeyUsageOID.TIME_STAMPING])

people = {}


def person(tag, gn, sn, cf, t, o, nb, na, key=None, serial=None, qc=True):
    k = key or rsa_key()
    c = cert(name(f"{gn} {sn}".upper(), gn=gn.upper(), sn=sn.upper(), cf=cf, t=t, o=o), k, ca_n, ca_k, nb, na, qc=qc, serial=serial,
             ocsp="http://127.0.0.1:9/ocsp", crl="http://127.0.0.1:9/ca.crl")
    people[tag] = (c, k)
    return c, k


person("mario", "Mario", "Rossi", "TINIT-RSSMRA71C12A965K", "Dirigente", "Comune di Bondeno", D(2024, 3, 15), D(2034, 3, 14))
person("giulia", "Giulia", "Bianchi", "TINIT-BNCGLI80A41D548P", "Direttrice", "ASP del Delta Ferrarese", D(2023, 11, 1), D(2026, 10, 31), key=ec.generate_private_key(ec.SECP256R1()))
person("paolo", "Paolo", "Neri", "TINIT-NREPLA65M04D548T", "Consulente", "Studio Neri", D(2024, 1, 1), D(2034, 1, 1), serial=4242)
person("anna", "Anna", "Verdi", "TINIT-VRDNNA68E52A965C", "Sindaco", "Comune di Bondeno", D(2024, 1, 1), D(2034, 1, 1))

# CRL del certificatore: Paolo revocato il 12/09/2026 (chiave compromessa).
crl = (
    x509.CertificateRevocationListBuilder()
    .issuer_name(ca_n)
    .last_update(D(2026, 11, 1))
    .next_update(D(2026, 12, 1))
    .add_revoked_certificate(
        x509.RevokedCertificateBuilder().serial_number(4242).revocation_date(D(2026, 9, 12)).add_extension(x509.CRLReason(x509.ReasonFlags.key_compromise), False).build()
    )
    .sign(ca_k, hashes.SHA256())
)
write("ca.crl", crl.public_bytes(serialization.Encoding.DER))
for tag, c in [("root", root), ("ca", ca), ("tsa", tsa)] + [(t, p[0]) for t, p in people.items()]:
    write(f"{tag}.cer", c.public_bytes(serialization.Encoding.DER))

# Lista di fiducia finta, nel formato ETSI TS 119 612 (solo le parti che servono).
def tl_service(kind, c):
    b64 = base64.b64encode(c.public_bytes(serialization.Encoding.DER)).decode()
    return f"""<TSPService><ServiceInformation><ServiceTypeIdentifier>{kind}</ServiceTypeIdentifier>
<ServiceName><Name xml:lang="it">{c.subject.get_attributes_for_oid(NameOID.COMMON_NAME)[0].value}</Name></ServiceName>
<ServiceDigitalIdentity><DigitalId><X509Certificate>{b64}</X509Certificate></DigitalId></ServiceDigitalIdentity>
<ServiceStatus>http://uri.etsi.org/TrstSvc/TrustedList/Svcstatus/granted</ServiceStatus><StatusStartingTime>2020-01-01T00:00:00Z</StatusStartingTime>
</ServiceInformation></TSPService>"""

tl = f"""<?xml version="1.0" encoding="UTF-8"?>
<TrustServiceStatusList xmlns="http://uri.etsi.org/02231/v2#"><TrustServiceProviderList><TrustServiceProvider>
<TSPInformation><TSPName><Name xml:lang="it">Cedro Test S.p.A.</Name></TSPName></TSPInformation><TSPServices>
{tl_service("http://uri.etsi.org/TrstSvc/Svctype/CA/QC", ca)}
{tl_service("http://uri.etsi.org/TrstSvc/Svctype/TSA/QTST", tsa)}
</TSPServices></TrustServiceProvider></TrustServiceProviderList></TrustServiceStatusList>"""
write("tl.xml", tl.encode())

registry = SimpleCertificateStore.from_certs([a_cert(ca), a_cert(root)])


def signer_of(tag):
    c, k = people[tag]
    return signers.SimpleSigner(a_cert(c), a_key(k), registry, embed_roots=False)


def stamper(when):
    return DummyTimeStamper(a_cert(tsa), a_key(tsa_k), certs_to_embed=SimpleCertificateStore.from_certs([a_cert(root)]), fixed_dt=when)


def sign_data(tag, data, when, ts=None, detached=False):
    s = signer_of(tag)
    attrs = PdfCMSSignedAttributes(signing_time=when)
    ci = asyncio.run(s.async_sign_general_data(data, "sha256", detached=detached, use_cades=True, timestamper=ts, signed_attr_settings=attrs))
    return ci


testo = "Convenzione di prova firmata con Cedro.\nBondeno, 2026.\n".encode()
write("documento.txt", testo)

# 1. CAdES-BES
bes = sign_data("mario", testo, D(2026, 11, 8, 16))
write("mario.txt.p7m", bes.dump())
# 2. CAdES-T
t = sign_data("mario", testo, D(2026, 11, 5, 9), ts=stamper(D(2026, 11, 5, 9)))
write("mario_t.txt.p7m", t.dump())
# 3. Busta dentro busta: Giulia (certificato scaduto il 31/10/2026) firma con marca del 3/10.
nested = sign_data("giulia", t.dump(), D(2026, 10, 3, 14), ts=stamper(D(2026, 10, 3, 14)))
write("doppia.txt.p7m.p7m", nested.dump())
# 4. Paolo, revocato prima della firma, con la CRL nella busta.
p = sign_data("paolo", testo, D(2026, 11, 6, 8))
p["content"]["crls"] = cms.RevocationInfoChoices([acrl.CertificateList.load(crl.public_bytes(serialization.Encoding.DER))])
write("paolo.txt.p7m", p.dump())
# 5. Contenuto cambiato dopo la firma.
bad = cms.ContentInfo.load(bes.dump())
bad["content"]["encap_content_info"]["content"] = core.OctetString(testo.replace(b"2026", b"2027"))
write("cambiato.txt.p7m", bad.dump())
# 6. Firma staccata.
det = sign_data("mario", testo, D(2026, 11, 8, 16), detached=True)
write("documento.txt.p7s", det.dump())
# 7. Base64.
write("base64.txt.p7m", base64.encodebytes(bes.dump()))

# 8. Controfirma di Anna sulla firma di Mario.
def countersign(ci, tag):
    ci = cms.ContentInfo.load(ci.dump())
    sd = ci["content"]
    si = sd["signer_infos"][0]
    c, k = people[tag]
    md = hashes.Hash(hashes.SHA256())
    md.update(si["signature"].native)
    digest = md.finalize()
    sa = cms.CMSAttributes([
        cms.CMSAttribute({"type": "signing_time", "values": [cms.Time({"utc_time": D(2026, 11, 8, 17)})]}),
        cms.CMSAttribute({"type": "message_digest", "values": [digest]}),
    ])
    sig = k.sign(sa.dump(), padding.PKCS1v15(), hashes.SHA256())
    ac = a_cert(c)
    csi = cms.SignerInfo({
        "version": "v1",
        "sid": cms.SignerIdentifier({"issuer_and_serial_number": cms.IssuerAndSerialNumber({"issuer": ac.issuer, "serial_number": ac.serial_number})}),
        "digest_algorithm": algos.DigestAlgorithm({"algorithm": "sha256"}),
        "signed_attrs": sa,
        "signature_algorithm": algos.SignedDigestAlgorithm({"algorithm": "sha256_rsa"}),
        "signature": sig,
    })
    ua = si["unsigned_attrs"] if si["unsigned_attrs"].native else cms.CMSAttributes([])
    ua = list(ua) + [cms.CMSAttribute({"type": "counter_signature", "values": [csi]})]
    si["unsigned_attrs"] = cms.CMSAttributes(ua)
    certs = list(sd["certificates"]) + [cms.CertificateChoices({"certificate": ac})]
    sd["certificates"] = cms.CertificateSet(certs)
    return ci

write("controfirma.txt.p7m", countersign(bes, "anna").dump())

# 9. .tsd (RFC 5544) con dentro mario.txt.p7m e una marca sopra.
def ts_token(data, when):
    st = stamper(when)
    md = hashes.Hash(hashes.SHA256())
    md.update(data)
    return asyncio.run(st.async_timestamp(md.finalize(), "sha256"))

tok = ts_token(bes.dump(), D(2026, 11, 8, 18))
TSD_OID = "1.2.840.113549.1.9.16.1.31"
class TimeStampAndCRL(core.Sequence):
    _fields = [("time_stamp", cms.ContentInfo), ("crl", acrl.CertificateList, {"optional": True})]
class TimeStampTokenEvidence(core.SequenceOf):
    _child_spec = TimeStampAndCRL
class Evidence(core.Choice):
    _alternatives = [("tst_evidence", TimeStampTokenEvidence, {"implicit": 0})]
class TimeStampedData(core.Sequence):
    _fields = [("version", core.Integer), ("content", core.OctetString, {"optional": True}), ("temporal_evidence", Evidence)]

tsd_body = TimeStampedData({"version": 1, "content": bes.dump(), "temporal_evidence": Evidence({"tst_evidence": TimeStampTokenEvidence([TimeStampAndCRL({"time_stamp": tok})])})})
body = tsd_body.dump()
# [0] EXPLICIT
def tlv(tag, b):
    n = len(b)
    if n < 0x80:
        l = bytes([n])
    else:
        lb = n.to_bytes((n.bit_length() + 7) // 8, "big")
        l = bytes([0x80 | len(lb)]) + lb
    return bytes([tag]) + l + b
write("mario.txt.p7m.tsd", tlv(0x30, core.ObjectIdentifier(TSD_OID).dump() + tlv(0xA0, body)))

# 10. .m7m: MIME con la busta e la marca (.tsr).
resp = tsp.TimeStampResp({"status": tsp.PKIStatusInfo({"status": "granted"}), "time_stamp_token": tok})
b = "----=_Part_cedro"
m7m = (f"MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"{b}\"\r\n\r\n--{b}\r\n"
       f"Content-Type: application/pkcs7-mime; smime-type=signed-data; name=\"mario.txt.p7m\"\r\nContent-Transfer-Encoding: base64\r\n\r\n"
       + base64.encodebytes(bes.dump()).decode().replace("\n", "\r\n") +
       f"\r\n--{b}\r\nContent-Type: application/timestamp-reply; name=\"mario.txt.tsr\"\r\nContent-Transfer-Encoding: base64\r\n\r\n"
       + base64.encodebytes(resp.dump()).decode().replace("\n", "\r\n") + f"\r\n--{b}--\r\n")
write("mario.txt.m7m", m7m.encode())
write("mario.txt.p7m.tsr", resp.dump())

# 11. PDF firmati.
from pyhanko.pdf_utils.writer import PdfFileWriter
from pyhanko.pdf_utils import generic
from pyhanko.pdf_utils.content import RawContent
from pyhanko.pdf_utils.layout import BoxConstraints

def simple_pdf(text):
    w = PdfFileWriter()
    stream = f"BT /F1 18 Tf 72 720 Td ({text}) Tj ET".encode()
    font = generic.DictionaryObject({generic.NameObject("/Type"): generic.NameObject("/Font"), generic.NameObject("/Subtype"): generic.NameObject("/Type1"), generic.NameObject("/BaseFont"): generic.NameObject("/Helvetica")})
    page = generic.DictionaryObject({
        generic.NameObject("/Type"): generic.NameObject("/Page"),
        generic.NameObject("/MediaBox"): generic.ArrayObject([generic.NumberObject(x) for x in (0, 0, 595, 842)]),
        generic.NameObject("/Resources"): generic.DictionaryObject({generic.NameObject("/Font"): generic.DictionaryObject({generic.NameObject("/F1"): w.add_object(font)})}),
        generic.NameObject("/Contents"): w.add_object(generic.StreamObject(stream_data=stream)),
    })
    w.insert_page(page)
    out = io.BytesIO()
    w.write(out)
    return out.getvalue()

def sign_pdf(data, tag, when, ts=None, field="Firma1", visible=True, reason="Approvazione", box=(350, 60, 550, 120)):
    w = IncrementalPdfFileWriter(io.BytesIO(data))
    meta = signers.PdfSignatureMetadata(field_name=field, reason=reason, location="Bondeno", subfilter=__import__("pyhanko.sign.fields", fromlist=["x"]).SigSeedSubFilter.PADES)
    spec = SigFieldSpec(field, on_page=0, box=box if visible else None)
    from pyhanko.sign.fields import append_signature_field
    append_signature_field(w, spec)
    out = signers.sign_pdf(w, meta, signer=signer_of(tag), timestamper=ts, existing_fields_only=True)
    return out.getvalue()

base = simple_pdf("Determina n. 412: importo 48.300,00 euro")
pdf1 = sign_pdf(base, "mario", D(2026, 11, 9, 10), ts=stamper(D(2026, 11, 9, 10)))
write("determina_firmata.pdf", pdf1)
pdf2 = sign_pdf(pdf1, "anna", D(2026, 11, 9, 11), field="Firma2", box=(60, 60, 260, 120))
write("determina_due_firme.pdf", pdf2)
# Modifica dopo la firma: nuova versione della pagina con un importo diverso (aggiornamento incrementale).
w = IncrementalPdfFileWriter(io.BytesIO(pdf1))
page = w.root["/Pages"]["/Kids"][0]
page_obj = page.get_object()
new = generic.StreamObject(stream_data=b"BT /F1 18 Tf 72 720 Td (Determina n. 412: importo 98.300,00 euro) Tj ET")
page_obj[generic.NameObject("/Contents")] = w.add_object(new)
w.update_container(page_obj)
out = io.BytesIO()
w.write(out)
write("determina_cambiata.pdf", out.getvalue())
print("fatto:", sorted(os.listdir(OUT)))
