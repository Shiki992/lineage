// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Small, splice-only edits to the bookkeeping parts every Office package has:
//! `[Content_Types].xml` and the `.rels` parts. Each keeps the rest of the part
//! exactly as it was (decision 0005), and uses the prefix the part already uses.

use super::{ContentTypes, Package, Relationships, rels_part_for};
use crate::error::{Error, Result};
use crate::xml::{self, Edits, escape_attr, ns};

fn element(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}:{name}")
    }
}

fn content_types_text(package: &Package) -> Result<String> {
    let bytes = package
        .part(ContentTypes::PART)
        .ok_or_else(|| Error::NotAnOfficeFile("no content types".to_owned()))?;
    Ok(xml::text(ContentTypes::PART, bytes)?.to_owned())
}

/// Adds an `Override` for `part` (no leading slash), unless it already has one.
///
/// # Errors
/// [`Error`] when the content types part is missing or damaged.
pub fn add_override(package: &mut Package, part: &str, content_type: &str) -> Result<()> {
    let text = content_types_text(package)?;
    let types = ContentTypes::parse(text.as_bytes())?;
    if types.has_override(part) {
        return Ok(());
    }
    let doc = xml::parse(ContentTypes::PART, &text)?;
    let root = doc.root_element();
    let name = element(xml::prefix_of(&text, root), "Override");
    let mut edits = Edits::new();
    xml::append_child(
        &mut edits,
        &text,
        root,
        &format!(
            r#"<{name} PartName="/{}" ContentType="{}"/>"#,
            escape_attr(part.trim_start_matches('/')),
            escape_attr(content_type)
        ),
    );
    let new = edits.apply(&text)?;
    package.set_part(ContentTypes::PART, new.into_bytes());
    Ok(())
}

/// Adds a `Default` for a file extension, unless one is there.
///
/// # Errors
/// [`Error`] when the content types part is missing or damaged.
pub fn add_default(package: &mut Package, extension: &str, content_type: &str) -> Result<()> {
    let text = content_types_text(package)?;
    let doc = xml::parse(ContentTypes::PART, &text)?;
    let root = doc.root_element();
    let lower = extension.to_ascii_lowercase();
    let present = root.children().any(|n| {
        xml::is(n, ns::CONTENT_TYPES, "Default")
            && n.attribute("Extension")
                .is_some_and(|e| e.eq_ignore_ascii_case(&lower))
    });
    if present {
        return Ok(());
    }
    let name = element(xml::prefix_of(&text, root), "Default");
    let mut edits = Edits::new();
    // Defaults conventionally come first; Office reads them in any order.
    xml::prepend_child(
        &mut edits,
        &text,
        root,
        &format!(
            r#"<{name} Extension="{}" ContentType="{}"/>"#,
            escape_attr(extension),
            escape_attr(content_type)
        ),
    );
    let new = edits.apply(&text)?;
    package.set_part(ContentTypes::PART, new.into_bytes());
    Ok(())
}

/// Removes the `Override` for `part`, if any.
///
/// # Errors
/// [`Error`] when the content types part is missing or damaged.
pub fn remove_override(package: &mut Package, part: &str) -> Result<()> {
    let text = content_types_text(package)?;
    let doc = xml::parse(ContentTypes::PART, &text)?;
    let wanted = part.trim_start_matches('/');
    let mut edits = Edits::new();
    for node in doc.root_element().children().filter(|n| {
        xml::is(*n, ns::CONTENT_TYPES, "Override")
            && n.attribute("PartName")
                .is_some_and(|p| p.trim_start_matches('/').eq_ignore_ascii_case(wanted))
    }) {
        edits.delete(node.range());
    }
    if edits.is_empty() {
        return Ok(());
    }
    let new = edits.apply(&text)?;
    package.set_part(ContentTypes::PART, new.into_bytes());
    Ok(())
}

/// A relationship to add with [`add_relationships`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRelationship {
    pub rel_type: String,
    pub target: String,
    pub external: bool,
}

/// Adds relationships from `source` (a part name, or "" for the package) and
/// returns their new ids, in order. Ids never reuse one already in the part.
/// The `.rels` part is created when the source has none.
///
/// # Errors
/// [`Error`] when an existing `.rels` part is damaged.
pub fn add_relationships(
    package: &mut Package,
    source: &str,
    rels: &[NewRelationship],
) -> Result<Vec<String>> {
    if rels.is_empty() {
        return Ok(Vec::new());
    }
    let rels_part = rels_part_for(source);
    let existing = match package.part(&rels_part) {
        Some(bytes) => Relationships::parse(&rels_part, bytes)?,
        None => Relationships::default(),
    };
    let mut taken: Vec<String> = existing.items.iter().map(|r| r.id.clone()).collect();
    let mut next = existing.items.len() + 1;
    let mut ids = Vec::with_capacity(rels.len());
    let mut fragment = String::new();
    let prefix = match package.part(&rels_part) {
        Some(bytes) => {
            let text = xml::text(&rels_part, bytes)?;
            let doc = xml::parse(&rels_part, text)?;
            xml::prefix_of(text, doc.root_element()).to_owned()
        }
        None => String::new(),
    };
    let name = element(&prefix, "Relationship");
    for rel in rels {
        let id = loop {
            let candidate = format!("rId{next}");
            next += 1;
            if !taken.contains(&candidate) {
                break candidate;
            }
        };
        taken.push(id.clone());
        let mode = if rel.external {
            r#" TargetMode="External""#
        } else {
            ""
        };
        fragment.push_str(&format!(
            r#"<{name} Id="{id}" Type="{}" Target="{}"{mode}/>"#,
            escape_attr(&rel.rel_type),
            escape_attr(&rel.target)
        ));
        ids.push(id);
    }
    let new = match package.part(&rels_part) {
        Some(bytes) => {
            let text = xml::text(&rels_part, bytes)?;
            let doc = xml::parse(&rels_part, text)?;
            let mut edits = Edits::new();
            xml::append_child(&mut edits, text, doc.root_element(), &fragment);
            edits.apply(text)?
        }
        None => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"{}\">{fragment}</Relationships>",
            ns::PACKAGE_RELS
        ),
    };
    package.set_part(&rels_part, new.into_bytes());
    Ok(ids)
}

/// Removes relationships by id from `source`'s `.rels` part.
///
/// # Errors
/// [`Error`] when the `.rels` part is damaged.
pub fn remove_relationships(package: &mut Package, source: &str, ids: &[String]) -> Result<()> {
    let rels_part = rels_part_for(source);
    let Some(bytes) = package.part(&rels_part) else {
        return Ok(());
    };
    let text = xml::text(&rels_part, bytes)?.to_owned();
    let doc = xml::parse(&rels_part, &text)?;
    let mut edits = Edits::new();
    for node in doc.root_element().children().filter(|n| {
        xml::is(*n, ns::PACKAGE_RELS, "Relationship")
            && n.attribute("Id")
                .is_some_and(|id| ids.iter().any(|i| i == id))
    }) {
        edits.delete(node.range());
    }
    if edits.is_empty() {
        return Ok(());
    }
    let new = edits.apply(&text)?;
    package.set_part(&rels_part, new.into_bytes());
    Ok(())
}

/// The first free part name made by `make(n)` for n = 1, 2, …, compared without
/// case, as Office compares part names.
#[must_use]
pub fn free_part_name(package: &Package, make: impl Fn(usize) -> String) -> String {
    let mut n = 1;
    loop {
        let name = make(n);
        if !package.contains(&name) {
            return name;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> Package {
        Package::from_parts(vec![
            (
                ContentTypes::PART.to_owned(),
                format!(
                    r#"<?xml version="1.0"?><Types xmlns="{}"><Default Extension="xml" ContentType="application/xml"/></Types>"#,
                    ns::CONTENT_TYPES
                )
                .into_bytes(),
            ),
            ("a/b.xml".to_owned(), b"<x/>".to_vec()),
        ])
    }

    #[test]
    fn overrides_are_added_once_and_removed() {
        let mut package = tiny();
        add_override(&mut package, "a/b.xml", "t/b").expect("adds");
        add_override(&mut package, "/a/b.xml", "t/b").expect("adds again");
        let types =
            ContentTypes::parse(package.part(ContentTypes::PART).expect("types")).expect("parses");
        assert_eq!(types.of("a/b.xml"), Some("t/b"));
        assert_eq!(types.override_parts().count(), 1);
        remove_override(&mut package, "A/B.xml").expect("removes");
        let types =
            ContentTypes::parse(package.part(ContentTypes::PART).expect("types")).expect("parses");
        assert_eq!(types.of("a/b.xml"), Some("application/xml"));
    }

    #[test]
    fn defaults_are_added_once() {
        let mut package = tiny();
        add_default(&mut package, "png", "image/png").expect("adds");
        add_default(&mut package, "PNG", "image/png").expect("adds again");
        let types =
            ContentTypes::parse(package.part(ContentTypes::PART).expect("types")).expect("parses");
        assert_eq!(types.of("x/y.png"), Some("image/png"));
        let text = String::from_utf8(package.part(ContentTypes::PART).expect("t").to_vec())
            .expect("utf-8");
        assert_eq!(
            text.to_ascii_lowercase()
                .matches("extension=\"png\"")
                .count(),
            1
        );
    }

    #[test]
    fn relationships_get_fresh_ids_and_can_be_removed() {
        let mut package = tiny();
        let first = add_relationships(
            &mut package,
            "a/b.xml",
            &[
                NewRelationship {
                    rel_type: "t/one".to_owned(),
                    target: "c.xml".to_owned(),
                    external: false,
                },
                NewRelationship {
                    rel_type: "t/link".to_owned(),
                    target: "https://example.com/?a=1&b=2".to_owned(),
                    external: true,
                },
            ],
        )
        .expect("adds");
        assert_eq!(first, ["rId1", "rId2"]);
        let second = add_relationships(
            &mut package,
            "a/b.xml",
            &[NewRelationship {
                rel_type: "t/one".to_owned(),
                target: "d.xml".to_owned(),
                external: false,
            }],
        )
        .expect("adds");
        assert_eq!(second, ["rId3"]);
        let rels_part = rels_part_for("a/b.xml");
        let rels = Relationships::parse(&rels_part, package.part(&rels_part).expect("rels"))
            .expect("parses");
        assert_eq!(rels.items.len(), 3);
        assert!(rels.by_id("rId2").expect("rId2").external);
        assert_eq!(
            rels.by_id("rId2").expect("rId2").target,
            "https://example.com/?a=1&b=2"
        );
        remove_relationships(&mut package, "a/b.xml", &["rId1".to_owned()]).expect("removes");
        let rels = Relationships::parse(&rels_part, package.part(&rels_part).expect("rels"))
            .expect("parses");
        assert_eq!(rels.items.len(), 2);
        // A later addition never reuses an id still in the part.
        let third = add_relationships(
            &mut package,
            "a/b.xml",
            &[NewRelationship {
                rel_type: "t/one".to_owned(),
                target: "e.xml".to_owned(),
                external: false,
            }],
        )
        .expect("adds");
        assert_eq!(third, ["rId4"]);
    }

    #[test]
    fn free_names_skip_taken_ones_ignoring_case() {
        let package = tiny();
        assert_eq!(
            free_part_name(&package, |n| format!("A/B{n}.xml")),
            "A/B1.xml"
        );
        let package = Package::from_parts(vec![("a/b1.xml".to_owned(), Vec::new())]);
        assert_eq!(
            free_part_name(&package, |n| format!("A/B{n}.xml")),
            "A/B2.xml"
        );
    }
}
