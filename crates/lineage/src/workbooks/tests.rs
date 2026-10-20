// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_core::Package;
use lineage_core::docx::props::{read_ids, write_ids};
use lineage_core::merge::Decision;
use lineage_core::xlsx::write::apply_steps;
use lineage_testkit::workbooks::{self, Assumptions};
use lineage_testkit::xlsx::Workbook as Built;

use super::*;
use crate::commands::{self, Context};
use crate::tests::Host;

/// "The founder edits the workbook in Excel": it becomes `built`, and keeps
/// the file's Lineage ids.
fn wb_edit(file: &[u8], built: &Built) -> Vec<u8> {
    let ids = read_ids(&Package::read(file).expect("reads"))
        .expect("reads")
        .expect("ids");
    let mut package = Package::read(&built.to_bytes()).expect("reads");
    write_ids(&mut package, &ids).expect("writes");
    package.to_bytes().expect("bytes")
}

fn shown(bytes: &[u8], sheet: &str, at: &str) -> String {
    let wb = Workbook::open(bytes).expect("opens");
    let i = wb.sheet_index(sheet).expect("sheet");
    let (c, r) = lineage_core::xlsx::refs::parse_cell(at).expect("cell");
    wb.sheets[i]
        .cell(c, r)
        .map(|cell| lineage_core::xlsx::value::either(&wb, wb.sheet_text(i), cell).text())
        .unwrap_or_default()
}

impl Host {
    fn start_workbook(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let (_, outcome) =
            commands::start_tracking(&mut ctx, bytes, "Model.xlsx", "Model.xlsx", &self.blobs)
                .expect("starts");
        self.take(outcome).file.expect("file with ids")
    }

    /// The pane's round: prepare, carry out the steps (here with the Rust
    /// writer, as Excel would), finish.
    fn bring(
        &mut self,
        version: &Id,
        file: &[u8],
        decisions: &[(String, Decision)],
    ) -> (WorkbookPlan, Vec<u8>) {
        let state = self.state().clone();
        let p = prepare(&state, version, file, decisions, &self.blobs).expect("prepares");
        let held = apply_steps(file, &p.steps).expect("Excel carries it out");
        let blobs = self.blobs.clone();
        let mut ctx = self.context();
        let outcome =
            finish(&mut ctx, &state, version, file, &held, decisions, &blobs).expect("finishes");
        let outcome = self.take(outcome);
        (p, outcome.file.unwrap_or(held))
    }
}

/// A model family: the master, Northwind's version (12 new seats a month),
/// and the master's price change offered to it.
fn model_family() -> (Host, Id, Vec<u8>, Id, Vec<u8>) {
    let mut host = Host::new();
    let master_file = host.start_workbook(&workbooks::model_master().to_bytes());
    let master = host.master();
    let (northwind, file) = host.make_version("Model – Northwind.xlsx");
    let file = host.save(
        &northwind,
        &wb_edit(&file, &workbooks::model_northwind()),
        "Northwind's plan",
    );
    let master_file = host.save(
        &master,
        &wb_edit(&master_file, &workbooks::model_price_update()),
        "New price",
    );
    host.offer(std::slice::from_ref(&northwind));
    (host, master, master_file, northwind, file)
}

#[test]
fn a_tailored_workbook_brings_in_the_masters_price() {
    let (mut host, _, _, northwind, file) = model_family();
    let panel = commands::updates(host.state(), &northwind, &file, &host.blobs).expect("updates");
    assert_eq!(panel.headline, "1 update from the master");
    let view = &panel.groups[0].view;
    let wv = view.workbook.as_ref().expect("a workbook view");
    assert_eq!(wv.groups[0].title, "Inputs");
    let card = &wv.groups[0].cards[0];
    assert_eq!(card.summary, "'$49' changed to '$55'");
    let labels: Vec<&str> = card.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Bring in", "Decline"]);

    let decisions = vec![(card.ids[0].clone(), Decision::BringIn)];
    let (plan, after) = host.bring(&northwind, &file, &decisions);
    assert_eq!(plan.brought, 1);
    assert_eq!(plan.text, "Brings in 1 change");
    assert_eq!(shown(&after, "Assumptions", "B4"), "55");
    assert_eq!(
        shown(&after, "Assumptions", "B3"),
        "12",
        "this version's own plan stays"
    );
    let latest = host.state().latest(&northwind).expect("checkpoint");
    assert_eq!(latest.note, "Brought in 1 change from master v2");
    let panel = commands::updates(host.state(), &northwind, &after, &host.blobs).expect("updates");
    assert_eq!(panel.headline, "You're up to date with the master");
}

#[test]
fn typing_while_the_plan_runs_puts_nothing_on_record() {
    let (host, _, _, northwind, file) = model_family();
    let state = host.state().clone();
    let id = commands::updates(&state, &northwind, &file, &host.blobs)
        .expect("updates")
        .groups[0]
        .view
        .changes[0]
        .id
        .clone();
    let decisions = vec![(id, Decision::BringIn)];
    let p = prepare(&state, &northwind, &file, &decisions, &host.blobs).expect("prepares");
    let mut held = apply_steps(&file, &p.steps).expect("written");
    // The person changed the churn rate while the plan ran.
    held = apply_steps(
        &held,
        &[Step::Set {
            sheet: "Assumptions".to_owned(),
            col: 2,
            row: 5,
            value: lineage_core::xlsx::apply::Write::Number("0.03".to_owned()),
        }],
    )
    .expect("written");
    let mut ids = crate::ids::SequentialIds::new(200);
    let mut ctx = Context {
        now: host.now,
        ids: &mut ids,
        author: None,
    };
    let err = finish(
        &mut ctx,
        &state,
        &northwind,
        &file,
        &held,
        &decisions,
        &host.blobs,
    )
    .expect_err("refused");
    assert!(
        matches!(err, Error::Engine(lineage_core::Error::NotLanded(_))),
        "{err:?}"
    );
}

#[test]
fn keep_mine_writes_nothing_and_records_the_choice() {
    let mut host = Host::new();
    let master_file = host.start_workbook(&workbooks::model_master().to_bytes());
    let master = host.master();
    let (northwind, file) = host.make_version("Model – Northwind.xlsx");
    let file = host.save(
        &northwind,
        &wb_edit(&file, &workbooks::model_northwind()),
        "Northwind's plan",
    );
    let eighteen = workbooks::financial_model(&Assumptions {
        new_seats: 18,
        ..Assumptions::default()
    });
    host.save(&master, &wb_edit(&master_file, &eighteen), "More seats");
    host.offer(std::slice::from_ref(&northwind));
    let panel = commands::updates(host.state(), &northwind, &file, &host.blobs).expect("updates");
    let overlap = &panel.groups[0].view.overlaps[0];
    assert_eq!(overlap.summary, "You and the master both changed this cell");
    let (plan, after) = host.bring(
        &northwind,
        &file,
        &[(overlap.id.clone(), Decision::KeepMine)],
    );
    assert!(plan.steps.is_empty());
    assert_eq!(shown(&after, "Assumptions", "B3"), "12");
    let panel = commands::updates(host.state(), &northwind, &after, &host.blobs).expect("updates");
    assert_eq!(panel.headline, "You're up to date with the master");
}

#[test]
fn two_master_updates_come_in_one_after_the_other() {
    let mut host = Host::new();
    let master_file = host.start_workbook(&workbooks::tracker_master().to_bytes());
    let master = host.master();
    let (version, file) = host.make_version("Job search – data.xlsx");
    // Update 1: application 2's status. Update 2: two new applications.
    let mut apps = workbooks::applications();
    apps[1].status = "Interview";
    let master_file = host.save(
        &master,
        &wb_edit(&master_file, &workbooks::tracker(&apps, false)),
        "Status",
    );
    let mut more = apps.clone();
    more.push(workbooks::Application {
        id: 9,
        company: "Lanternfish",
        role: "Data analyst",
        applied: 46042,
        status: "Applied",
        next: "",
    });
    host.save(
        &master,
        &wb_edit(&master_file, &workbooks::tracker(&more, false)),
        "New application",
    );
    host.offer(std::slice::from_ref(&version));
    let panel = commands::updates(host.state(), &version, &file, &host.blobs).expect("updates");
    assert_eq!(panel.groups.len(), 2);
    let decisions: Vec<(String, Decision)> = panel
        .groups
        .iter()
        .flat_map(|g| {
            g.view
                .changes
                .iter()
                .map(|c| (c.id.clone(), Decision::BringIn))
        })
        .collect();
    let (plan, after) = host.bring(&version, &file, &decisions);
    assert_eq!(plan.brought, 2);
    assert_eq!(shown(&after, "Applications", "E3"), "Interview");
    assert_eq!(shown(&after, "Applications", "B10"), "Lanternfish");
    let panel = commands::updates(host.state(), &version, &after, &host.blobs).expect("updates");
    assert_eq!(panel.headline, "You're up to date with the master");
}

#[test]
fn a_workbook_that_came_back_is_reviewed_and_kept() {
    use crate::event::MatchedBy;
    use crate::returned::review;

    let mut host = Host::new();
    let master_file = host.start_workbook(&workbooks::model_master().to_bytes());
    let master = host.master();
    let (_, copy) = host.send(&master, &master_file, "Harbor Lane Capital", true);
    // The investor changes the churn assumption and sends it back.
    let back = wb_edit(
        &copy,
        &workbooks::financial_model(&Assumptions {
            churn: "0.03".to_owned(),
            ..Assumptions::default()
        }),
    );
    let found = host.recognise(&back);
    let returned = host.record(&back, &found, MatchedBy::Ids);
    let r = review(host.state(), &returned, &master, &master_file, &host.blobs).expect("reviews");
    let wv = r.view.workbook.as_ref().expect("a workbook view");
    let card = &wv.groups[0].cards[0];
    assert_eq!(card.summary, "'2.00%' changed to '3.00%'");
    let labels: Vec<&str> = card.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Keep", "Skip"]);
    let cards = vec![(card.ids[0].clone(), Decision::BringIn)];
    let state = host.state().clone();
    let p = prepare_keep(
        &state,
        &returned,
        &master,
        &master_file,
        &cards,
        &host.blobs,
    )
    .expect("prepares");
    assert_eq!(p.text, "Keeps 1 change");
    let held = apply_steps(&master_file, &p.steps).expect("written");
    let blobs = host.blobs.clone();
    let mut ctx = host.context();
    let outcome = finish_keep(
        &mut ctx,
        &state,
        &returned,
        &master,
        &master_file,
        &held,
        &cards,
        &blobs,
    )
    .expect("finishes");
    let outcome = host.take(outcome);
    assert_eq!(
        shown(&outcome.file.expect("file"), "Assumptions", "B5"),
        "0.03"
    );
    let latest = host.state().latest(&master).expect("checkpoint");
    assert_eq!(latest.note, "Kept 1 change from Harbor Lane Capital");
    let r = review(host.state(), &returned, &master, &held, &host.blobs).expect("reviews");
    assert_eq!(r.headline, "Nothing left to review");
}

#[test]
fn what_changed_uses_excels_hints_and_shows_a_cell_side_by_side() {
    let mut host = Host::new();
    let file = host.start_workbook(&workbooks::model_master().to_bytes());
    let master = host.master();
    let edited = wb_edit(&file, &workbooks::model_price_update());
    let hint = Hint {
        sheet: "Assumptions".to_owned(),
        area: lineage_core::xlsx::refs::parse_area("B4").expect("area"),
    };
    let state = host.state().clone();
    let view = what_changed(&state, &master, &edited, &[hint], &host.blobs).expect("view");
    let wv = view.workbook.as_ref().expect("a workbook view");
    assert_eq!(
        wv.groups[0].cards[0].why.as_deref(),
        Some("Edited in this session")
    );
    assert!(view.headline.ends_with(" since v1"), "{}", view.headline);
    let without = what_changed(&state, &master, &edited, &[], &host.blobs).expect("view");
    assert_ne!(
        without.workbook.expect("view").groups[0].cards[0]
            .why
            .as_deref(),
        Some("Edited in this session"),
        "no hint, no claim"
    );

    let sb =
        side_by_side(&state, &master, &edited, "Assumptions", "B4", &host.blobs).expect("a cell");
    assert_eq!(sb.before.expect("before").value, "49");
    assert_eq!(sb.after.expect("after").value, "55");
    assert!(side_by_side(&state, &master, &edited, "Nowhere", "B4", &host.blobs).is_err());
}

#[test]
fn send_a_copy_says_what_goes_with_it() {
    let notes = goes_with_copy(&workbooks::awkward_cells().to_bytes()).expect("reads");
    assert_eq!(notes, ["2 hidden sheets go with this copy"]);
    let notes = goes_with_copy(&workbooks::tracker_filtered().to_bytes()).expect("reads");
    assert_eq!(notes, ["2 hidden rows go with this copy"]);
    assert!(
        goes_with_copy(&workbooks::model_master().to_bytes())
            .expect("reads")
            .is_empty()
    );
    let notes = goes_with_copy(&workbooks::model_xlsm().to_bytes()).expect("reads");
    assert!(
        notes.contains(&"Macros go with this copy".to_owned()),
        "{notes:?}"
    );
    let deck = lineage_testkit::decks::pitch_deck().to_bytes();
    assert!(goes_with_copy(&deck).expect("reads").is_empty());
}

#[test]
fn the_way_back_matches_how_far_excel_got() {
    use lineage_core::xlsx::apply::Write;
    let ins = |at| Step::InsertRows {
        sheet: "Data".to_owned(),
        at,
        count: 2,
    };
    let del = |at| Step::DeleteRows {
        sheet: "Data".to_owned(),
        at,
        count: 2,
    };
    let set = |row, n: &str| Step::Set {
        sheet: "Data".to_owned(),
        col: 1,
        row,
        value: Write::Number(n.to_owned()),
    };
    // Rows inserted at 50 and then 10 (bottom-up), then a cell set.
    let first = ApplyPlan {
        steps: vec![ins(50), ins(10), set(3, "5")],
        undo: vec![set(3, "4"), del(52), del(10)],
        ..ApplyPlan::default()
    };
    let second = ApplyPlan {
        steps: vec![set(4, "9")],
        undo: vec![set(4, "8")],
        ..ApplyPlan::default()
    };
    let plans = [&first, &second];
    assert!(undo_after(&plans, 0).is_empty());
    assert_eq!(
        undo_after(&plans, 1),
        [del(50)],
        "only the rows it inserted"
    );
    assert_eq!(undo_after(&plans, 2), [del(10), del(50)], "last first");
    assert_eq!(undo_after(&plans, 3), first.undo);
    assert_eq!(
        undo_after(&plans, 4),
        [set(4, "8"), set(3, "4"), del(52), del(10)],
        "the later plan's way back first"
    );
}

// --- Phase 5B: macros, pivots and queries in a family --------------------------

#[test]
fn a_copy_that_came_back_with_new_macro_code_is_flagged() {
    use crate::event::MatchedBy;
    use crate::returned::review;
    use lineage_core::xlsx::view::{NEW_MACRO_CODE, Object};
    use lineage_testkit::advanced;

    let mut host = Host::new();
    let mut ctx = Context {
        now: host.now,
        ids: &mut host.ids,
        author: None,
    };
    let (_, outcome) = commands::start_tracking(
        &mut ctx,
        &advanced::model_full().to_bytes(),
        "Model.xlsm",
        "Model.xlsm",
        &host.blobs,
    )
    .expect("starts");
    let master_file = host.take(outcome).file.expect("file with ids");
    let master = host.master();
    let first = host.state().latest(&master).expect("v1");
    assert_eq!(
        first.macros,
        Some(true),
        "the checkpoint says it has macros"
    );

    // My documents shows the badge.
    let entry = crate::library::LibraryEntry::from_facts(crate::library::EntryFacts::of_state(
        host.state(),
    ));
    assert_eq!(entry.badges, ["Contains macros"]);

    let (_, copy) = host.send(&master, &master_file, "Harbor Lane Capital", true);
    let back = wb_edit(&copy, &advanced::model_full_returned_with_macro());
    let found = host.recognise(&back);
    let returned = host.record(&back, &found, MatchedBy::Ids);
    let r = host
        .state()
        .returned_file(&returned)
        .expect("recorded")
        .clone();
    assert!(r.new_macro_code);

    let sent = crate::views::sent_list(host.state(), None);
    let chip = sent.groups[0].rows[0].chip.clone().expect("a chip");
    assert!(chip.ends_with(", including new macro code"), "{chip}");

    let r = review(host.state(), &returned, &master, &master_file, &host.blobs).expect("reviews");
    let wv = r.view.workbook.as_ref().expect("a workbook view");
    assert!(wv.new_macro_code, "the banner shows");
    assert!(NEW_MACRO_CODE.starts_with("This file came back with new macro code"));
    let macros = wv
        .groups
        .iter()
        .find(|g| g.object == Some(Object::Macros))
        .expect("a Macros group");
    // Each module on its own card, by hand, with only Skip.
    assert_eq!(macros.cards.len(), 2, "{:?}", macros.cards);
    for card in &macros.cards {
        let labels: Vec<&str> = card.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Skip"], "{}", card.summary);
        assert!(
            card.by_hand
                .as_deref()
                .is_some_and(|b| b.contains("VBA editor"))
        );
        assert!(!card.code.is_empty());
    }
}

#[test]
fn a_workbook_without_macros_has_no_badge() {
    let mut host = Host::new();
    host.start_workbook(&workbooks::model_master().to_bytes());
    let master = host.master();
    assert_eq!(
        host.state().latest(&master).expect("v1").macros,
        Some(false)
    );
    let entry = crate::library::LibraryEntry::from_facts(crate::library::EntryFacts::of_state(
        host.state(),
    ));
    assert!(entry.badges.is_empty());
}
