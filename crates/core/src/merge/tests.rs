// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::resume::{self, Line};

use super::*;
use crate::testutil::{open, open_lines};

fn statuses(plan: &Plan<'_>) -> Vec<(ChangeKind, Status)> {
    plan.items
        .iter()
        .map(|i| (i.change.kind, i.status.clone()))
        .collect()
}

#[test]
fn nothing_to_bring_in_when_theirs_is_the_base() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::tailored_data_lines());
    let plan = plan(&base, &ours, &base);
    assert!(plan.items.is_empty());
}

#[test]
fn edits_to_different_paragraphs_merge_cleanly() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::tailored_data_lines());
    let theirs = open_lines(&resume::master_v2_lines());
    let plan = plan(&base, &ours, &theirs);
    assert_eq!(
        statuses(&plan),
        [
            (ChangeKind::Insert, Status::Clean),
            (ChangeKind::Insert, Status::Clean)
        ]
    );
}

#[test]
fn edits_to_the_same_paragraph_overlap() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::tailored_same_para_lines());
    let theirs = open_lines(&resume::master_v3_lines());
    let plan = plan(&base, &ours, &theirs);
    let corvia = plan
        .items
        .iter()
        .find(|i| i.change.after_text.as_deref() == Some(resume::CORVIA_TITLE_V3))
        .expect("the Corvia edit");
    assert_eq!(corvia.status, Status::Overlap(OverlapKind::BothEdited));
    let mine = corvia.mine.as_ref().expect("both sides are shown");
    assert_eq!(
        mine.after_text.as_deref(),
        Some("Data engineer (logistics), Corvia Freight, 2020 to 2023")
    );
    assert_eq!(
        corvia.change.before_text.as_deref(),
        Some(resume::CORVIA_TITLE)
    );
    assert_eq!(corvia.choices(), [Decision::KeepMine, Decision::UseMasters]);
    // Everything else from the master is still clean.
    assert!(
        plan.items
            .iter()
            .filter(|i| i.change.id != corvia.change.id)
            .all(|i| i.status == Status::Clean)
    );
}

#[test]
fn deleted_against_edited_overlaps_both_ways() {
    let base = open_lines(&resume::master_v1_lines());
    let deleted = open_lines(&resume::tailored_deleted_job_lines());
    let edited = open_lines(&resume::master_v3_lines());

    // The master edited what this version deleted.
    let plan_a = plan(&base, &deleted, &edited);
    let corvia = plan_a
        .items
        .iter()
        .find(|i| i.change.after_text.as_deref() == Some(resume::CORVIA_TITLE_V3))
        .expect("the Corvia edit");
    assert_eq!(corvia.status, Status::Overlap(OverlapKind::EditedVsDeleted));

    // The master deleted what this version edited.
    let same_para = open_lines(&resume::tailored_same_para_lines());
    let plan_b = plan(&base, &same_para, &deleted);
    let deletion = plan_b
        .items
        .iter()
        .find(|i| i.change.before_text.as_deref() == Some(resume::CORVIA_TITLE))
        .expect("the Corvia deletion");
    assert_eq!(
        deletion.status,
        Status::Overlap(OverlapKind::DeletedVsEdited)
    );
    // The bullets under it were not edited here, so deleting them is clean.
    let bullets: Vec<&Item> = plan_b
        .items
        .iter()
        .filter(|i| i.change.kind == ChangeKind::Delete && i.change.id != deletion.change.id)
        .collect();
    assert_eq!(bullets.len(), 2);
    assert!(bullets.iter().all(|i| i.status == Status::Clean));
}

#[test]
fn formatting_only_changes_are_items_of_their_own() {
    let base = open_lines(&resume::master_v2_lines());
    let ours = open_lines(&resume::tailored_data_lines());
    let theirs = open_lines(&resume::master_v3_lines());
    let plan = plan(&base, &ours, &theirs);
    let formatting: Vec<&Item> = plan
        .items
        .iter()
        .filter(|i| i.change.formatting_only)
        .collect();
    assert_eq!(formatting.len(), 1);
    assert_eq!(formatting[0].change.kind, ChangeKind::Format);
    assert_eq!(formatting[0].status, Status::Clean);
    let content: Vec<&Item> = plan
        .items
        .iter()
        .filter(|i| !i.change.formatting_only)
        .collect();
    assert_eq!(content.len(), 1);
}

#[test]
fn a_change_typed_in_by_hand_is_already_there() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::tailored_copied_job_lines());
    let theirs = open_lines(&resume::master_v2_lines());
    let plan = plan(&base, &ours, &theirs);
    assert_eq!(
        statuses(&plan),
        [
            (ChangeKind::Insert, Status::AlreadyThere { confidence: 100 }),
            (ChangeKind::Insert, Status::AlreadyThere { confidence: 100 })
        ]
    );
}

#[test]
fn the_same_edit_on_both_sides_is_already_there() {
    let base = open_lines(&resume::master_v2_lines());
    let mut both = resume::master_v2_lines();
    let corvia = both
        .iter()
        .position(|l| *l == Line::H2(resume::CORVIA_TITLE.into()))
        .expect("corvia");
    both[corvia] = Line::H2(resume::CORVIA_TITLE_V3.into());
    let ours = open_lines(&both);
    let theirs = open_lines(&resume::master_v3_lines());
    let plan = plan(&base, &ours, &theirs);
    let edit = plan
        .items
        .iter()
        .find(|i| i.change.kind == ChangeKind::Modify)
        .expect("edit");
    assert_eq!(edit.status, Status::AlreadyThere { confidence: 100 });
}

#[test]
fn both_adding_in_one_place_overlaps() {
    let base = open_lines(&resume::master_v1_lines());
    let mut lines = resume::master_v1_lines();
    let experience = lines
        .iter()
        .position(|l| *l == Line::H1("Experience".into()))
        .expect("experience");
    lines.insert(
        experience + 1,
        Line::Plain("Open to contract roles.".into()),
    );
    let ours = open_lines(&lines);
    let theirs = open_lines(&resume::master_v2_lines());
    let plan = plan(&base, &ours, &theirs);
    assert!(
        plan.items
            .iter()
            .all(|i| i.status == Status::Overlap(OverlapKind::BothAdded)),
        "{:?}",
        statuses(&plan)
    );
    assert_eq!(plan.items[0].choices().len(), 4);
}

#[test]
fn a_version_can_take_v3_without_v2() {
    // The changes v3 made, measured against v2, brought into a version made from v1.
    let v2 = open_lines(&resume::master_v2_lines());
    let v3 = open_lines(&resume::master_v3_lines());
    let ours = open_lines(&resume::tailored_data_lines());
    let plan = plan(&v2, &ours, &v3);
    assert_eq!(plan.items.len(), 2, "{:?}", statuses(&plan));
    assert!(
        plan.items.iter().all(|i| i.status == Status::Clean),
        "{:?}",
        statuses(&plan)
    );
}

#[test]
fn a_v3_edit_to_what_only_v2_added_overlaps() {
    let v2 = open_lines(&resume::master_v2_lines());
    let mut v3_lines = resume::master_v2_lines();
    let job = v3_lines
        .iter()
        .position(|l| *l == Line::H2(resume::NEW_JOB_TITLE.into()))
        .expect("new job");
    v3_lines[job] = Line::H2("Staff data engineer, Halden Robotics, 2023 to now".into());
    let v3 = open_lines(&v3_lines);
    let ours = open_lines(&resume::master_v1_lines());
    let plan = plan(&v2, &ours, &v3);
    assert_eq!(plan.items.len(), 1);
    assert_eq!(
        plan.items[0].status,
        Status::Overlap(OverlapKind::EditedVsDeleted)
    );
}

fn table_doc(rows: &[[&str; 3]]) -> Docx {
    use lineage_testkit::docx::{Package, plain, styled};
    let cells: String = rows
        .iter()
        .map(|r| {
            let tc = |t: &str| {
                format!(
                    r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{}</w:tc>"#,
                    plain(t)
                )
            };
            format!("<w:tr>{}{}{}</w:tr>", tc(r[0]), tc(r[1]), tc(r[2]))
        })
        .collect();
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="9000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>{cells}</w:tbl>"#
    );
    open(
        &Package::new(
            "t",
            [styled("Heading1", "Traction"), table, plain("After.")].concat(),
        )
        .to_bytes(),
    )
}

#[test]
fn table_cells_are_units() {
    let base = table_doc(&[["Quarter", "Revenue", "Customers"], ["Q3", "$26k", "17"]]);
    let ours = table_doc(&[["Quarter", "Revenue", "Customers"], ["Q3", "$26k", "18"]]);
    let theirs_other_cell = table_doc(&[["Quarter", "Revenue", "Customers"], ["Q3", "$31k", "17"]]);
    let plan_a = plan(&base, &ours, &theirs_other_cell);
    assert_eq!(statuses(&plan_a), [(ChangeKind::Modify, Status::Clean)]);

    let theirs_same_cell = table_doc(&[["Quarter", "Revenue", "Customers"], ["Q3", "$26k", "19"]]);
    let plan_b = plan(&base, &ours, &theirs_same_cell);
    assert_eq!(
        statuses(&plan_b),
        [(ChangeKind::Modify, Status::Overlap(OverlapKind::BothEdited))]
    );

    let theirs_new_row = table_doc(&[
        ["Quarter", "Revenue", "Customers"],
        ["Q3", "$26k", "17"],
        ["Q4", "$34k", "23"],
    ]);
    let plan_c = plan(&base, &ours, &theirs_new_row);
    assert_eq!(statuses(&plan_c), [(ChangeKind::Insert, Status::Clean)]);
    assert_eq!(plan_c.items[0].change.unit, Unit::Row);
}

#[test]
fn a_later_block_is_traced_back_to_where_it_came_from() {
    use crate::diff::{CellStep, Path};
    use locate::{map_path, source_of};
    let base = open_lines(&resume::master_v1_lines());
    let theirs = open_lines(&resume::master_v2_lines());
    let d = crate::diff::diff(&base, &theirs);
    let story = d.story(&StoryKey::Body);
    // Every block that came from the base maps back to where it is now.
    let mut traced = 0;
    for j in 0..theirs.body().blocks.len() {
        if let Some(from) = source_of(story, &Path::top(j)) {
            assert_eq!(map_path(story, &from).path(), Some(&Path::top(j)));
            traced += 1;
        }
    }
    assert_eq!(traced, theirs.body().blocks.len() - 2, "two new paragraphs");

    // Inside a table: a cell of a new row has no source; a changed cell does.
    let before = table_doc(&[["Quarter", "Revenue", "Customers"], ["Q3", "$26k", "17"]]);
    let after = table_doc(&[
        ["Quarter", "Revenue", "Customers"],
        ["Q4", "$34k", "23"],
        ["Q3", "$31k", "17"],
    ]);
    let d = crate::diff::diff(&before, &after);
    let story = d.story(&StoryKey::Body);
    let cell = |row: usize, column: usize| Path {
        block: 1,
        cells: vec![CellStep {
            row,
            column: Some(column),
            block: Some(0),
        }],
    };
    assert_eq!(source_of(story, &cell(1, 1)), None);
    assert_eq!(source_of(story, &cell(2, 1)), Some(cell(1, 1)));
    assert_eq!(source_of(story, &cell(0, 2)), Some(cell(0, 2)));
}
