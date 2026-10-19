// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::{advanced, mashup};
use proptest::prelude::*;

use super::*;
use crate::code::LineKind;
use crate::xlsx::Workbook;
use crate::xlsx::model::UnsupportedKind;

fn open(wb: &lineage_testkit::xlsx::Workbook) -> Workbook {
    Workbook::open(&wb.to_bytes()).unwrap()
}

#[test]
fn reads_the_models_queries() {
    let wb = open(&advanced::model_full());
    let m = wb.queries.as_ref().expect("queries");
    let names: Vec<&str> = m.queries.iter().map(|q| q.name.as_str()).collect();
    assert_eq!(names, ["Rates", "RegionList"]);
    let rates = &m.queries[0];
    assert_eq!(rates.lines[0], "let");
    assert_eq!(rates.lines.last().unwrap(), "    Typed");
    assert_eq!(
        rates.id.as_deref(),
        Some("{6A1F3C2E-4B7D-4E90-8C15-2D9B7E4F1A36}")
    );
    assert_eq!(rates.load.label(), "loads to Rates");
    assert_eq!(rates.rows.as_deref(), Some("3"));
    assert_eq!(m.queries[1].load.label(), "is connection only");
    assert!(
        !wb.unsupported
            .iter()
            .any(|u| u.kind == UnsupportedKind::PowerQuery),
        "{:?}",
        wb.unsupported
    );
}

#[test]
fn an_edited_query_is_a_code_change() {
    let (a, b) = (
        open(&advanced::model_full()),
        open(&advanced::model_full_query_edited()),
    );
    let d = diff(a.queries.as_ref(), b.queries.as_ref());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].kind, QueryChangeKind::Changed);
    assert_eq!(d[0].summary, "Query Rates: 3 lines changed");
    assert!(
        d[0].code
            .iter()
            .any(|l| l.kind == LineKind::Added && l.text.contains("Positive"))
    );
}

#[test]
fn an_added_query() {
    let (a, b) = (
        open(&advanced::model_full()),
        open(&advanced::model_full_query_added()),
    );
    let d = diff(a.queries.as_ref(), b.queries.as_ref());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].summary, "Added query Targets (4 lines)");
    assert_eq!(d[0].details, ["It is connection only"]);
}

#[test]
fn a_refresh_is_not_an_edit() {
    let (a, b) = (
        open(&advanced::model_full()),
        open(&advanced::model_full_query_refreshed()),
    );
    let d = diff(a.queries.as_ref(), b.queries.as_ref());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].kind, QueryChangeKind::Refreshed);
    assert_eq!(
        d[0].summary,
        "Query Rates was refreshed (3 rows, 2026-10-16)"
    );
    assert_eq!(d[0].lines, 0);
}

#[test]
fn a_rename_keeps_the_query() {
    let mut q = advanced::model_queries();
    q[1].name = "Region list".to_owned();
    let a = read("x", &mashup::item_xml(&advanced::model_queries())).unwrap();
    let b = read("x", &mashup::item_xml(&q)).unwrap();
    assert_eq!(b.queries[1].name, "Region list");
    let d = diff(Some(&a), Some(&b));
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].summary, "Renamed query RegionList to Region list");
    // Without its id, the same code still says it was renamed.
    let mut c = b.clone();
    c.queries[1].id = None;
    let d = diff(Some(&a), Some(&c));
    assert_eq!(d[0].summary, "Renamed query RegionList to Region list");
}

#[test]
fn members_follow_m_syntax() {
    let src = "section Section1;\r\n\r\n// a comment; with a semicolon\r\nshared A = \"x;y\" & \"\"\"\";\r\nshared #\"B \"\"quoted\"\"\" = /* ; */ 1;\r\nshared C.D = [a = 1];\r\n";
    let m = members(src);
    assert_eq!(
        m,
        [
            ("A".to_owned(), "\"x;y\" & \"\"\"\"".to_owned()),
            ("B \"quoted\"".to_owned(), "/* ; */ 1".to_owned()),
            ("C.D".to_owned(), "[a = 1]".to_owned()),
        ]
    );
}

#[test]
fn base64_round_trips_and_refuses_junk() {
    for s in [&b""[..], b"f", b"fo", b"foo", b"foobar", &[0u8, 255, 128]] {
        assert_eq!(base64(&mashup::base64(s)).unwrap(), s);
    }
    assert!(base64("not base64!").is_err());
}

#[test]
fn malformed_items_are_refused_not_panicked() {
    let good = mashup::item_xml(&advanced::model_queries());
    assert!(
        read(
            "x",
            b"<DataMashup xmlns=\"http://schemas.microsoft.com/DataMashup\">AAAA</DataMashup>"
        )
        .is_err()
    );
    assert!(read("x", b"<Other/>").is_err());
    for cut in (0..good.len()).step_by(211) {
        let _ = read("x", &good[..cut]);
    }
    let stream = mashup::stream(&advanced::model_queries());
    for cut in (0..stream.len()).step_by(37) {
        let text = format!(
            "<DataMashup xmlns=\"http://schemas.microsoft.com/DataMashup\">{}</DataMashup>",
            mashup::base64(&stream[..cut])
        );
        let _ = read("x", text.as_bytes());
    }
}

proptest! {
    #[test]
    fn members_never_panic(src in "[a-z =;\"#/*\r\n]{0,200}") {
        let _ = members(&src);
    }

    #[test]
    fn reordering_queries_is_no_change(swap in any::<bool>()) {
        let mut q = advanced::model_queries();
        if swap {
            q.reverse();
        }
        let a = read("x", &mashup::item_xml(&advanced::model_queries())).unwrap();
        let b = read("x", &mashup::item_xml(&q)).unwrap();
        prop_assert!(diff(Some(&a), Some(&b)).is_empty());
    }
}
