//! The ZATCA QR code: TLV fields, Base64, and the QR symbol.
//!
//! Security Features Standard v1.2 §4 (`docs/research/mena-compliance.md`
//! §6.3 [PRIMARY]): each field is a **1-byte tag, a 1-byte length and the
//! value** (UTF-8 for text); the fields are concatenated and Base64-encoded,
//! at most **700 characters**. Tags:
//! 1. seller name; 2. seller VAT number; 3. timestamp (ISO 8601); 4. invoice
//!    total with VAT; 5. VAT total (from 4 Dec 2021, Phase 1);
//! 6. invoice hash; 7. ECDSA signature; 8. ECDSA public key; 9. for
//!    simplified documents only, the signature of the CSID certificate by
//!    ZATCA's CA (from 1 Jan 2023, Phase 2).
//!
//! Lengths count UTF-8 **bytes**, not characters: an Arabic seller name of
//! 10 letters is 20 bytes or more (research §6.3 calls this a known bug
//! source).
//!
//! Tags 6 and 7 hold whatever bytes the caller puts in [`QrStamp`]. The
//! stamp ([`crate::xades::qr_stamp`]) puts the **Base64 text** of the hash
//! and of the signature there, as ZATCA's SDK sample invoices do, although
//! the standard's §4 says "32 bytes" for tag 6 [UNCERTAIN: see
//! docs/ksa-zatca.md for what ZATCA's sandbox validator accepted].

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;

/// Longest Base64 QR payload ZATCA accepts.
pub const MAX_QR_CHARS: usize = 700;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrError {
    /// A value longer than 255 bytes cannot be stored with a 1-byte length.
    ValueTooLong { tag: u8, bytes: usize },
    /// The Base64 payload is over [`MAX_QR_CHARS`].
    TooLong { chars: usize },
    /// The payload is not valid Base64 or TLV.
    Malformed(&'static str),
}

impl std::fmt::Display for QrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QrError::ValueTooLong { tag, bytes } => write!(f, "QR field {tag} is {bytes} bytes; at most 255 fit"),
            QrError::TooLong { chars } => write!(f, "the QR code is {chars} characters; ZATCA allows {MAX_QR_CHARS}"),
            QrError::Malformed(why) => write!(f, "the QR code is malformed: {why}"),
        }
    }
}

impl std::error::Error for QrError {}

/// The QR fields of one document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QrFields {
    /// Tag 1.
    pub seller_name: String,
    /// Tag 2.
    pub vat_number: String,
    /// Tag 3: "2026-05-01T10:15:30".
    pub timestamp: String,
    /// Tag 4: "1150.00".
    pub total_with_vat: String,
    /// Tag 5: "150.00".
    pub vat_total: String,
    /// Tags 6 to 9, Phase 2 only.
    pub stamp: Option<QrStamp>,
}

/// The Phase 2 fields, present once the document is stamped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrStamp {
    /// Tag 6: the invoice hash (see the module notes for its form).
    pub invoice_hash: Vec<u8>,
    /// Tag 7: the ECDSA signature (see the module notes for its form).
    pub signature: Vec<u8>,
    /// Tag 8: the public key (DER SubjectPublicKeyInfo).
    pub public_key: Vec<u8>,
    /// Tag 9: the CSID certificate's signature, simplified documents only.
    pub certificate_signature: Option<Vec<u8>>,
}

fn push(out: &mut Vec<u8>, tag: u8, value: &[u8]) -> Result<(), QrError> {
    let len = u8::try_from(value.len()).map_err(|_| QrError::ValueTooLong { tag, bytes: value.len() })?;
    out.extend([tag, len]);
    out.extend_from_slice(value);
    Ok(())
}

/// The raw TLV bytes.
pub fn tlv(f: &QrFields) -> Result<Vec<u8>, QrError> {
    let mut out = Vec::new();
    for (tag, v) in [
        (1, f.seller_name.as_bytes()),
        (2, f.vat_number.as_bytes()),
        (3, f.timestamp.as_bytes()),
        (4, f.total_with_vat.as_bytes()),
        (5, f.vat_total.as_bytes()),
    ] {
        push(&mut out, tag, v)?;
    }
    if let Some(s) = &f.stamp {
        push(&mut out, 6, &s.invoice_hash)?;
        push(&mut out, 7, &s.signature)?;
        push(&mut out, 8, &s.public_key)?;
        if let Some(c) = &s.certificate_signature {
            push(&mut out, 9, c)?;
        }
    }
    Ok(out)
}

/// The QR payload: Base64 of the TLV bytes, at most [`MAX_QR_CHARS`].
pub fn encode(f: &QrFields) -> Result<String, QrError> {
    let b64 = STANDARD.encode(tlv(f)?);
    if b64.len() > MAX_QR_CHARS {
        return Err(QrError::TooLong { chars: b64.len() });
    }
    Ok(b64)
}

/// Split a Base64 payload back into (tag, value) pairs.
pub fn decode(payload: &str) -> Result<Vec<(u8, Vec<u8>)>, QrError> {
    let bytes = STANDARD.decode(payload.trim()).map_err(|_| QrError::Malformed("not Base64"))?;
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let (tag, len) = match bytes.get(i..i + 2) {
            Some([t, l]) => (*t, usize::from(*l)),
            _ => return Err(QrError::Malformed("a field is cut short")),
        };
        let value = bytes.get(i + 2..i + 2 + len).ok_or(QrError::Malformed("a field is cut short"))?;
        out.push((tag, value.to_vec()));
        i += 2 + len;
    }
    Ok(out)
}

/// A QR symbol as a square of modules, row by row (`true` is dark), for
/// printing as SVG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QrMatrix {
    pub width: usize,
    pub dark: Vec<bool>,
}

/// Encode a payload as a QR symbol (error correction level M).
pub fn matrix(payload: &str) -> Result<QrMatrix, QrError> {
    let code = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::M)
        .map_err(|_| QrError::Malformed("too long for a QR code"))?;
    let dark = code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
    Ok(QrMatrix { width: code.width(), dark })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phase1() -> QrFields {
        QrFields {
            seller_name: "Riyadh Trading Co".into(),
            vat_number: "300075588700003".into(),
            timestamp: "2026-05-01T10:15:30".into(),
            total_with_vat: "1150.00".into(),
            vat_total: "150.00".into(),
            stamp: None,
        }
    }

    #[test]
    fn phase1_tags_round_trip() {
        let payload = encode(&phase1()).unwrap();
        let fields = decode(&payload).unwrap();
        let text: Vec<(u8, String)> = fields.into_iter().map(|(t, v)| (t, String::from_utf8(v).unwrap())).collect();
        assert_eq!(text, [
            (1, "Riyadh Trading Co".to_owned()),
            (2, "300075588700003".to_owned()),
            (3, "2026-05-01T10:15:30".to_owned()),
            (4, "1150.00".to_owned()),
            (5, "150.00".to_owned()),
        ]);
    }

    #[test]
    fn lengths_count_utf8_bytes() {
        let f = QrFields { seller_name: "شركة الرياض".into(), ..phase1() };
        let raw = tlv(&f).unwrap();
        assert_eq!((raw[0], usize::from(raw[1])), (1, "شركة الرياض".len()));
        assert_eq!(usize::from(raw[1]), 21, "11 characters, 21 bytes");
        assert_eq!(String::from_utf8(decode(&encode(&f).unwrap()).unwrap()[0].1.clone()).unwrap(), "شركة الرياض");
    }

    #[test]
    fn stamp_tags_follow_in_order_and_tag_9_is_optional() {
        let stamp = QrStamp { invoice_hash: vec![7; 32], signature: vec![1, 2, 3], public_key: vec![4; 88], certificate_signature: None };
        let f = QrFields { stamp: Some(stamp.clone()), ..phase1() };
        let tags: Vec<u8> = decode(&encode(&f).unwrap()).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(tags, [1, 2, 3, 4, 5, 6, 7, 8]);
        let f = QrFields { stamp: Some(QrStamp { certificate_signature: Some(vec![9; 70]), ..stamp }), ..phase1() };
        let fields = decode(&encode(&f).unwrap()).unwrap();
        assert_eq!(fields.last().map(|(t, v)| (*t, v.len())), Some((9, 70)));
        assert_eq!(fields[5].1, vec![7; 32]);
    }

    #[test]
    fn limits_and_malformed_payloads() {
        let f = QrFields { seller_name: "x".repeat(256), ..phase1() };
        assert_eq!(encode(&f), Err(QrError::ValueTooLong { tag: 1, bytes: 256 }));
        let stamp = QrStamp { invoice_hash: vec![0; 255], signature: vec![0; 255], public_key: vec![0; 255], certificate_signature: None };
        assert!(matches!(encode(&QrFields { stamp: Some(stamp), ..phase1() }), Err(QrError::TooLong { .. })));
        assert_eq!(decode("***"), Err(QrError::Malformed("not Base64")));
        assert_eq!(decode(&STANDARD.encode([1u8, 5, b'a'])), Err(QrError::Malformed("a field is cut short")));
        assert_eq!(decode(""), Ok(vec![]));
    }

    #[test]
    fn the_payload_becomes_a_square_symbol() {
        let m = matrix(&encode(&phase1()).unwrap()).unwrap();
        assert!(m.width >= 21 && (m.width - 17) % 4 == 0, "a QR version's width: {}", m.width);
        assert_eq!(m.dark.len(), m.width * m.width);
        assert!(m.dark[0] && m.dark[6] && !m.dark[7], "the top-left finder pattern");
    }
}
