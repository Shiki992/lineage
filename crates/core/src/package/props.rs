// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Custom document properties, and the two Lineage puts in a file:
//! `LineageFamilyId` and `LineageCheckpointId` (BLUEPRINT §4, Identity).
//!
//! The person's own custom properties are kept. Changes are splices, so the rest
//! of `docProps/custom.xml`, `[Content_Types].xml` and `_rels/.rels` stays as it was.

use crate::error::{Error, Result};
use crate::package::edit::{NewRelationship, add_override, add_relationships, free_part_name};
use crate::package::{ContentTypes, Package, Relationships, resolve_target};
use crate::xml::{self, Edits, escape_attr, escape_text, ns};

/// Name of the custom property holding the family id.
pub const FAMILY_ID: &str = "LineageFamilyId";
/// Name of the custom property holding the checkpoint id.
pub const CHECKPOINT_ID: &str = "LineageCheckpointId";
/// The format id Office uses for user-defined properties.
pub const FMTID_USER: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";

/// The two ids Lineage writes into a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineageIds {
    pub family: String,
    pub checkpoint: String,
}

/// One custom property: its name and its value as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomProperty {
    pub name: String,
    pub value: String,
}

/// The custom properties part, if the package has one.
#[must_use]
pub fn custom_part(package: &Package) -> Option<String> {
    let rels = package
        .part("_rels/.rels")
        .and_then(|b| Relationships::parse("_rels/.rels", b).ok())?;
    let rel = rels.of_kind("custom-properties").next()?;
    let part = resolve_target("", &rel.target);
    package.canonical_name(&part).map(str::to_owned)
}

/// Every custom property, in order.
///
/// # Errors
/// [`Error::BadXml`] when the part is damaged.
pub fn custom_properties(package: &Package) -> Result<Vec<CustomProperty>> {
    let Some(part) = custom_part(package) else {
        return Ok(Vec::new());
    };
    let bytes = package.part(&part).unwrap_or_default();
    let text = xml::text(&part, bytes)?;
    let doc = xml::parse(&part, text)?;
    Ok(doc
        .root_element()
        .children()
        .filter(|n| xml::is(*n, ns::CUSTOM_PROPS, "property"))
        .map(|p| CustomProperty {
            name: p.attribute("name").unwrap_or_default().to_owned(),
            value: p
                .children()
                .find(roxmltree::Node::is_element)
                .and_then(|v| v.text())
                .unwrap_or_default()
                .to_owned(),
        })
        .collect())
}

/// The Lineage ids in a file, if both are there.
///
/// # Errors
/// [`Error::BadXml`] when the properties part is damaged.
pub fn read_ids(package: &Package) -> Result<Option<LineageIds>> {
    let props = custom_properties(package)?;
    let get = |name: &str| {
        props
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.value.clone())
    };
    Ok(match (get(FAMILY_ID), get(CHECKPOINT_ID)) {
        (Some(family), Some(checkpoint)) if !family.is_empty() && !checkpoint.is_empty() => {
            Some(LineageIds { family, checkpoint })
        }
        _ => None,
    })
}

fn property_xml(prefix: &str, vt: &str, pid: u32, name: &str, value: &str) -> String {
    let element = if prefix.is_empty() {
        "property".to_owned()
    } else {
        format!("{prefix}:property")
    };
    format!(
        r#"<{element} fmtid="{FMTID_USER}" pid="{pid}" name="{}"><{vt}:lpwstr>{}</{vt}:lpwstr></{element}>"#,
        escape_attr(name),
        escape_text(value)
    )
}

/// A fresh custom properties part holding exactly these properties.
#[must_use]
pub fn custom_xml(props: &[(&str, &str)]) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Properties xmlns=\"{}\" xmlns:vt=\"{}\">",
        ns::CUSTOM_PROPS,
        ns::VT
    );
    for (index, (name, value)) in props.iter().enumerate() {
        let pid = u32::try_from(index).unwrap_or(0) + 2;
        out.push_str(&property_xml("", "vt", pid, name, value));
    }
    out.push_str("</Properties>");
    out
}

/// Writes (or updates) the two Lineage ids, keeping every other property.
///
/// # Errors
/// [`Error`] when an existing properties part, the content types or the package
/// relationships cannot be read.
pub fn write_ids(package: &mut Package, ids: &LineageIds) -> Result<()> {
    let wanted = [
        (FAMILY_ID, ids.family.as_str()),
        (CHECKPOINT_ID, ids.checkpoint.as_str()),
    ];
    let Some(part) = custom_part(package) else {
        add_custom_part(package, &custom_xml(&wanted))?;
        return Ok(());
    };
    let bytes = package.part(&part).unwrap_or_default().to_vec();
    let text = xml::text(&part, &bytes)?;
    let doc = xml::parse(&part, text)?;
    let root = doc.root_element();
    if !xml::is(root, ns::CUSTOM_PROPS, "Properties") {
        return Err(Error::BadXml {
            part: part.clone(),
            detail: "not a custom properties part".to_owned(),
        });
    }
    let prefix = xml::prefix_of(text, root).to_owned();
    let vt_declared = root.lookup_prefix(ns::VT).map(str::to_owned);
    let mut edits = Edits::new();
    let mut max_pid = 1_u32;
    for property in root
        .children()
        .filter(|n| xml::is(*n, ns::CUSTOM_PROPS, "property"))
    {
        if let Some(pid) = property
            .attribute("pid")
            .and_then(|p| p.parse::<u32>().ok())
        {
            max_pid = max_pid.max(pid);
        }
    }
    let vt = vt_declared.clone().unwrap_or_else(|| "vt".to_owned());
    let vt_decl = if vt_declared.is_some() {
        String::new()
    } else {
        format!(r#" xmlns:vt="{}""#, ns::VT)
    };
    // New properties go in together: appending twice to a self-closing
    // <Properties/> would expand the same tag twice.
    let mut added = String::new();
    for (name, value) in wanted {
        let existing = root.children().find(|n| {
            xml::is(*n, ns::CUSTOM_PROPS, "property") && n.attribute("name") == Some(name)
        });
        let value_xml = format!("<{vt}:lpwstr{vt_decl}>{}</{vt}:lpwstr>", escape_text(value));
        match existing {
            Some(property) => match property.children().find(roxmltree::Node::is_element) {
                Some(old) => edits.replace(old.range(), value_xml),
                None => xml::append_child(&mut edits, text, property, &value_xml),
            },
            None => {
                max_pid += 1;
                let mut fragment = property_xml(&prefix, &vt, max_pid, name, value);
                if vt_declared.is_none() {
                    fragment = fragment.replacen(
                        &format!("<{vt}:lpwstr>"),
                        &format!("<{vt}:lpwstr{vt_decl}>"),
                        1,
                    );
                }
                added.push_str(&fragment);
            }
        }
    }
    if !added.is_empty() {
        xml::append_child(&mut edits, text, root, &added);
    }
    let new = edits.apply(text)?;
    package.set_part(&part, new.into_bytes());
    Ok(())
}

/// Adds `docProps/custom.xml` with its content type and package relationship.
pub(crate) fn add_custom_part(package: &mut Package, xml_text: &str) -> Result<()> {
    if !package.contains(ContentTypes::PART) {
        return Err(Error::NotAnOfficeFile("no content types".to_owned()));
    }
    let part = if package.contains("docProps/custom.xml") {
        free_part_name(package, |n| format!("docProps/custom{n}.xml"))
    } else {
        "docProps/custom.xml".to_owned()
    };
    package.set_part(&part, xml_text.as_bytes().to_vec());
    add_override(package, &part, ns::CT_CUSTOM_PROPS)?;
    add_relationships(
        package,
        "",
        &[NewRelationship {
            rel_type: ns::REL_CUSTOM_PROPS.to_owned(),
            target: part,
            external: false,
        }],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fixture;

    fn ids(n: u8) -> LineageIds {
        LineageIds {
            family: format!("f{n:031}"),
            checkpoint: format!("c{n:031}"),
        }
    }

    #[test]
    fn adds_the_part_when_there_is_none() {
        let mut package =
            Package::read(&fixture("headings-paragraphs.docx").to_bytes()).expect("reads");
        assert_eq!(read_ids(&package).expect("reads"), None);
        write_ids(&mut package, &ids(1)).expect("writes");
        let reread = Package::read(&package.to_bytes().expect("bytes")).expect("rereads");
        assert_eq!(read_ids(&reread).expect("reads"), Some(ids(1)));
        crate::Docx::from_package(reread.clone(), crate::View::Accepted)
            .expect("still a Word document");
        let types = ContentTypes::parse(reread.part("[Content_Types].xml").expect("types"))
            .expect("parses");
        assert_eq!(types.of("docProps/custom.xml"), Some(ns::CT_CUSTOM_PROPS));
        // The document itself is untouched.
        assert!(reread.is_unchanged("word/document.xml"));
    }

    #[test]
    fn updates_ids_and_keeps_the_persons_own_properties() {
        let mut package =
            Package::read(&fixture("custom-properties.docx").to_bytes()).expect("reads");
        let before = custom_properties(&package).expect("reads");
        assert_eq!(before.len(), 4);
        write_ids(&mut package, &ids(1)).expect("writes");
        write_ids(&mut package, &ids(2)).expect("writes again");
        let after = custom_properties(&package).expect("reads");
        assert_eq!(after.len(), 6);
        assert_eq!(after[..4], before[..]);
        assert_eq!(read_ids(&package).expect("reads"), Some(ids(2)));
        let text = String::from_utf8(package.part("docProps/custom.xml").expect("part").to_vec())
            .expect("utf-8");
        assert!(
            text.contains(r#"pid="6""#),
            "new properties get the next pids"
        );
    }

    #[test]
    fn writes_into_an_empty_properties_element() {
        // Found in Phase 2: 40 of the maintainer's 45 documents have an empty,
        // self-closing <Properties/>. Adding two properties expanded it twice,
        // the edits overlapped, and Start tracking failed.
        for empty in [
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="{}" xmlns:vt="{}"/>"#,
                ns::CUSTOM_PROPS,
                ns::VT
            ),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><Properties xmlns="{}" xmlns:vt="{}" />"#,
                ns::CUSTOM_PROPS,
                ns::VT
            ),
            format!(r#"<Properties xmlns="{}"/>"#, ns::CUSTOM_PROPS),
        ] {
            let mut package =
                Package::read(&fixture("headings-paragraphs.docx").to_bytes()).expect("reads");
            add_custom_part(&mut package, &empty).expect("adds");
            assert_eq!(custom_properties(&package).expect("reads"), []);
            write_ids(&mut package, &ids(4)).expect("writes into <Properties/>");
            let reread = Package::read(&package.to_bytes().expect("bytes")).expect("rereads");
            assert_eq!(read_ids(&reread).expect("reads"), Some(ids(4)), "{empty}");
            let text =
                String::from_utf8(reread.part("docProps/custom.xml").expect("part").to_vec())
                    .expect("utf-8");
            assert!(text.contains(r#"pid="2""#) && text.contains(r#"pid="3""#));
        }
    }

    #[test]
    fn writing_the_same_ids_twice_is_stable() {
        let mut once =
            Package::read(&fixture("headings-paragraphs.docx").to_bytes()).expect("reads");
        write_ids(&mut once, &ids(3)).expect("writes");
        let mut twice = once.clone();
        write_ids(&mut twice, &ids(3)).expect("writes");
        assert_eq!(
            once.to_bytes().expect("bytes"),
            twice.to_bytes().expect("bytes")
        );
    }
}
