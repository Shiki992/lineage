// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Builds minimal but complete .docx packages from hand-written WordprocessingML.
//! Element order inside `pPr`, `rPr`, `sectPr` and `settings` follows the schema
//! order in ECMA-376 Part 1, which Word enforces.

use crate::zip_writer;

const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";
const NS_W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_WML: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

/// Fixed timestamp for every date inside fixtures, so output never changes.
pub const FIXED_DATE: &str = "2026-01-01T00:00:00Z";

/// Escapes text for XML content and attribute values.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// A run of plain text.
pub fn run(text: &str) -> String {
    format!(
        r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
        esc(text)
    )
}

/// A run with run properties (`rpr` must already be in schema order).
pub fn run_with(rpr: &str, text: &str) -> String {
    format!(
        r#"<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
        esc(text)
    )
}

/// A paragraph with optional paragraph properties and raw content.
pub fn para(ppr: &str, content: &str) -> String {
    if ppr.is_empty() {
        format!("<w:p>{content}</w:p>")
    } else {
        format!("<w:p><w:pPr>{ppr}</w:pPr>{content}</w:p>")
    }
}

/// A paragraph in a named style holding one run of text.
pub fn styled(style: &str, text: &str) -> String {
    para(&format!(r#"<w:pStyle w:val="{style}"/>"#), &run(text))
}

/// A Normal paragraph holding one run of text.
pub fn plain(text: &str) -> String {
    para("", &run(text))
}

/// A list paragraph using numbering definition `num_id` at `level`.
pub fn list_item(num_id: u32, level: u32, text: &str) -> String {
    para(
        &format!(
            r#"<w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{num_id}"/></w:numPr>"#
        ),
        &run(text),
    )
}

/// Page orientation for a section.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Section properties. `next_page` starts the following section on a new page.
pub fn sect_pr(header_footer: bool, orientation: Orientation, next_page: bool) -> String {
    let mut out = String::from("<w:sectPr>");
    if header_footer {
        out.push_str(r#"<w:headerReference w:type="default" r:id="rIdHeader1"/>"#);
        out.push_str(r#"<w:footerReference w:type="default" r:id="rIdFooter1"/>"#);
    }
    if next_page {
        out.push_str(r#"<w:type w:val="nextPage"/>"#);
    }
    match orientation {
        Orientation::Portrait => out.push_str(r#"<w:pgSz w:w="12240" w:h="15840"/>"#),
        Orientation::Landscape => {
            out.push_str(r#"<w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/>"#);
        }
    }
    out.push_str(
        r#"<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#,
    );
    out.push_str("</w:sectPr>");
    out
}

/// One synthetic Word document. Optional parts are added only when set.
pub struct Package {
    pub title: &'static str,
    pub body: String,
    pub numbering: bool,
    pub header: Option<String>,
    pub footer: Option<String>,
    pub footnotes: Option<String>,
    pub comments: Option<String>,
    pub image_png: Option<Vec<u8>>,
}

impl Package {
    pub fn new(title: &'static str, body: String) -> Self {
        Self {
            title,
            body,
            numbering: false,
            header: None,
            footer: None,
            footnotes: None,
            comments: None,
            image_png: None,
        }
    }

    fn has_header_footer(&self) -> bool {
        self.header.is_some() || self.footer.is_some()
    }

    /// The package parts, in a fixed order.
    pub fn parts(&self) -> Vec<(String, Vec<u8>)> {
        let mut overrides = vec![
            (
                "/docProps/core.xml",
                "application/vnd.openxmlformats-package.core-properties+xml".to_owned(),
            ),
            (
                "/docProps/app.xml",
                "application/vnd.openxmlformats-officedocument.extended-properties+xml".to_owned(),
            ),
            ("/word/document.xml", format!("{CT_WML}.document.main+xml")),
            ("/word/styles.xml", format!("{CT_WML}.styles+xml")),
            ("/word/settings.xml", format!("{CT_WML}.settings+xml")),
        ];
        let mut rels = vec![
            ("rIdStyles", "styles", "styles.xml"),
            ("rIdSettings", "settings", "settings.xml"),
        ];
        let mut extra: Vec<(String, Vec<u8>)> = Vec::new();

        if self.numbering {
            overrides.push(("/word/numbering.xml", format!("{CT_WML}.numbering+xml")));
            rels.push(("rIdNumbering", "numbering", "numbering.xml"));
            extra.push((
                "word/numbering.xml".to_owned(),
                numbering_xml().into_bytes(),
            ));
        }
        if self.has_header_footer() {
            let header = self.header.as_deref().unwrap_or("");
            let footer = self.footer.as_deref().unwrap_or("");
            overrides.push(("/word/header1.xml", format!("{CT_WML}.header+xml")));
            overrides.push(("/word/footer1.xml", format!("{CT_WML}.footer+xml")));
            rels.push(("rIdHeader1", "header", "header1.xml"));
            rels.push(("rIdFooter1", "footer", "footer1.xml"));
            extra.push((
                "word/header1.xml".to_owned(),
                story("w:hdr", header).into_bytes(),
            ));
            extra.push((
                "word/footer1.xml".to_owned(),
                story("w:ftr", footer).into_bytes(),
            ));
        }
        if let Some(notes) = &self.footnotes {
            overrides.push(("/word/footnotes.xml", format!("{CT_WML}.footnotes+xml")));
            rels.push(("rIdFootnotes", "footnotes", "footnotes.xml"));
            extra.push((
                "word/footnotes.xml".to_owned(),
                footnotes_xml(notes).into_bytes(),
            ));
        }
        if let Some(comments) = &self.comments {
            overrides.push(("/word/comments.xml", format!("{CT_WML}.comments+xml")));
            rels.push(("rIdComments", "comments", "comments.xml"));
            extra.push((
                "word/comments.xml".to_owned(),
                format!(r#"{DECL}<w:comments xmlns:w="{NS_W}">{comments}</w:comments>"#)
                    .into_bytes(),
            ));
        }
        if let Some(png) = &self.image_png {
            rels.push(("rIdImage1", "image", "media/image1.png"));
            extra.push(("word/media/image1.png".to_owned(), png.clone()));
        }

        let mut parts = vec![
            (
                "[Content_Types].xml".to_owned(),
                content_types(&overrides, self.image_png.is_some()).into_bytes(),
            ),
            ("_rels/.rels".to_owned(), root_rels().into_bytes()),
            (
                "docProps/core.xml".to_owned(),
                core_xml(self.title).into_bytes(),
            ),
            ("docProps/app.xml".to_owned(), app_xml().into_bytes()),
            (
                "word/document.xml".to_owned(),
                self.document_xml().into_bytes(),
            ),
            (
                "word/_rels/document.xml.rels".to_owned(),
                document_rels(&rels).into_bytes(),
            ),
            ("word/styles.xml".to_owned(), styles_xml().into_bytes()),
            (
                "word/settings.xml".to_owned(),
                settings_xml(self.footnotes.is_some()).into_bytes(),
            ),
        ];
        parts.extend(extra);
        parts
    }

    /// The finished .docx bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        zip_writer::write_stored(&self.parts())
    }

    fn document_xml(&self) -> String {
        format!(
            concat!(
                "{decl}<w:document xmlns:w=\"{w}\" xmlns:r=\"{r}\"",
                " xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"",
                " xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"",
                " xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
                "<w:body>{body}{sect}</w:body></w:document>"
            ),
            decl = DECL,
            w = NS_W,
            r = NS_R,
            body = self.body,
            sect = sect_pr(self.has_header_footer(), Orientation::Portrait, false),
        )
    }
}

fn story(root: &str, content: &str) -> String {
    format!(r#"{DECL}<{root} xmlns:w="{NS_W}" xmlns:r="{NS_R}">{content}</{root}>"#)
}

fn content_types(overrides: &[(&str, String)], png: bool) -> String {
    let mut out = format!(
        r#"{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#
    );
    if png {
        out.push_str(r#"<Default Extension="png" ContentType="image/png"/>"#);
    }
    for (part, content_type) in overrides {
        out.push_str(&format!(
            r#"<Override PartName="{part}" ContentType="{content_type}"/>"#
        ));
    }
    out.push_str("</Types>");
    out
}

fn root_rels() -> String {
    format!(
        concat!(
            "{decl}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            "<Relationship Id=\"rId1\" Type=\"{rel}/officeDocument\" Target=\"word/document.xml\"/>",
            "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>",
            "<Relationship Id=\"rId3\" Type=\"{rel}/extended-properties\" Target=\"docProps/app.xml\"/>",
            "</Relationships>"
        ),
        decl = DECL,
        rel = REL,
    )
}

fn document_rels(rels: &[(&str, &str, &str)]) -> String {
    let mut out = format!(
        r#"{DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#
    );
    for (id, kind, target) in rels {
        out.push_str(&format!(
            r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{target}"/>"#
        ));
    }
    out.push_str("</Relationships>");
    out
}

fn core_xml(title: &str) -> String {
    format!(
        concat!(
            "{decl}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"",
            " xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\"",
            " xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">",
            "<dc:title>{title}</dc:title><dc:creator>Lineage fixture generator</dc:creator>",
            "<dcterms:created xsi:type=\"dcterms:W3CDTF\">{date}</dcterms:created>",
            "<dcterms:modified xsi:type=\"dcterms:W3CDTF\">{date}</dcterms:modified>",
            "</cp:coreProperties>"
        ),
        decl = DECL,
        title = esc(title),
        date = FIXED_DATE,
    )
}

fn app_xml() -> String {
    format!(
        r#"{DECL}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Lineage fixture generator</Application></Properties>"#
    )
}

fn styles_xml() -> String {
    let heading = |id: &str, name: &str, level: u32, size: u32| {
        format!(
            r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{name}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="{level}"/></w:pPr><w:rPr><w:b/><w:sz w:val="{size}"/></w:rPr></w:style>"#
        )
    };
    format!(
        concat!(
            "{decl}<w:styles xmlns:w=\"{w}\">",
            "<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault>",
            "<w:pPrDefault><w:pPr><w:spacing w:after=\"120\"/></w:pPr></w:pPrDefault></w:docDefaults>",
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>",
            "<w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"><w:name w:val=\"Default Paragraph Font\"/><w:uiPriority w:val=\"1\"/><w:semiHidden/></w:style>",
            "<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:rPr><w:sz w:val=\"48\"/></w:rPr></w:style>",
            "{h1}{h2}",
            "<w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:ind w:left=\"720\"/></w:pPr></w:style>",
            "<w:style w:type=\"paragraph\" w:styleId=\"FootnoteText\"><w:name w:val=\"footnote text\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"0\"/></w:pPr><w:rPr><w:sz w:val=\"20\"/></w:rPr></w:style>",
            "<w:style w:type=\"character\" w:styleId=\"FootnoteReference\"><w:name w:val=\"footnote reference\"/><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr></w:style>",
            "<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/><w:uiPriority w:val=\"99\"/><w:semiHidden/><w:tblPr><w:tblInd w:w=\"0\" w:type=\"dxa\"/><w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"108\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr></w:style>",
            "<w:style w:type=\"table\" w:styleId=\"TableGrid\"><w:name w:val=\"Table Grid\"/><w:basedOn w:val=\"TableNormal\"/><w:tblPr><w:tblBorders>",
            "<w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
            "<w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
            "<w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
            "</w:tblBorders></w:tblPr></w:style>",
            "</w:styles>"
        ),
        decl = DECL,
        w = NS_W,
        h1 = heading("Heading1", "heading 1", 0, 32),
        h2 = heading("Heading2", "heading 2", 1, 26),
    )
}

fn settings_xml(footnotes: bool) -> String {
    let footnote_pr = if footnotes {
        r#"<w:footnotePr><w:footnote w:id="-1"/><w:footnote w:id="0"/></w:footnotePr>"#
    } else {
        ""
    };
    format!(
        concat!(
            "{decl}<w:settings xmlns:w=\"{w}\"><w:defaultTabStop w:val=\"720\"/>",
            "<w:characterSpacingControl w:val=\"doNotCompress\"/>{footnote_pr}",
            "<w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/></w:compat>",
            "</w:settings>"
        ),
        decl = DECL,
        w = NS_W,
        footnote_pr = footnote_pr,
    )
}

fn numbering_xml() -> String {
    let level = |ilvl: u32, fmt: &str, text: &str, left: u32| {
        format!(
            r#"<w:lvl w:ilvl="{ilvl}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{left}" w:hanging="360"/></w:pPr></w:lvl>"#
        )
    };
    format!(
        concat!(
            "{decl}<w:numbering xmlns:w=\"{w}\">",
            "<w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>{b0}{b1}</w:abstractNum>",
            "<w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"hybridMultilevel\"/>{n0}{n1}</w:abstractNum>",
            "<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>",
            "<w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>",
            "</w:numbering>"
        ),
        decl = DECL,
        w = NS_W,
        b0 = level(0, "bullet", "\u{2022}", 720),
        b1 = level(1, "bullet", "\u{2013}", 1440),
        n0 = level(0, "decimal", "%1.", 720),
        n1 = level(1, "lowerLetter", "%2)", 1440),
    )
}

fn footnotes_xml(notes: &str) -> String {
    format!(
        concat!(
            "{decl}<w:footnotes xmlns:w=\"{w}\" xmlns:r=\"{r}\">",
            "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r><w:separator/></w:r></w:p></w:footnote>",
            "<w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>",
            "{notes}</w:footnotes>"
        ),
        decl = DECL,
        w = NS_W,
        r = NS_R,
        notes = notes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup() {
        assert_eq!(esc(r#"R&D <"lead">"#), "R&amp;D &lt;&quot;lead&quot;&gt;");
    }

    #[test]
    fn optional_parts_only_when_set() {
        let names = |p: &Package| p.parts().into_iter().map(|(n, _)| n).collect::<Vec<_>>();
        let bare = Package::new("Bare", plain("Hello"));
        assert!(
            !names(&bare)
                .iter()
                .any(|n| n.contains("numbering") || n.contains("header"))
        );

        let mut full = Package::new("Full", plain("Hello"));
        full.numbering = true;
        full.header = Some(plain("H"));
        full.footnotes = Some(String::new());
        let full_names = names(&full);
        for part in [
            "word/numbering.xml",
            "word/header1.xml",
            "word/footer1.xml",
            "word/footnotes.xml",
        ] {
            assert!(full_names.iter().any(|n| n == part), "missing {part}");
        }
    }
}
