// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The pilot workbooks of Phase 5, built from invented data:
//!
//! - **Brightwell Analytics' financial model**, with investor versions for
//!   Harbor Lane Capital and Northwind Angels;
//! - **a household budget** kept by month, where rows are added, sorted and
//!   recategorised;
//! - **a job-application tracker**, filtered, sorted and added to;
//! - large workbooks (50k and 200k rows), many sheets, a data connection, and
//!   a sheet of awkward cells.
//!
//! Cached results are computed here with exact decimal arithmetic, so a
//! workbook's figures agree with its formulas. Every company, person and
//! figure is invented.

use crate::xlsx::{
    Cached, Comment, Connection, Name, Opaque, QueryTable, Sheet, SheetState, Style, Table, Value,
    Workbook, a1, formula, formula_num, name, num, text,
};

/// Where `cargo xtask fixtures` writes these.
pub const OUT_DIR: &str = "fixtures/xlsx/synthetic";

/// An exact decimal: `v / 10^scale`. Enough for the fixtures' sums and
/// products; never a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dec {
    pub v: i128,
    pub scale: u32,
}

impl Dec {
    #[must_use]
    pub const fn int(v: i128) -> Self {
        Self { v, scale: 0 }
    }

    /// Parses a plain decimal such as "0.02" or "-12.5".
    #[must_use]
    pub fn parse(s: &str) -> Self {
        let (int, frac) = s.split_once('.').unwrap_or((s, ""));
        let digits = format!("{int}{frac}");
        Self {
            v: digits.parse().unwrap_or(0),
            scale: u32::try_from(frac.len()).unwrap_or(0),
        }
    }

    fn at_scale(self, scale: u32) -> i128 {
        self.v * 10i128.pow(scale - self.scale)
    }

    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        let scale = self.scale.max(other.scale);
        Self {
            v: self.at_scale(scale) + other.at_scale(scale),
            scale,
        }
        .trim()
    }

    #[must_use]
    pub fn minus(self, other: Self) -> Self {
        self.plus(Self {
            v: -other.v,
            scale: other.scale,
        })
    }

    #[must_use]
    pub fn times(self, other: Self) -> Self {
        Self {
            v: self.v * other.v,
            scale: self.scale + other.scale,
        }
        .trim()
    }

    fn trim(mut self) -> Self {
        while self.scale > 0 && self.v % 10 == 0 {
            self.v /= 10;
            self.scale -= 1;
        }
        self
    }

    /// The shortest lexeme, as Excel writes it: "5762.4", "120", "-0.5".
    #[must_use]
    pub fn lexeme(self) -> String {
        let t = self.trim();
        if t.scale == 0 {
            return t.v.to_string();
        }
        let negative = t.v < 0;
        let digits = t.v.unsigned_abs().to_string();
        let scale = t.scale as usize;
        let padded = if digits.len() <= scale {
            format!("{}{digits}", "0".repeat(scale - digits.len() + 1))
        } else {
            digits
        };
        let (int, frac) = padded.split_at(padded.len() - scale);
        format!("{}{int}.{frac}", if negative { "-" } else { "" })
    }
}

fn bold(t: &str) -> Option<(Value, Style)> {
    Some((text(t), Style::Bold))
}

fn plain(t: &str) -> Option<(Value, Style)> {
    Some((text(t), Style::General))
}

fn cell(value: Value, style: Style) -> Option<(Value, Style)> {
    Some((value, style))
}

fn cached(d: Dec) -> Option<Cached> {
    Some(Cached::Number(d.lexeme()))
}

// --- The financial model -----------------------------------------------------

/// The model's assumptions, as the file stores them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assumptions {
    pub start_seats: i128,
    pub new_seats: i128,
    pub price: String,
    pub churn: String,
    pub cogs_share: String,
    pub opex: i128,
    /// Hosting per month, added to COGS when set (the master's later
    /// formula change).
    pub hosting: Option<i128>,
}

impl Default for Assumptions {
    fn default() -> Self {
        Self {
            start_seats: 120,
            new_seats: 15,
            price: "49".to_owned(),
            churn: "0.02".to_owned(),
            cogs_share: "0.18".to_owned(),
            opex: 85_000,
            hosting: None,
        }
    }
}

/// Brightwell Analytics' 24-month model: Assumptions, Revenue, P&L and
/// Summary, with named assumptions and cross-sheet formulas.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn financial_model(a: &Assumptions) -> Workbook {
    let mut wb = Workbook::new("Brightwell Analytics model");

    let mut inputs = Sheet::new("Assumptions", 1);
    inputs.cols = vec![(1, 1, "28".to_owned()), (2, 2, "14".to_owned())];
    inputs.push_row(vec![bold("Assumption"), bold("Value")]);
    inputs.push_row(vec![
        plain("Starting seats"),
        cell(num(a.start_seats.to_string()), Style::General),
    ]);
    inputs.push_row(vec![
        plain("New seats per month"),
        cell(num(a.new_seats.to_string()), Style::General),
    ]);
    inputs.push_row(vec![
        plain("Price per seat"),
        cell(num(a.price.clone()), Style::Currency),
    ]);
    inputs.push_row(vec![
        plain("Monthly churn"),
        cell(num(a.churn.clone()), Style::Percent2),
    ]);
    inputs.push_row(vec![
        plain("COGS share"),
        cell(num(a.cogs_share.clone()), Style::Percent),
    ]);
    inputs.push_row(vec![
        plain("Monthly opex"),
        cell(num(a.opex.to_string()), Style::Currency),
    ]);
    if let Some(hosting) = a.hosting {
        inputs.push_row(vec![
            plain("Hosting per month"),
            cell(num(hosting.to_string()), Style::Currency),
        ]);
    }
    inputs.comments.push(Comment {
        cell: "B3".to_owned(),
        author: "Dana Whitfield".to_owned(),
        text: "Based on the Q3 pipeline.".to_owned(),
    });
    wb.names = vec![
        name("Churn", "Assumptions!$B$5"),
        name("CogsShare", "Assumptions!$B$6"),
        name("NewSeats", "Assumptions!$B$3"),
        name("Opex", "Assumptions!$B$7"),
        name("Price", "Assumptions!$B$4"),
        name("StartSeats", "Assumptions!$B$2"),
    ];
    if a.hosting.is_some() {
        wb.names.push(name("Hosting", "Assumptions!$B$8"));
    }
    wb.sheets.push(inputs);

    let price = Dec::parse(&a.price);
    let keep = Dec::int(1).minus(Dec::parse(&a.churn));
    let cogs_share = Dec::parse(&a.cogs_share);
    let hosting = a.hosting.map(Dec::int);

    let mut revenue = Sheet::new("Revenue", 2);
    revenue.freeze_header = true;
    revenue.push_row(vec![
        bold("Month"),
        bold("Seats"),
        bold("MRR"),
        bold("COGS"),
        bold("Gross margin"),
    ]);
    let mut seats = Dec::int(a.start_seats);
    let mut mrr_by_month = Vec::new();
    let mut cogs_by_month = Vec::new();
    for month in 1..=24u32 {
        let r = month + 1;
        if month > 1 {
            seats = seats.plus(Dec::int(a.new_seats));
        }
        let mrr = seats.times(price).times(keep);
        let cogs = match hosting {
            Some(h) => mrr.times(cogs_share).plus(h),
            None => mrr.times(cogs_share),
        };
        let margin = mrr.minus(cogs);
        mrr_by_month.push(mrr);
        cogs_by_month.push(cogs);
        let seats_cell = if month == 1 {
            formula("StartSeats", cached(seats))
        } else if month == 2 {
            Value::SharedMaster {
                f: "B2+NewSeats".to_owned(),
                si: 0,
                range: "B3:B25".to_owned(),
                cached: cached(seats),
            }
        } else {
            Value::Shared {
                si: 0,
                cached: cached(seats),
            }
        };
        let cogs_formula = if hosting.is_some() {
            "C2*CogsShare+Hosting"
        } else {
            "C2*CogsShare"
        };
        let shared = |si: u32, f: &str, col: &str, value: Dec| {
            if month == 1 {
                Value::SharedMaster {
                    f: f.to_owned(),
                    si,
                    range: format!("{col}2:{col}25"),
                    cached: cached(value),
                }
            } else {
                Value::Shared {
                    si,
                    cached: cached(value),
                }
            }
        };
        revenue.put_row(
            r,
            vec![
                cell(num(month.to_string()), Style::General),
                cell(seats_cell, Style::Thousands),
                cell(shared(1, "B2*Price*(1-Churn)", "C", mrr), Style::Money),
                cell(shared(2, cogs_formula, "D", cogs), Style::Money),
                cell(shared(3, "C2-D2", "E", margin), Style::Money),
            ],
        );
    }
    wb.sheets.push(revenue);

    let sum = |v: &[Dec]| v.iter().fold(Dec::int(0), |acc, d| acc.plus(*d));
    let rev_y1 = sum(&mrr_by_month[..12]);
    let rev_y2 = sum(&mrr_by_month[12..]);
    let cogs_y1 = sum(&cogs_by_month[..12]);
    let cogs_y2 = sum(&cogs_by_month[12..]);
    let gm_y1 = rev_y1.minus(cogs_y1);
    let gm_y2 = rev_y2.minus(cogs_y2);
    let opex_y = Dec::int(a.opex * 12);
    let ebitda_y1 = gm_y1.minus(opex_y);
    let ebitda_y2 = gm_y2.minus(opex_y);

    let mut pl = Sheet::new("P&L", 3);
    pl.push_row(vec![None, bold("Year 1"), bold("Year 2")]);
    pl.push_row(vec![
        plain("Revenue"),
        cell(
            formula_num("SUM(Revenue!C2:C13)", rev_y1.lexeme()),
            Style::Currency,
        ),
        cell(
            formula_num("SUM(Revenue!C14:C25)", rev_y2.lexeme()),
            Style::Currency,
        ),
    ]);
    pl.push_row(vec![
        plain("COGS"),
        cell(
            formula_num("SUM(Revenue!D2:D13)", cogs_y1.lexeme()),
            Style::Currency,
        ),
        cell(
            formula_num("SUM(Revenue!D14:D25)", cogs_y2.lexeme()),
            Style::Currency,
        ),
    ]);
    pl.push_row(vec![
        plain("Gross margin"),
        cell(formula_num("B2-B3", gm_y1.lexeme()), Style::Currency),
        cell(formula_num("C2-C3", gm_y2.lexeme()), Style::Currency),
    ]);
    pl.push_row(vec![
        plain("Operating expenses"),
        cell(formula_num("Opex*12", opex_y.lexeme()), Style::Currency),
        cell(formula_num("Opex*12", opex_y.lexeme()), Style::Currency),
    ]);
    pl.push_row(vec![
        bold("EBITDA"),
        cell(formula_num("B4-B5", ebitda_y1.lexeme()), Style::Currency),
        cell(formula_num("C4-C5", ebitda_y2.lexeme()), Style::Currency),
    ]);
    wb.sheets.push(pl);

    let mut summary = Sheet::new("Summary", 4);
    summary.cols = vec![(1, 1, "24".to_owned()), (2, 2, "16".to_owned())];
    summary.push_row(vec![bold("Brightwell Analytics: summary"), None]);
    summary.merged.push("A1:B1".to_owned());
    summary.push_row(vec![
        plain("Year 1 revenue"),
        cell(formula_num("'P&L'!B2", rev_y1.lexeme()), Style::Currency),
    ]);
    summary.push_row(vec![
        plain("Year 2 revenue"),
        cell(formula_num("'P&L'!C2", rev_y2.lexeme()), Style::Currency),
    ]);
    summary.push_row(vec![
        plain("Year 2 EBITDA"),
        cell(formula_num("'P&L'!C6", ebitda_y2.lexeme()), Style::Currency),
    ]);
    summary.push_row(vec![
        plain("Seats at month 24"),
        cell(formula_num("Revenue!B25", seats.lexeme()), Style::Thousands),
    ]);
    summary.push_row(vec![
        plain("Prepared on"),
        cell(formula_num("TODAY()", "46023"), Style::Date),
    ]);
    summary
        .conditional
        .push(("B4".to_owned(), "B4<0".to_owned()));
    wb.sheets.push(summary);
    wb
}

/// The master as it starts.
#[must_use]
pub fn model_master() -> Workbook {
    financial_model(&Assumptions::default())
}

/// The master after the founder updates the price (an input change that moves
/// most calculated cells).
#[must_use]
pub fn model_price_update() -> Workbook {
    financial_model(&Assumptions {
        price: "55".to_owned(),
        ..Assumptions::default()
    })
}

/// The master after hosting is added to COGS (a formula change in 24 cells,
/// plus a new assumption row and name).
#[must_use]
pub fn model_hosting() -> Workbook {
    financial_model(&Assumptions {
        hosting: Some(1200),
        ..Assumptions::default()
    })
}

/// Harbor Lane Capital's version: who it's for on the summary, and a Use of
/// funds sheet.
#[must_use]
pub fn model_harbor_lane(mut wb: Workbook) -> Workbook {
    let summary = wb.sheet_mut("Summary");
    let r = summary.rows.last().map_or(1, |row| row.r) + 1;
    summary.put_row(r, vec![plain("Prepared for"), plain("Harbor Lane Capital")]);
    let mut funds = Sheet::new("Use of funds", 5);
    funds.push_row(vec![bold("Use"), bold("Share")]);
    for (use_, share) in [("Hiring", "0.6"), ("Marketing", "0.25"), ("Runway", "0.15")] {
        funds.push_row(vec![plain(use_), cell(num(share), Style::Percent)]);
    }
    funds.push_row(vec![
        bold("Total"),
        cell(formula_num("SUM(B2:B4)", "1"), Style::Percent),
    ]);
    wb.sheets.push(funds);
    wb
}

/// Northwind Angels' version: a more cautious seat plan (its own input change
/// to the cell the master may change too).
#[must_use]
pub fn model_northwind() -> Workbook {
    financial_model(&Assumptions {
        new_seats: 12,
        ..Assumptions::default()
    })
}

// --- The household budget ----------------------------------------------------

/// One line of the budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Txn {
    /// Excel date serial.
    pub date: u32,
    pub category: &'static str,
    pub description: &'static str,
    /// The amount's lexeme, such as "1450" or "62.35".
    pub amount: &'static str,
}

pub const CATEGORIES: [&str; 6] = [
    "Rent",
    "Groceries",
    "Transport",
    "Utilities",
    "Eating out",
    "Savings",
];

/// January and February 2026 (serial 46023 is 1 January 2026).
#[must_use]
pub fn budget_transactions() -> Vec<Txn> {
    let t = |date, category, description, amount| Txn {
        date,
        category,
        description,
        amount,
    };
    vec![
        t(46023, "Rent", "January rent", "1450"),
        t(46025, "Groceries", "Fernhill Market", "62.35"),
        t(46027, "Transport", "Monthly rail pass", "118"),
        t(46030, "Utilities", "Electricity", "74.2"),
        t(46031, "Eating out", "Lunch with Priya", "23.5"),
        t(46034, "Groceries", "Fernhill Market", "48.9"),
        t(46038, "Savings", "Rainy-day fund", "300"),
        t(46041, "Groceries", "Corner shop", "12.75"),
        t(46045, "Utilities", "Water", "31.6"),
        t(46050, "Eating out", "Birthday dinner", "86"),
        t(46054, "Rent", "February rent", "1450"),
        t(46056, "Groceries", "Fernhill Market", "71.1"),
        t(46058, "Transport", "Monthly rail pass", "118"),
        t(46062, "Utilities", "Electricity", "69.85"),
        t(46066, "Savings", "Rainy-day fund", "300"),
        t(46070, "Groceries", "Fernhill Market", "55.4"),
    ]
}

/// Renders the budget: a Transactions table and a Summary by category.
#[must_use]
pub fn budget(txns: &[Txn]) -> Workbook {
    let mut wb = Workbook::new("Household budget 2026");
    let mut sheet = Sheet::new("Transactions", 1);
    sheet.freeze_header = true;
    sheet.cols = vec![(1, 1, "12".to_owned()), (3, 3, "22".to_owned())];
    let header = ["Date", "Category", "Description", "Amount"];
    sheet.push_row(header.iter().map(|h| bold(h)).collect());
    for txn in txns {
        sheet.push_row(vec![
            cell(num(txn.date.to_string()), Style::Date),
            plain(txn.category),
            plain(txn.description),
            cell(num(txn.amount), Style::Money),
        ]);
    }
    let last = u32::try_from(txns.len()).unwrap_or(0) + 1;
    sheet.tables.push(Table {
        name: "Transactions".to_owned(),
        range: format!("A1:D{last}"),
        columns: header.iter().map(|h| (*h).to_owned()).collect(),
    });
    wb.sheets.push(sheet);

    let mut summary = Sheet::new("Summary", 2);
    summary.push_row(vec![bold("Category"), bold("Spent")]);
    let mut total = Dec::int(0);
    for category in CATEGORIES {
        let spent = txns
            .iter()
            .filter(|t| t.category == category)
            .fold(Dec::int(0), |acc, t| acc.plus(Dec::parse(t.amount)));
        total = total.plus(spent);
        let r = summary.rows.len() + 1;
        summary.push_row(vec![
            plain(category),
            cell(
                formula_num(
                    format!("SUMIFS(Transactions[Amount],Transactions[Category],A{r})"),
                    spent.lexeme(),
                ),
                Style::Money,
            ),
        ]);
    }
    summary.push_row(vec![
        bold("Total"),
        cell(formula_num("SUM(B2:B7)", total.lexeme()), Style::Money),
    ]);
    wb.sheets.push(summary);
    wb
}

#[must_use]
pub fn budget_master() -> Workbook {
    budget(&budget_transactions())
}

/// Three March lines added at the end.
#[must_use]
pub fn budget_added() -> Workbook {
    let mut txns = budget_transactions();
    txns.push(Txn {
        date: 46082,
        category: "Rent",
        description: "March rent",
        amount: "1450",
    });
    txns.push(Txn {
        date: 46084,
        category: "Groceries",
        description: "Fernhill Market",
        amount: "66.2",
    });
    txns.push(Txn {
        date: 46086,
        category: "Transport",
        description: "Monthly rail pass",
        amount: "118",
    });
    budget(&txns)
}

/// The same lines, sorted by amount, largest first (ties by date).
#[must_use]
pub fn budget_sorted() -> Workbook {
    let mut txns = budget_transactions();
    txns.sort_by(|a, b| {
        let (da, db) = (Dec::parse(a.amount), Dec::parse(b.amount));
        let scale = da.scale.max(db.scale);
        db.at_scale(scale)
            .cmp(&da.at_scale(scale))
            .then(a.date.cmp(&b.date))
    });
    budget(&txns)
}

/// Lunch with Priya moved from Eating out to Groceries (recategorised).
#[must_use]
pub fn budget_recategorised() -> Workbook {
    let mut txns = budget_transactions();
    for txn in &mut txns {
        if txn.description == "Lunch with Priya" {
            txn.category = "Groceries";
        }
    }
    budget(&txns)
}

// --- The job tracker ---------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Application {
    pub id: u32,
    pub company: &'static str,
    pub role: &'static str,
    pub applied: u32,
    pub status: &'static str,
    pub next: &'static str,
}

#[must_use]
pub fn applications() -> Vec<Application> {
    let a = |id, company, role, applied, status, next| Application {
        id,
        company,
        role,
        applied,
        status,
        next,
    };
    vec![
        a(
            1,
            "Quillstone Labs",
            "Data analyst",
            46024,
            "Interview",
            "Prepare case study",
        ),
        a(
            2,
            "Marrow & Finch",
            "Analytics engineer",
            46026,
            "Applied",
            "Follow up 20 Jan",
        ),
        a(3, "Tidewater Health", "BI developer", 46028, "Rejected", ""),
        a(
            4,
            "Copperleaf Energy",
            "Data analyst",
            46031,
            "Applied",
            "Ask Sam for referral",
        ),
        a(
            5,
            "Orchard Row",
            "Product analyst",
            46033,
            "Offer",
            "Decide by 14 Feb",
        ),
        a(6, "Kestrel Freight", "Data engineer", 46035, "Rejected", ""),
        a(
            7,
            "Bramblewood",
            "Analyst",
            46038,
            "Interview",
            "Second round 3 Feb",
        ),
        a(
            8,
            "Saltmarsh Media",
            "Insights analyst",
            46040,
            "Applied",
            "",
        ),
    ]
}

/// Renders the tracker. With `filtered`, rejected applications are hidden by
/// the autoFilter, as Excel saves a filtered sheet.
#[must_use]
pub fn tracker(apps: &[Application], filtered: bool) -> Workbook {
    let mut wb = Workbook::new("Job search 2026");
    let mut sheet = Sheet::new("Applications", 1);
    sheet.freeze_header = true;
    sheet.cols = vec![(2, 3, "22".to_owned()), (6, 6, "26".to_owned())];
    let header = ["ID", "Company", "Role", "Applied", "Status", "Next step"];
    sheet.push_row(header.iter().map(|h| bold(h)).collect());
    for app in apps {
        let r = sheet.push_row(vec![
            plain(&format!("APP-{:03}", app.id)),
            plain(app.company),
            plain(app.role),
            cell(num(app.applied.to_string()), Style::Date),
            plain(app.status),
            if app.next.is_empty() {
                None
            } else {
                plain(app.next)
            },
        ]);
        if filtered
            && app.status == "Rejected"
            && let Some(row) = sheet.rows.iter_mut().find(|row| row.r == r)
        {
            row.hidden = true;
        }
    }
    let last = u32::try_from(apps.len()).unwrap_or(0) + 1;
    sheet.auto_filter = Some(format!("A1:F{last}"));
    wb.sheets.push(sheet);

    let mut counts = Sheet::new("Counts", 2);
    counts.push_row(vec![bold("Status"), bold("Count")]);
    for status in ["Applied", "Interview", "Offer", "Rejected"] {
        let n = apps.iter().filter(|a| a.status == status).count();
        let r = counts.rows.len() + 1;
        counts.push_row(vec![
            plain(status),
            cell(
                formula_num(format!("COUNTIF(Applications!E:E,A{r})"), n.to_string()),
                Style::General,
            ),
        ]);
    }
    wb.sheets.push(counts);
    wb
}

#[must_use]
pub fn tracker_master() -> Workbook {
    tracker(&applications(), false)
}

#[must_use]
pub fn tracker_filtered() -> Workbook {
    tracker(&applications(), true)
}

#[must_use]
pub fn tracker_sorted() -> Workbook {
    let mut apps = applications();
    apps.sort_by_key(|a| a.company);
    tracker(&apps, false)
}

/// Two new applications and a status change.
#[must_use]
pub fn tracker_updated() -> Workbook {
    let mut apps = applications();
    for app in &mut apps {
        if app.id == 2 {
            app.status = "Interview";
            app.next = "Call with hiring manager";
        }
    }
    apps.push(Application {
        id: 9,
        company: "Lanternfish",
        role: "Data analyst",
        applied: 46042,
        status: "Applied",
        next: "",
    });
    apps.push(Application {
        id: 10,
        company: "Greyhaven Bank",
        role: "Risk analyst",
        applied: 46043,
        status: "Applied",
        next: "Tailor cover letter",
    });
    tracker(&apps, false)
}

// --- Large workbooks ---------------------------------------------------------

const REGIONS: [&str; 5] = ["North", "South", "East", "West", "Central"];

/// A deterministic pseudo-random sequence (xorshift), so large fixtures are
/// the same on every run.
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// One order line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    pub id: u32,
    pub customer: u32,
    pub region: &'static str,
    pub qty: u32,
    /// Unit price in cents.
    pub cents: u32,
}

#[must_use]
pub fn orders(rows: u32) -> Vec<Order> {
    let mut state = 0x9E37_79B9_7F4A_7C15;
    (1..=rows)
        .map(|id| {
            let a = next(&mut state);
            Order {
                id,
                customer: u32::try_from(a % 5000).unwrap_or(0) + 1,
                region: REGIONS[usize::try_from((a >> 16) % 5).unwrap_or(0)],
                qty: u32::try_from((a >> 24) % 40).unwrap_or(0) + 1,
                cents: u32::try_from((a >> 32) % 90_000).unwrap_or(0) + 100,
            }
        })
        .collect()
}

fn cents(c: i128) -> Dec {
    Dec { v: c, scale: 2 }.trim()
}

/// An Orders sheet of `orders` (ID, Customer, Region, Quantity, Unit price and
/// a shared Total formula), with a Summary by region.
#[must_use]
pub fn orders_workbook(orders: &[Order]) -> Workbook {
    let mut wb = Workbook::new("Orders");
    let mut sheet = Sheet::new("Orders", 1);
    sheet.freeze_header = true;
    let header = [
        "ID",
        "Customer",
        "Region",
        "Quantity",
        "Unit price",
        "Total",
    ];
    sheet.push_row(header.iter().map(|h| bold(h)).collect());
    let last = u32::try_from(orders.len()).unwrap_or(0) + 1;
    let mut by_region = [Dec::int(0); 5];
    for (n, order) in orders.iter().enumerate() {
        let total = cents(i128::from(order.qty) * i128::from(order.cents));
        let region = REGIONS.iter().position(|r| *r == order.region).unwrap_or(0);
        by_region[region] = by_region[region].plus(total);
        let total_cell = if n == 0 {
            Value::SharedMaster {
                f: "D2*E2".to_owned(),
                si: 0,
                range: format!("F2:F{last}"),
                cached: cached(total),
            }
        } else {
            Value::Shared {
                si: 0,
                cached: cached(total),
            }
        };
        sheet.rows.push(crate::xlsx::Row {
            r: u32::try_from(n).unwrap_or(0) + 2,
            hidden: false,
            cells: vec![
                crate::xlsx::Cell {
                    col: 1,
                    value: text(format!("ORD-{:06}", order.id)),
                    style: Style::General,
                },
                crate::xlsx::Cell {
                    col: 2,
                    value: text(format!("Customer {}", order.customer)),
                    style: Style::General,
                },
                crate::xlsx::Cell {
                    col: 3,
                    value: text(order.region),
                    style: Style::General,
                },
                crate::xlsx::Cell {
                    col: 4,
                    value: num(order.qty.to_string()),
                    style: Style::General,
                },
                crate::xlsx::Cell {
                    col: 5,
                    value: num(cents(i128::from(order.cents)).lexeme()),
                    style: Style::Money,
                },
                crate::xlsx::Cell {
                    col: 6,
                    value: total_cell,
                    style: Style::Money,
                },
            ],
        });
    }
    wb.sheets.push(sheet);

    let mut summary = Sheet::new("Summary", 2);
    summary.push_row(vec![bold("Region"), bold("Total")]);
    for (n, region) in REGIONS.iter().enumerate() {
        let r = n + 2;
        summary.push_row(vec![
            plain(region),
            cell(
                formula_num(
                    format!("SUMIF(Orders!C2:C{last},A{r},Orders!F2:F{last})"),
                    by_region[n].lexeme(),
                ),
                Style::Money,
            ),
        ]);
    }
    wb.sheets.push(summary);
    wb
}

/// `rows` orders.
#[must_use]
pub fn large(rows: u32) -> Workbook {
    orders_workbook(&orders(rows))
}

/// The same orders, sorted by customer then id: every row moves, nothing
/// changes.
#[must_use]
pub fn large_sorted(rows: u32) -> Workbook {
    let mut list = orders(rows);
    list.sort_by_key(|o| (o.customer, o.id));
    orders_workbook(&list)
}

/// `sheets` store sheets of 40 days each, and an All stores sheet that refers
/// to every one of them.
#[must_use]
pub fn many_sheets(sheets: u32) -> Workbook {
    let mut wb = Workbook::new("Stores");
    let mut all = Sheet::new("All stores", 1);
    all.push_row(vec![bold("Store"), bold("Takings")]);
    let mut state = 0x2545_F491_4F6C_DD1D;
    let mut grand = Dec::int(0);
    for s in 1..=sheets {
        let label = format!("Store {s:02}");
        let mut sheet = Sheet::new(&label, s + 1);
        sheet.push_row(vec![bold("Day"), bold("Takings")]);
        let mut total = Dec::int(0);
        for day in 1..=40u32 {
            let c = cents(i128::from(next(&mut state) % 500_000));
            total = total.plus(c);
            sheet.push_row(vec![
                cell(num(day.to_string()), Style::General),
                cell(num(c.lexeme()), Style::Money),
            ]);
        }
        sheet.push_row(vec![
            bold("Total"),
            cell(formula_num("SUM(B2:B41)", total.lexeme()), Style::Money),
        ]);
        grand = grand.plus(total);
        all.push_row(vec![
            plain(&label),
            cell(
                formula_num(format!("'{label}'!B42"), total.lexeme()),
                Style::Money,
            ),
        ]);
        wb.sheets.push(sheet);
    }
    let r = sheets + 2;
    all.push_row(vec![
        bold("All"),
        cell(
            formula_num(format!("SUM(B2:B{})", r - 1), grand.lexeme()),
            Style::Money,
        ),
    ]);
    wb.sheets.insert(0, all);
    wb
}

/// Exchange rates loaded from a text file through a data connection (a query
/// table), and a sheet that converts prices with them.
#[must_use]
pub fn connected(rates: &[(&str, &str)]) -> Workbook {
    let mut wb = Workbook::new("Prices in other currencies");
    let mut sheet = Sheet::new("Rates", 1);
    sheet.push_row(vec![bold("Currency"), bold("Rate")]);
    for (currency, rate) in rates {
        sheet.push_row(vec![plain(currency), cell(num(*rate), Style::General)]);
    }
    let last = rates.len() + 1;
    sheet.query_table = Some(QueryTable {
        name: "ExternalData_1".to_owned(),
        connection: 1,
        range: format!("A1:B{last}"),
    });
    wb.connections.push(Connection {
        id: 1,
        name: "rates".to_owned(),
        source_file: r"C:\Shared\rates.csv".to_owned(),
    });
    wb.sheets.push(sheet);

    let mut prices = Sheet::new("Prices", 2);
    prices.push_row(vec![bold("Product"), bold("USD"), bold("EUR")]);
    let eur = rates
        .iter()
        .find(|(c, _)| *c == "EUR")
        .map_or(Dec::int(1), |(_, r)| Dec::parse(r));
    for (n, (product, usd)) in [("Starter plan", "29"), ("Team plan", "99")]
        .iter()
        .enumerate()
    {
        let r = n + 2;
        prices.push_row(vec![
            plain(product),
            cell(num(*usd), Style::Money),
            cell(
                formula_num(
                    format!("B{r}*VLOOKUP(\"EUR\",Rates!A:B,2,FALSE)"),
                    Dec::parse(usd).times(eur).lexeme(),
                ),
                Style::Money,
            ),
        ]);
    }
    wb.sheets.push(prices);
    wb
}

#[must_use]
pub fn connected_master() -> Workbook {
    connected(&[("EUR", "0.92"), ("GBP", "0.79"), ("JPY", "151.3")])
}

/// The same workbook after its data was refreshed (new rates, nobody typed).
#[must_use]
pub fn connected_refreshed() -> Workbook {
    connected(&[("EUR", "0.94"), ("GBP", "0.78"), ("JPY", "149.8")])
}

/// Cells that are easy to get wrong: text that looks like a number, odd
/// number lexemes, booleans, errors, inline strings, array formulas, formulas
/// with text or error results, a hidden and a very hidden sheet.
#[must_use]
pub fn awkward_cells() -> Workbook {
    let mut wb = Workbook::new("Awkward cells");
    let mut sheet = Sheet::new("Cells", 1);
    sheet.push_row(vec![bold("What"), bold("Cell")]);
    let rows: Vec<(&str, Value, Style)> = vec![
        ("Leading zeros", text("00123"), Style::Text),
        ("Long digits", text("1234567890123456789"), Style::Text),
        ("Looks like true", text("TRUE"), Style::Text),
        ("Exponent", num("1E-3"), Style::General),
        ("Float noise", num("0.30000000000000004"), Style::General),
        ("Negative zero", num("-0"), Style::General),
        ("Big", num("12345678901234567000"), Style::General),
        ("True", Value::Bool(true), Style::General),
        (
            "Not available",
            Value::Error("#N/A".to_owned()),
            Style::General,
        ),
        (
            "Inline",
            Value::Inline("Written by another app".to_owned()),
            Style::General,
        ),
        (
            "Text result",
            formula(
                "A2&\" and more\"",
                Some(Cached::Text("Leading zeros and more".to_owned())),
            ),
            Style::General,
        ),
        (
            "Error result",
            formula("1/0", Some(Cached::Error("#DIV/0!".to_owned()))),
            Style::General,
        ),
        (
            "Boolean result",
            formula("B5>0", Some(Cached::Bool(true))),
            Style::General,
        ),
        (
            "Array",
            Value::Array {
                f: "SUM(LEN(A2:A4))".to_owned(),
                range: "B15".to_owned(),
                cached: Some(Cached::Number("39".to_owned())),
            },
            Style::General,
        ),
        ("Space at the end", text("trailing "), Style::General),
        ("Date", num("46023"), Style::Date),
        ("Percent", num("0.125"), Style::Percent2),
    ];
    for (label, value, style) in rows {
        sheet.push_row(vec![plain(label), cell(value, style)]);
    }
    wb.sheets.push(sheet);
    let mut hidden = Sheet::new("Lookup", 2);
    hidden.state = SheetState::Hidden;
    hidden.push_row(vec![plain("Rate"), cell(num("0.2"), Style::Percent)]);
    wb.sheets.push(hidden);
    let mut very = Sheet::new("Internal", 3);
    very.state = SheetState::VeryHidden;
    very.push_row(vec![plain("Build"), cell(num("7"), Style::General)]);
    wb.sheets.push(very);
    wb.names.push(Name {
        name: "TaxRate".to_owned(),
        refers_to: "Lookup!$B$1".to_owned(),
        local: None,
        hidden: false,
    });
    wb
}

/// A macro-enabled copy of the model, with an opaque `vbaProject.bin`. The
/// bytes are not a real VBA project, so Excel itself won't open this one; it
/// tests that Lineage carries the part untouched.
#[must_use]
pub fn model_xlsm() -> Workbook {
    let mut wb = model_master();
    wb.macro_enabled = true;
    let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    bytes.extend((0u16..2040).map(|n| u8::try_from(n % 251).unwrap_or(0)));
    wb.opaque.push(Opaque {
        name: "xl/vbaProject.bin".to_owned(),
        content_type: "application/vnd.ms-office.vbaProject".to_owned(),
        rel_type: "http://schemas.microsoft.com/office/2006/relationships/vbaProject".to_owned(),
        bytes,
    });
    wb
}

/// Every committed synthetic workbook, by file name.
#[must_use]
pub fn all() -> Vec<(&'static str, Workbook)> {
    let mut stored = model_master();
    stored.zip = crate::xlsx::Zip::Stored;
    let mut custom = model_master();
    custom.custom = vec![
        ("Client".to_owned(), "Brightwell Analytics".to_owned()),
        ("Reviewed".to_owned(), "No".to_owned()),
    ];
    vec![
        ("model.xlsx", model_master()),
        ("model-price-update.xlsx", model_price_update()),
        ("model-hosting.xlsx", model_hosting()),
        ("model-harbor-lane.xlsx", model_harbor_lane(model_master())),
        ("model-northwind.xlsx", model_northwind()),
        ("model-stored.xlsx", stored),
        ("model-custom-properties.xlsx", custom),
        ("model.xlsm", model_xlsm()),
        ("budget.xlsx", budget_master()),
        ("budget-added.xlsx", budget_added()),
        ("budget-sorted.xlsx", budget_sorted()),
        ("budget-recategorised.xlsx", budget_recategorised()),
        ("tracker.xlsx", tracker_master()),
        ("tracker-filtered.xlsx", tracker_filtered()),
        ("tracker-sorted.xlsx", tracker_sorted()),
        ("tracker-updated.xlsx", tracker_updated()),
        ("orders-1000.xlsx", large(1000)),
        ("orders-1000-sorted.xlsx", large_sorted(1000)),
        ("stores-60.xlsx", many_sheets(60)),
        ("rates.xlsx", connected_master()),
        ("rates-refreshed.xlsx", connected_refreshed()),
        ("awkward-cells.xlsx", awkward_cells()),
    ]
}

/// The column letter of the model's MRR column, for tests.
#[must_use]
pub fn mrr_cell(month: u32) -> String {
    a1(3, month + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_are_exact() {
        assert_eq!(
            Dec::parse("120")
                .times(Dec::parse("49"))
                .times(Dec::parse("0.98"))
                .lexeme(),
            "5762.4"
        );
        assert_eq!(Dec::parse("0.1").plus(Dec::parse("0.2")).lexeme(), "0.3");
        assert_eq!(Dec::parse("-0.5").lexeme(), "-0.5");
        assert_eq!(Dec::parse("0.05").lexeme(), "0.05");
        assert_eq!(cents(1205).lexeme(), "12.05");
        assert_eq!(cents(1200).lexeme(), "12");
    }

    #[test]
    fn the_model_agrees_with_itself() {
        let wb = model_master();
        let revenue = wb.sheet("Revenue");
        // Month 1: 120 seats at $49, less 2% churn.
        let c2 = revenue.get("C2").map(|c| c.value.clone());
        assert!(matches!(
            c2,
            Some(Value::SharedMaster { cached: Some(Cached::Number(ref n)), .. }) if n == "5762.4"
        ));
        assert_eq!(revenue.rows.len(), 25);
    }

    #[test]
    fn sorting_the_budget_keeps_every_line() {
        let a = budget_master();
        let b = budget_sorted();
        assert_eq!(
            a.sheet("Transactions").rows.len(),
            b.sheet("Transactions").rows.len()
        );
        assert_ne!(a.to_bytes(), b.to_bytes());
    }

    #[test]
    fn large_orders_are_the_same_every_time() {
        assert_eq!(orders(50), orders(50));
        assert_eq!(large(10).to_bytes(), large(10).to_bytes());
    }
}
