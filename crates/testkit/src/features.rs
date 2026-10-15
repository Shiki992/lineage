// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Fixtures for Word features the engine must read and preserve: markup Word
//! itself writes, unknown XML, links, fields, tables, notes, content controls,
//! shapes, maths, special characters, styles, headers, custom properties, zip
//! layouts and existing revisions. All content is invented.

use crate::docx::{FIXED_DATE, Package, Part, Rel, Zip, esc, para, plain, run, run_with, styled};

const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const NS_W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const NS_W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
const NS_WP14: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing";
const NS_WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const NS_V: &str = "urn:schemas-microsoft-com:vml";
const NS_O: &str = "urn:schemas-microsoft-com:office:office";
const NS_M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const NS_TEST: &str = "urn:example:lineage-test";
const CT_WML: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

fn who() -> String {
    format!(r#"w:author="Sam Rivera" w:date="{FIXED_DATE}""#)
}

fn resume_top() -> String {
    [
        styled("Title", "Avery Quinn"),
        plain("Data engineer · Lakeside, Ontario · avery.quinn@example.com"),
    ]
    .concat()
}

/// Every feature fixture, as (file name, package).
pub fn all() -> Vec<(&'static str, Package)> {
    vec![
        ("word-markup.docx", word_markup()),
        ("unknown-xml.docx", unknown_xml()),
        ("hyperlinks.docx", hyperlinks()),
        ("bookmarks-crossref.docx", bookmarks_crossref()),
        ("toc-field.docx", toc_field()),
        ("nested-table.docx", nested_table()),
        ("table-merged-cells.docx", table_merged_cells()),
        ("endnotes.docx", endnotes()),
        ("content-controls.docx", content_controls()),
        ("text-box.docx", text_box()),
        ("math.docx", math()),
        ("inline-specials.docx", inline_specials()),
        ("unicode-text.docx", unicode_text()),
        ("character-styles.docx", character_styles()),
        ("empty-and-whitespace.docx", empty_and_whitespace()),
        ("multi-headers.docx", multi_headers()),
        ("custom-properties.docx", custom_properties()),
        ("deflated.docx", deflated()),
        ("data-descriptor.docx", data_descriptor()),
        ("tracked-moves-and-tables.docx", tracked_moves_and_tables()),
    ]
}

/// Markup Word writes on save: paragraph and text ids, rsids, proofing marks, the
/// `_GoBack` bookmark and rendered page breaks.
fn word_markup() -> Package {
    let p = |id: u32, rsid: &str, content: &str| {
        format!(
            r#"<w:p w14:paraId="{id:08X}" w14:textId="77{id:06X}" w:rsidR="{rsid}" w:rsidRDefault="{rsid}" w:rsidP="{rsid}">{content}</w:p>"#
        )
    };
    let r = |rsid: &str, text: &str| {
        format!(
            r#"<w:r w:rsidRPr="{rsid}"><w:t xml:space="preserve">{}</w:t></w:r>"#,
            esc(text)
        )
    };
    let body = [
        p(
            0x1A2B_0001,
            "00A1B2C3",
            &format!(r#"<w:pPr><w:pStyle w:val="Title"/></w:pPr>{}"#, r("00A1B2C3", "Avery Quinn")),
        ),
        p(
            0x1A2B_0002,
            "00A1B2C3",
            &[
                r("00A1B2C3", "Data engineer who "),
                r#"<w:proofErr w:type="spellStart"/>"#.to_owned(),
                r("00D4E5F6", "replatformed"),
                r#"<w:proofErr w:type="spellEnd"/>"#.to_owned(),
                r("00A1B2C3", " two warehouses."),
            ]
            .concat(),
        ),
        p(
            0x1A2B_0003,
            "00D4E5F6",
            r#"<w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:lastRenderedPageBreak/><w:t>Experience</w:t></w:r>"#,
        ),
        p(
            0x1A2B_0004,
            "00D4E5F6",
            &[
                r#"<w:bookmarkStart w:id="0" w:name="_GoBack"/>"#.to_owned(),
                r("00D4E5F6", "Led the move to a shared metrics layer."),
                r#"<w:bookmarkEnd w:id="0"/>"#.to_owned(),
            ]
            .concat(),
        ),
    ]
    .concat();
    let mut package = Package::new("Avery Quinn resume: Word markup", body);
    package.doc_ns = vec![
        ("mc", NS_MC),
        ("w14", NS_W14),
        ("w15", NS_W15),
        ("wp14", NS_WP14),
    ];
    package.ignorable = Some("w14 w15 wp14");
    package.settings_tail = concat!(
        r#"<w:rsids><w:rsidRoot w:val="00A1B2C3"/><w:rsid w:val="00A1B2C3"/>"#,
        r#"<w:rsid w:val="00D4E5F6"/></w:rsids>"#
    )
    .to_owned();
    package
}

/// Elements, attributes and parts the engine does not know, which must survive.
fn unknown_xml() -> Package {
    let body = [
        resume_top(),
        "<!-- A comment written by some other tool. -->".to_owned(),
        r#"<lx:note lx:kind="private"><lx:text>Not for Word</lx:text></lx:note>"#.to_owned(),
        format!(
            r#"<w:p lx:tag="summary"><w:r><lx:mark lx:level="2"/><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            esc("Data engineer with six years of experience.")
        ),
        "<?lineage-test keep this instruction?>".to_owned(),
        plain("The last paragraph."),
    ]
    .concat();
    let mut package = Package::new("Avery Quinn resume: unknown XML", body);
    package.doc_ns = vec![("mc", NS_MC), ("lx", NS_TEST)];
    package.ignorable = Some("lx");
    package.defaults = vec![("bin", "application/octet-stream")];
    package.parts = vec![Part {
        name: "lineage-test/data.bin".to_owned(),
        content_type: None,
        data: (0_u8..64).collect(),
    }];
    package.rels = vec![Rel {
        id: "rIdUnknown",
        // Word refuses relationship types that are not http URIs.
        rel_type: "http://example.com/lineage-test/relationship".to_owned(),
        target: "../lineage-test/data.bin".to_owned(),
        external: false,
    }];
    package
}

const HYPERLINK_STYLE: &str = r#"<w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:basedOn w:val="DefaultParagraphFont"/><w:uiPriority w:val="99"/><w:unhideWhenUsed/><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr></w:style>"#;

fn link_run(text: &str) -> String {
    run_with(r#"<w:rStyle w:val="Hyperlink"/>"#, text)
}

fn hyperlinks() -> Package {
    let body = [
        styled("Title", "Avery Quinn"),
        para(
            "",
            &[
                run("Email: "),
                format!(
                    r#"<w:hyperlink r:id="rIdMail" w:history="1">{}</w:hyperlink>"#,
                    link_run("avery.quinn@example.com")
                ),
                run(" · Portfolio: "),
                format!(
                    r#"<w:hyperlink r:id="rIdWeb" w:history="1">{}{}</w:hyperlink>"#,
                    link_run("example.com/"),
                    link_run("avery")
                ),
            ]
            .concat(),
        ),
        para(
            "",
            &[
                run("See "),
                format!(
                    r#"<w:hyperlink w:anchor="Education" w:history="1">{}</w:hyperlink>"#,
                    link_run("Education")
                ),
                run(" below."),
            ]
            .concat(),
        ),
        para(
            r#"<w:pStyle w:val="Heading1"/>"#,
            &[
                r#"<w:bookmarkStart w:id="1" w:name="Education"/>"#.to_owned(),
                run("Education"),
                r#"<w:bookmarkEnd w:id="1"/>"#.to_owned(),
            ]
            .concat(),
        ),
        plain("BSc Computer Science, Northbridge University, 2019"),
    ]
    .concat();
    let mut package = Package::new("Avery Quinn resume: hyperlinks", body);
    package.extra_styles = HYPERLINK_STYLE.to_owned();
    package.rels = vec![
        Rel::hyperlink("rIdMail", "mailto:avery.quinn@example.com"),
        Rel::hyperlink("rIdWeb", "https://example.com/avery"),
    ];
    package
}

fn field(instruction: &str, result: &str) -> String {
    format!(
        concat!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
            r#"<w:r><w:instrText xml:space="preserve">{}</w:instrText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>"#,
            r#"<w:r><w:t>{}</w:t></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
        ),
        esc(instruction),
        esc(result)
    )
}

fn bookmarks_crossref() -> Package {
    let body = [
        resume_top(),
        para(
            r#"<w:pStyle w:val="Heading1"/>"#,
            &[
                r#"<w:bookmarkStart w:id="1" w:name="_Ref1001"/>"#.to_owned(),
                run("Certifications"),
                r#"<w:bookmarkEnd w:id="1"/>"#.to_owned(),
            ]
            .concat(),
        ),
        plain("Certified data steward, 2024."),
        para(
            "",
            &[
                run("Details are under "),
                field(r" REF _Ref1001 \h ", "Certifications"),
                run(" on page "),
                field(r" PAGEREF _Ref1001 \h ", "1"),
                run("."),
            ]
            .concat(),
        ),
    ]
    .concat();
    Package::new("Avery Quinn resume: bookmarks and cross-references", body)
}

const TOC_STYLES: &str = r#"<w:style w:type="paragraph" w:styleId="TOC1"><w:name w:val="toc 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:uiPriority w:val="39"/><w:unhideWhenUsed/><w:pPr><w:spacing w:after="100"/></w:pPr></w:style>"#;

/// A table of contents: one complex field that starts in the first entry and ends
/// in a paragraph of its own, with a hyperlink and page reference per entry.
fn toc_field() -> Package {
    let toc_ppr = r#"<w:pStyle w:val="TOC1"/><w:tabs><w:tab w:val="right" w:leader="dot" w:pos="9350"/></w:tabs>"#;
    let entry = |anchor: &str, text: &str| {
        format!(
            r#"<w:hyperlink w:anchor="{anchor}" w:history="1">{}<w:r><w:tab/></w:r>{}</w:hyperlink>"#,
            run(text),
            field(&format!(" PAGEREF {anchor} \\h "), "1")
        )
    };
    let heading = |id: u32, anchor: &str, text: &str| {
        para(
            r#"<w:pStyle w:val="Heading1"/>"#,
            &[
                format!(r#"<w:bookmarkStart w:id="{id}" w:name="{anchor}"/>"#),
                run(text),
                format!(r#"<w:bookmarkEnd w:id="{id}"/>"#),
            ]
            .concat(),
        )
    };
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        para(
            toc_ppr,
            &[
                r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#.to_owned(),
                r#"<w:r><w:instrText xml:space="preserve"> TOC \o "1-1" \h \z \u </w:instrText></w:r>"#
                    .to_owned(),
                r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>"#.to_owned(),
                entry("_Toc1", "Problem"),
            ]
            .concat(),
        ),
        para(toc_ppr, &entry("_Toc2", "Traction")),
        para("", r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#),
        heading(1, "_Toc1", "Problem"),
        plain("Small grocers waste a fifth of their fresh stock."),
        heading(2, "_Toc2", "Traction"),
        plain("Seventeen paying customers after three quarters."),
    ]
    .concat();
    let mut package = Package::new("Marrow Foods pitch brief: table of contents", body);
    package.extra_styles = TOC_STYLES.to_owned();
    package
}

fn tc(width: u32, extra_pr: &str, content: &str) -> String {
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{extra_pr}</w:tcPr>{content}</w:tc>"#
    )
}

fn table_xml(widths: &[u32], rows: &str) -> String {
    let total: u32 = widths.iter().sum();
    let grid: String = widths
        .iter()
        .map(|w| format!(r#"<w:gridCol w:w="{w}"/>"#))
        .collect();
    format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="{total}" w:type="dxa"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{rows}</w:tbl>"#
    )
}

fn nested_table() -> Package {
    let inner = table_xml(
        &[1800, 1800],
        &[
            format!(
                "<w:tr>{}{}</w:tr>",
                tc(1800, "", &plain("Region")),
                tc(1800, "", &plain("Stores"))
            ),
            format!(
                "<w:tr>{}{}</w:tr>",
                tc(1800, "", &plain("North")),
                tc(1800, "", &plain("11"))
            ),
        ]
        .concat(),
    );
    let outer = table_xml(
        &[4500, 4500],
        &[
            format!(
                "<w:tr>{}{}</w:tr>",
                tc(4500, "", &plain("Pilot regions")),
                // A cell must end with a paragraph, even after a nested table.
                tc(4500, "", &[inner, plain("")].concat())
            ),
            format!(
                "<w:tr>{}{}</w:tr>",
                tc(4500, "", &plain("Next step")),
                tc(4500, "", &plain("Open two more regions."))
            ),
        ]
        .concat(),
    );
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        outer,
        plain("After the table."),
    ]
    .concat();
    Package::new("Marrow Foods pitch brief: nested table", body)
}

fn table_merged_cells() -> Package {
    let header = format!(
        r#"<w:tr><w:trPr><w:tblHeader/></w:trPr>{}{}{}</w:tr>"#,
        tc(3000, "", &plain("Quarter")),
        tc(3000, "", &plain("Metric")),
        tc(3000, "", &plain("Value"))
    );
    let shaded = r#"<w:shd w:val="clear" w:color="auto" w:fill="E3F2F1"/>"#;
    let rows = [
        header,
        format!(
            "<w:tr>{}{}{}</w:tr>",
            tc(
                3000,
                &format!(r#"<w:vMerge w:val="restart"/>{shaded}"#),
                &plain("Q3 2026")
            ),
            tc(3000, "", &plain("Revenue")),
            tc(3000, "", &plain("$26k"))
        ),
        format!(
            "<w:tr>{}{}{}</w:tr>",
            tc(3000, &format!("<w:vMerge/>{shaded}"), &plain("")),
            tc(3000, "", &plain("Customers")),
            tc(3000, "", &plain("17"))
        ),
        format!(
            "<w:tr>{}{}</w:tr>",
            tc(6000, r#"<w:gridSpan w:val="2"/>"#, &plain("Total raised")),
            tc(3000, "", &plain("$400k"))
        ),
    ]
    .concat();
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        table_xml(&[3000, 3000, 3000], &rows),
        plain("Merged cells above: one spans two rows, one spans two columns."),
    ]
    .concat();
    Package::new("Marrow Foods pitch brief: merged cells", body)
}

const ENDNOTE_STYLES: &str = r#"<w:style w:type="paragraph" w:styleId="EndnoteText"><w:name w:val="endnote text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:sz w:val="20"/></w:rPr></w:style><w:style w:type="character" w:styleId="EndnoteReference"><w:name w:val="endnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>"#;

fn endnotes() -> Package {
    let reference = |id: u32| {
        format!(
            r#"<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteReference w:id="{id}"/></w:r>"#
        )
    };
    let note = |id: u32, text: &str| {
        format!(
            concat!(
                r#"<w:endnote w:id="{id}"><w:p><w:pPr><w:pStyle w:val="EndnoteText"/></w:pPr>"#,
                r#"<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteRef/></w:r>{text}</w:p></w:endnote>"#
            ),
            id = id,
            text = run(text)
        )
    };
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        para(
            "",
            &[
                run("Grocers waste a fifth of fresh stock"),
                reference(1),
                run("."),
            ]
            .concat(),
        ),
        para(
            "",
            &[
                run("Pilot stores cut waste by half"),
                reference(2),
                run("."),
            ]
            .concat(),
        ),
    ]
    .concat();
    let mut package = Package::new("Marrow Foods pitch brief: endnotes", body);
    package.endnotes = Some(
        [
            note(1, " Invented survey of 40 grocers, 2025."),
            note(2, " Invented pilot, three stores, 2026."),
        ]
        .concat(),
    );
    package.extra_styles = ENDNOTE_STYLES.to_owned();
    package
}

fn content_controls() -> Package {
    let block = format!(
        concat!(
            r#"<w:sdt><w:sdtPr><w:alias w:val="Contact details"/><w:tag w:val="contact"/><w:id w:val="1001"/></w:sdtPr>"#,
            r#"<w:sdtContent>{}{}</w:sdtContent></w:sdt>"#
        ),
        plain("Lakeside, Ontario"),
        plain("avery.quinn@example.com")
    );
    let inline = para(
        "",
        &[
            run("Phone: "),
            format!(
                r#"<w:sdt><w:sdtPr><w:id w:val="1002"/><w:text/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#,
                run("555 0100")
            ),
        ]
        .concat(),
    );
    let checkbox = para(
        "",
        &[
            format!(
                concat!(
                    r#"<w:sdt><w:sdtPr><w:id w:val="1003"/><w14:checkbox><w14:checked w14:val="1"/>"#,
                    r#"<w14:checkedState w14:val="2612" w14:font="MS Gothic"/><w14:uncheckedState w14:val="2610" w14:font="MS Gothic"/>"#,
                    r#"</w14:checkbox></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#
                ),
                run_with(
                    r#"<w:rFonts w:ascii="MS Gothic" w:eastAsia="MS Gothic" w:hAnsi="MS Gothic" w:hint="eastAsia"/>"#,
                    "\u{2612}"
                )
            ),
            run(" Open to relocation"),
        ]
        .concat(),
    );
    let body = [styled("Title", "Avery Quinn"), block, inline, checkbox].concat();
    let mut package = Package::new("Avery Quinn resume: content controls", body);
    package.doc_ns = vec![("mc", NS_MC), ("w14", NS_W14)];
    package.ignorable = Some("w14");
    package
}

/// A text box, written the way Word does: a DrawingML shape with a VML fallback.
fn text_box() -> Package {
    let content = plain("Open to relocation");
    let shape = format!(
        concat!(
            r#"<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing>"#,
            r#"<wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="251659264" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">"#,
            r#"<wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>"#,
            r#"<wp:extent cx="2743200" cy="457200"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/>"#,
            r#"<wp:docPr id="1" name="Text Box 1"/><wp:cNvGraphicFramePr/>"#,
            r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">"#,
            r#"<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2743200" cy="457200"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln w="6350"><a:solidFill><a:srgbClr val="0E7C7B"/></a:solidFill></a:ln></wps:spPr>"#,
            r#"<wps:txbx><w:txbxContent>{content}</w:txbxContent></wps:txbx>"#,
            r#"<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="t" anchorCtr="0"><a:noAutofit/></wps:bodyPr>"#,
            r#"</wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice>"#,
            r##"<mc:Fallback><w:pict><v:shape id="Text Box 1" o:spid="_x0000_s1026" style="position:absolute;margin-left:0;margin-top:0;width:216pt;height:36pt;z-index:251659264" strokecolor="#0e7c7b" strokeweight=".5pt">"##,
            r#"<v:textbox><w:txbxContent>{content}</w:txbxContent></v:textbox></v:shape></w:pict></mc:Fallback>"#,
            r#"</mc:AlternateContent></w:r>"#
        ),
        content = content
    );
    let body = [
        resume_top(),
        para("", &shape),
        plain("The text box above floats beside this paragraph."),
    ]
    .concat();
    let mut package = Package::new("Avery Quinn resume: text box", body);
    package.doc_ns = vec![("mc", NS_MC), ("wps", NS_WPS), ("v", NS_V), ("o", NS_O)];
    package
}

fn math() -> Package {
    let r = |text: &str| format!("<m:r><m:t>{}</m:t></m:r>", esc(text));
    let equation = format!(
        "<m:oMathPara><m:oMath>{}{}{}<m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup></m:oMath></m:oMathPara>",
        r("E"),
        r("="),
        r("m"),
        r("c"),
        r("2")
    );
    let inline = format!("<m:oMath>{}{}{}</m:oMath>", r("a"), r("+"), r("b"));
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        plain("Our pricing model in one line:"),
        para("", &equation),
        para(
            "",
            &[run("And inline: "), inline, run(" works too.")].concat(),
        ),
    ]
    .concat();
    let mut package = Package::new("Marrow Foods pitch brief: maths", body);
    package.doc_ns = vec![("m", NS_M)];
    package
}

fn inline_specials() -> Package {
    let body = [
        styled("Title", "Avery Quinn"),
        para(
            "",
            &[
                run("Skills"),
                "<w:r><w:tab/></w:r>".to_owned(),
                run("SQL, Python"),
            ]
            .concat(),
        ),
        para(
            "",
            &[
                run("Line one"),
                "<w:r><w:br/></w:r>".to_owned(),
                run("Line two after a line break"),
                "<w:r><w:cr/></w:r>".to_owned(),
                run("Line three after a carriage return"),
            ]
            .concat(),
        ),
        para(
            "",
            &[
                r#"<w:r><w:sym w:font="Symbol" w:char="F0B7"/></w:r>"#.to_owned(),
                run(" A symbol bullet, a soft"),
                "<w:r><w:softHyphen/></w:r>".to_owned(),
                run("hyphen and a non"),
                "<w:r><w:noBreakHyphen/></w:r>".to_owned(),
                run("breaking hyphen."),
            ]
            .concat(),
        ),
        para(
            "",
            &[
                run("Before a page break."),
                r#"<w:r><w:br w:type="page"/></w:r>"#.to_owned(),
                run("After a page break."),
            ]
            .concat(),
        ),
        para(
            "",
            r#"<w:r><w:t xml:space="preserve">  two leading spaces and three   inside  </w:t></w:r>"#,
        ),
    ]
    .concat();
    Package::new("Avery Quinn resume: tabs, breaks and symbols", body)
}

fn unicode_text() -> Package {
    let body = [
        styled("Title", "Zoë Ångström-Nakamura"),
        plain("\u{201C}Curly quotes\u{201D}, \u{2018}single\u{2019}, an em dash \u{2014} and an en dash \u{2013}."),
        plain("Japanese: \u{30C7}\u{30FC}\u{30BF}\u{30A8}\u{30F3}\u{30B8}\u{30CB}\u{30A2}"),
        plain("Emoji outside the basic plane: \u{1F680} \u{1F4C8}"),
        plain("A combining accent: Rene\u{0301}e"),
        para(
            r#"<w:bidi/>"#,
            &run_with(
                "<w:rtl/>",
                "\u{645}\u{647}\u{646}\u{62F}\u{633} \u{628}\u{64A}\u{627}\u{646}\u{627}\u{62A}",
            ),
        ),
        plain("Markup characters: <tags> & \"quotes\" stay text."),
    ]
    .concat();
    Package::new("Avery Quinn resume: Unicode text", body)
}

const CHARACTER_STYLES: &str = r#"<w:style w:type="character" w:styleId="Strong"><w:name w:val="Strong"/><w:basedOn w:val="DefaultParagraphFont"/><w:uiPriority w:val="22"/><w:qFormat/><w:rPr><w:b/><w:bCs/></w:rPr></w:style><w:style w:type="character" w:styleId="Emphasis"><w:name w:val="Emphasis"/><w:basedOn w:val="DefaultParagraphFont"/><w:uiPriority w:val="20"/><w:qFormat/><w:rPr><w:i/><w:iCs/></w:rPr></w:style>"#;

fn character_styles() -> Package {
    let body = [
        styled("Title", "Avery Quinn"),
        para(
            "",
            &[
                run("Styles: "),
                run_with(r#"<w:rStyle w:val="Strong"/>"#, "strong"),
                run(", "),
                run_with(r#"<w:rStyle w:val="Emphasis"/>"#, "emphasis"),
                run("."),
            ]
            .concat(),
        ),
        para(
            "",
            &[
                run_with(r#"<w:u w:val="single"/>"#, "underlined"),
                run(" "),
                run_with("<w:strike/>", "struck"),
                run(" "),
                run_with(r#"<w:color w:val="0E7C7B"/>"#, "teal"),
                run(" "),
                run_with(r#"<w:highlight w:val="yellow"/>"#, "highlighted"),
                run(" "),
                run_with(r#"<w:sz w:val="28"/><w:szCs w:val="28"/>"#, "larger"),
                run(" "),
                run_with(
                    r#"<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/>"#,
                    "monospace",
                ),
                run(" "),
                run_with("<w:caps/>", "caps"),
                run(" "),
                run_with("<w:smallCaps/>", "small caps"),
                run(" x"),
                run_with(r#"<w:vertAlign w:val="superscript"/>"#, "2"),
                run(" "),
                run_with(r#"<w:lang w:val="fr-CA"/>"#, "bonjour"),
            ]
            .concat(),
        ),
        para(
            r#"<w:jc w:val="center"/>"#,
            &run_with("<w:b/><w:i/>", "Bold italic, centred."),
        ),
        para(
            r#"<w:spacing w:before="240" w:after="240"/><w:ind w:left="720" w:right="720"/>"#,
            &run("Indented with extra spacing."),
        ),
    ]
    .concat();
    let mut package = Package::new("Avery Quinn resume: character styles", body);
    package.extra_styles = CHARACTER_STYLES.to_owned();
    package
}

fn empty_and_whitespace() -> Package {
    let body = [
        styled("Title", "Avery Quinn"),
        "<w:p/>".to_owned(),
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr></w:p>"#.to_owned(),
        "<w:p><w:r><w:t></w:t></w:r></w:p>".to_owned(),
        "<w:p><w:r><w:t> no preserve attribute </w:t></w:r></w:p>".to_owned(),
        "<w:p><w:r><w:tab/></w:r></w:p>".to_owned(),
        plain("A normal paragraph between empty ones."),
        "<w:p/>".to_owned(),
        "<w:p/>".to_owned(),
        plain("Two empty paragraphs above."),
    ]
    .concat();
    Package::new("Avery Quinn resume: empty paragraphs and whitespace", body)
}

/// First-page, even and default headers and footers, and a second section with
/// its own default header.
fn multi_headers() -> Package {
    let hdr = |text: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>{}<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{}</w:hdr>"#,
            "\r\n",
            para(r#"<w:jc w:val="right"/>"#, &run(text))
        )
    };
    let ftr = |text: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>{}<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{}</w:ftr>"#,
            "\r\n",
            para(r#"<w:jc w:val="center"/>"#, &run(text))
        )
    };
    let hdr_part = |name: &str, data: String| Part {
        name: format!("word/{name}"),
        content_type: Some(format!("{CT_WML}.header+xml")),
        data: data.into_bytes(),
    };
    let ftr_part = |name: &str, data: String| Part {
        name: format!("word/{name}"),
        content_type: Some(format!("{CT_WML}.footer+xml")),
        data: data.into_bytes(),
    };
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let first_section = format!(
        concat!(
            r#"<w:sectPr><w:headerReference w:type="even" r:id="rIdHdrEven"/><w:headerReference w:type="default" r:id="rIdHdrDefault"/>"#,
            r#"<w:headerReference w:type="first" r:id="rIdHdrFirst"/><w:footerReference w:type="default" r:id="rIdFtrDefault"/>"#,
            r#"<w:footerReference w:type="first" r:id="rIdFtrFirst"/><w:type w:val="nextPage"/>{page}<w:titlePg/></w:sectPr>"#
        ),
        page = page
    );
    let body = [
        styled("Title", "Marrow Foods pitch brief"),
        plain("The first page has its own header and footer."),
        para(&first_section, &run("End of the first section.")),
        styled("Heading1", "Appendix"),
        plain("The appendix section has a different default header."),
    ]
    .concat();
    let mut package = Package::new("Marrow Foods pitch brief: several headers", body);
    package.even_and_odd_headers = true;
    package.body_sect = Some(format!(
        r#"<w:sectPr><w:headerReference w:type="default" r:id="rIdHdrAppendix"/>{page}</w:sectPr>"#
    ));
    package.parts = vec![
        hdr_part("header1.xml", hdr("Marrow Foods · Confidential")),
        hdr_part("header2.xml", hdr("Marrow Foods · Even page")),
        hdr_part("header3.xml", hdr("Marrow Foods pitch brief · First page")),
        hdr_part("header4.xml", hdr("Marrow Foods · Appendix")),
        ftr_part("footer1.xml", ftr("Invented figures for testing")),
        ftr_part("footer2.xml", ftr("Prepared for an invented investor")),
    ];
    package.rels = vec![
        Rel::office("rIdHdrDefault", "header", "header1.xml"),
        Rel::office("rIdHdrEven", "header", "header2.xml"),
        Rel::office("rIdHdrFirst", "header", "header3.xml"),
        Rel::office("rIdHdrAppendix", "header", "header4.xml"),
        Rel::office("rIdFtrDefault", "footer", "footer1.xml"),
        Rel::office("rIdFtrFirst", "footer", "footer2.xml"),
    ];
    package
}

/// A person's own custom properties, which working copies keep and sent copies drop.
fn custom_properties() -> Package {
    let prop = |pid: u32, name: &str, value: &str| {
        format!(
            r#"<property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="{pid}" name="{}">{value}</property>"#,
            esc(name)
        )
    };
    let custom = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
            r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">"#,
            "{}{}{}{}</Properties>"
        ),
        prop(2, "Client", "<vt:lpwstr>Northgate Analytics</vt:lpwstr>"),
        prop(3, "Revision count", "<vt:i4>7</vt:i4>"),
        prop(4, "Reviewed", "<vt:bool>true</vt:bool>"),
        prop(5, "Due", "<vt:filetime>2026-11-01T00:00:00Z</vt:filetime>"),
    );
    let body = [
        resume_top(),
        plain("This file carries four custom document properties."),
    ]
    .concat();
    let mut package = Package::new("Avery Quinn resume: custom properties", body);
    package.parts = vec![Part {
        name: "docProps/custom.xml".to_owned(),
        content_type: Some(
            "application/vnd.openxmlformats-officedocument.custom-properties+xml".to_owned(),
        ),
        data: custom.into_bytes(),
    }];
    package.root_rels = vec![Rel {
        id: "rId4",
        rel_type:
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties"
                .to_owned(),
        target: "docProps/custom.xml".to_owned(),
        external: false,
    }];
    package
}

fn two_page_body() -> String {
    [
        resume_top(),
        styled("Heading1", "Summary"),
        plain("Data engineer with six years of experience building reliable pipelines."),
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
    ]
    .concat()
}

fn deflated() -> Package {
    let mut package = Package::new("Avery Quinn resume: deflated like Word", two_page_body());
    package.zip = Zip::Deflated;
    package
}

fn data_descriptor() -> Package {
    let mut package = Package::new("Avery Quinn resume: streamed zip", two_page_body());
    package.zip = Zip::Streamed;
    package
}

/// Revisions beyond the Phase 0 fixture: a paragraph moved with move ranges, an
/// inserted and a deleted table row, a paragraph style change and a deleted
/// paragraph mark that joins two paragraphs.
fn tracked_moves_and_tables() -> Package {
    let who = who();
    let moved = "Skills: SQL, Python, dbt.";
    let ins_cell = |text: &str, id: u32| {
        tc(
            4500,
            "",
            &para(
                &format!(r#"<w:rPr><w:ins w:id="{id}" {who}/></w:rPr>"#),
                &format!(r#"<w:ins w:id="{}" {who}>{}</w:ins>"#, id + 1, run(text)),
            ),
        )
    };
    let del_cell = |text: &str, id: u32| {
        tc(
            4500,
            "",
            &para(
                &format!(r#"<w:rPr><w:del w:id="{id}" {who}/></w:rPr>"#),
                &format!(
                    r#"<w:del w:id="{}" {who}><w:r><w:delText>{}</w:delText></w:r></w:del>"#,
                    id + 1,
                    esc(text)
                ),
            ),
        )
    };
    let rows = [
        format!(
            "<w:tr>{}{}</w:tr>",
            tc(4500, "", &plain("Quarter")),
            tc(4500, "", &plain("Customers"))
        ),
        format!(
            "<w:tr>{}{}</w:tr>",
            tc(4500, "", &plain("Q3 2026")),
            tc(4500, "", &plain("17"))
        ),
        format!(
            r#"<w:tr><w:trPr><w:ins w:id="310" {who}/></w:trPr>{}{}</w:tr>"#,
            ins_cell("Q4 2026", 311),
            ins_cell("23", 313)
        ),
        format!(
            r#"<w:tr><w:trPr><w:del w:id="320" {who}/></w:trPr>{}{}</w:tr>"#,
            del_cell("Q2 2026", 321),
            del_cell("12", 323)
        ),
    ]
    .concat();
    let body = [
        resume_top(),
        para(
            &format!(r#"<w:rPr><w:moveFrom w:id="203" {who}/></w:rPr>"#),
            &format!(
                concat!(
                    r#"<w:moveFromRangeStart w:id="201" {who} w:name="move1"/>"#,
                    r#"<w:moveFrom w:id="202" {who}>{}</w:moveFrom><w:moveFromRangeEnd w:id="201"/>"#
                ),
                run(moved),
                who = who
            ),
        ),
        styled("Heading1", "Experience"),
        plain("Built the shipment data warehouse used by forty analysts."),
        para(
            &format!(r#"<w:rPr><w:moveTo w:id="206" {who}/></w:rPr>"#),
            &format!(
                concat!(
                    r#"<w:moveToRangeStart w:id="204" {who} w:name="move1"/>"#,
                    r#"<w:moveTo w:id="205" {who}>{}</w:moveTo><w:moveToRangeEnd w:id="204"/>"#
                ),
                run(moved),
                who = who
            ),
        ),
        para(
            &format!(
                r#"<w:pStyle w:val="Heading2"/><w:pPrChange w:id="207" {who}><w:pPr><w:pStyle w:val="Heading1"/></w:pPr></w:pPrChange>"#
            ),
            &run("Education"),
        ),
        para(
            &format!(r#"<w:rPr><w:del w:id="208" {who}/></w:rPr>"#),
            &run("BSc Computer Science, "),
        ),
        plain("Northbridge University, 2019."),
        table_xml(&[4500, 4500], &rows),
        plain("After the table."),
    ]
    .concat();
    Package::new(
        "Avery Quinn resume: moves, table rows and more revisions",
        body,
    )
}
