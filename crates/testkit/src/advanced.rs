// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The Phase 5B workbooks: Brightwell Analytics' financial model with
//! macros, two pivot tables on one cache with a slicer, and Power Query
//! queries, and the changes each kind can show.
//!
//! The macros are invented and harmless; Lineage never runs them. The sales
//! figures, regions and plans are invented.

use crate::mashup;
use crate::pivots::{CacheField, Pivot, PivotCache, Slicer, ValueSpec};
use crate::vba::{self, Module, ModuleKind, Project};
use crate::workbooks::model_master;
use crate::xlsx::{Connection, Extra, Opaque, QueryTable, Sheet, Style, Workbook, num, text};

pub const REL_VBA: &str = "http://schemas.microsoft.com/office/2006/relationships/vbaProject";
pub const CT_VBA: &str = "application/vnd.ms-office.vbaProject";
pub const REL_VBA_SIGNATURE: &str =
    "http://schemas.microsoft.com/office/2006/relationships/vbaProjectSignature";
pub const CT_VBA_SIGNATURE: &str = "application/vnd.ms-office.vbaProjectSignature";

/// The forecast macro, as the master has it.
pub const FORECAST: &str = "Option Explicit

' Copies the forecast months into the Summary sheet.
Public Sub RefreshForecast()
    Dim months As Integer
    months = 12
    Worksheets(\"Summary\").Range(\"B2\").Value = months
    Application.StatusBar = \"Forecast updated\"
End Sub

Public Function Growth(rate As Double) As Double
    Growth = 1 + rate
End Function";

/// The model's VBA project: workbook and sheet code, a standard module, a
/// class and a form.
#[must_use]
pub fn model_project() -> Project {
    let mut form = Module::new(
        "ScenarioForm",
        ModuleKind::Form,
        "Private Sub OkButton_Click()\n    Me.Hide\nEnd Sub",
    );
    form.layout = "Scenario name, OK".to_owned();
    Project::new(vec![
        Module::new(
            "ThisWorkbook",
            ModuleKind::Workbook,
            "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n    Application.StatusBar = False\nEnd Sub",
        ),
        Module::new("Sheet1", ModuleKind::Sheet, ""),
        Module::new("Forecast", ModuleKind::Standard, FORECAST),
        Module::new(
            "Scenario",
            ModuleKind::Class,
            "Public Name As String\nPublic Growth As Double\n\nPublic Function Label() As String\n    Label = Name & \" (\" & Format(Growth, \"0%\") & \")\"\nEnd Function",
        ),
        form,
    ])
}

/// The same project without the form: a form's designer here is a stand-in
/// (see [`crate::vba`]), so only this one is opened in real Excel.
#[must_use]
pub fn model_project_for_excel() -> Project {
    let mut p = model_project();
    p.modules.retain(|m| m.kind != ModuleKind::Form);
    p
}

/// Puts a VBA project (and optionally a signature) into a workbook.
pub fn set_project(wb: &mut Workbook, project: &Project, signer: Option<&str>) {
    wb.macro_enabled = true;
    wb.opaque.retain(|o| o.rel_type != REL_VBA);
    wb.opaque.push(Opaque {
        name: "xl/vbaProject.bin".to_owned(),
        content_type: CT_VBA.to_owned(),
        rel_type: REL_VBA.to_owned(),
        bytes: project.to_bytes(),
    });
    wb.extra.retain(|e| e.name != "xl/vbaProjectSignature.bin");
    if let Some(signer) = signer {
        wb.extra.push(Extra {
            name: "xl/vbaProjectSignature.bin".to_owned(),
            content_type: Some(CT_VBA_SIGNATURE.to_owned()),
            bytes: vba::signature_part(signer, "Fabrikam Test CA"),
            related_from: Some(("xl/vbaProject.bin".to_owned(), REL_VBA_SIGNATURE.to_owned())),
        });
    }
}

// --- Sales and pivots ----------------------------------------------------------

/// One sale per region, plan and month: (region, plan, month, seats, revenue).
#[must_use]
pub fn sales() -> Vec<[String; 5]> {
    let rows: [(&str, &str, &str, i64); 12] = [
        ("North", "Starter", "Jan", 10),
        ("North", "Team", "Jan", 4),
        ("South", "Starter", "Jan", 6),
        ("South", "Team", "Jan", 2),
        ("West", "Starter", "Jan", 8),
        ("West", "Team", "Jan", 3),
        ("North", "Starter", "Feb", 12),
        ("North", "Team", "Feb", 5),
        ("South", "Starter", "Feb", 7),
        ("South", "Team", "Feb", 2),
        ("West", "Starter", "Feb", 9),
        ("West", "Team", "Feb", 4),
    ];
    rows.iter()
        .map(|(region, plan, month, seats)| {
            let price = if *plan == "Starter" { 29 } else { 99 };
            [
                (*region).to_owned(),
                (*plan).to_owned(),
                (*month).to_owned(),
                seats.to_string(),
                (seats * price).to_string(),
            ]
        })
        .collect()
}

/// Adds the Sales sheet, its pivot cache, two pivot tables on a Pivots
/// sheet and a Region slicer on both.
pub fn add_pivots(wb: &mut Workbook, rows: &[[String; 5]]) {
    let sales_index = wb.sheets.len();
    let next_id = wb.sheets.iter().map(|s| s.sheet_id).max().unwrap_or(0) + 1;
    let mut sales = Sheet::new("Sales", next_id);
    sales.push_row(
        ["Region", "Plan", "Month", "Seats", "Revenue"]
            .iter()
            .map(|h| Some((text(*h), Style::Bold)))
            .collect(),
    );
    for r in rows {
        sales.push_row(vec![
            Some((text(&r[0]), Style::General)),
            Some((text(&r[1]), Style::General)),
            Some((text(&r[2]), Style::General)),
            Some((num(r[3].clone()), Style::General)),
            Some((num(r[4].clone()), Style::General)),
        ]);
    }
    wb.sheets.push(sales);
    let pivots_index = wb.sheets.len();
    wb.sheets.push(Sheet::new("Pivots", next_id + 1));
    wb.pivot_caches.push(PivotCache {
        sheet: "Sales".to_owned(),
        range: format!("A1:E{}", rows.len() + 1),
        fields: vec![
            CacheField::text("Region"),
            CacheField::text("Plan"),
            CacheField::text("Month"),
            CacheField::number("Seats"),
            CacheField::number("Revenue"),
            CacheField::calculated("Revenue per seat", "Revenue/Seats"),
        ],
        records: rows.iter().map(|r| r.to_vec()).collect(),
        refreshed: "46310.5".to_owned(),
    });
    let cache = wb.pivot_caches.len() - 1;
    let _ = sales_index;
    wb.pivots.push(Pivot {
        name: "RevenueByRegion".to_owned(),
        sheet: pivots_index,
        cache,
        at: "A3".to_owned(),
        rows: 0,
        values: vec![
            ValueSpec::sum("Sum of Revenue", 4),
            ValueSpec::sum("Sum of Seats", 3),
        ],
        filter: Some((2, None)),
        hidden: Vec::new(),
        style: "PivotStyleLight16".to_owned(),
        form: crate::pivots::Form::Compact,
    });
    wb.pivots.push(Pivot {
        name: "SeatsByPlan".to_owned(),
        sheet: pivots_index,
        cache,
        at: "A12".to_owned(),
        rows: 1,
        values: vec![ValueSpec::sum("Sum of Seats", 3)],
        filter: None,
        hidden: Vec::new(),
        style: "PivotStyleLight16".to_owned(),
        form: crate::pivots::Form::Compact,
    });
    let first = wb.pivots.len() - 2;
    wb.slicers.push(Slicer {
        name: "Slicer_Region".to_owned(),
        caption: "Region".to_owned(),
        field: 0,
        pivots: vec![first, first + 1],
        sheet: pivots_index,
        selected: None,
    });
}

/// The pivot named `name`.
///
/// # Panics
/// When there is none.
pub fn pivot_mut<'a>(wb: &'a mut Workbook, name: &str) -> &'a mut Pivot {
    wb.pivots
        .iter_mut()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no pivot {name}"))
}

// --- Queries ---------------------------------------------------------------------

pub const RATES: &str = r#"let
    Source = Csv.Document(File.Contents("C:\Shared\rates.csv"), [Delimiter = ",", Columns = 2]),
    Promoted = Table.PromoteHeaders(Source, [PromoteAllScalars = true]),
    Typed = Table.TransformColumnTypes(Promoted, {{"Currency", type text}, {"Rate", type number}})
in
    Typed"#;

pub const REGION_LIST: &str = r#"let
    Source = Excel.CurrentWorkbook(){[Name = "SalesTable"]}[Content],
    Regions = Table.Distinct(Table.SelectColumns(Source, {"Region"}))
in
    Regions"#;

/// The model's queries: Rates loads to its own sheet; RegionList is
/// connection only.
#[must_use]
pub fn model_queries() -> Vec<mashup::Query> {
    let mut rates = mashup::Query::new("Rates", "{6A1F3C2E-4B7D-4E90-8C15-2D9B7E4F1A36}", RATES);
    rates.load = mashup::Load::Sheet("Rates".to_owned());
    rates.last_loaded = Some(("2026-10-14T09:30:00.0000000Z".to_owned(), 3));
    vec![
        rates,
        mashup::Query::new(
            "RegionList",
            "{0C8E2B51-93A4-4F6D-B27E-5A1D9C3F8E02}",
            REGION_LIST,
        ),
    ]
}

/// Adds the queries, and the Rates sheet the first one loads into.
pub fn add_queries(wb: &mut Workbook, queries: &[mashup::Query], rates: &[(&str, &str)]) {
    let next_id = wb.sheets.iter().map(|s| s.sheet_id).max().unwrap_or(0) + 1;
    let mut sheet = Sheet::new("Rates", next_id);
    sheet.push_row(vec![
        Some((text("Currency"), Style::Bold)),
        Some((text("Rate"), Style::Bold)),
    ]);
    for &(c, r) in rates {
        sheet.push_row(vec![
            Some((text(c), Style::General)),
            Some((num(r), Style::General)),
        ]);
    }
    let connection = u32::try_from(wb.connections.len()).unwrap_or(0) + 1;
    sheet.query_table = Some(QueryTable {
        name: "ExternalData_1".to_owned(),
        connection,
        range: format!("A1:B{}", rates.len() + 1),
    });
    wb.sheets.push(sheet);
    wb.connections.push(Connection {
        id: connection,
        name: "Query - Rates".to_owned(),
        source_file: String::new(),
        mashup: Some("Rates".to_owned()),
    });
    wb.extra.retain(|e| !e.name.starts_with("customXml/"));
    wb.extra.extend(mashup::parts(queries));
}

pub const RATES_NOW: [(&str, &str); 3] = [("EUR", "0.92"), ("GBP", "0.79"), ("JPY", "151.3")];

// --- The model, with everything -------------------------------------------------

/// The founder's model with macros, pivots and Power Query queries.
#[must_use]
pub fn model_full() -> Workbook {
    with_queries(&model_queries(), &RATES_NOW)
}

fn with_queries(queries: &[mashup::Query], rates: &[(&str, &str)]) -> Workbook {
    let mut wb = model_master();
    wb.title = "Brightwell Analytics model with macros".to_owned();
    set_project(&mut wb, &model_project(), None);
    add_pivots(&mut wb, &sales());
    add_queries(&mut wb, queries, rates);
    wb
}

/// Rates leaves out rates that aren't positive.
#[must_use]
pub fn model_full_query_edited() -> Workbook {
    let mut q = model_queries();
    q[0].formula = RATES.replace(
        "in\n    Typed",
        "    Positive = Table.SelectRows(Typed, each [Rate] > 0)\nin\n    Positive",
    );
    q[0].formula = q[0].formula.replace(
        "{\"Rate\", type number}})\n",
        "{\"Rate\", type number}}),\n",
    );
    with_queries(&q, &RATES_NOW)
}

/// A new query, Targets.
#[must_use]
pub fn model_full_query_added() -> Workbook {
    let mut q = model_queries();
    q.push(mashup::Query::new(
        "Targets",
        "{9D4E7A10-2C6B-4F38-A5E1-7B3C0D8F2E64}",
        "let\n    Source = #table({\"Region\", \"Target\"}, {{\"North\", 3000}, {\"South\", 1500}})\nin\n    Source",
    ));
    with_queries(&q, &RATES_NOW)
}

/// Rates was refreshed: new rates loaded, no query changed.
#[must_use]
pub fn model_full_query_refreshed() -> Workbook {
    let mut q = model_queries();
    q[0].last_loaded = Some(("2026-10-16T08:05:00.0000000Z".to_owned(), 3));
    with_queries(&q, &[("EUR", "0.94"), ("GBP", "0.78"), ("JPY", "149.8")])
}

/// The master's RefreshForecast now covers 18 months.
#[must_use]
pub fn model_full_macro_edited() -> Workbook {
    let mut p = model_project();
    if let Some(m) = p.module_mut("Forecast") {
        m.code = FORECAST.replace("months = 12", "months = 18");
    }
    let mut wb = model_full();
    set_project(&mut wb, &p, None);
    wb
}

/// The second pivot shows seats by month instead of by plan.
#[must_use]
pub fn model_full_pivot_rows() -> Workbook {
    let mut wb = model_full();
    pivot_mut(&mut wb, "SeatsByPlan").rows = 2;
    wb
}

/// The first pivot's Month filter shows February only.
#[must_use]
pub fn model_full_pivot_filter() -> Workbook {
    let mut wb = model_full();
    pivot_mut(&mut wb, "RevenueByRegion").filter = Some((2, Some("Feb".to_owned())));
    wb
}

/// A February sale was corrected on Sales, and the pivots refreshed.
#[must_use]
pub fn model_full_refreshed() -> Workbook {
    let mut rows = sales();
    rows[11][4] = "420".to_owned();
    let mut wb = model_master();
    wb.title = "Brightwell Analytics model with macros".to_owned();
    set_project(&mut wb, &model_project(), None);
    add_pivots(&mut wb, &rows);
    add_queries(&mut wb, &model_queries(), &RATES_NOW);
    wb.pivot_caches[0].refreshed = "46312.25".to_owned();
    wb
}

/// The slicer shows North and West only.
#[must_use]
pub fn model_full_slicer() -> Workbook {
    let mut wb = model_full();
    wb.slicers[0].selected = Some(vec!["North".to_owned(), "West".to_owned()]);
    wb
}

/// A copy that came back with a new module and a changed macro.
#[must_use]
pub fn model_full_returned_with_macro() -> Workbook {
    let mut p = model_project();
    if let Some(m) = p.module_mut("Forecast") {
        m.code = FORECAST.replace(
            "    Application.StatusBar = \"Forecast updated\"",
            "    Application.StatusBar = \"Forecast updated\"\n    Call SendSummary",
        );
    }
    p.modules.push(Module::new(
        "Mailer",
        ModuleKind::Standard,
        "Public Sub SendSummary()\n    ' Invented: would e-mail the summary.\n    Debug.Print \"summary\"\nEnd Sub",
    ));
    let mut wb = model_full();
    set_project(&mut wb, &p, None);
    wb
}

/// The model's project signed by an invented test publisher.
#[must_use]
pub fn model_full_signed() -> Workbook {
    let mut wb = model_full();
    set_project(&mut wb, &model_project(), Some("Fabrikam Test Signing"));
    wb
}

/// The model with only its macros (no form), for real Excel: the VBA
/// spike's file (decision 0028).
#[must_use]
pub fn model_macros_only() -> Workbook {
    let mut wb = model_master();
    wb.title = "Brightwell Analytics model with macros".to_owned();
    set_project(&mut wb, &model_project_for_excel(), None);
    wb
}

/// The model with only its pivot tables and slicer.
#[must_use]
pub fn model_pivots_only() -> Workbook {
    let mut wb = model_master();
    add_pivots(&mut wb, &sales());
    wb
}

/// The model with only its queries.
#[must_use]
pub fn model_queries_only() -> Workbook {
    let mut wb = model_master();
    add_queries(&mut wb, &model_queries(), &RATES_NOW);
    wb
}

/// The full model without the stand-in form, for real Excel.
#[must_use]
pub fn model_full_for_excel() -> Workbook {
    let mut wb = model_full();
    set_project(&mut wb, &model_project_for_excel(), None);
    wb
}

/// Every Phase 5B workbook, by file name.
#[must_use]
pub fn all() -> Vec<(&'static str, Workbook)> {
    vec![
        ("model-full.xlsm", model_full()),
        ("model-full-macro-edited.xlsm", model_full_macro_edited()),
        ("model-full-pivot-rows.xlsm", model_full_pivot_rows()),
        ("model-full-pivot-filter.xlsm", model_full_pivot_filter()),
        ("model-full-refreshed.xlsm", model_full_refreshed()),
        ("model-full-slicer.xlsm", model_full_slicer()),
        (
            "model-full-returned-macro.xlsm",
            model_full_returned_with_macro(),
        ),
        ("model-full-signed.xlsm", model_full_signed()),
        ("model-full-query-edited.xlsm", model_full_query_edited()),
        ("model-full-query-added.xlsm", model_full_query_added()),
        (
            "model-full-query-refreshed.xlsm",
            model_full_query_refreshed(),
        ),
        ("model-macros-only.xlsm", model_macros_only()),
        ("model-pivots-only.xlsx", model_pivots_only()),
        ("model-queries-only.xlsx", model_queries_only()),
        ("model-full-for-excel.xlsm", model_full_for_excel()),
    ]
}
