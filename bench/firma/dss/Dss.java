// Strumento di prova basato su EU DSS (la libreria della Commissione europea che sta dietro al validatore europeo).
//   java -cp "lib/*:." Dss firma <p12> <password> <documento> <uscita-cades-.p7m> <uscita-pades-.pdf|-> [staccata]
//   java -cp "lib/*:." Dss valida <radice.cer> <crl|-> <file>...      -> una riga TSV per firma: file, formato, indicazione, sottoindicazione
import eu.europa.esig.dss.cades.CAdESSignatureParameters;
import eu.europa.esig.dss.cades.signature.CAdESService;
import eu.europa.esig.dss.enumerations.DigestAlgorithm;
import eu.europa.esig.dss.enumerations.SignatureLevel;
import eu.europa.esig.dss.enumerations.SignaturePackaging;
import eu.europa.esig.dss.model.DSSDocument;
import eu.europa.esig.dss.model.FileDocument;
import eu.europa.esig.dss.model.SignatureValue;
import eu.europa.esig.dss.model.ToBeSigned;
import eu.europa.esig.dss.model.x509.CertificateToken;
import eu.europa.esig.dss.pades.PAdESSignatureParameters;
import eu.europa.esig.dss.pades.signature.PAdESService;
import eu.europa.esig.dss.simplereport.SimpleReport;
import eu.europa.esig.dss.spi.DSSUtils;
import eu.europa.esig.dss.spi.validation.CommonCertificateVerifier;
import eu.europa.esig.dss.spi.x509.CommonTrustedCertificateSource;
import eu.europa.esig.dss.spi.x509.revocation.crl.ExternalResourcesCRLSource;
import eu.europa.esig.dss.token.DSSPrivateKeyEntry;
import eu.europa.esig.dss.token.Pkcs12SignatureToken;
import eu.europa.esig.dss.validation.SignedDocumentValidator;
import eu.europa.esig.dss.validation.reports.Reports;
import java.io.*;
import java.security.KeyStore;
import java.util.List;

public class Dss {
    public static void main(String[] a) throws Exception {
        if (a[0].equals("firma")) firma(a); else valida(a);
    }

    static void firma(String[] a) throws Exception {
        try (Pkcs12SignatureToken token = new Pkcs12SignatureToken(new FileInputStream(a[1]), new KeyStore.PasswordProtection(a[2].toCharArray()))) {
            DSSPrivateKeyEntry key = token.getKeys().get(0);
            DSSDocument doc = new FileDocument(a[3]);
            boolean staccata = a.length > 6 && a[6].equals("staccata");
            CAdESSignatureParameters p = new CAdESSignatureParameters();
            p.setSignatureLevel(SignatureLevel.CAdES_BASELINE_B);
            p.setSignaturePackaging(staccata ? SignaturePackaging.DETACHED : SignaturePackaging.ENVELOPING);
            p.setSigningCertificate(key.getCertificate());
            p.setCertificateChain(key.getCertificateChain());
            p.setDigestAlgorithm(DigestAlgorithm.SHA256);
            CAdESService s = new CAdESService(new CommonCertificateVerifier());
            ToBeSigned tbs = s.getDataToSign(doc, p);
            SignatureValue sv = token.sign(tbs, DigestAlgorithm.SHA256, key);
            s.signDocument(doc, p, sv).save(a[4]);
            if (!a[5].equals("-")) {
                PAdESSignatureParameters pp = new PAdESSignatureParameters();
                pp.setSignatureLevel(SignatureLevel.PAdES_BASELINE_B);
                pp.setSigningCertificate(key.getCertificate());
                pp.setCertificateChain(key.getCertificateChain());
                pp.setDigestAlgorithm(DigestAlgorithm.SHA256);
                PAdESService ps = new PAdESService(new CommonCertificateVerifier());
                DSSDocument pdf = new FileDocument(a[5].replace(".pdf", "") + ".sorgente.pdf");
                ToBeSigned t2 = ps.getDataToSign(pdf, pp);
                SignatureValue v2 = token.sign(t2, DigestAlgorithm.SHA256, key);
                ps.signDocument(pdf, pp, v2).save(a[5]);
            }
        }
    }

    static String msg(SimpleReport sr, String id) {
        StringBuilder b = new StringBuilder();
        for (var m : sr.getAdESValidationErrors(id)) b.append("E:").append(m.getValue()).append("|");
        for (var m : sr.getAdESValidationWarnings(id)) b.append("W:").append(m.getValue()).append("|");
        return b.length() == 0 ? "-" : b.toString().replace('\n', ' ');
    }

    static void valida(String[] a) throws Exception {
        CertificateToken radice = DSSUtils.loadCertificate(new File(a[1]));
        CommonTrustedCertificateSource trusted = new CommonTrustedCertificateSource();
        trusted.addCertificate(radice);
        for (String extra : a[2].split(",")) if (!extra.equals("-")) trusted.addCertificate(DSSUtils.loadCertificate(new File(extra)));
        for (int i = 4; i < a.length; i++) {
            File f = new File(a[i]);
            try {
                CommonCertificateVerifier cv = new CommonCertificateVerifier();
                cv.setTrustedCertSources(trusted);
                if (!a[3].equals("-")) cv.setCrlSource(new ExternalResourcesCRLSource(new FileInputStream(a[3])));
                SignedDocumentValidator v = SignedDocumentValidator.fromDocument(new FileDocument(f));
                v.setCertificateVerifier(cv);
                String nome = f.getName();
                if (nome.endsWith(".p7s")) {
                    File orig = new File(f.getParentFile(), nome.substring(0, nome.length() - 4));
                    if (orig.exists()) v.setDetachedContents(List.of(new FileDocument(orig)));
                }
                Reports r = v.validateDocument();
                SimpleReport sr = r.getSimpleReport();
                if (sr.getSignatureIdList().isEmpty()) { System.out.println(nome + "\t-\tNESSUNA_FIRMA\t-"); continue; }
                for (String id : sr.getSignatureIdList()) {
                    System.out.println(nome + "\t" + sr.getSignatureFormat(id) + "\t" + sr.getIndication(id) + "\t" + (sr.getSubIndication(id) == null ? "-" : sr.getSubIndication(id)) + "\t" + msg(sr, id));
                }
            } catch (Throwable e) {
                System.out.println(f.getName() + "\t-\tERRORE\t" + e.getClass().getSimpleName() + ":" + String.valueOf(e.getMessage()).replace('\n', ' '));
            }
        }
    }
}
