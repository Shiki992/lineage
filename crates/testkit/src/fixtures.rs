// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The synthetic .docx fixtures. All content is invented: "Avery Quinn" and every
//! company, school and person named here are fictional. Never put real documents
//! or real people's data in this file.
//!
//! Each fixture is listed in `fixtures/docx/README.md`. Add a fixture here, run
//! `cargo xtask fixtures`, and add its row to the README.

use crate::docx::{
    FIXED_DATE, Orientation, Package, esc, list_item, para, plain, run, run_with, sect_pr, styled,
};
use crate::png;

/// Where generated fixtures live, relative to the repository root.
pub const OUT_DIR: &str = "fixtures/docx/synthetic";

/// Every synthetic fixture, as (file name, package).
pub fn all() -> Vec<(&'static str, Package)> {
    let mut all = phase_0();
    all.extend(crate::features::all());
    all.extend(crate::resume::all());
    all
}

/// The ten fixtures from Phase 0.
fn phase_0() -> Vec<(&'static str, Package)> {
    vec![
        ("headings-paragraphs.docx", headings_paragraphs()),
        ("lists.docx", lists()),
        ("table.docx", table()),
        ("header-footer.docx", header_footer()),
        ("footnote.docx", footnote()),
        ("comment.docx", comment()),
        ("tracked-changes.docx", tracked_changes()),
        ("image.docx", image()),
        ("fields.docx", fields()),
        ("sections.docx", sections()),
    ]
}

fn resume_top() -> String {
    [
        styled("Title", "Avery Quinn"),
        plain("Data engineer · Lakeside, Ontario · avery.quinn@example.com"),
    ]
    .concat()
}

fn headings_paragraphs() -> Package {
    let body = [
        resume_top(),
        styled("Heading1", "Summary"),
        para(
            "",
            &[
                run("Data engineer with "),
                run_with("<w:b/>", "six years"),
                run(" of experience building "),
                run_with("<w:i/>", "reliable"),
                run(" pipelines for retail and logistics teams."),
            ]
            .concat(),
        ),
        styled("Heading1", "Experience"),
        styled(
            "Heading2",
            "Senior data engineer, Halden Robotics, 2023 to now",
        ),
        plain(
            "Moved nightly batch jobs to streaming, cutting report delays from hours to minutes.",
        ),
        styled("Heading2", "Data engineer, Corvia Freight, 2020 to 2023"),
        plain("Built the shipment data warehouse used by forty analysts."),
        styled("Heading1", "Education"),
        plain("BSc Computer Science, Northbridge University, 2019"),
    ]
    .concat();
    Package::new("Avery Quinn resume: headings and paragraphs", body)
}

fn lists() -> Package {
    let body = [
        resume_top(),
        styled("Heading1", "Skills"),
        list_item(1, 0, "SQL and dbt"),
        list_item(1, 0, "Python"),
        list_item(1, 1, "pandas and Polars"),
        list_item(1, 1, "Airflow"),
        list_item(1, 0, "Cloud warehouses"),
        styled("Heading1", "Highlights"),
        list_item(2, 0, "Cut warehouse costs by a third."),
        list_item(2, 0, "Mentored three junior engineers."),
        list_item(2, 1, "Two were promoted within a year."),
        list_item(2, 0, "Ran the team's on-call rota."),
    ]
    .concat();
    let mut package = Package::new("Avery Quinn resume: lists", body);
    package.numbering = true;
    package
}

fn cell(width: u32, span: u32, content: &str) -> String {
    let span_xml = if span > 1 {
        format!(r#"<w:gridSpan w:val="{span}"/>"#)
    } else {
        String::new()
    };
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{span_xml}</w:tcPr>{content}</w:tc>"#
    )
}

fn table() -> Package {
    let header = format!(
        "<w:tr>{}</w:tr>",
        cell(
            9000,
            3,
            &para("", &run_with("<w:b/>", "Marrow Foods: traction by quarter"))
        )
    );
    let rows = [
        ("Quarter", "Monthly revenue", "Customers"),
        ("Q1 2026", "$12k", "8"),
        ("Q2 2026", "$18k", "12"),
        ("Q3 2026", "$26k", "17"),
    ]
    .iter()
    .map(|(a, b, c)| {
        format!(
            "<w:tr>{}{}{}</w:tr>",
            cell(3000, 1, &plain(a)),
            cell(3000, 1, &plain(b)),
            cell(3000, 1, &plain(c))
        )
    })
    .collect::<String>();
    let tbl = format!(
        concat!(
            r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="9000" w:type="dxa"/></w:tblPr>"#,
            r#"<w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>"#,
            "{header}{rows}</w:tbl>"
        ),
        header = header,
        rows = rows
    );
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        styled("Heading1", "Traction"),
        tbl,
        plain("All figures are invented for testing."),
    ]
    .concat();
    Package::new("Marrow Foods pitch brief: table", body)
}

fn page_field() -> String {
    concat!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
        r#"<w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>"#,
        r#"<w:r><w:t>1</w:t></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
    )
    .to_owned()
}

fn header_footer() -> Package {
    let body = [
        resume_top(),
        styled("Heading1", "Summary"),
        plain("This file has a header with the name and a footer with the page number."),
    ]
    .concat();
    let mut package = Package::new("Avery Quinn resume: header and footer", body);
    package.header = Some(para(
        r#"<w:jc w:val="right"/>"#,
        &run("Avery Quinn · Resume"),
    ));
    package.footer = Some(para(
        r#"<w:jc w:val="center"/>"#,
        &[run("Page "), page_field()].concat(),
    ));
    package
}

fn footnote() -> Package {
    let reference = r#"<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="1"/></w:r>"#;
    let body = [
        resume_top(),
        styled("Heading1", "Certifications"),
        para(
            "",
            &[
                run("Certified data steward"),
                reference.to_owned(),
                run(", 2024."),
            ]
            .concat(),
        ),
    ]
    .concat();
    let note = format!(
        concat!(
            r#"<w:footnote w:id="1"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr>"#,
            r#"<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r>"#,
            "{text}</w:p></w:footnote>"
        ),
        text = run(" Issued by the Northbridge Data Guild, an invented body."),
    );
    let mut package = Package::new("Avery Quinn resume: footnote", body);
    package.footnotes = Some(note);
    package
}

fn comment() -> Package {
    let body = [
        resume_top(),
        styled("Heading1", "Experience"),
        para(
            "",
            &[
                r#"<w:commentRangeStart w:id="0"/>"#.to_owned(),
                run("Led the analytics platform team."),
                r#"<w:commentRangeEnd w:id="0"/>"#.to_owned(),
                r#"<w:r><w:commentReference w:id="0"/></w:r>"#.to_owned(),
            ]
            .concat(),
        ),
    ]
    .concat();
    let comments = format!(
        r#"<w:comment w:id="0" w:author="Sam Rivera" w:date="{FIXED_DATE}" w:initials="SR"><w:p><w:r><w:annotationRef/></w:r>{}</w:p></w:comment>"#,
        run("Could this line mention the team size?")
    );
    let mut package = Package::new("Avery Quinn resume: comment", body);
    package.comments = Some(comments);
    package
}

fn tracked_changes() -> Package {
    let who = format!(r#"w:author="Sam Rivera" w:date="{FIXED_DATE}""#);
    let body = [
        resume_top(),
        styled("Heading1", "Experience"),
        // A word replaced: deletion then insertion.
        para(
            "",
            &format!(
                concat!(
                    "{a}<w:del w:id=\"101\" {who}><w:r><w:delText>four</w:delText></w:r></w:del>",
                    "<w:ins w:id=\"102\" {who}>{b}</w:ins>{c}"
                ),
                a = run("Led a team of "),
                b = run("six"),
                c = run(" analysts."),
                who = who
            ),
        ),
        // A whole inserted paragraph, including its paragraph mark.
        para(
            &format!(r#"<w:rPr><w:ins w:id="103" {who}/></w:rPr>"#),
            &format!(r#"<w:ins w:id="104" {who}>{}</w:ins>"#, run("Presented quarterly results to the board.")),
        ),
        // A formatting-only change: text made bold.
        para(
            "",
            &format!(
                r#"<w:r><w:rPr><w:b/><w:rPrChange w:id="105" {who}><w:rPr/></w:rPrChange></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                esc("Owned the data quality checks.")
            ),
        ),
    ]
    .concat();
    Package::new("Avery Quinn resume: tracked changes", body)
}

fn image() -> Package {
    let emu = 914_400; // one inch
    let drawing = format!(
        concat!(
            r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0">"#,
            r#"<wp:extent cx="{emu}" cy="{emu}"/><wp:docPr id="1" name="Picture 1" descr="Teal square logo"/>"#,
            r#"<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr>"#,
            r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
            r#"<pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="image1.png"/><pic:cNvPicPr/></pic:nvPicPr>"#,
            r#"<pic:blipFill><a:blip r:embed="rIdImage1"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
            r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{emu}" cy="{emu}"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>"#,
            r#"</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        ),
        emu = emu
    );
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        para("", &drawing),
        plain("The logo above is a generated teal square."),
    ]
    .concat();
    let mut package = Package::new("Marrow Foods pitch brief: image", body);
    package.image_png = Some(png::solid_square(16, [0x0E, 0x7C, 0x7B]));
    package
}

fn fields() -> Package {
    let date_field = concat!(
        r#"<w:fldSimple w:instr=" DATE \@ &quot;d MMMM yyyy&quot; ">"#,
        r#"<w:r><w:t>1 January 2026</w:t></w:r></w:fldSimple>"#
    );
    let body = [
        resume_top(),
        para("", &[run("Last updated: "), date_field.to_owned()].concat()),
        para("", &[run("This is page "), page_field(), run(".")].concat()),
    ]
    .concat();
    Package::new("Avery Quinn resume: fields", body)
}

fn sections() -> Package {
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        plain("Section one is portrait."),
        para(
            &sect_pr(false, Orientation::Portrait, true),
            &run("End of section one."),
        ),
        styled("Heading1", "Market map"),
        plain("Section two is landscape, for a wide chart."),
        para(
            &sect_pr(false, Orientation::Landscape, true),
            &run("End of section two."),
        ),
        styled("Heading1", "Team"),
        plain("Section three is portrait again."),
    ]
    .concat();
    Package::new("Marrow Foods pitch brief: sections", body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::io::{Cursor, Read};

    #[test]
    fn names_are_unique_docx() {
        let fixtures = all();
        let names: HashSet<_> = fixtures.iter().map(|(name, _)| *name).collect();
        assert_eq!(names.len(), fixtures.len());
        assert!(names.iter().all(|name| name.ends_with(".docx")));
    }

    #[test]
    fn generation_is_deterministic() {
        for ((name, a), (_, b)) in all().iter().zip(all().iter()) {
            assert_eq!(a.to_bytes(), b.to_bytes(), "{name} differs between runs");
        }
    }

    #[test]
    fn every_fixture_is_a_readable_package() {
        for (name, package) in all() {
            let expected = package.parts();
            let mut archive = zip::ZipArchive::new(Cursor::new(package.to_bytes()))
                .unwrap_or_else(|e| panic!("{name}: not a zip: {e}"));
            // Streamed fixtures also hold directory entries, which are not parts.
            let files: Vec<usize> = (0..archive.len())
                .filter(|&i| !archive.by_index(i).expect("entry").is_dir())
                .collect();
            assert_eq!(files.len(), expected.len(), "{name}: entry count");

            let content_types = String::from_utf8(expected[0].1.clone()).expect("utf-8");
            for (&index, (part, data)) in files.iter().zip(&expected) {
                let mut file = archive.by_index(index).expect("entry");
                assert_eq!(
                    file.name().expect("entry name").as_ref(),
                    part,
                    "{name}: entry order"
                );
                let mut read = Vec::new();
                file.read_to_end(&mut read)
                    .unwrap_or_else(|e| panic!("{name}/{part}: unreadable or bad CRC: {e}"));
                assert_eq!(&read, data, "{name}/{part}: content");

                let is_xml_part = part.ends_with(".xml") && !part.starts_with('[');
                if is_xml_part {
                    assert!(
                        content_types.contains(&format!("PartName=\"/{part}\"")),
                        "{name}: {part} has no content type override"
                    );
                }
            }
        }
    }
}
