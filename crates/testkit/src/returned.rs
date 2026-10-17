// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Files that come back: what an outside person does to a copy before sending it
//! back, and the other documents the recognition test needs. Every name, company,
//! school and sentence here is invented.
//!
//! - Edits: [`light_edits`] (a mentor's few changes) and [`heavy_edits`] (a rewrite).
//! - How the file was saved ([`Saved`]): as built, as an **imitation** of a Google
//!   Docs download (built from the traits such exports are known to have, not made
//!   by Google), or heavily reformatted with the text unchanged.
//! - [`with_ids`]: the two Lineage ids in custom properties, as a sent copy carries them.
//! - Other documents: a cover letter in two versions, and a stranger's resume on the
//!   same template, which must never be taken for Avery's.
//! - [`LIBREOFFICE`]: the inputs `scripts/libreoffice-resave.ps1` re-saves with real
//!   LibreOffice into `fixtures/docx/returned/libreoffice/`.

use crate::docx::{Package, Part, Rel, esc, para};
use crate::resume::{self, Line};

fn t(text: &str) -> String {
    text.to_owned()
}

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// The text of a line, if it has plain text.
fn text_of(line: &Line) -> Option<&str> {
    match line {
        Line::Title(s) | Line::Plain(s) | Line::H1(s) | Line::H2(s) | Line::Bullet(s) => Some(s),
        Line::Raw(_) => None,
    }
}

fn with_text(line: &Line, text: String) -> Line {
    match line {
        Line::Title(_) => Line::Title(text),
        Line::H1(_) => Line::H1(text),
        Line::H2(_) => Line::H2(text),
        Line::Bullet(_) => Line::Bullet(text),
        Line::Plain(_) | Line::Raw(_) => Line::Plain(text),
    }
}

fn is_body(line: &Line) -> bool {
    matches!(line, Line::Plain(_) | Line::Bullet(_))
}

/// A mentor's light touch: the first word of the first long body line changed,
/// a new line after it, and a few words added to the last body line.
#[must_use]
pub fn light_edits(lines: &[Line]) -> Vec<Line> {
    let mut out = lines.to_vec();
    let long = |l: &Line| text_of(l).is_some_and(|s| words(s) >= 6);
    let first = out
        .iter()
        .position(|l| matches!(l, Line::Bullet(_)) && long(l))
        .or_else(|| {
            out.iter()
                .enumerate()
                .skip(2)
                .find(|(_, l)| is_body(l) && long(l))
                .map(|(i, _)| i)
        });
    if let Some(i) = first {
        let text = text_of(&out[i]).unwrap_or_default();
        let edited = if matches!(out[i], Line::Bullet(_)) {
            let rest = text.split_once(' ').map_or("", |(_, r)| r);
            format!("Designed {rest}")
        } else {
            let mut chars = text.chars();
            let head = chars.next().map(|c| c.to_lowercase().collect::<String>());
            format!("In short, {}{}", head.unwrap_or_default(), chars.as_str())
        };
        out[i] = with_text(&out[i], edited);
        out.insert(
            i + 1,
            with_text(
                &out[i],
                t("Mentored two new team members and wrote their onboarding guide."),
            ),
        );
    }
    if let Some(i) = out.iter().rposition(|l| is_body(l) && text_of(l).is_some()) {
        let text = text_of(&out[i]).unwrap_or_default();
        let trimmed = text.trim_end_matches('.');
        out[i] = with_text(&out[i], format!("{trimmed}, with honours."));
    }
    out
}

const REWRITES: [&str; 6] = [
    "Turned a tangle of spreadsheets into one dependable reporting service.",
    "Led the move to a shared data catalogue that every team now uses.",
    "Wrote the playbook the support desk follows when a nightly load fails.",
    "Ran fortnightly sessions teaching analysts to write their own queries.",
    "Cut cloud costs by a third by retiring tables nobody had read in a year.",
    "Paired with designers to make dashboards people could read at a glance.",
];

/// A rewrite: every other body line replaced, the last two lines removed and a
/// new section added at the end.
#[must_use]
pub fn heavy_edits(lines: &[Line]) -> Vec<Line> {
    let mut out = lines.to_vec();
    let mut next = 0;
    let mut toggle = false;
    for line in out.iter_mut().skip(2) {
        if is_body(line) {
            toggle = !toggle;
            if toggle {
                *line = with_text(line, t(REWRITES[next % REWRITES.len()]));
                next += 1;
            }
        }
    }
    out.truncate(out.len().saturating_sub(2));
    out.push(Line::H1(t("Volunteering")));
    out.push(Line::Plain(t(
        "Runs a weekend coding club for teenagers at the Lakeside library.",
    )));
    out
}

/// How a returned file was saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    /// As the testkit builds it, which is close to how Word saves.
    Word,
    /// An imitation of a Google Docs "Download as .docx": Google's style sheet,
    /// explicit Arial runs, `w:rtl` on every run, rsid attributes of zeros,
    /// `_heading=h.` bookmarks on headings, explicit spacing, a rewritten page
    /// setup, and no `docProps/app.xml` or custom properties. Not made by Google.
    /// Real exports (`fixtures/docx/returned/google/`) keep custom properties and
    /// put most fonts and spacing in their style sheet; this one is noisier.
    GoogleExport,
    /// Fonts, sizes, colours and heading styles changed throughout; numbered
    /// instead of bulleted lists; the same text.
    Reformatted,
}

const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const NS_W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

const GOOGLE_RPR: &str = r#"<w:rFonts w:ascii="Arial" w:cs="Arial" w:eastAsia="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:rtl w:val="0"/>"#;

fn google_run(text: &str) -> String {
    format!(
        r#"<w:r w:rsidDel="00000000" w:rsidR="00000000" w:rsidRPr="00000000"><w:rPr>{GOOGLE_RPR}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
        esc(text)
    )
}

fn google_para(n: usize, ppr: &str, bookmark: bool, text: &str) -> String {
    let mark = if bookmark {
        format!(
            r#"<w:bookmarkStart w:colFirst="0" w:colLast="0" w:name="_heading=h.{n:x}gjdgxs" w:id="{n}"/><w:bookmarkEnd w:id="{n}"/>"#
        )
    } else {
        String::new()
    };
    format!(
        r#"<w:p w:rsidR="00000000" w:rsidDel="00000000" w:rsidP="00000000" w:rsidRDefault="00000000" w:rsidRPr="00000000" w14:paraId="{:08X}"><w:pPr>{ppr}<w:rPr/></w:pPr>{mark}{}</w:p>"#,
        n + 1,
        google_run(text)
    )
}

fn google_line(n: usize, line: &Line) -> String {
    let keep = r#"<w:keepNext w:val="0"/><w:keepLines w:val="0"/>"#;
    match line {
        Line::Title(s) => google_para(
            n,
            &format!(
                r#"<w:pStyle w:val="Title"/>{keep}<w:spacing w:after="60" w:before="0" w:line="276" w:lineRule="auto"/>"#
            ),
            true,
            s,
        ),
        Line::H1(s) => google_para(
            n,
            &format!(
                r#"<w:pStyle w:val="Heading1"/>{keep}<w:spacing w:after="120" w:before="400" w:line="276" w:lineRule="auto"/>"#
            ),
            true,
            s,
        ),
        Line::H2(s) => google_para(
            n,
            &format!(
                r#"<w:pStyle w:val="Heading2"/>{keep}<w:spacing w:after="120" w:before="360" w:line="276" w:lineRule="auto"/>"#
            ),
            true,
            s,
        ),
        Line::Bullet(s) => google_para(
            n,
            r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr><w:spacing w:after="0" w:before="0" w:line="276" w:lineRule="auto"/><w:ind w:left="720" w:hanging="360"/>"#,
            false,
            s,
        ),
        Line::Plain(s) => google_para(
            n,
            r#"<w:spacing w:after="0" w:before="0" w:line="276" w:lineRule="auto"/>"#,
            false,
            s,
        ),
        Line::Raw(xml) => xml.clone(),
    }
}

fn google_styles() -> String {
    let heading = |id: &str, name: &str, before: u32, size: u32| {
        format!(
            r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{name}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext w:val="1"/><w:keepLines w:val="1"/><w:spacing w:after="120" w:before="{before}" w:lineRule="auto"/></w:pPr><w:rPr><w:sz w:val="{size}"/><w:szCs w:val="{size}"/></w:rPr></w:style>"#
        )
    };
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
            "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
            "<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Arial\" w:cs=\"Arial\" w:eastAsia=\"Arial\" w:hAnsi=\"Arial\"/>",
            "<w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/><w:lang w:val=\"en\"/></w:rPr></w:rPrDefault>",
            "<w:pPrDefault><w:pPr><w:spacing w:line=\"276\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>",
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"normal\"/><w:qFormat/></w:style>",
            "{h1}{h2}",
            "<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>",
            "<w:pPr><w:keepNext w:val=\"1\"/><w:keepLines w:val=\"1\"/><w:spacing w:after=\"60\" w:lineRule=\"auto\"/></w:pPr>",
            "<w:rPr><w:sz w:val=\"52\"/><w:szCs w:val=\"52\"/></w:rPr></w:style>",
            "<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/>",
            "<w:tblPr><w:tblInd w:w=\"0.0\" w:type=\"dxa\"/><w:tblCellMar><w:top w:w=\"0.0\" w:type=\"dxa\"/>",
            "<w:left w:w=\"0.0\" w:type=\"dxa\"/><w:bottom w:w=\"0.0\" w:type=\"dxa\"/><w:right w:w=\"0.0\" w:type=\"dxa\"/>",
            "</w:tblCellMar></w:tblPr></w:style>",
            "</w:styles>"
        ),
        h1 = heading("Heading1", "heading 1", 400, 40),
        h2 = heading("Heading2", "heading 2", 360, 32),
    )
}

const SERIF: &str = r#"<w:rFonts w:ascii="Georgia" w:hAnsi="Georgia" w:cs="Georgia"/>"#;

fn reformatted_line(line: &Line) -> String {
    let run = |rpr: &str, text: &str| {
        format!(
            r#"<w:r><w:rPr>{SERIF}{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
            esc(text)
        )
    };
    match line {
        Line::Title(s) => para(
            r#"<w:pStyle w:val="Title"/><w:jc w:val="center"/>"#,
            &run(r#"<w:color w:val="1F3864"/><w:sz w:val="56"/>"#, s),
        ),
        Line::H1(s) => para(
            r#"<w:spacing w:before="360" w:after="120"/>"#,
            &run(r#"<w:b/><w:caps/><w:sz w:val="28"/>"#, s),
        ),
        Line::H2(s) => para("", &run(r#"<w:b/><w:i/><w:sz w:val="24"/>"#, s)),
        Line::Bullet(s) => para(
            r#"<w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr>"#,
            &run(r#"<w:sz w:val="22"/>"#, s),
        ),
        Line::Plain(s) => para(r#"<w:jc w:val="both"/>"#, &run(r#"<w:sz w:val="22"/>"#, s)),
        Line::Raw(xml) => xml.clone(),
    }
}

/// A document from lines, saved the way `saved` says.
#[must_use]
pub fn package(title: &'static str, lines: &[Line], saved: Saved) -> Package {
    match saved {
        Saved::Word => resume::package(title, lines),
        Saved::GoogleExport => {
            let body: String = lines
                .iter()
                .enumerate()
                .map(|(n, l)| google_line(n, l))
                .collect();
            let mut p = Package::new(title, body);
            p.numbering = true;
            p.doc_ns = vec![("mc", NS_MC), ("w14", NS_W14)];
            p.ignorable = Some("w14");
            p.styles_xml = Some(google_styles());
            p.app_props = false;
            p.body_sect = Some(
                r#"<w:sectPr><w:pgSz w:h="15840" w:w="12240" w:orient="portrait"/><w:pgMar w:bottom="1440" w:top="1440" w:left="1440" w:right="1440" w:header="720" w:footer="720"/><w:pgNumType w:start="1"/></w:sectPr>"#
                    .to_owned(),
            );
            p
        }
        Saved::Reformatted => {
            let body: String = lines.iter().map(reformatted_line).collect();
            let mut p = Package::new(title, body);
            p.numbering = true;
            p
        }
    }
}

/// Adds the two Lineage ids as custom document properties, the way a sent copy
/// carries them (`docProps/custom.xml`, written here without the engine).
pub fn with_ids(package: &mut Package, family: &str, checkpoint: &str) {
    let property = |pid: u32, name: &str, value: &str| {
        format!(
            r#"<property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="{pid}" name="{name}"><vt:lpwstr>{}</vt:lpwstr></property>"#,
            esc(value)
        )
    };
    let xml = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
            "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/custom-properties\"",
            " xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">{}{}</Properties>"
        ),
        property(2, "LineageFamilyId", family),
        property(3, "LineageCheckpointId", checkpoint),
    );
    package.parts.push(Part {
        name: "docProps/custom.xml".to_owned(),
        content_type: Some(
            "application/vnd.openxmlformats-officedocument.custom-properties+xml".to_owned(),
        ),
        data: xml.into_bytes(),
    });
    package.root_rels.push(Rel {
        id: "rIdCustom",
        rel_type:
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties"
                .to_owned(),
        target: "docProps/custom.xml".to_owned(),
        external: false,
    });
}

/// A cover letter, version 1.
#[must_use]
pub fn cover_letter_v1_lines() -> Vec<Line> {
    vec![
        Line::Title(t("Avery Quinn")),
        Line::Plain(t("Lakeside, Ontario · avery.quinn@example.com")),
        Line::Plain(t("Hiring team, Northgate Analytics")),
        Line::Plain(t("Dear hiring team,")),
        Line::Plain(t(
            "I am writing about the senior data engineer role on your platform team. For six years I have built the pipelines that freight and insurance analysts rely on every morning, and I would like to bring that care for dependable data to Northgate.",
        )),
        Line::Plain(t(
            "At Corvia Freight I rebuilt the shipment warehouse so that forty analysts could trust one set of numbers. The nightly load that once took six hours now finishes in forty minutes, and the support desk has a written playbook for the rare night it fails.",
        )),
        Line::Plain(t(
            "I enjoy working closely with the people who use the data. I run short sessions on writing good queries, and I ask analysts what they need before I design a table.",
        )),
        Line::Plain(t(
            "Thank you for reading. I would be glad to talk about how I could help your team.",
        )),
        Line::Plain(t("Kind regards,")),
        Line::Plain(t("Avery Quinn")),
    ]
}

/// The cover letter, version 2: a new paragraph about streaming.
#[must_use]
pub fn cover_letter_v2_lines() -> Vec<Line> {
    let mut lines = cover_letter_v1_lines();
    lines.insert(
        6,
        Line::Plain(t(
            "Most recently at Halden Robotics I moved our nightly batch jobs to streaming, which cut report delays from hours to minutes.",
        )),
    );
    lines
}

/// Someone else's resume on the same template: the same headings, a different
/// person and different work. Never Avery's.
#[must_use]
pub fn stranger_lines() -> Vec<Line> {
    vec![
        Line::Title(t("Jordan Ellis")),
        Line::Plain(t(
            "Business analyst · Port Willow, Nova Scotia · jordan.ellis@example.com",
        )),
        Line::H1(t("Summary")),
        Line::Plain(t(
            "Business analyst with five years of experience turning sales and inventory data into plain reports for store managers.",
        )),
        Line::H1(t("Experience")),
        Line::H2(t("Analyst, Tidewater Grocers, 2021 to now")),
        Line::Bullet(t(
            "Built the weekly stock report that sixty store managers read on Monday mornings.",
        )),
        Line::Bullet(t(
            "Moved pricing checks from paper forms to a shared sheet.",
        )),
        Line::H2(t("Sales coordinator, Harbourline Books, 2019 to 2021")),
        Line::Bullet(t("Tracked orders for three hundred school libraries.")),
        Line::H1(t("Skills")),
        Line::Plain(t("Excel, SQL, Tableau, writing for busy readers")),
        Line::H1(t("Education")),
        Line::Plain(t("BA Economics, Saltmarsh College, 2019")),
    ]
}

/// A short unrelated document: meeting notes.
#[must_use]
pub fn meeting_notes_lines() -> Vec<Line> {
    vec![
        Line::Title(t("Garden committee notes")),
        Line::Plain(t("Present: four members and the treasurer.")),
        Line::H1(t("Decisions")),
        Line::Bullet(t(
            "The spring plant sale moves to the second Saturday of May.",
        )),
        Line::Bullet(t("New raised beds will be built from the donated cedar.")),
        Line::H1(t("Actions")),
        Line::Bullet(t(
            "The treasurer will ask the hardware shop for a quote on screws.",
        )),
        Line::Plain(t(
            "Next meeting: the first Tuesday of next month, at the library.",
        )),
    ]
}

/// The document a LibreOffice input is made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Avery's master resume, v1.
    MasterV1,
    /// The tailored version for data roles.
    TailoredData,
    /// The cover letter, v1.
    CoverLetterV1,
}

impl Source {
    #[must_use]
    pub fn lines(self) -> Vec<Line> {
        match self {
            Self::MasterV1 => resume::master_v1_lines(),
            Self::TailoredData => resume::tailored_data_lines(),
            Self::CoverLetterV1 => cover_letter_v1_lines(),
        }
    }
}

/// What the outside person did to the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edits {
    None,
    Light,
    Heavy,
}

impl Edits {
    #[must_use]
    pub fn apply(self, lines: &[Line]) -> Vec<Line> {
        match self {
            Self::None => lines.to_vec(),
            Self::Light => light_edits(lines),
            Self::Heavy => heavy_edits(lines),
        }
    }
}

/// The ids written into the LibreOffice inputs that carry them.
pub const LIBREOFFICE_FAMILY: &str = "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";
/// The sent copy id in those inputs.
pub const LIBREOFFICE_SENT: &str = "0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e";

/// One file for LibreOffice to re-save.
pub struct LibreOfficeInput {
    pub name: &'static str,
    pub source: Source,
    pub edits: Edits,
    /// Whether the input carries [`LIBREOFFICE_FAMILY`] and [`LIBREOFFICE_SENT`].
    pub ids: bool,
}

/// The inputs `scripts/libreoffice-resave.ps1` re-saves. The committed outputs
/// in `fixtures/docx/returned/libreoffice/` have the same names.
pub const LIBREOFFICE: [LibreOfficeInput; 6] = [
    LibreOfficeInput {
        name: "resume-unchanged-with-ids.docx",
        source: Source::MasterV1,
        edits: Edits::None,
        ids: true,
    },
    LibreOfficeInput {
        name: "resume-light-edits-with-ids.docx",
        source: Source::MasterV1,
        edits: Edits::Light,
        ids: true,
    },
    LibreOfficeInput {
        name: "resume-heavy-edits.docx",
        source: Source::MasterV1,
        edits: Edits::Heavy,
        ids: false,
    },
    LibreOfficeInput {
        name: "tailored-unchanged.docx",
        source: Source::TailoredData,
        edits: Edits::None,
        ids: false,
    },
    LibreOfficeInput {
        name: "tailored-light-edits.docx",
        source: Source::TailoredData,
        edits: Edits::Light,
        ids: false,
    },
    LibreOfficeInput {
        name: "cover-letter-light-edits-with-ids.docx",
        source: Source::CoverLetterV1,
        edits: Edits::Light,
        ids: true,
    },
];

impl LibreOfficeInput {
    /// The input file, as built here (before LibreOffice re-saves it).
    #[must_use]
    pub fn package(&self) -> Package {
        let mut p = resume::package(
            "Returned file for LibreOffice",
            &self.edits.apply(&self.source.lines()),
        );
        if self.ids {
            with_ids(&mut p, LIBREOFFICE_FAMILY, LIBREOFFICE_SENT);
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .filter_map(text_of)
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn light_edits_change_three_places() {
        let before = resume::master_v1_lines();
        let after = light_edits(&before);
        assert_eq!(after.len(), before.len() + 1);
        let changed = all_text(&after)
            .iter()
            .filter(|l| !all_text(&before).contains(l))
            .count();
        assert_eq!(changed, 3);
        assert!(
            all_text(&after)
                .iter()
                .any(|l| l.starts_with("Designed the shipment"))
        );
        let letter = light_edits(&cover_letter_v1_lines());
        assert_eq!(letter.len(), cover_letter_v1_lines().len() + 1);
    }

    #[test]
    fn heavy_edits_rewrite_much_of_the_text() {
        let before = resume::master_v1_lines();
        let after = heavy_edits(&before);
        let kept = all_text(&after)
            .iter()
            .filter(|l| all_text(&before).contains(l))
            .count();
        assert!(kept * 2 < all_text(&before).len() + 6, "{kept}");
    }

    #[test]
    fn every_way_of_saving_builds() {
        for saved in [Saved::Word, Saved::GoogleExport, Saved::Reformatted] {
            let p = package("t", &resume::master_v1_lines(), saved);
            let names: Vec<String> = p.parts().into_iter().map(|(n, _)| n).collect();
            assert_eq!(
                names.iter().any(|n| n == "docProps/app.xml"),
                saved != Saved::GoogleExport
            );
            assert!(!p.to_bytes().is_empty());
        }
    }

    #[test]
    fn ids_go_in_custom_properties() {
        let mut p = resume::package("t", &resume::master_v1_lines());
        with_ids(&mut p, LIBREOFFICE_FAMILY, LIBREOFFICE_SENT);
        let parts = p.parts();
        let custom = parts
            .iter()
            .find(|(n, _)| n == "docProps/custom.xml")
            .expect("custom properties");
        let text = String::from_utf8(custom.1.clone()).expect("utf-8");
        assert!(text.contains(LIBREOFFICE_FAMILY) && text.contains(LIBREOFFICE_SENT));
        let rels = String::from_utf8(
            parts
                .iter()
                .find(|(n, _)| n == "_rels/.rels")
                .expect("rels")
                .1
                .clone(),
        )
        .expect("utf-8");
        assert!(rels.contains("docProps/custom.xml"));
    }
}
