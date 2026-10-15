// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::fixtures;

use super::*;

fn bytes(name: &str) -> Vec<u8> {
    fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
        .1
        .to_bytes()
}

fn open(name: &str) -> Docx {
    Docx::open(&bytes(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn rejected(name: &str) -> Docx {
    Docx::open_view(&bytes(name), View::Rejected).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn paragraphs(blocks: &[Block]) -> Vec<&Paragraph> {
    blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect()
}

fn atoms(p: &Paragraph) -> Vec<&Atom> {
    p.inlines
        .iter()
        .filter_map(|i| match i {
            Inline::Atom(a) => Some(a),
            _ => None,
        })
        .collect()
}

#[test]
fn every_fixture_opens_in_both_views() {
    for (name, fixture) in fixtures::all() {
        let data = fixture.to_bytes();
        for view in [View::Accepted, View::Rejected] {
            let docx =
                Docx::open_view(&data, view).unwrap_or_else(|e| panic!("{name} {view:?}: {e}"));
            assert!(!docx.body().blocks.is_empty(), "{name}");
            assert!(
                matches!(docx.body().blocks.last(), Some(Block::SectionBreak(s)) if s.is_final),
                "{name}: the body ends with its section properties"
            );
        }
    }
}

#[test]
fn headings_and_runs_reference_styles() {
    let docx = open("headings-paragraphs.docx");
    let paras = paragraphs(&docx.body().blocks);
    assert_eq!(paras.len(), 11);
    assert_eq!(paras[0].props.style.as_deref(), Some("Title"));
    assert_eq!(paras[0].text(), "Avery Quinn");
    assert_eq!(paras[2].props.style.as_deref(), Some("Heading1"));
    assert_eq!(
        paras[3].text(),
        "Data engineer with six years of experience building reliable pipelines for retail and logistics teams."
    );
    let bold_run = paras[3]
        .inlines
        .iter()
        .find_map(|i| match i {
            Inline::Text { text, run } if text == "six years" => Some(*run),
            _ => None,
        })
        .expect("bold run");
    assert_eq!(
        paras[3].runs[bold_run].props.direct.get("b"),
        Some("val=1;")
    );
    assert_eq!(docx.styles.display("Heading1"), "Heading 1");
    assert_eq!(docx.styles.display("FootnoteText"), "Footnote Text");
}

#[test]
fn lists_reference_numbering_not_flattened() {
    let docx = open("lists.docx");
    let paras = paragraphs(&docx.body().blocks);
    let items: Vec<&NumRef> = paras
        .iter()
        .filter_map(|p| p.props.numbering.as_ref())
        .collect();
    assert_eq!(items.len(), 9);
    assert_eq!(
        items[0],
        &NumRef {
            num_id: "1".into(),
            level: "0".into()
        }
    );
    assert_eq!(
        items[2],
        &NumRef {
            num_id: "1".into(),
            level: "1".into()
        }
    );
    assert!(
        paras.iter().all(|p| !p.text().starts_with('•')),
        "no bullet characters in text"
    );
}

#[test]
fn tables_have_rows_cells_and_spans() {
    let docx = open("table.docx");
    let table = docx
        .body()
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    assert_eq!(table.columns, 3);
    assert_eq!(table.rows.len(), 5);
    assert_eq!(table.rows[0].cells[0].grid_span, 3);
    assert_eq!(table.rows[2].text(), "Q1 2026\t$12k\t8");

    let merged = open("table-merged-cells.docx");
    let table = merged
        .body()
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    assert_eq!(table.rows[1].cells[0].vmerge, Some(true));
    assert_eq!(table.rows[2].cells[0].vmerge, Some(false));
    assert_eq!(table.rows[3].cells[0].grid_span, 2);

    let nested = open("nested-table.docx");
    let outer = nested
        .body()
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    assert!(matches!(outer.rows[0].cells[1].blocks[0], Block::Table(_)));
}

#[test]
fn headers_footers_and_notes_are_stories() {
    let docx = open("header-footer.docx");
    let keys: Vec<&StoryKey> = docx.stories.iter().map(|s| &s.key).collect();
    assert!(keys.contains(&&StoryKey::Header {
        section: 0,
        kind: "default".into()
    }));
    let footer = docx
        .story(&StoryKey::Footer {
            section: 0,
            kind: "default".into(),
        })
        .expect("footer");
    let page = paragraphs(&footer.blocks)[0];
    assert_eq!(page.text(), "Page 1");
    assert_eq!(atoms(page)[0].kind, AtomKind::Field);

    let multi = open("multi-headers.docx");
    let headers = multi
        .stories
        .iter()
        .filter(|s| matches!(s.key, StoryKey::Header { .. } | StoryKey::Footer { .. }))
        .count();
    assert_eq!(headers, 6);
    assert!(
        multi
            .story(&StoryKey::Header {
                section: 1,
                kind: "default".into()
            })
            .is_some()
    );

    let notes = open("footnote.docx");
    let story = notes.story(&StoryKey::Footnotes).expect("footnotes");
    let note = paragraphs(&story.blocks);
    assert_eq!(note.len(), 1);
    assert_eq!(note[0].lock, Some(Lock::Note));
    assert_eq!(note[0].note_id.as_deref(), Some("1"));
    let body = paragraphs(&notes.body().blocks);
    assert!(
        body.iter()
            .any(|p| atoms(p).iter().any(|a| a.kind == AtomKind::NoteReference))
    );

    let end = open("endnotes.docx");
    assert_eq!(
        paragraphs(&end.story(&StoryKey::Endnotes).expect("endnotes").blocks).len(),
        2
    );
}

#[test]
fn note_reference_keys_follow_the_note_text() {
    let a = open("footnote.docx");
    let mut changed = lineage_testkit::fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == "footnote.docx")
        .expect("fixture")
        .1;
    changed.footnotes = changed
        .footnotes
        .map(|notes| notes.replace("invented body", "imaginary body"));
    let b = Docx::open(&changed.to_bytes()).expect("opens");
    let key = |d: &Docx| {
        paragraphs(&d.body().blocks)
            .iter()
            .flat_map(|p| atoms(p))
            .find(|a| a.kind == AtomKind::NoteReference)
            .expect("reference")
            .key
    };
    assert_ne!(key(&a), key(&b));
}

#[test]
fn fields_are_atoms_keyed_by_instruction() {
    let docx = open("fields.docx");
    let paras = paragraphs(&docx.body().blocks);
    assert_eq!(paras[2].text(), "Last updated: 1 January 2026");
    assert_eq!(paras[3].text(), "This is page 1.");
    assert_eq!(atoms(paras[2])[0].kind, AtomKind::Field);
    assert_eq!(atoms(paras[3])[0].kind, AtomKind::Field);

    let toc = open("toc-field.docx");
    let paras = paragraphs(&toc.body().blocks);
    assert_eq!(paras[1].lock, Some(Lock::SpanningField));
    assert_eq!(paras[2].lock, Some(Lock::SpanningField));
    assert_eq!(paras[3].lock, Some(Lock::SpanningField));
    assert_eq!(
        paras[4].lock, None,
        "the heading after the table of contents"
    );
    assert_eq!(paras[4].text(), "Problem");
}

#[test]
fn hyperlinks_are_containers_with_ordinary_text() {
    let docx = open("hyperlinks.docx");
    let paras = paragraphs(&docx.body().blocks);
    assert_eq!(
        paras[1].text(),
        "Email: avery.quinn@example.com · Portfolio: example.com/avery"
    );
    assert_eq!(paras[1].containers.len(), 2);
    assert_eq!(paras[1].containers[0].rel_ids, ["rIdMail"]);
    assert_eq!(paras[1].containers[0].qname, "w:hyperlink");
    assert!(atoms(paras[1]).is_empty());
}

#[test]
fn comments_are_markers() {
    let docx = open("comment.docx");
    let para = paragraphs(&docx.body().blocks)[3];
    assert_eq!(para.text(), "Led the analytics platform team.");
    let comment_markers = para
        .inlines
        .iter()
        .filter(|i| matches!(i, Inline::Marker(m) if m.comment))
        .count();
    assert_eq!(
        comment_markers, 3,
        "range start, range end and the reference run"
    );
}

#[test]
fn accepted_and_rejected_views_of_revisions() {
    let accepted = open("tracked-changes.docx").body_lines();
    let before = rejected("tracked-changes.docx").body_lines();
    assert_eq!(
        accepted[3..],
        [
            "Led a team of six analysts.",
            "Presented quarterly results to the board.",
            "Owned the data quality checks."
        ]
    );
    assert_eq!(
        before[3..],
        [
            "Led a team of four analysts.",
            "Owned the data quality checks."
        ]
    );

    let docx = open("tracked-changes.docx");
    let last = *paragraphs(&docx.body().blocks).last().expect("paragraph");
    let bold = |p: &Paragraph| p.runs.iter().any(|r| r.props.direct.get("b").is_some());
    assert!(bold(last));
    let old = rejected("tracked-changes.docx");
    assert!(!bold(
        paragraphs(&old.body().blocks).last().expect("paragraph")
    ));
    assert!(last.has_revisions);
}

#[test]
fn moves_rows_styles_and_joined_paragraphs() {
    let accepted = open("tracked-moves-and-tables.docx");
    let lines = accepted.body_lines();
    assert_eq!(
        lines[2..7],
        [
            "Experience",
            "Built the shipment data warehouse used by forty analysts.",
            "Skills: SQL, Python, dbt.",
            "Education",
            "BSc Computer Science, Northbridge University, 2019."
        ]
    );
    let paras = paragraphs(&accepted.body().blocks);
    assert_eq!(paras[5].props.style.as_deref(), Some("Heading2"));
    assert_eq!(paras[6].joined.len(), 1);
    let table = accepted
        .body()
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    let rows: Vec<String> = table.rows.iter().map(Row::text).collect();
    assert_eq!(rows, ["Quarter\tCustomers", "Q3 2026\t17", "Q4 2026\t23"]);

    let old = rejected("tracked-moves-and-tables.docx");
    let lines = old.body_lines();
    assert_eq!(
        lines[2..8],
        [
            "Skills: SQL, Python, dbt.",
            "Experience",
            "Built the shipment data warehouse used by forty analysts.",
            "Education",
            "BSc Computer Science, ",
            "Northbridge University, 2019."
        ]
    );
    let paras = paragraphs(&old.body().blocks);
    assert_eq!(paras[5].props.style.as_deref(), Some("Heading1"));
    let table = old
        .body()
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    let rows: Vec<String> = table.rows.iter().map(Row::text).collect();
    assert_eq!(rows, ["Quarter\tCustomers", "Q3 2026\t17", "Q2 2026\t12"]);
}

#[test]
fn content_controls_lock_their_paragraphs() {
    let docx = open("content-controls.docx");
    let paras = paragraphs(&docx.body().blocks);
    assert_eq!(paras[1].text(), "Lakeside, Ontario");
    assert_eq!(paras[1].lock, Some(Lock::ContentControl));
    assert_eq!(paras[3].text(), "Phone: 555 0100");
    assert_eq!(paras[3].lock, None);
    assert_eq!(atoms(paras[3])[0].kind, AtomKind::ContentControl);
}

#[test]
fn word_markup_and_unknown_xml_are_read_around() {
    let docx = open("word-markup.docx");
    let paras = paragraphs(&docx.body().blocks);
    assert_eq!(paras[0].anchor.as_deref(), Some("1A2B0001"));
    assert_eq!(
        paras[1].text(),
        "Data engineer who replatformed two warehouses."
    );
    assert!(paras.iter().all(|p| !p.has_revisions));

    let unknown = open("unknown-xml.docx");
    let lines = unknown.body_lines();
    assert_eq!(
        lines,
        [
            "Avery Quinn",
            "Data engineer · Lakeside, Ontario · avery.quinn@example.com",
            "Data engineer with six years of experience.",
            "The last paragraph."
        ]
    );
    assert!(
        !unknown
            .body()
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Opaque(_))),
        "elements in ignorable namespaces are not blocks"
    );
}

#[test]
fn special_characters_and_atoms() {
    let docx = open("inline-specials.docx");
    let paras = paragraphs(&docx.body().blocks);
    assert_eq!(paras[1].text(), "Skills\tSQL, Python");
    assert_eq!(
        paras[2].text(),
        "Line one\nLine two after a line break\nLine three after a carriage return"
    );
    assert!(
        paras[3]
            .text()
            .contains("soft\u{ad}hyphen and a non\u{2011}breaking")
    );
    assert_eq!(atoms(paras[3])[0].kind, AtomKind::Symbol);
    assert_eq!(atoms(paras[4])[0].kind, AtomKind::PageBreak);
    assert_eq!(paras[5].text(), "  two leading spaces and three   inside  ");

    let unicode = open("unicode-text.docx");
    let lines = unicode.body_lines();
    assert_eq!(lines[0], "Zoë Ångström-Nakamura");
    assert!(lines[3].contains('\u{1F680}'));
    assert_eq!(
        lines[6],
        "Markup characters: <tags> & \"quotes\" stay text."
    );

    let math = open("math.docx");
    let paras = paragraphs(&math.body().blocks);
    assert_eq!(atoms(paras[2])[0].kind, AtomKind::Math);
    assert_eq!(paras[3].text(), "And inline: a+b works too.");

    let text_box = open("text-box.docx");
    let paras = paragraphs(&text_box.body().blocks);
    assert_eq!(atoms(paras[2])[0].kind, AtomKind::Drawing);
}

#[test]
fn empty_paragraphs() {
    let docx = open("empty-and-whitespace.docx");
    let paras = paragraphs(&docx.body().blocks);
    assert!(paras[1].is_empty() && paras[2].is_empty() && paras[3].is_empty());
    assert_eq!(paras[4].text(), " no preserve attribute ");
    assert_eq!(paras[5].text(), "\t");
}

#[test]
fn property_maps_ignore_bookkeeping() {
    let docx = open("character-styles.docx");
    let paras = paragraphs(&docx.body().blocks);
    let styled_run = paras[1]
        .runs
        .iter()
        .find(|r| r.props.style.as_deref() == Some("Strong"))
        .expect("strong run");
    assert!(styled_run.props.direct.is_empty());
    let lang_run = paras[2]
        .runs
        .iter()
        .find(|r| r.props.direct.is_empty() && r.props.style.is_none());
    assert!(lang_run.is_some(), "language alone is not formatting");
    assert_eq!(paras[3].props.direct.get("jc"), Some("val=center;"));
}

#[test]
fn rejects_what_this_version_cannot_handle() {
    let docm = {
        let mut package = Package::read(&bytes("headings-paragraphs.docx")).expect("reads");
        let types = String::from_utf8(package.part("[Content_Types].xml").expect("types").to_vec())
            .expect("utf-8")
            .replace(ns::CT_DOCUMENT, ns::CT_DOCUMENT_MACRO);
        package.set_part("[Content_Types].xml", types.into_bytes());
        package.to_bytes().expect("writes")
    };
    assert!(matches!(Docx::open(&docm), Err(Error::Unsupported(_))));

    let strict = {
        let mut package = Package::read(&bytes("headings-paragraphs.docx")).expect("reads");
        let doc = String::from_utf8(package.part("word/document.xml").expect("doc").to_vec())
            .expect("utf-8")
            .replace(ns::W, ns::W_STRICT);
        package.set_part("word/document.xml", doc.into_bytes());
        package.to_bytes().expect("writes")
    };
    assert!(matches!(Docx::open(&strict), Err(Error::Unsupported(_))));

    let not_word =
        lineage_testkit::zip_writer::write_stored(&[("a.txt".to_owned(), b"hi".to_vec())]);
    assert!(matches!(
        Docx::open(&not_word),
        Err(Error::NotAWordDocument(_))
    ));
}
