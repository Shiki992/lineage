// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Who signed a VBA project. Lineage reads the signer's name from the
//! PKCS#7 SignedData in the signature part (`vbaProjectSignature.bin` and
//! its Agile and V3 forms, or the `\x05DigitalSignature…` streams inside the
//! project) and never checks the signature itself: it has no cryptography,
//! and doesn't claim a signature is valid (decision 0028).
//!
//! The DER walk is bounded: every length is checked against what holds it.

/// The signer's name and the certificate issuer's, as the certificate
/// states them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signer {
    pub name: Option<String>,
    pub issuer: Option<String>,
}

/// The signedData content type, 1.2.840.113549.1.7.2.
const SIGNED_DATA: [u8; 11] = [
    0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02,
];
const COMMON_NAME: [u8; 3] = [0x55, 0x04, 0x03];
const ORGANIZATION: [u8; 3] = [0x55, 0x04, 0x0A];

/// One DER element: its tag, its content, and what follows it.
fn tlv(b: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = b.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let n = usize::from(first & 0x7F);
        if n == 0 || n > 4 || rest.len() < n {
            return None;
        }
        let mut len = 0usize;
        for &d in &rest[..n] {
            len = (len << 8) | usize::from(d);
        }
        (len, &rest[n..])
    };
    if len > rest.len() {
        return None;
    }
    Some((tag, &rest[..len], &rest[len..]))
}

/// The elements of a constructed element's content, at most `max`.
fn children(content: &[u8], max: usize) -> Vec<(u8, &[u8], &[u8])> {
    let mut out = Vec::new();
    let mut rest = content;
    let mut start = content;
    while !rest.is_empty() && out.len() < max {
        let Some((tag, inner, after)) = tlv(rest) else {
            break;
        };
        let whole = &start[..start.len() - after.len()];
        out.push((tag, inner, whole));
        rest = after;
        start = after;
    }
    out
}

fn string_of(tag: u8, bytes: &[u8]) -> Option<String> {
    match tag {
        // UTF8String, PrintableString, T61String, IA5String
        0x0C | 0x13 | 0x14 | 0x16 => Some(String::from_utf8_lossy(bytes).into_owned()),
        // BMPString: UTF-16 big-endian
        0x1E => {
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect();
            Some(String::from_utf16_lossy(&units))
        }
        _ => None,
    }
}

/// The common name in an X.501 Name, or its organisation.
fn name_of(name: &[u8]) -> Option<String> {
    let mut organization = None;
    for (_, set, _) in children(name, 64) {
        for (_, atv, _) in children(set, 16) {
            let parts = children(atv, 2);
            if let [(0x06, oid, _), (tag, value, _)] = parts.as_slice() {
                if *oid == COMMON_NAME {
                    if let Some(s) = string_of(*tag, value) {
                        return Some(s);
                    }
                } else if *oid == ORGANIZATION {
                    organization = string_of(*tag, value);
                }
            }
        }
    }
    organization
}

/// The signer of the first SignedData found in `part`, or `None` when the
/// part holds none Lineage can read.
#[must_use]
pub fn signer(part: &[u8]) -> Option<Signer> {
    let at = part
        .windows(SIGNED_DATA.len())
        .position(|w| w == SIGNED_DATA)?;
    let after = &part[at + SIGNED_DATA.len()..];
    let (tag, explicit, _) = tlv(after)?;
    if tag != 0xA0 {
        return None;
    }
    let (tag, signed_data, _) = tlv(explicit)?;
    if tag != 0x30 {
        return None;
    }
    let fields = children(signed_data, 8);
    // (serial, issuer bytes, subject's name, issuer's name)
    type Cert = (Vec<u8>, Vec<u8>, Option<String>, Option<String>);
    let mut certs: Vec<Cert> = Vec::new();
    let mut signer_id: Option<(Vec<u8>, Vec<u8>)> = None;
    for (tag, content, _) in &fields {
        match tag {
            // [0] IMPLICIT certificates
            0xA0 => {
                for (_, cert, _) in children(content, 64) {
                    let Some((0x30, tbs, _)) = tlv(cert) else {
                        continue;
                    };
                    let mut parts = children(tbs, 10);
                    if parts.first().is_some_and(|p| p.0 == 0xA0) {
                        parts.remove(0);
                    }
                    if let [
                        (0x02, serial, _),
                        _,
                        (0x30, issuer, _),
                        _,
                        (0x30, subject, _),
                        ..,
                    ] = parts.as_slice()
                    {
                        certs.push((
                            serial.to_vec(),
                            issuer.to_vec(),
                            name_of(subject),
                            name_of(issuer),
                        ));
                    }
                }
            }
            // signerInfos
            0x31 => {
                if let Some((_, info, _)) = children(content, 1).first() {
                    let parts = children(info, 3);
                    if let Some((0x30, ias, _)) = parts.get(1)
                        && let [(0x30, issuer, _), (0x02, serial, _)] = children(ias, 2).as_slice()
                    {
                        signer_id = Some((issuer.to_vec(), serial.to_vec()));
                    }
                }
            }
            _ => {}
        }
    }
    let cert = signer_id
        .as_ref()
        .and_then(|(issuer, serial)| certs.iter().find(|c| &c.0 == serial && &c.1 == issuer))
        .or_else(|| certs.first())?;
    Some(Signer {
        name: cert.2.clone(),
        issuer: cert.3.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_signer_of_a_test_signature() {
        let part =
            lineage_testkit::vba::signature_part("Fabrikam Test Signing", "Fabrikam Test CA");
        assert_eq!(
            signer(&part),
            Some(Signer {
                name: Some("Fabrikam Test Signing".to_owned()),
                issuer: Some("Fabrikam Test CA".to_owned()),
            })
        );
    }

    #[test]
    fn nonsense_is_not_a_signer() {
        assert_eq!(signer(b"not a signature"), None);
        let mut cut = lineage_testkit::vba::signature_part("A", "B");
        cut.truncate(80);
        assert_eq!(signer(&cut), None);
    }
}
