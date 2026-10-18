// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The workbook model (decision 0020): sheets, rows and cells, with each
//! cell's stored content, formula, cached result and style kept apart.
//!
//! A cell never owns its text when it doesn't need to. Numbers, formulas and
//! cached results are [`Str`] spans into the worksheet part, which the
//! [`super::Workbook`]'s package already holds, so a 200,000-row sheet costs
//! little more than its XML. Only text that had to be unescaped is owned.

/// A byte range in a part's text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    #[must_use]
    pub fn get<'a>(&self, text: &'a str) -> &'a str {
        let start = self.start as usize;
        text.get(start..start + self.len as usize).unwrap_or("")
    }
}

/// Text as the file stores it: a span into the part when it needed no
/// unescaping, or owned when it did.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Str {
    At(Span),
    Own(Box<str>),
}

impl Str {
    #[must_use]
    pub fn get<'a>(&'a self, text: &'a str) -> &'a str {
        match self {
            Self::At(span) => span.get(text),
            Self::Own(s) => s,
        }
    }
}

/// A formula's last calculated result, as stored in the file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Cached {
    /// The number's lexeme (`<v>` of a formula cell without `t`).
    Number(Str),
    /// `t="str"`.
    Text(Str),
    /// `t="b"`.
    Bool(bool),
    /// `t="e"`, such as `#N/A`.
    Error(Str),
}

/// How a formula is stored.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FormulaKind {
    Normal,
    /// `t="array"` over `range`.
    Array {
        range: Option<Area>,
    },
    /// `t="shared"`. The first cell (with `range` and the text) is the master;
    /// the others carry only `si`.
    Shared {
        si: u32,
        range: Option<Area>,
    },
    /// `t="dataTable"`, a what-if table. Kept, never re-rendered.
    DataTable,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Formula {
    /// The formula text without `=`. `None` for a shared formula's other
    /// cells, whose text comes from the master.
    pub text: Option<Str>,
    pub kind: FormulaKind,
    /// The cell has cell metadata (`cm`), as dynamic-array formulas do.
    pub dynamic: bool,
    /// `ca="1"`: Excel recalculates it on every calculation.
    pub always: bool,
    pub cached: Option<Cached>,
}

/// What a cell stores (decision 0020).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    /// No value: a cell kept only for its format.
    Blank,
    /// An index into the shared-strings table.
    Shared(u32),
    /// Inline text (`t="inlineStr"`), runs joined.
    Inline(Box<str>),
    /// A number, by its lexeme exactly as stored.
    Number(Span),
    Bool(bool),
    Error(Str),
    /// `t="d"`: an ISO 8601 date as text, which few programs write.
    IsoDate(Span),
    Formula(Box<Formula>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// 1-based column.
    pub col: u32,
    /// The cell format: an index into `cellXfs`.
    pub style: u32,
    pub content: Content,
    /// The cell has value metadata (`vm`): a linked data type or a picture in
    /// the cell. Kept, and not tracked yet.
    pub value_meta: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// 1-based row number.
    pub r: u32,
    /// The whole `<row>` element in the worksheet part.
    pub span: Span,
    pub hidden: bool,
    /// A key of the row's own format: style, custom height, outline level.
    pub format: u64,
    /// Cells in column order.
    pub cells: Vec<Cell>,
}

/// A rectangle of cells, 1-based and inclusive. A whole column or row has
/// the sheet's limits as its far edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Area {
    pub col1: u32,
    pub row1: u32,
    pub col2: u32,
    pub row2: u32,
}

impl Area {
    #[must_use]
    pub fn cell(col: u32, row: u32) -> Self {
        Self {
            col1: col,
            row1: row,
            col2: col,
            row2: row,
        }
    }

    #[must_use]
    pub fn contains(&self, col: u32, row: u32) -> bool {
        (self.col1..=self.col2).contains(&col) && (self.row1..=self.row2).contains(&row)
    }

    #[must_use]
    pub fn rows(&self) -> u32 {
        self.row2 - self.row1 + 1
    }
}

/// Column settings from `<cols>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColInfo {
    pub min: u32,
    pub max: u32,
    pub hidden: bool,
    /// Width, style and outline, as a key.
    pub format: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SheetState {
    #[default]
    Visible,
    Hidden,
    VeryHidden,
}

/// What kind of sheet a tab is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SheetKind {
    #[default]
    Worksheet,
    /// A chart sheet, kept whole and not tracked yet.
    Chart,
    /// A dialog or macro sheet (Excel 4.0), kept whole and never run.
    Other,
}

/// Keys for everything about a sheet that isn't its cells, so a change to one
/// is reported as a structure or formatting change without listing XML.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SheetKeys {
    /// `sheetPr` (tab colour, outline settings) and `sheetFormatPr`.
    pub format: u64,
    pub conditional: u64,
    pub validation: u64,
    pub protection: u64,
    pub hyperlinks: u64,
    /// The filter's criteria and sort state.
    pub filter: u64,
    /// Print and page setup.
    pub page: u64,
    /// Everything else outside the cells, such as `extLst`.
    pub other: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    pub name: String,
    pub sheet_id: u32,
    /// The worksheet part, such as `xl/worksheets/sheet1.xml`.
    pub part: String,
    pub state: SheetState,
    pub kind: SheetKind,
    /// Rows in order. Rows that hold nothing are kept when they carry a
    /// format.
    pub rows: Vec<Row>,
    pub cols: Vec<ColInfo>,
    pub merged: Vec<Area>,
    pub auto_filter: Option<Area>,
    pub keys: SheetKeys,
    /// The `<sheetData>` element, so the writer can splice rows.
    pub sheet_data: Span,
    /// Parts this sheet reaches that the engine keeps but doesn't track:
    /// drawings, charts, pictures.
    pub drawings: Vec<String>,
    /// Legacy notes (`commentsN.xml`) and threaded comments.
    pub comments: Vec<String>,
}

impl Sheet {
    /// The row with this number.
    #[must_use]
    pub fn row(&self, r: u32) -> Option<&Row> {
        self.rows
            .binary_search_by_key(&r, |row| row.r)
            .ok()
            .map(|i| &self.rows[i])
    }

    /// The cell at (column, row).
    #[must_use]
    pub fn cell(&self, col: u32, r: u32) -> Option<&Cell> {
        let row = self.row(r)?;
        row.cells
            .binary_search_by_key(&col, |c| c.col)
            .ok()
            .map(|i| &row.cells[i])
    }

    /// How many cells hold something.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.rows.iter().map(|row| row.cells.len()).sum()
    }
}

/// A defined name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinedName {
    pub name: String,
    /// A sheet position for a sheet-scoped name.
    pub local: Option<usize>,
    pub hidden: bool,
    /// What it refers to, as a formula without `=`.
    pub refers_to: String,
}

impl DefinedName {
    /// A name Excel makes itself: print areas, filters, data imports.
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        self.name.starts_with("_xlnm.") || self.name.starts_with("ExternalData_")
    }
}

/// One entry of the shared-strings table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedString {
    pub text: String,
    /// A key of its run formatting, 0 when it is plain text.
    pub rich: u64,
}

/// A table (`xl/tables/tableN.xml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableDef {
    pub sheet: usize,
    pub part: String,
    pub name: String,
    pub area: Area,
    pub columns: Vec<String>,
    pub header_rows: u32,
    pub totals_rows: u32,
    /// Loaded by a data connection (`tableType="queryTable"`).
    pub query: bool,
    /// A key of its settings (style, filter, totals), for structure changes.
    pub key: u64,
}

/// A data connection (`xl/connections.xml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionDef {
    pub id: u32,
    pub name: String,
    /// The connection's `type`: 1 ODBC, 4 web, 5 OLE DB (Power Query), 6 text, …
    pub kind: u32,
    pub key: u64,
}

/// What the engine keeps but doesn't track yet (decision 0026). The panel
/// labels these "Preserved; semantic tracking not yet supported."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnsupportedKind {
    Macros,
    PivotTables,
    PowerQuery,
    Charts,
    ExternalLinks,
    Slicers,
    Pictures,
    DataTypes,
    ChartSheets,
    MacroSheets,
}

impl UnsupportedKind {
    /// How the panel names it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Macros => "Macros",
            Self::PivotTables => "Pivot tables",
            Self::PowerQuery => "Power Query queries",
            Self::Charts => "Charts",
            Self::ExternalLinks => "Links to other workbooks",
            Self::Slicers => "Slicers and timelines",
            Self::Pictures => "Pictures and shapes",
            Self::DataTypes => "Linked data types and pictures in cells",
            Self::ChartSheets => "Chart sheets",
            Self::MacroSheets => "Macro and dialog sheets",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsupported {
    pub kind: UnsupportedKind,
    pub part: String,
}
