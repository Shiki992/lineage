// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Which app last saved a file, and whether the formatting of a file that came
//! back can be trusted (decision 0014).
//!
//! A file opened and saved again in another app (LibreOffice, or downloaded
//! from Google Docs) comes back with that app's fonts, sizes and spacing on
//! every paragraph. Those are not changes the person made, so keeping one of
//! their changes should bring in the words only, in the master's formatting.
//! Two signals decide it, and the review always says which one did:
//!
//! - **The app**, from `docProps/app.xml`: Word, another app it knows, or none
//!   at all where the copy that left had one (an export: Word never drops it).
//! - **The formatting changed almost everywhere**: of the paragraphs whose text
//!   came back unchanged, at least half were reformatted.

use crate::diff::{DocDiff, Fate};
use crate::docx::{Block, Docx, StoryKey};
use crate::package::{Package, Relationships, resolve_target};
use crate::xml::{self, ns};

/// The least number of reformatted paragraphs for "changed almost everywhere".
pub const REFORMATTED_MIN: usize = 3;

/// Who last saved a file, as far as the file says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedBy {
    /// Microsoft Word.
    Word,
    /// Another app Lineage knows by name, such as "LibreOffice".
    Other(String),
    /// No name, or one Lineage doesn't know.
    Unknown,
}

/// The extended properties part (`docProps/app.xml`), if the package has one.
fn app_part(package: &Package) -> Option<String> {
    let rels = package
        .part("_rels/.rels")
        .and_then(|b| Relationships::parse("_rels/.rels", b).ok())?;
    let rel = rels.of_kind("extended-properties").next()?;
    let part = resolve_target("", &rel.target);
    package.canonical_name(&part).map(str::to_owned)
}

/// True when the package has extended properties (`docProps/app.xml`).
#[must_use]
pub fn has_app_properties(package: &Package) -> bool {
    app_part(package).is_some()
}

/// The `Application` named in the extended properties, if any.
#[must_use]
pub fn application(package: &Package) -> Option<String> {
    let part = app_part(package)?;
    let text = xml::text(&part, package.part(&part)?).ok()?;
    let doc = xml::parse(&part, text).ok()?;
    let name = xml::child(doc.root_element(), ns::EXTENDED_PROPS, "Application")?
        .text()?
        .trim()
        .to_owned();
    (!name.is_empty()).then_some(name)
}

/// Apps known to rewrite a document's formatting when they save it, by a piece
/// of the name they write, and the name to show.
const OTHER_APPS: [(&str, &str); 8] = [
    ("libreoffice", "LibreOffice"),
    ("openoffice", "OpenOffice"),
    ("neooffice", "NeoOffice"),
    ("onlyoffice", "ONLYOFFICE"),
    ("wps office", "WPS Office"),
    ("kingsoft", "WPS Office"),
    ("google", "Google Docs"),
    ("abiword", "AbiWord"),
];

/// Who last saved the file, from the name in its extended properties.
#[must_use]
pub fn saved_by(package: &Package) -> SavedBy {
    let Some(name) = application(package) else {
        return SavedBy::Unknown;
    };
    let lower = name.to_lowercase();
    if lower.contains("microsoft") && lower.contains("word") {
        return SavedBy::Word;
    }
    OTHER_APPS
        .iter()
        .find(|(piece, _)| lower.contains(piece))
        .map_or(SavedBy::Unknown, |(_, shown)| {
            SavedBy::Other((*shown).to_owned())
        })
}

/// Why a returned file's formatting is not to be trusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unreliable {
    /// Another app saved it last; its name.
    OtherApp(String),
    /// It has no extended properties though the copy that left had them: it
    /// was exported, as Google Docs does, not saved by Word.
    Exported,
    /// Most paragraphs whose text came back unchanged were reformatted.
    ReformattedThroughout,
}

/// Of the body paragraphs whose text came back unchanged, how many there are
/// and how many were reformatted.
#[must_use]
pub fn reformatted_share(base: &Docx, theirs_diff: &DocDiff) -> (usize, usize) {
    let Some(seq) = theirs_diff
        .story(&StoryKey::Body)
        .and_then(|s| s.blocks.as_ref())
    else {
        return (0, 0);
    };
    let blocks = &base.body().blocks;
    let (mut same_text, mut reformatted) = (0, 0);
    for (i, fate) in seq.alignment.fates.iter().enumerate() {
        if !matches!(blocks.get(i), Some(Block::Paragraph(_))) {
            continue;
        }
        match *fate {
            Fate::Same(_) => same_text += 1,
            Fate::Formatted(_) => {
                same_text += 1;
                reformatted += 1;
            }
            Fate::Moved {
                content_same: true,
                format_same,
                ..
            } => {
                same_text += 1;
                reformatted += usize::from(!format_same);
            }
            _ => {}
        }
    }
    (same_text, reformatted)
}

/// Whether the formatting of `theirs`, a file that came back, can be trusted.
/// `base` is what left, and `theirs_diff` the changes from it to `theirs`.
/// `None` means it can: kept changes bring their formatting with them.
#[must_use]
pub fn formatting_trust(base: &Docx, theirs: &Docx, theirs_diff: &DocDiff) -> Option<Unreliable> {
    if let SavedBy::Other(name) = saved_by(&theirs.package) {
        return Some(Unreliable::OtherApp(name));
    }
    if has_app_properties(&base.package) && !has_app_properties(&theirs.package) {
        return Some(Unreliable::Exported);
    }
    let (same_text, reformatted) = reformatted_share(base, theirs_diff);
    if reformatted >= REFORMATTED_MIN && reformatted * 2 >= same_text {
        return Some(Unreliable::ReformattedThroughout);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::diff;
    use crate::testutil::open;
    use lineage_testkit::docx::Package as Builder;
    use lineage_testkit::resume;
    use lineage_testkit::returned::{self as kit, LIBREOFFICE, Saved};

    fn trust(base: &Docx, theirs: &Docx) -> Option<Unreliable> {
        formatting_trust(base, theirs, &diff(base, theirs))
    }

    fn with_app(name: &str) -> Docx {
        let mut b = resume::package("t", &kit::light_edits(&resume::master_v1_lines()));
        b.app_name = Some(name.to_owned());
        open(&b.to_bytes())
    }

    fn base() -> Docx {
        open(&resume::package("t", &resume::master_v1_lines()).to_bytes())
    }

    #[test]
    fn the_app_is_read_from_the_extended_properties() {
        assert_eq!(
            saved_by(&with_app("Microsoft Office Word").package),
            SavedBy::Word
        );
        assert_eq!(
            saved_by(&with_app("Microsoft Macintosh Word").package),
            SavedBy::Word
        );
        assert_eq!(
            saved_by(&with_app("LibreOffice/26.8.1.1$Windows_X86_64").package),
            SavedBy::Other("LibreOffice".to_owned())
        );
        assert_eq!(saved_by(&with_app("Some editor").package), SavedBy::Unknown);
        let mut none = Builder::new("t", String::new());
        none.app_props = false;
        assert_eq!(saved_by(&open(&none.to_bytes()).package), SavedBy::Unknown);
        assert!(!has_app_properties(&open(&none.to_bytes()).package));
    }

    #[test]
    fn real_libreoffice_saves_are_not_trusted() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/docx/returned/libreoffice");
        for input in &LIBREOFFICE {
            let base = open(&resume::package("t", &input.source.lines()).to_bytes());
            let theirs = open(&std::fs::read(root.join(input.name)).expect("fixture"));
            assert_eq!(
                trust(&base, &theirs),
                Some(Unreliable::OtherApp("LibreOffice".to_owned())),
                "{}",
                input.name
            );
        }
    }

    #[test]
    fn real_google_docs_exports_are_exports() {
        // Downloaded from Google Docs (File > Download > Microsoft Word, here
        // through the Drive API's export): no docProps/app.xml at all.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/docx/returned/google");
        for input in &LIBREOFFICE {
            let base = open(&resume::package("t", &input.source.lines()).to_bytes());
            let theirs = open(&std::fs::read(root.join(input.name)).expect("fixture"));
            assert!(!has_app_properties(&theirs.package), "{}", input.name);
            assert_eq!(
                trust(&base, &theirs),
                Some(Unreliable::Exported),
                "{}",
                input.name
            );
        }
    }

    #[test]
    fn exports_and_restyles_are_not_trusted_word_edits_are() {
        let lines = resume::master_v1_lines();
        for edits in [
            lines.clone(),
            kit::light_edits(&lines),
            kit::heavy_edits(&lines),
        ] {
            let google = open(&kit::package("t", &edits, Saved::GoogleExport).to_bytes());
            assert_eq!(trust(&base(), &google), Some(Unreliable::Exported));
            let restyled = open(&kit::package("t", &edits, Saved::Reformatted).to_bytes());
            assert_eq!(
                trust(&base(), &restyled),
                Some(Unreliable::ReformattedThroughout)
            );
            let word = open(&kit::package("t", &edits, Saved::Word).to_bytes());
            assert_eq!(trust(&base(), &word), None);
        }
        assert_eq!(trust(&base(), &with_app("Microsoft Office Word")), None);
    }

    #[test]
    fn a_missing_app_part_is_a_signal_only_when_the_copy_that_left_had_one() {
        let mut base_pkg = resume::package("t", &resume::master_v1_lines());
        base_pkg.app_props = false;
        let base = open(&base_pkg.to_bytes());
        let mut theirs = resume::package("t", &kit::light_edits(&resume::master_v1_lines()));
        theirs.app_props = false;
        assert_eq!(trust(&base, &open(&theirs.to_bytes())), None);
    }

    #[test]
    fn a_few_reformatted_paragraphs_are_not_throughout() {
        let base_bytes = resume::package("t", &resume::master_v2_lines()).to_bytes();
        let promoted = resume::package("t", &resume::master_v3_lines()).to_bytes();
        let (base, theirs) = (open(&base_bytes), open(&promoted));
        let (same, reformatted) = reformatted_share(&base, &diff(&base, &theirs));
        assert_eq!(reformatted, 1, "the name made bold");
        assert!(same > 10);
        assert_eq!(trust(&base, &theirs), None);
    }
}
