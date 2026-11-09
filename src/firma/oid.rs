//! Gli identificatori (OID) che servono alla firma digitale.

// Tipi di contenuto CMS
pub const DATA: &str = "1.2.840.113549.1.7.1";
pub const SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
pub const ENVELOPED_DATA: &str = "1.2.840.113549.1.7.3";
pub const TST_INFO: &str = "1.2.840.113549.1.9.16.1.4";
pub const TIMESTAMPED_DATA: &str = "1.2.840.113549.1.9.16.1.31";

// Attributi
pub const CONTENT_TYPE: &str = "1.2.840.113549.1.9.3";
pub const MESSAGE_DIGEST: &str = "1.2.840.113549.1.9.4";
pub const SIGNING_TIME: &str = "1.2.840.113549.1.9.5";
pub const COUNTER_SIGNATURE: &str = "1.2.840.113549.1.9.6";
pub const SIGNING_CERT: &str = "1.2.840.113549.1.9.16.2.12";
pub const SIGNING_CERT_V2: &str = "1.2.840.113549.1.9.16.2.47";
pub const SIG_TIMESTAMP: &str = "1.2.840.113549.1.9.16.2.14";
pub const CONTENT_TIMESTAMP: &str = "1.2.840.113549.1.9.16.2.20";
pub const SIG_POLICY: &str = "1.2.840.113549.1.9.16.2.15";

// Hash
pub const SHA1: &str = "1.3.14.3.2.26";
pub const SHA256: &str = "2.16.840.1.101.3.4.2.1";
pub const SHA384: &str = "2.16.840.1.101.3.4.2.2";
pub const SHA512: &str = "2.16.840.1.101.3.4.2.3";
pub const SHA224: &str = "2.16.840.1.101.3.4.2.4";

// Firme
pub const RSA: &str = "1.2.840.113549.1.1.1";
pub const RSA_SHA1: &str = "1.2.840.113549.1.1.5";
pub const RSA_SHA256: &str = "1.2.840.113549.1.1.11";
pub const RSA_SHA384: &str = "1.2.840.113549.1.1.12";
pub const RSA_SHA512: &str = "1.2.840.113549.1.1.13";
pub const RSA_SHA224: &str = "1.2.840.113549.1.1.14";
pub const RSA_PSS: &str = "1.2.840.113549.1.1.10";
pub const EC_PUBLIC_KEY: &str = "1.2.840.10045.2.1";
pub const ECDSA_SHA1: &str = "1.2.840.10045.4.1";
pub const ECDSA_SHA256: &str = "1.2.840.10045.4.3.2";
pub const ECDSA_SHA384: &str = "1.2.840.10045.4.3.3";
pub const ECDSA_SHA512: &str = "1.2.840.10045.4.3.4";
pub const P256: &str = "1.2.840.10045.3.1.7";
pub const P384: &str = "1.3.132.0.34";

// Nomi
pub const CN: &str = "2.5.4.3";
pub const SURNAME: &str = "2.5.4.4";
pub const SERIAL_NUMBER: &str = "2.5.4.5";
pub const COUNTRY: &str = "2.5.4.6";
pub const ORG: &str = "2.5.4.10";
pub const OU: &str = "2.5.4.11";
pub const TITLE: &str = "2.5.4.12";
pub const GIVEN_NAME: &str = "2.5.4.42";
pub const DN_QUALIFIER: &str = "2.5.4.46";
pub const ORG_ID: &str = "2.5.4.97";
pub const EMAIL: &str = "1.2.840.113549.1.9.1";

// Estensioni
pub const EXT_SKI: &str = "2.5.29.14";
pub const EXT_KEY_USAGE: &str = "2.5.29.15";
pub const EXT_BASIC_CONSTRAINTS: &str = "2.5.29.19";
pub const EXT_CRL_DP: &str = "2.5.29.31";
pub const EXT_CERT_POLICIES: &str = "2.5.29.32";
pub const EXT_AKI: &str = "2.5.29.35";
pub const EXT_EXT_KEY_USAGE: &str = "2.5.29.37";
pub const EXT_AIA: &str = "1.3.6.1.5.5.7.1.1";
pub const EXT_QC_STATEMENTS: &str = "1.3.6.1.5.5.7.1.3";
pub const EXT_OCSP_NOCHECK: &str = "1.3.6.1.5.5.7.48.1.5";
pub const AD_OCSP: &str = "1.3.6.1.5.5.7.48.1";
pub const AD_CA_ISSUERS: &str = "1.3.6.1.5.5.7.48.2";
pub const OCSP_BASIC: &str = "1.3.6.1.5.5.7.48.1.1";
pub const OCSP_NONCE: &str = "1.3.6.1.5.5.7.48.1.2";
pub const EKU_TIMESTAMPING: &str = "1.3.6.1.5.5.7.3.8";
pub const EKU_OCSP_SIGNING: &str = "1.3.6.1.5.5.7.3.9";
pub const CRL_REASON: &str = "2.5.29.21";

// QcStatements (ETSI EN 319 412-5)
pub const QC_COMPLIANCE: &str = "0.4.0.1862.1.1";
pub const QC_SSCD: &str = "0.4.0.1862.1.4";
pub const QC_TYPE: &str = "0.4.0.1862.1.6";
pub const QC_TYPE_ESIGN: &str = "0.4.0.1862.1.6.1";
