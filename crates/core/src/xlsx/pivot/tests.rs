// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::advanced;

use super::*;
use crate::xlsx::Workbook;
use crate::xlsx::model::UnsupportedKind;

fn open(wb: &lineage_testkit::xlsx::Workbook) -> Workbook {
    Workbook::open(&wb.to_bytes()).unwrap()
}

fn diff_of(a: &Workbook, b: &Workbook) -> (Vec<PivotChange>, Vec<SlicerChange>) {
    let names = |w: &Workbook| w.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    diff(&a.pivots, &b.pivots, &names(a), &names(b), &|i| Some(i))
}

#[test]
fn reads_two_pivots_on_one_cache_and_a_slicer() {
    let wb = open(&advanced::model_full());
    let p = &wb.pivots;
    assert!(p.unreadable.is_empty(), "{:?}", p.unreadable);
    assert_eq!(p.caches.len(), 1);
    let c = &p.caches[0];
    assert_eq!(
        c.source,
        Source::Range {
            sheet: "Sales".to_owned(),
            range: "A1:E13".to_owned()
        }
    );
    assert_eq!(c.fields[0].items, ["North", "South", "West"]);
    assert_eq!(c.fields[5].formula.as_deref(), Some("Revenue/Seats"));
    assert_eq!(c.refresh.record_count, Some(12));

    assert_eq!(p.tables.len(), 2);
    let t = &p.tables[0];
    assert_eq!(t.name, "RevenueByRegion");
    assert_eq!(wb.sheets[t.sheet].name, "Pivots");
    assert_eq!(t.fields.rows, ["Region"]);
    assert_eq!(t.fields.columns, ["Values"]);
    let values: Vec<&str> = t.fields.values.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(values, ["Sum of Revenue", "Sum of Seats"]);
    assert_eq!(
        t.fields.filters,
        [FilterField {
            field: "Month".to_owned(),
            item: None
        }]
    );
    // The report filter is two rows above the table.
    assert_eq!((t.area.row1, t.area.row2), (1, 7));
    assert_eq!(t.layout.form, "compact");
    assert_eq!(p.tables[1].fields.rows, ["Plan"]);
    assert_eq!(p.tables[1].cache, 0);

    assert_eq!(p.slicers.len(), 1);
    let s = &p.slicers[0];
    assert_eq!(s.caption, "Region");
    assert_eq!(s.pivots, ["RevenueByRegion", "SeatsByPlan"]);
    assert!(s.selected.is_empty(), "shows all");

    // Pivots and the slicer are tracked now; nothing about them is left as
    // "Preserved".
    assert!(
        !wb.unsupported.iter().any(|u| matches!(
            u.kind,
            UnsupportedKind::PivotTables | UnsupportedKind::Slicers
        )),
        "{:?}",
        wb.unsupported
    );
    assert_eq!(wb.pivot_areas.len(), 2);
}

#[test]
fn the_rendered_cells_agree_with_the_cache() {
    let wb = open(&advanced::model_full());
    let j = wb.sheet_index("Pivots").unwrap();
    let text = wb.sheet_text(j);
    let v = |cell: &str| {
        let (col, row) = crate::xlsx::refs::parse_cell(cell).unwrap();
        crate::xlsx::value::either(&wb, text, wb.sheets[j].cell(col, row).unwrap()).key()
    };
    // North: 10*29 + 4*99 + 12*29 + 5*99.
    let north = crate::xlsx::value::Value::Number("1529").key();
    assert_eq!(v("B4"), north);
}

#[test]
fn rows_changed_on_one_pivot() {
    let (a, b) = (
        open(&advanced::model_full()),
        open(&advanced::model_full_pivot_rows()),
    );
    let (changes, slicers) = diff_of(&a, &b);
    assert!(slicers.is_empty());
    assert_eq!(changes.len(), 1);
    let c = &changes[0];
    assert_eq!(c.kind, PivotChangeKind::Changed);
    assert_eq!(c.details, ["Rows: Month (was Plan)"]);
    assert_eq!(
        c.summary(),
        "Pivot table SeatsByPlan: Rows: Month (was Plan)"
    );
    assert_eq!(c.before.as_ref().unwrap().rows, ["Plan"]);
    assert_eq!(c.after.as_ref().unwrap().rows, ["Month"]);
}

#[test]
fn a_filter_shows_one_item() {
    let (a, b) = (
        open(&advanced::model_full()),
        open(&advanced::model_full_pivot_filter()),
    );
    let (changes, _) = diff_of(&a, &b);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].details, ["Filter Month: shows Feb (was all)"]);
}

#[test]
fn a_refresh_is_not_a_definition_change() {
    let (a, b) = (
        open(&advanced::model_full()),
        open(&advanced::model_full_refreshed()),
    );
    let (changes, _) = diff_of(&a, &b);
    assert_eq!(changes.len(), 2);
    for c in &changes {
        assert_eq!(c.kind, PivotChangeKind::Refreshed, "{c:?}");
        assert!(c.refreshed);
        assert!(c.details.is_empty());
    }
}

#[test]
fn a_slicer_selection() {
    let (a, b) = (
        open(&advanced::model_full()),
        open(&advanced::model_full_slicer()),
    );
    let (changes, slicers) = diff_of(&a, &b);
    assert_eq!(slicers.len(), 1);
    assert_eq!(
        slicers[0].summary,
        "Slicer Region: shows North, West (was all)"
    );
    // Both pivots now hide South.
    assert_eq!(changes.len(), 2);
    assert!(
        changes
            .iter()
            .all(|c| c.details == ["Region: hides South (showed every item)"])
    );
}

#[test]
fn the_same_workbook_has_no_pivot_changes() {
    let (a, b) = (open(&advanced::model_full()), open(&advanced::model_full()));
    let (changes, slicers) = diff_of(&a, &b);
    assert!(changes.is_empty() && slicers.is_empty());
}

#[test]
fn a_damaged_pivot_stays_untracked() {
    let mut parts = advanced::model_full().parts();
    for (name, bytes) in &mut parts {
        if name == "xl/pivotTables/pivotTable1.xml" {
            *bytes = b"<pivotTableDefinition".to_vec();
        }
    }
    let bytes = lineage_testkit::zip_writer::write_stored(&parts);
    let wb = Workbook::open(&bytes).unwrap();
    assert_eq!(wb.pivots.tables.len(), 1);
    assert_eq!(wb.pivots.unreadable, ["xl/pivotTables/pivotTable1.xml"]);
    assert!(
        wb.unsupported
            .iter()
            .any(|u| u.kind == UnsupportedKind::PivotTables
                && u.part == "xl/pivotTables/pivotTable1.xml")
    );
}
