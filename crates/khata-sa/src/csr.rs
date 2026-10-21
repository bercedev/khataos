//! Onboarding: the device's (EGS's) certificate signing request (PKCS#10).
//!
//! Each device that issues e-invoices has its own secp256k1 key pair and
//! asks ZATCA for a certificate (the CSID) with a CSR carrying these fields
//! (Security Features Implementation Standard v1.1 §2.2.2, Table 1
//! [PRIMARY]; the exact encoding follows the sample CSR in ZATCA's official
//! sandbox API specification, "e-Invoicing Sandbox Release 2.1.0",
//! Compliance CSID API, which the sandbox accepts [PRIMARY]):
//!
//! | Where | Field | OID | Value | String type |
//! |---|---|---|---|---|
//! | subject | C | 2.5.4.6 | "SA" | PrintableString |
//! | subject | OU | 2.5.4.11 | branch name (for a VAT group, the member's 10-digit TIN) | UTF8String |
//! | subject | O | 2.5.4.10 | taxpayer name | UTF8String |
//! | subject | CN | 2.5.4.3 | unique name or asset number of the device | UTF8String |
//! | extension | certificate template name | 1.3.6.1.4.1.311.20.2 | see [`Environment::template_name`] | UTF8String |
//! | SAN dirName | SN (surname) | 2.5.4.4 | EGS serial `1-<maker>\|2-<model or version>\|3-<serial>` | UTF8String |
//! | SAN dirName | UID | 0.9.2342.19200300.100.1.1 | VAT number | UTF8String |
//! | SAN dirName | title | 2.5.4.12 | functionality map "TSCZ", e.g. "1100" | UTF8String |
//! | SAN dirName | registeredAddress | 2.5.4.26 | branch location (National Address short address) | UTF8String |
//! | SAN dirName | businessCategory | 2.5.4.15 | industry | UTF8String |
//!
//! Subject order C, OU, O, CN and one attribute per RDN, as in the sample.
//! The signature is ECDSA with SHA-256 over the DER `CertificationRequestInfo`.
//!
//! [UNCERTAIN] The standard's Table 1 names organizationIdentifier
//! (2.5.4.97) for the VAT number and businessCategory for the invoice
//! types; the SDK's CSR configuration and the sandbox sample put the VAT
//! number in the SAN's UID and the invoice types in `title`. KhataOS follows
//! the sample, which ZATCA's sandbox accepts.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use x509_cert::attr::{Attribute, AttributeTypeAndValue};
use x509_cert::der::asn1::{Any, BitString, ObjectIdentifier, OctetString, PrintableStringRef, SetOfVec, Utf8StringRef};
use x509_cert::der::{Decode, Encode, EncodePem};
use x509_cert::ext::Extension;
use x509_cert::ext::pkix::SubjectAltName;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::name::{Name, RdnSequence, RelativeDistinguishedName};
use x509_cert::request::{CertReq, CertReqInfo, Version};
use x509_cert::spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};

use crate::sign::Signer;
use crate::vat_number;

/// Where the device is onboarded. Each has its own certificate authority:
/// a certificate from one is refused by the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// The developer portal (Integration Sandbox): open to anyone, fixed
    /// test OTP, mock taxpayer data.
    Sandbox,
    /// The simulation environment (Fatoora simulation portal): a copy of
    /// production for testing with real taxpayer logins.
    Simulation,
    /// Production (Fatoora portal).
    Production,
}

impl Environment {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sandbox" | "developer-portal" | "developer portal" => Some(Self::Sandbox),
            "simulation" => Some(Self::Simulation),
            "production" | "core" => Some(Self::Production),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Sandbox => "sandbox",
            Self::Simulation => "simulation",
            Self::Production => "production",
        }
    }

    /// The certificate template name extension's value.
    ///
    /// Production "ZATCA-Code-Signing" (the sandbox API sample [PRIMARY]);
    /// simulation "PREZATCA-Code-Signing" (the SDK's `-sim` value
    /// [SECONDARY; UNCERTAIN until onboarded with a real OTP: the
    /// simulation server checks the OTP first]); the sandbox
    /// "TSTZATCA-Code-Signing" (the SDK's `-nonprod` value [SECONDARY]).
    /// The sandbox accepts "TSTZATCA-Code-Signing" and also its own
    /// sample's "ZATCA-Code-Signing" (checked live 20 Oct 2026).
    pub fn template_name(self) -> &'static str {
        match self {
            Self::Sandbox => "TSTZATCA-Code-Signing",
            Self::Simulation => "PREZATCA-Code-Signing",
            Self::Production => "ZATCA-Code-Signing",
        }
    }
}

/// What the CSR says about the device and the taxpayer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CsrInput {
    /// CN: unique name or asset tracking number of the device.
    pub common_name: String,
    /// The EGS serial: `1-<solution>|2-<model or version>|3-<serial>`
    /// (see [`egs_serial`]).
    pub serial_number: String,
    /// The taxpayer's VAT number (15 digits, first and last "3").
    pub vat_number: String,
    /// OU: branch name; for a VAT group, the member's 10-digit TIN.
    pub organization_unit: String,
    /// O: taxpayer name.
    pub organization: String,
    /// "TSCZ" flags: "1100" = standard and simplified invoices.
    pub invoice_types: String,
    /// registeredAddress: where the device is (the National Address short
    /// address, or a website for e-commerce).
    pub location: String,
    /// businessCategory: the industry.
    pub industry: String,
}

/// The EGS serial number KhataOS puts in its CSRs:
/// `1-KhataOS|2-<app version>|3-<device UUID>` (Table 1: maker or solution
/// provider, model or version, serial).
pub fn egs_serial(version: &str, device_uuid: &str) -> String {
    format!("1-KhataOS|2-{}|3-{}", version.trim(), device_uuid.trim())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsrError(pub String);

impl std::fmt::Display for CsrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot build the CSR: {}", self.0)
    }
}

impl std::error::Error for CsrError {}

fn bad(why: impl Into<String>) -> CsrError {
    CsrError(why.into())
}

/// A built CSR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Csr {
    pub der: Vec<u8>,
    /// "-----BEGIN CERTIFICATE REQUEST-----" PEM, LF line ends.
    pub pem: String,
}

impl Csr {
    /// The value of the compliance API's `csr` field: Base64 of the PEM
    /// text (the sandbox sample's `csr` decodes to a PEM [PRIMARY]).
    pub fn api_value(&self) -> String {
        STANDARD.encode(self.pem.as_bytes())
    }
}

const OID_C: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.6");
const OID_OU: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.11");
const OID_O: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.10");
const OID_CN: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.3");
const OID_SURNAME: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.4");
const OID_UID: ObjectIdentifier = ObjectIdentifier::new_unwrap("0.9.2342.19200300.100.1.1");
const OID_TITLE: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.12");
const OID_REGISTERED_ADDRESS: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.26");
const OID_BUSINESS_CATEGORY: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.15");
/// Microsoft's certificate template name (szOID_ENROLL_CERTTYPE_EXTENSION).
pub const OID_TEMPLATE_NAME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.4.1.311.20.2");
const OID_SUBJECT_ALT_NAME: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.17");
const OID_EXTENSION_REQUEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.14");
const OID_ECDSA_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");

fn utf8(s: &str) -> Result<Any, CsrError> {
    Any::encode_from(&Utf8StringRef::new(s).map_err(|e| bad(e.to_string()))?).map_err(|e| bad(e.to_string()))
}

fn der(v: &impl Encode) -> Result<Vec<u8>, CsrError> {
    v.to_der().map_err(|e| bad(e.to_string()))
}

fn rdn(oid: ObjectIdentifier, value: Any) -> Result<RelativeDistinguishedName, CsrError> {
    let set = SetOfVec::try_from(vec![AttributeTypeAndValue { oid, value }]).map_err(|e| bad(e.to_string()))?;
    Ok(RelativeDistinguishedName::from(set))
}

fn name(parts: Vec<(ObjectIdentifier, Any)>) -> Result<Name, CsrError> {
    Ok(RdnSequence(parts.into_iter().map(|(o, v)| rdn(o, v)).collect::<Result<_, _>>()?))
}

/// Check the input against Table 1's rules.
pub fn check(input: &CsrInput) -> Result<(), CsrError> {
    for (field, value) in [
        ("common name", &input.common_name),
        ("EGS serial number", &input.serial_number),
        ("branch (organisation unit)", &input.organization_unit),
        ("taxpayer name", &input.organization),
        ("location", &input.location),
        ("industry", &input.industry),
    ] {
        if value.trim().is_empty() {
            return Err(bad(format!("the {field} is missing")));
        }
    }
    vat_number::validate(&input.vat_number).map_err(|e| bad(format!("the VAT number is invalid: {e}")))?;
    let parts: Vec<&str> = input.serial_number.split('|').collect();
    let well_formed = parts.len() == 3
        && parts.iter().zip(["1-", "2-", "3-"]).all(|(p, prefix)| p.len() > 2 && p.starts_with(prefix));
    if !well_formed {
        return Err(bad("the EGS serial number must read 1-…|2-…|3-…"));
    }
    let t = input.invoice_types.as_bytes();
    if !(t.len() == 4 && t.iter().all(|b| matches!(b, b'0' | b'1')) && t[..2].contains(&b'1')) {
        return Err(bad("the invoice types are 4 digits of 0 and 1 (TSCZ), with standard or simplified set: 1100, 1000 or 0100"));
    }
    // A VAT group (11th digit "1") names the member's 10-digit TIN in OU.
    if vat_number::normalise(&input.vat_number).as_bytes().get(10) == Some(&b'1') {
        let ou = input.organization_unit.trim();
        if !(ou.len() == 10 && ou.bytes().all(|b| b.is_ascii_digit())) {
            return Err(bad("a VAT group's branch field is the member's 10-digit TIN"));
        }
    }
    Ok(())
}

/// Build and sign the CSR for a device.
pub fn build(input: &CsrInput, env: Environment, signer: &Signer) -> Result<Csr, CsrError> {
    check(input)?;
    let printable = |s: &str| -> Result<Any, CsrError> {
        Any::encode_from(&PrintableStringRef::new(s).map_err(|e| bad(e.to_string()))?).map_err(|e| bad(e.to_string()))
    };
    let subject = name(vec![
        (OID_C, printable("SA")?),
        (OID_OU, utf8(input.organization_unit.trim())?),
        (OID_O, utf8(input.organization.trim())?),
        (OID_CN, utf8(input.common_name.trim())?),
    ])?;
    let directory = name(vec![
        (OID_SURNAME, utf8(input.serial_number.trim())?),
        (OID_UID, utf8(&vat_number::normalise(&input.vat_number))?),
        (OID_TITLE, utf8(input.invoice_types.trim())?),
        (OID_REGISTERED_ADDRESS, utf8(input.location.trim())?),
        (OID_BUSINESS_CATEGORY, utf8(input.industry.trim())?),
    ])?;
    let template = Extension {
        extn_id: OID_TEMPLATE_NAME,
        critical: false,
        extn_value: OctetString::new(der(&Utf8StringRef::new(env.template_name()).map_err(|e| bad(e.to_string()))?)?)
            .map_err(|e| bad(e.to_string()))?,
    };
    let san = Extension {
        extn_id: OID_SUBJECT_ALT_NAME,
        critical: false,
        extn_value: OctetString::new(der(&SubjectAltName(vec![GeneralName::DirectoryName(directory)]))?)
            .map_err(|e| bad(e.to_string()))?,
    };
    let extensions: Vec<Extension> = vec![template, san];
    let request = Attribute {
        oid: OID_EXTENSION_REQUEST,
        values: SetOfVec::try_from(vec![Any::encode_from(&extensions).map_err(|e| bad(e.to_string()))?])
            .map_err(|e| bad(e.to_string()))?,
    };
    let info = CertReqInfo {
        version: Version::V1,
        subject,
        public_key: SubjectPublicKeyInfoOwned::from_der(&signer.public_key_der()).map_err(|e| bad(e.to_string()))?,
        attributes: SetOfVec::try_from(vec![request]).map_err(|e| bad(e.to_string()))?,
    };
    let signature = signer.sign_message(&der(&info)?);
    let req = CertReq {
        info,
        algorithm: AlgorithmIdentifierOwned { oid: OID_ECDSA_SHA256, parameters: None },
        signature: BitString::from_bytes(&signature).map_err(|e| bad(e.to_string()))?,
    };
    let der = der(&req)?;
    let pem = req.to_pem(x509_cert::der::pem::LineEnding::LF).map_err(|e| bad(e.to_string()))?;
    Ok(Csr { der, pem })
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::signature::Verifier;
    use x509_cert::der::Tag;
    use x509_cert::der::Tagged;

    /// A test key; never a real one.
    const SECRET: [u8; 32] = [0x11; 32];

    fn input() -> CsrInput {
        CsrInput {
            common_name: "KhataOS-3cf5ee18".into(),
            serial_number: egs_serial("0.1.0", "3cf5ee18-ee25-44ea-a444-2c37ba7f28be"),
            vat_number: "399999999900003".into(),
            organization_unit: "Riyadh Branch".into(),
            organization: "Riyadh Trading Co".into(),
            invoice_types: "1100".into(),
            location: "RRRD2929".into(),
            industry: "Supply activities".into(),
        }
    }

    fn text(v: &Any) -> (Tag, String) {
        let s = match v.tag() {
            Tag::PrintableString => PrintableStringRef::try_from(v).unwrap().as_str().to_owned(),
            _ => Utf8StringRef::try_from(v).unwrap().as_str().to_owned(),
        };
        (v.tag(), s)
    }

    fn fields(n: &Name) -> Vec<(String, Tag, String)> {
        n.0.iter()
            .map(|r| {
                assert_eq!(r.0.len(), 1, "one attribute per RDN");
                let atv = r.0.get(0).unwrap();
                let (tag, s) = text(&atv.value);
                (atv.oid.to_string(), tag, s)
            })
            .collect()
    }

    #[test]
    fn the_csr_parses_back_with_zatcas_fields_and_a_valid_signature() {
        let signer = Signer::from_secret_bytes(&SECRET).unwrap();
        let csr = build(&input(), Environment::Production, &signer).unwrap();
        assert!(csr.pem.starts_with("-----BEGIN CERTIFICATE REQUEST-----\n"));
        assert_eq!(STANDARD.decode(csr.api_value()).unwrap(), csr.pem.as_bytes());
        let req = CertReq::from_der(&csr.der).unwrap();
        assert_eq!(req.info.version, Version::V1);
        assert_eq!(fields(&req.info.subject), [
            ("2.5.4.6".to_owned(), Tag::PrintableString, "SA".to_owned()),
            ("2.5.4.11".to_owned(), Tag::Utf8String, "Riyadh Branch".to_owned()),
            ("2.5.4.10".to_owned(), Tag::Utf8String, "Riyadh Trading Co".to_owned()),
            ("2.5.4.3".to_owned(), Tag::Utf8String, "KhataOS-3cf5ee18".to_owned()),
        ]);
        assert_eq!(req.info.public_key.to_der().unwrap(), signer.public_key_der());
        assert_eq!(req.info.public_key.algorithm.parameters.as_ref().unwrap().decode_as::<ObjectIdentifier>().unwrap().to_string(), "1.3.132.0.10", "secp256k1");

        // The extension request: template name and the SAN directory name.
        let attr = req.info.attributes.get(0).unwrap();
        assert_eq!(attr.oid, OID_EXTENSION_REQUEST);
        let exts: Vec<Extension> = attr.values.get(0).unwrap().decode_as().unwrap();
        assert_eq!(exts.len(), 2);
        assert_eq!((exts[0].extn_id, exts[0].critical), (OID_TEMPLATE_NAME, false));
        let template = Any::from_der(exts[0].extn_value.as_bytes()).unwrap();
        assert_eq!(text(&template), (Tag::Utf8String, "ZATCA-Code-Signing".to_owned()));
        assert_eq!(exts[1].extn_id, OID_SUBJECT_ALT_NAME);
        let san = SubjectAltName::from_der(exts[1].extn_value.as_bytes()).unwrap();
        let GeneralName::DirectoryName(dir) = &san.0[0] else { panic!("a directory name") };
        assert_eq!(fields(dir), [
            ("2.5.4.4".to_owned(), Tag::Utf8String, "1-KhataOS|2-0.1.0|3-3cf5ee18-ee25-44ea-a444-2c37ba7f28be".to_owned()),
            ("0.9.2342.19200300.100.1.1".to_owned(), Tag::Utf8String, "399999999900003".to_owned()),
            ("2.5.4.12".to_owned(), Tag::Utf8String, "1100".to_owned()),
            ("2.5.4.26".to_owned(), Tag::Utf8String, "RRRD2929".to_owned()),
            ("2.5.4.15".to_owned(), Tag::Utf8String, "Supply activities".to_owned()),
        ]);

        // ECDSA-SHA256 over the DER CertificationRequestInfo, by this key.
        assert_eq!(req.algorithm.oid, OID_ECDSA_SHA256);
        let key = k256::ecdsa::VerifyingKey::from_sec1_bytes(req.info.public_key.subject_public_key.raw_bytes()).unwrap();
        let sig = k256::ecdsa::Signature::from_der(req.signature.raw_bytes()).unwrap();
        assert!(key.verify(&req.info.to_der().unwrap(), &sig).is_ok());
        let other = Signer::from_secret_bytes(&[0x22; 32]).unwrap();
        let other_key = k256::ecdsa::VerifyingKey::from_sec1_bytes(
            CertReq::from_der(&build(&input(), Environment::Production, &other).unwrap().der).unwrap().info.public_key.subject_public_key.raw_bytes(),
        )
        .unwrap();
        assert!(other_key.verify(&req.info.to_der().unwrap(), &sig).is_err(), "another key does not verify it");
        assert_eq!(build(&input(), Environment::Production, &signer).unwrap(), csr, "deterministic (RFC 6979)");
    }

    #[test]
    fn the_template_name_follows_the_environment() {
        let signer = Signer::from_secret_bytes(&SECRET).unwrap();
        for (env, name) in [
            (Environment::Sandbox, "TSTZATCA-Code-Signing"),
            (Environment::Simulation, "PREZATCA-Code-Signing"),
            (Environment::Production, "ZATCA-Code-Signing"),
        ] {
            let der = build(&input(), env, &signer).unwrap().der;
            let needle = name.as_bytes();
            assert!(der.windows(needle.len()).any(|w| w == needle), "{env:?}");
            assert_eq!(Environment::parse(env.name()), Some(env));
        }
        assert_eq!(Environment::parse("core"), Some(Environment::Production));
        assert_eq!(Environment::parse("live"), None);
    }

    #[test]
    fn bad_input_is_refused() {
        let signer = Signer::from_secret_bytes(&SECRET).unwrap();
        let fails = |f: &dyn Fn(&mut CsrInput)| {
            let mut i = input();
            f(&mut i);
            build(&i, Environment::Sandbox, &signer).is_err()
        };
        assert!(!fails(&|_| ()));
        assert!(fails(&|i| i.vat_number = "123".into()));
        assert!(fails(&|i| i.serial_number = "KhataOS".into()));
        assert!(fails(&|i| i.serial_number = "1-a|2-b".into()));
        assert!(fails(&|i| i.invoice_types = "0011".into()));
        assert!(fails(&|i| i.invoice_types = "11".into()));
        assert!(fails(&|i| i.common_name = " ".into()));
        assert!(fails(&|i| i.industry = String::new()));
        // A VAT group (digit 11 is "1") needs the member's TIN as OU.
        assert!(fails(&|i| i.vat_number = "300000000010003".into()));
        assert!(!fails(&|i| {
            i.vat_number = "300000000010003".into();
            i.organization_unit = "1234567890".into();
        }));
    }
}
