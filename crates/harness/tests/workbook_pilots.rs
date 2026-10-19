// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The three Excel pilots, pressed through the panel the way a person would,
//! on in-memory storage and on a real folder. Excel is stood in for by the
//! Rust writer, which carries out an apply plan's steps as Excel's own row
//! inserts and cell writes do (decision 0023).
//!
//! 1. A founder's financial model: versions for Northwind Angels and Harbor
//!    Lane Capital, the master's new price brought into both, and Harbor
//!    Lane's copy coming back with a different churn rate, kept in their
//!    version.
//! 2. A household budget: a version sorted largest first, and the master's
//!    March lines brought into it without undoing the sort.
//! 3. A job tracker: a filtered version, the master's new applications and a
//!    status change, and a change typed while the plan ran, which puts
//!    nothing on record and leaves the workbook as it was.
//!
//! With `LINEAGE_PILOT_DIR` set, the on-disk runs leave their folders there,
//! for the `dev_folder` audit.

use std::path::PathBuf;

use lineage::Timestamp;
use lineage::event::MatchedBy;
use lineage_core::Package;
use lineage_core::docx::props::{read_ids, write_ids};
use lineage_core::merge::Decision;
use lineage_core::xlsx::Workbook;
use lineage_core::xlsx::apply::Write;
use lineage_harness::{LocalFolderStorage, MemoryStorage, Panel, Step, Storage, fake_excel};
use lineage_testkit::workbooks::{self, Assumptions};
use lineage_testkit::xlsx::Workbook as Built;

/// 2026-10-18T09:00:00Z.
const START: Timestamp = Timestamp(1_792_314_000_000);

/// The person edits a workbook in Excel: it becomes `built`, keeping its ids.
fn edit_in_excel<S: Storage>(storage: &mut S, file: &str, built: &Built) {
    let bytes = storage.read_file(file).expect("file");
    storage
        .write_file(file, &with_ids_of(&bytes, built))
        .expect("writes");
}

fn with_ids_of(file: &[u8], built: &Built) -> Vec<u8> {
    let ids = read_ids(&Package::read(file).expect("reads"))
        .expect("reads")
        .expect("ids");
    let mut package = Package::read(&built.to_bytes()).expect("reads");
    write_ids(&mut package, &ids).expect("writes");
    package.to_bytes().expect("bytes")
}

/// What a cell shows in `bytes` (its raw value, as Excel would hold it).
fn shown(bytes: &[u8], sheet: &str, at: &str) -> String {
    let wb = Workbook::open(bytes).expect("opens");
    let i = wb.sheet_index(sheet).expect("sheet");
    let (c, r) = lineage_core::xlsx::refs::parse_cell(at).expect("cell");
    wb.sheets[i]
        .cell(c, r)
        .map(|cell| lineage_core::xlsx::value::either(&wb, wb.sheet_text(i), cell).text())
        .unwrap_or_default()
}

fn at<S: Storage>(panel: &Panel<S>, file: &str, sheet: &str, cell: &str) -> String {
    shown(&panel.storage.read_file(file).expect("file"), sheet, cell)
}

/// Every card on a version's updates panel, brought in.
fn bring_in_all<S: Storage>(panel: &Panel<S>, file: &str) -> Vec<String> {
    let updates = panel.updates(file).expect("updates");
    updates
        .groups
        .iter()
        .flat_map(|g| g.view.changes.iter().map(|c| c.id.clone()))
        .collect()
}

fn bring(ids: &[String]) -> Vec<(&str, Decision)> {
    ids.iter()
        .map(|id| (id.as_str(), Decision::BringIn))
        .collect()
}

// --- 1. The financial model ---------------------------------------------------

const MODEL: &str = "Startup model.xlsx";
const NORTHWIND: &str = "Tailored versions/Startup model – Northwind.xlsx";
const HARBOR: &str = "Tailored versions/Startup model – Harbor Lane.xlsx";
const CAME_BACK: &str = "Startup model – Harbor Lane comments.xlsx";

fn financial_model<S: Storage>(mut storage: S) -> Panel<S> {
    storage
        .write_file(MODEL, &workbooks::model_master().to_bytes())
        .expect("puts the master");
    let mut panel = Panel::new(storage, START);
    panel.start_tracking(MODEL).expect("starts tracking");
    for file in [NORTHWIND, HARBOR] {
        panel.make_tailored_version(file).expect("makes a version");
    }
    edit_in_excel(&mut panel.storage, NORTHWIND, &workbooks::model_northwind());
    panel
        .save_checkpoint(NORTHWIND, "Northwind's seat plan")
        .expect("saves");
    edit_in_excel(
        &mut panel.storage,
        HARBOR,
        &workbooks::model_harbor_lane(workbooks::model_master()),
    );
    panel
        .save_checkpoint(HARBOR, "For Harbor Lane")
        .expect("saves");
    let copy = panel
        .send_a_copy(HARBOR, "Harbor Lane Capital", Some("Series A"), true)
        .expect("sends");
    assert!(copy.ends_with(".xlsx"), "{copy}");

    // The founder changes the price: one input, and everything it moves.
    edit_in_excel(&mut panel.storage, MODEL, &workbooks::model_price_update());
    let changed = panel.show_what_changed(MODEL).expect("what changed");
    let wv = changed.workbook.as_ref().expect("a workbook view");
    assert_eq!(wv.groups[0].title, "Inputs");
    assert_eq!(wv.groups[0].cards.len(), 1, "one input changed");
    assert_eq!(wv.groups[0].cards[0].summary, "'$49' changed to '$55'");
    assert!(
        wv.groups.iter().all(|g| g.title != "Formulas"),
        "no formula changed"
    );
    panel.save_checkpoint(MODEL, "New price").expect("saves");
    panel.offer_to_versions().expect("offers");

    // Northwind brings it in; their own seat plan stays.
    let ids = bring_in_all(&panel, NORTHWIND);
    assert_eq!(ids.len(), 1);
    let plan = panel
        .bring_into_workbook(NORTHWIND, &bring(&ids), fake_excel)
        .expect("brings in");
    assert_eq!(plan.text, "Brings in 1 change");
    assert_eq!(at(&panel, NORTHWIND, "Assumptions", "B4"), "55");
    assert_eq!(at(&panel, NORTHWIND, "Assumptions", "B3"), "12");
    assert_eq!(
        panel.updates(NORTHWIND).expect("updates").headline,
        "You're up to date with the master"
    );

    // Harbor Lane's version too; its own sheet stays.
    let ids = bring_in_all(&panel, HARBOR);
    panel
        .bring_into_workbook(HARBOR, &bring(&ids), fake_excel)
        .expect("brings in");
    assert_eq!(at(&panel, HARBOR, "Assumptions", "B4"), "55");
    assert_eq!(at(&panel, HARBOR, "Use of funds", "A2"), "Hiring");

    // Harbor Lane sends their copy back with a different churn rate.
    let sent = panel.storage.read_file(&copy).expect("copy");
    let theirs = workbooks::model_harbor_lane(workbooks::financial_model(&Assumptions {
        churn: "0.03".to_owned(),
        ..Assumptions::default()
    }));
    panel
        .storage
        .write_file(CAME_BACK, &with_ids_of(&sent, &theirs))
        .expect("arrives");
    let (found, view) = panel.a_file_came_back(CAME_BACK).expect("recognises");
    assert!(found.by_ids);
    assert_eq!(view.headline, "This came back from Harbor Lane Capital");
    let returned = panel
        .record_returned(CAME_BACK, &found.candidates[0], MatchedBy::Ids)
        .expect("records");
    let review = panel.returned_review(HARBOR, &returned).expect("review");
    let wv = review.view.workbook.as_ref().expect("a workbook view");
    let card = &wv.groups[0].cards[0];
    assert_eq!(card.summary, "'2.00%' changed to '3.00%'");
    let kept = card.ids.clone();
    panel
        .keep_into_workbook(HARBOR, &returned, &bring(&kept), fake_excel)
        .expect("keeps");
    assert_eq!(at(&panel, HARBOR, "Assumptions", "B5"), "0.03");
    assert_eq!(
        at(&panel, HARBOR, "Assumptions", "B4"),
        "55",
        "the new price stays"
    );
    assert_eq!(
        panel
            .returned_review(HARBOR, &returned)
            .expect("review")
            .headline,
        "Nothing left to review"
    );
    // What came back is kept exactly as it came.
    assert_eq!(
        panel.open_what_came_back(&returned).expect("opens"),
        panel.storage.read_file(CAME_BACK).expect("file")
    );
    panel
}

#[test]
fn the_financial_model_in_memory() {
    financial_model(MemoryStorage::default());
}

#[test]
fn the_financial_model_on_disk() {
    let dir = temp("model");
    let panel = financial_model(LocalFolderStorage::new(&dir).expect("folder"));
    let bytes = std::fs::read(dir.join(NORTHWIND)).expect("on disk");
    assert_eq!(shown(&bytes, "Assumptions", "B4"), "55");
    drop(panel);
    done(&dir);
}

// --- 2. The household budget --------------------------------------------------

const BUDGET: &str = "Budget 2026.xlsx";
const BROKER: &str = "Tailored versions/Budget 2026 – Mortgage broker.xlsx";

fn budget<S: Storage>(mut storage: S) -> Panel<S> {
    storage
        .write_file(BUDGET, &workbooks::budget_master().to_bytes())
        .expect("puts the master");
    let mut panel = Panel::new(storage, START);
    panel.start_tracking(BUDGET).expect("starts tracking");
    panel
        .make_tailored_version(BROKER)
        .expect("makes a version");

    // The broker's version is sorted largest first: one change, not all.
    edit_in_excel(&mut panel.storage, BROKER, &workbooks::budget_sorted());
    let changed = panel.show_what_changed(BROKER).expect("what changed");
    let wv = changed.workbook.as_ref().expect("a workbook view");
    let cards: Vec<&str> = wv
        .groups
        .iter()
        .flat_map(|g| g.cards.iter().map(|c| c.summary.as_str()))
        .collect();
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert_eq!(
        cards[0],
        "Sorted the rows on Transactions by Amount, largest first"
    );
    panel
        .save_checkpoint(BROKER, "Largest first")
        .expect("saves");

    // March in the master.
    edit_in_excel(&mut panel.storage, BUDGET, &workbooks::budget_added());
    panel.save_checkpoint(BUDGET, "March").expect("saves");
    panel.offer_to_versions().expect("offers");
    let updates = panel.updates(BROKER).expect("updates");
    let wv = updates.groups[0]
        .view
        .workbook
        .as_ref()
        .expect("a workbook view");
    let rows = wv
        .groups
        .iter()
        .flat_map(|g| &g.cards)
        .find(|c| c.summary.starts_with("Added rows"))
        .expect("the March lines");
    assert_eq!(
        rows.why.as_deref(),
        Some(lineage_core::xlsx::view::ROWS_FORMATTED)
    );
    let ids = bring_in_all(&panel, BROKER);
    assert!(!ids.is_empty());
    panel
        .bring_into_workbook(BROKER, &bring(&ids), fake_excel)
        .expect("brings in");
    let file = panel.storage.read_file(BROKER).expect("file");
    let wb = Workbook::open(&file).expect("opens");
    let i = wb.sheet_index("Transactions").expect("sheet");
    let descriptions: Vec<String> = (2..=20)
        .map(|r| shown(&file, "Transactions", &format!("C{r}")))
        .collect();
    assert!(
        descriptions.iter().any(|d| d == "March rent"),
        "{descriptions:?}"
    );
    assert_eq!(wb.sheets[i].rows.len(), 20, "header and 19 lines");
    assert_eq!(
        shown(&file, "Transactions", "C2"),
        "January rent",
        "the broker's order stays: largest first"
    );
    assert_eq!(
        panel.updates(BROKER).expect("updates").headline,
        "You're up to date with the master"
    );
    panel
}

#[test]
fn the_budget_in_memory() {
    budget(MemoryStorage::default());
}

#[test]
fn the_budget_on_disk() {
    let dir = temp("budget");
    drop(budget(LocalFolderStorage::new(&dir).expect("folder")));
    done(&dir);
}

// --- 3. The job tracker -------------------------------------------------------

const TRACKER: &str = "Job search.xlsx";
const COACH: &str = "Tailored versions/Job search – Coach.xlsx";

fn tracker<S: Storage>(mut storage: S) -> Panel<S> {
    storage
        .write_file(TRACKER, &workbooks::tracker_master().to_bytes())
        .expect("puts the master");
    let mut panel = Panel::new(storage, START);
    panel.start_tracking(TRACKER).expect("starts tracking");
    panel.make_tailored_version(COACH).expect("makes a version");
    edit_in_excel(&mut panel.storage, COACH, &workbooks::tracker_filtered());
    panel
        .save_checkpoint(COACH, "Only the live ones")
        .expect("saves");

    edit_in_excel(&mut panel.storage, TRACKER, &workbooks::tracker_updated());
    panel
        .save_checkpoint(TRACKER, "Two new applications")
        .expect("saves");
    panel.offer_to_versions().expect("offers");
    let ids = bring_in_all(&panel, COACH);
    assert!(!ids.is_empty());

    // The person types into the workbook while the plan runs: nothing is
    // recorded, and the workbook is as it was.
    let before = panel.storage.read_file(COACH).expect("file");
    let checkpoints = panel.state().expect("state").checkpoints.len();
    let err = panel
        .bring_into_workbook(COACH, &bring(&ids), |bytes, steps| {
            let held = fake_excel(bytes, steps)?;
            fake_excel(
                &held,
                &[Step::Set {
                    sheet: "Applications".to_owned(),
                    col: 6,
                    row: 2,
                    value: Write::Text("Send thank-you note".to_owned()),
                }],
            )
        })
        .expect_err("didn't land");
    assert!(err.to_string().contains("land"), "{err}");
    assert_eq!(panel.storage.read_file(COACH).expect("file"), before);
    assert_eq!(panel.state().expect("state").checkpoints.len(), checkpoints);

    // Again, left alone this time.
    panel
        .bring_into_workbook(COACH, &bring(&ids), fake_excel)
        .expect("brings in");
    let file = panel.storage.read_file(COACH).expect("file");
    assert_eq!(shown(&file, "Applications", "E3"), "Interview");
    let wb = Workbook::open(&file).expect("opens");
    let i = wb.sheet_index("Applications").expect("sheet");
    assert!(wb.sheets[i].auto_filter.is_some(), "the filter stays");
    assert_eq!(
        panel.updates(COACH).expect("updates").headline,
        "You're up to date with the master"
    );
    panel
}

#[test]
fn the_tracker_in_memory() {
    tracker(MemoryStorage::default());
}

#[test]
fn the_tracker_on_disk() {
    let dir = temp("tracker");
    drop(tracker(LocalFolderStorage::new(&dir).expect("folder")));
    done(&dir);
}

fn temp(name: &str) -> PathBuf {
    let dir = match std::env::var_os("LINEAGE_PILOT_DIR") {
        Some(root) => PathBuf::from(root).join(name),
        None => std::env::temp_dir().join(format!("lineage-{name}-{}", std::process::id())),
    };
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn done(dir: &std::path::Path) {
    if std::env::var_os("LINEAGE_PILOT_DIR").is_none() {
        let _ = std::fs::remove_dir_all(dir);
    }
}
