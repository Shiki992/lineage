// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Power Query queries for synthetic workbooks: the DataMashup custom XML
//! item ([MS-QDEFF]), with its inner package (`Config/Package.xml`,
//! `Formulas/Section1.m`), permissions, per-query metadata, and a
//! permission binding of a single 0x00 byte, which the spec allows a
//! writer instead of the DPAPI checksum.
//!
//! Every query, path and name is invented; nothing is ever evaluated.
//!
//! [MS-QDEFF]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-qdeff/22557f6d-7c29-4554-8fe4-7b7a54ac7a2b

use std::fmt::Write;

use crate::xlsx::Extra;
use crate::zip_writer;

/// Where a query's result goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Load {
    /// Connection only.
    Nowhere,
    /// A table on this sheet.
    Sheet(String),
    /// The data model.
    Model,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub name: String,
    /// The M expression, `\n` line ends.
    pub formula: String,
    pub load: Load,
    /// `QueryID`, a GUID.
    pub id: String,
    /// When its result was last loaded (`FillLastUpdated`), and how many
    /// rows (`FillCount`).
    pub last_loaded: Option<(String, u32)>,
}

impl Query {
    #[must_use]
    pub fn new(name: &str, id: &str, formula: &str) -> Self {
        Self {
            name: name.to_owned(),
            formula: formula.to_owned(),
            load: Load::Nowhere,
            id: id.to_owned(),
            last_loaded: None,
        }
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
#[must_use]
pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(char::from(B64[(n >> 18) as usize & 63]));
        out.push(char::from(B64[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(B64[(n >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(B64[n as usize & 63])
        } else {
            '='
        });
    }
    out
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `Formulas/Section1.m` for `queries`.
#[must_use]
pub fn section(queries: &[Query]) -> String {
    let mut out = "section Section1;\r\n".to_owned();
    for q in queries {
        let name = if q.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            q.name.clone()
        } else {
            format!("#\"{}\"", q.name.replace('"', "\"\""))
        };
        let _ = write!(out, "\r\nshared {name} = ");
        out.push_str(&q.formula.replace('\n', "\r\n"));
        out.push_str(";\r\n");
    }
    out
}

fn metadata_xml(queries: &[Query]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?><LocalPackageMetadataFile xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema"><Items><Item><ItemLocation><ItemType>AllFormulas</ItemType><ItemPath /></ItemLocation><StableEntries /></Item>"#,
    );
    for q in queries {
        let path = q.name.replace(' ', "%20");
        let _ = write!(
            out,
            r#"<Item><ItemLocation><ItemType>Formula</ItemType><ItemPath>Section1/{}</ItemPath></ItemLocation><StableEntries><Entry Type="IsPrivate" Value="l0" /><Entry Type="QueryID" Value="s{}" />"#,
            esc(&path),
            esc(&q.id)
        );
        let (fill, model, target) = match &q.load {
            Load::Nowhere => ("l0", "l0", None),
            Load::Sheet(sheet) => ("l1", "l0", Some(sheet.clone())),
            Load::Model => ("l0", "l1", None),
        };
        let _ = write!(
            out,
            r#"<Entry Type="FillEnabled" Value="{fill}" /><Entry Type="FillToDataModelEnabled" Value="{model}" />"#
        );
        if let Some(target) = target {
            let _ = write!(
                out,
                r#"<Entry Type="FillTarget" Value="s{}" /><Entry Type="FillObjectType" Value="sTable" />"#,
                esc(&target)
            );
        } else {
            out.push_str(r#"<Entry Type="FillObjectType" Value="sConnectionOnly" />"#);
        }
        if let Some((when, rows)) = &q.last_loaded {
            let _ = write!(
                out,
                r#"<Entry Type="FillLastUpdated" Value="d{when}" /><Entry Type="FillCount" Value="l{rows}" /><Entry Type="FillStatus" Value="sComplete" />"#
            );
        }
        out.push_str(r#"<Entry Type="ResultType" Value="sTable" /></StableEntries></Item>"#);
    }
    out.push_str("</Items></LocalPackageMetadataFile>");
    out
}

/// The DataMashup binary stream (before base64).
#[must_use]
pub fn stream(queries: &[Query]) -> Vec<u8> {
    let package = zip_writer::write_deflated(
        &[
            (
                "[Content_Types].xml".to_owned(),
                br#"<?xml version="1.0" encoding="utf-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="text/xml" /><Default Extension="m" ContentType="application/x-ms-m" /></Types>"#
                    .to_vec(),
            ),
            (
                "Config/Package.xml".to_owned(),
                br#"<?xml version="1.0" encoding="utf-8"?><Package xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><Version>1.0.0.0</Version><MinVersion>1.0.0.0</MinVersion><Culture>en-US</Culture></Package>"#
                    .to_vec(),
            ),
            (
                "Formulas/Section1.m".to_owned(),
                section(queries).into_bytes(),
            ),
        ],
        false,
    );
    let permissions = br#"<?xml version="1.0" encoding="utf-8"?><PermissionList xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema"><CanEvaluateFuturePackages>false</CanEvaluateFuturePackages><FirewallEnabled>true</FirewallEnabled><WorkbookGroupType xsi:nil="true" /></PermissionList>"#;
    let meta_xml = metadata_xml(queries).into_bytes();
    let mut metadata = Vec::new();
    metadata.extend_from_slice(&0u32.to_le_bytes());
    metadata.extend_from_slice(&u32::try_from(meta_xml.len()).unwrap_or(0).to_le_bytes());
    metadata.extend(meta_xml);
    metadata.extend_from_slice(&0u32.to_le_bytes());
    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes());
    for part in [
        package.as_slice(),
        permissions.as_slice(),
        metadata.as_slice(),
        &[0u8],
    ] {
        out.extend_from_slice(&u32::try_from(part.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(part);
    }
    out
}

/// The custom XML item, as Excel writes it: UTF-16 with a byte order mark.
#[must_use]
pub fn item_xml(queries: &[Query]) -> Vec<u8> {
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"utf-16\"?><DataMashup xmlns=\"http://schemas.microsoft.com/DataMashup\">{}</DataMashup>",
        base64(&stream(queries))
    );
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// The parts that put `queries` into a workbook: the item, its properties,
/// and their relationships.
#[must_use]
pub fn parts(queries: &[Query]) -> Vec<Extra> {
    vec![
        Extra {
            name: "customXml/item1.xml".to_owned(),
            content_type: None,
            bytes: item_xml(queries),
            related_from: Some((
                "xl/workbook.xml".to_owned(),
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml"
                    .to_owned(),
            )),
        },
        Extra {
            name: "customXml/itemProps1.xml".to_owned(),
            content_type: Some(
                "application/vnd.openxmlformats-officedocument.customXmlProperties+xml".to_owned(),
            ),
            bytes: br#"<?xml version="1.0" encoding="UTF-8" standalone="no"?><ds:datastoreItem ds:itemID="{3B5C1E2A-7D44-4F0B-9A61-2C8E5D7F1B90}" xmlns:ds="http://schemas.openxmlformats.org/officeDocument/2006/customXml"><ds:schemaRefs/></ds:datastoreItem>"#
                .to_vec(),
            related_from: Some((
                "customXml/item1.xml".to_owned(),
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps"
                    .to_owned(),
            )),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn names_with_spaces_are_quoted() {
        let s = section(&[Query::new("Rates by month", "{X}", "1")]);
        assert!(s.contains("shared #\"Rates by month\" = 1;"));
    }
}
