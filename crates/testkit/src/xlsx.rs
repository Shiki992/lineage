// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Builds complete .xlsx and .xlsm packages from hand-written SpreadsheetML,
//! independently of the engine. Element order follows ECMA-376 Part 1, which
//! Excel enforces. Every workbook has a styles part with a fixed set of cell
//! formats ([`Style`]), a shared-strings part, and core and app properties, so
//! Excel and LibreOffice open them without repair.
//!
//! A [`Workbook`] is plain data. The pilot workbooks in `workbooks.rs` are built
//! from invented domain data (transactions, applications, assumptions), so a
//! "tailored version" is the same data changed and rendered again, the way
//! Excel itself would save it (formulas below an inserted row refer to their
//! new rows).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::docx::{FIXED_DATE, esc};
use crate::zip_writer;

const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";
pub const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_SML: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";

/// A cell format. The index is the `s` attribute, a position in `cellXfs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Style {
    #[default]
    General,
    /// `0%`
    Percent,
    /// `#,##0`
    Thousands,
    /// `yyyy-mm-dd` (custom format 164)
    Date,
    Bold,
    /// `"$"#,##0` (custom format 165)
    Currency,
    /// `@`, text
    Text,
    /// `0.00%`
    Percent2,
    /// `#,##0.00`
    Money,
}

impl Style {
    pub const ALL: [Self; 9] = [
        Self::General,
        Self::Percent,
        Self::Thousands,
        Self::Date,
        Self::Bold,
        Self::Currency,
        Self::Text,
        Self::Percent2,
        Self::Money,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    fn xf(self) -> &'static str {
        match self {
            Self::General => r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>"#,
            Self::Percent => {
                r#"<xf numFmtId="9" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#
            }
            Self::Thousands => {
                r#"<xf numFmtId="3" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#
            }
            Self::Date => {
                r#"<xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#
            }
            Self::Bold => {
                r#"<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/>"#
            }
            Self::Currency => {
                r#"<xf numFmtId="165" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#
            }
            Self::Text => {
                r#"<xf numFmtId="49" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#
            }
            Self::Percent2 => {
                r#"<xf numFmtId="10" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#
            }
            Self::Money => {
                r#"<xf numFmtId="4" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#
            }
        }
    }
}

/// A formula's cached result, as the file stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cached {
    Number(String),
    Text(String),
    Bool(bool),
    Error(String),
}

/// What a cell holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A number, by its lexeme exactly as written (`"0.15"`, `"1200"`).
    Number(String),
    /// Text in the shared-strings table.
    Text(String),
    /// Text written inline (`t="inlineStr"`), as some other programs do.
    Inline(String),
    Bool(bool),
    /// An error value such as `#N/A`.
    Error(String),
    /// A formula (without the `=`), with its cached result.
    Formula {
        f: String,
        cached: Option<Cached>,
    },
    /// The first cell of a shared formula: its text and the range it covers.
    SharedMaster {
        f: String,
        si: u32,
        range: String,
        cached: Option<Cached>,
    },
    /// Another cell of a shared formula.
    Shared {
        si: u32,
        cached: Option<Cached>,
    },
    /// An array formula over `range`.
    Array {
        f: String,
        range: String,
        cached: Option<Cached>,
    },
}

/// Shorthands for building cells.
#[must_use]
pub fn num(lexeme: impl Into<String>) -> Value {
    Value::Number(lexeme.into())
}

#[must_use]
pub fn text(t: impl Into<String>) -> Value {
    Value::Text(t.into())
}

#[must_use]
pub fn formula(f: impl Into<String>, cached: Option<Cached>) -> Value {
    Value::Formula {
        f: f.into(),
        cached,
    }
}

#[must_use]
pub fn formula_num(f: impl Into<String>, cached: impl Into<String>) -> Value {
    formula(f, Some(Cached::Number(cached.into())))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// 1-based column.
    pub col: u32,
    pub value: Value,
    pub style: Style,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// 1-based row number.
    pub r: u32,
    pub hidden: bool,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetState {
    #[default]
    Visible,
    Hidden,
    VeryHidden,
}

/// A table (`xl/tables/tableN.xml`). Its header cells must hold `columns`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub range: String,
    pub columns: Vec<String>,
}

/// A range loaded by a data connection (`xl/queryTables/queryTableN.xml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryTable {
    /// The query table's name, also a sheet-scoped defined name over `range`
    /// ("ExternalData_1").
    pub name: String,
    pub connection: u32,
    pub range: String,
}

/// A note on a cell (legacy comment, with its VML shape).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub cell: String,
    pub author: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    pub name: String,
    pub sheet_id: u32,
    pub state: SheetState,
    pub rows: Vec<Row>,
    /// Column widths: (first, last, width).
    pub cols: Vec<(u32, u32, String)>,
    pub merged: Vec<String>,
    pub auto_filter: Option<String>,
    pub tables: Vec<Table>,
    pub query_table: Option<QueryTable>,
    pub comments: Vec<Comment>,
    /// Freeze the first row.
    pub freeze_header: bool,
    /// Conditional formats: (range, formula of an expression rule).
    pub conditional: Vec<(String, String)>,
}

/// A defined name. `local` is a sheet position for a sheet-scoped name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub name: String,
    pub refers_to: String,
    pub local: Option<usize>,
    pub hidden: bool,
}

#[must_use]
pub fn name(name: &str, refers_to: &str) -> Name {
    Name {
        name: name.to_owned(),
        refers_to: refers_to.to_owned(),
        local: None,
        hidden: false,
    }
}

/// A text-file data connection (`xl/connections.xml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub id: u32,
    pub name: String,
    pub source_file: String,
    /// A Power Query query this connection loads, instead of a text file.
    pub mashup: Option<String>,
}

/// A part the builder carries without understanding, such as `vbaProject.bin`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opaque {
    pub name: String,
    pub content_type: String,
    /// Relationship type from the workbook part.
    pub rel_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Zip {
    Stored,
    /// Deflated, as Excel saves.
    #[default]
    Deflated,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Workbook {
    pub title: String,
    pub sheets: Vec<Sheet>,
    pub names: Vec<Name>,
    pub connections: Vec<Connection>,
    pub opaque: Vec<Opaque>,
    pub custom: Vec<(String, String)>,
    /// `.xlsm`: the macro-enabled main content type.
    pub macro_enabled: bool,
    pub zip: Zip,
    /// Pivot caches, pivot tables (with their output rendered into their
    /// sheets) and slicers.
    pub pivot_caches: Vec<crate::pivots::PivotCache>,
    pub pivots: Vec<crate::pivots::Pivot>,
    pub slicers: Vec<crate::pivots::Slicer>,
    /// Parts related from another part than the workbook, such as a VBA
    /// project's signature or Power Query's custom XML item.
    pub extra: Vec<Extra>,
}

/// A part with its relationship from `source` (a part name, or
/// "xl/workbook.xml"), and its own content type override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extra {
    pub name: String,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
    /// (source part, full relationship type); `None` for a part nothing
    /// points at (only its content type).
    pub related_from: Option<(String, String)>,
}

/// "A" for 1, "AA" for 27.
#[must_use]
pub fn col_name(mut col: u32) -> String {
    let mut out = Vec::new();
    while col > 0 {
        let rem = (col - 1) % 26;
        out.push(b'A' + u8::try_from(rem).unwrap_or(0));
        col = (col - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// "B7" for (2, 7).
#[must_use]
pub fn a1(col: u32, row: u32) -> String {
    format!("{}{row}", col_name(col))
}

/// Splits "B7" into (2, 7).
#[must_use]
pub fn parse_a1(cell: &str) -> (u32, u32) {
    let letters: String = cell.chars().take_while(char::is_ascii_alphabetic).collect();
    let digits = &cell[letters.len()..];
    let col = letters.bytes().fold(0u32, |acc, b| {
        acc * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
    });
    (col, digits.parse().unwrap_or(0))
}

impl Sheet {
    #[must_use]
    pub fn new(name: &str, sheet_id: u32) -> Self {
        Self {
            name: name.to_owned(),
            sheet_id,
            ..Self::default()
        }
    }

    /// Appends a row of cells starting at column A, skipping `None`s.
    pub fn push_row(&mut self, cells: Vec<Option<(Value, Style)>>) -> u32 {
        let r = self.rows.last().map_or(1, |row| row.r + 1);
        self.put_row(r, cells);
        r
    }

    /// Writes row `r` (replacing any row with that number).
    pub fn put_row(&mut self, r: u32, cells: Vec<Option<(Value, Style)>>) {
        let cells = cells
            .into_iter()
            .enumerate()
            .filter_map(|(i, cell)| {
                cell.map(|(value, style)| Cell {
                    col: u32::try_from(i).unwrap_or(0) + 1,
                    value,
                    style,
                })
            })
            .collect();
        self.rows.retain(|row| row.r != r);
        self.rows.push(Row {
            r,
            hidden: false,
            cells,
        });
        self.rows.sort_by_key(|row| row.r);
    }

    /// Sets one cell, creating its row if needed.
    pub fn set(&mut self, cell: &str, value: Value, style: Style) {
        let (col, r) = parse_a1(cell);
        let index = match self.rows.iter().position(|row| row.r == r) {
            Some(index) => index,
            None => {
                self.rows.push(Row {
                    r,
                    ..Row::default()
                });
                self.rows.sort_by_key(|row| row.r);
                self.rows.iter().position(|row| row.r == r).unwrap_or(0)
            }
        };
        let row = &mut self.rows[index];
        row.cells.retain(|c| c.col != col);
        row.cells.push(Cell { col, value, style });
        row.cells.sort_by_key(|c| c.col);
    }

    #[must_use]
    pub fn get(&self, cell: &str) -> Option<&Cell> {
        let (col, r) = parse_a1(cell);
        self.rows
            .iter()
            .find(|row| row.r == r)
            .and_then(|row| row.cells.iter().find(|c| c.col == col))
    }

    /// The used range, such as "A1:F20".
    #[must_use]
    pub fn dimension(&self) -> String {
        let max_col = self
            .rows
            .iter()
            .flat_map(|row| row.cells.iter().map(|c| c.col))
            .max()
            .unwrap_or(1);
        let max_row = self.rows.last().map_or(1, |row| row.r);
        let min_row = self.rows.first().map_or(1, |row| row.r);
        let min_col = self
            .rows
            .iter()
            .flat_map(|row| row.cells.iter().map(|c| c.col))
            .min()
            .unwrap_or(1);
        if max_col == min_col && max_row == min_row {
            a1(min_col, min_row)
        } else {
            format!("{}:{}", a1(min_col, min_row), a1(max_col, max_row))
        }
    }
}

/// Builds the shared-strings table, in first-use order.
#[derive(Default)]
struct Strings {
    index: BTreeMap<String, usize>,
    list: Vec<String>,
    uses: usize,
}

impl Strings {
    fn add(&mut self, text: &str) -> usize {
        self.uses += 1;
        if let Some(index) = self.index.get(text) {
            return *index;
        }
        let index = self.list.len();
        self.list.push(text.to_owned());
        self.index.insert(text.to_owned(), index);
        index
    }

    fn xml(&self) -> String {
        let mut out = format!(
            r#"{DECL}<sst xmlns="{NS_MAIN}" count="{}" uniqueCount="{}">"#,
            self.uses,
            self.list.len()
        );
        for text in &self.list {
            let space = if text.starts_with(' ') || text.ends_with(' ') {
                r#" xml:space="preserve""#
            } else {
                ""
            };
            let _ = write!(out, "<si><t{space}>{}</t></si>", esc(text));
        }
        out.push_str("</sst>");
        out
    }
}

fn cached_xml(out: &mut String, cached: Option<&Cached>) -> &'static str {
    match cached {
        None => "",
        Some(Cached::Number(n)) => {
            let _ = write!(out, "<v>{}</v>", esc(n));
            ""
        }
        Some(Cached::Text(t)) => {
            let _ = write!(out, "<v>{}</v>", esc(t));
            "str"
        }
        Some(Cached::Bool(b)) => {
            out.push_str(if *b { "<v>1</v>" } else { "<v>0</v>" });
            "b"
        }
        Some(Cached::Error(e)) => {
            let _ = write!(out, "<v>{}</v>", esc(e));
            "e"
        }
    }
}

fn cell_xml(cell: &Cell, r: u32, strings: &mut Strings) -> String {
    let at = a1(cell.col, r);
    let s = cell.style.index();
    let style = if s == 0 {
        String::new()
    } else {
        format!(r#" s="{s}""#)
    };
    let mut body = String::new();
    let t = match &cell.value {
        Value::Number(n) => {
            let _ = write!(body, "<v>{}</v>", esc(n));
            ""
        }
        Value::Text(t) => {
            let _ = write!(body, "<v>{}</v>", strings.add(t));
            "s"
        }
        Value::Inline(t) => {
            let _ = write!(body, "<is><t>{}</t></is>", esc(t));
            "inlineStr"
        }
        Value::Bool(b) => {
            body.push_str(if *b { "<v>1</v>" } else { "<v>0</v>" });
            "b"
        }
        Value::Error(e) => {
            let _ = write!(body, "<v>{}</v>", esc(e));
            "e"
        }
        Value::Formula { f, cached } => {
            let _ = write!(body, "<f>{}</f>", esc(f));
            cached_xml(&mut body, cached.as_ref())
        }
        Value::SharedMaster {
            f,
            si,
            range,
            cached,
        } => {
            let _ = write!(
                body,
                r#"<f t="shared" ref="{range}" si="{si}">{}</f>"#,
                esc(f)
            );
            cached_xml(&mut body, cached.as_ref())
        }
        Value::Shared { si, cached } => {
            let _ = write!(body, r#"<f t="shared" si="{si}"/>"#);
            cached_xml(&mut body, cached.as_ref())
        }
        Value::Array { f, range, cached } => {
            let _ = write!(body, r#"<f t="array" ref="{range}">{}</f>"#, esc(f));
            cached_xml(&mut body, cached.as_ref())
        }
    };
    let t = if t.is_empty() {
        String::new()
    } else {
        format!(r#" t="{t}""#)
    };
    format!(r#"<c r="{at}"{style}{t}>{body}</c>"#)
}

type RelRow = (String, String, String);

fn rel(id: usize, kind: &str, target: &str) -> RelRow {
    (
        format!("rId{id}"),
        format!("{REL}/{kind}"),
        target.to_owned(),
    )
}

/// The target of a relationship from a part in `dir` to `part`.
fn relative_to(dir: &str, part: &str) -> String {
    let from: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = part.split('/').collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; from.len() - common];
    out.extend(&to[common..]);
    out.join("/")
}

fn rels_xml(rels: &[RelRow]) -> String {
    let mut out = format!(
        r#"{DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#
    );
    for (id, kind, target) in rels {
        let _ = write!(
            out,
            r#"<Relationship Id="{id}" Type="{kind}" Target="{}"/>"#,
            esc(target)
        );
    }
    out.push_str("</Relationships>");
    out
}

fn styles_xml() -> String {
    let mut xfs = String::new();
    for style in Style::ALL {
        xfs.push_str(style.xf());
    }
    format!(
        r#"{DECL}<styleSheet xmlns="{NS_MAIN}"><numFmts count="2"><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd"/><numFmt numFmtId="165" formatCode="&quot;$&quot;#,##0"/></numFmts><fonts count="2"><font><sz val="11"/><color theme="1"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font><font><b/><sz val="11"/><color theme="1"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="{}">{xfs}</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles><dxfs count="1"><dxf><font><b/></font></dxf></dxfs><tableStyles count="0" defaultTableStyle="TableStyleMedium2" defaultPivotStyle="PivotStyleLight16"/></styleSheet>"#,
        Style::ALL.len()
    )
}

fn core_xml(title: &str) -> String {
    format!(
        r#"{DECL}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>Lineage fixture generator</dc:creator><cp:lastModifiedBy>Lineage fixture generator</cp:lastModifiedBy><dcterms:created xsi:type="dcterms:W3CDTF">{FIXED_DATE}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">{FIXED_DATE}</dcterms:modified></cp:coreProperties>"#,
        esc(title)
    )
}

fn app_xml() -> String {
    format!(
        r#"{DECL}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>Lineage fixture generator</Application><DocSecurity>0</DocSecurity><ScaleCrop>false</ScaleCrop></Properties>"#
    )
}

fn custom_xml(props: &[(String, String)]) -> String {
    let mut out = format!(
        r#"{DECL}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">"#
    );
    for (n, (name, value)) in props.iter().enumerate() {
        let _ = write!(
            out,
            r#"<property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="{}" name="{}"><vt:lpwstr>{}</vt:lpwstr></property>"#,
            n + 2,
            esc(name),
            esc(value)
        );
    }
    out.push_str("</Properties>");
    out
}

fn comments_xml(comments: &[Comment]) -> String {
    let mut authors: Vec<&str> = Vec::new();
    for c in comments {
        if !authors.contains(&c.author.as_str()) {
            authors.push(&c.author);
        }
    }
    let mut out = format!(r#"{DECL}<comments xmlns="{NS_MAIN}"><authors>"#);
    for author in &authors {
        let _ = write!(out, "<author>{}</author>", esc(author));
    }
    out.push_str("</authors><commentList>");
    for c in comments {
        let id = authors.iter().position(|a| *a == c.author).unwrap_or(0);
        let _ = write!(
            out,
            r#"<comment ref="{}" authorId="{id}"><text><r><t xml:space="preserve">{}</t></r></text></comment>"#,
            c.cell,
            esc(&c.text)
        );
    }
    out.push_str("</commentList></comments>");
    out
}

fn vml_xml(comments: &[Comment], sheet_index: usize) -> String {
    let mut out = format!(
        r#"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="{}"/></o:shapelayout><v:shapetype id="_x0000_t202" coordsize="21600,21600" o:spt="202" path="m,l,21600r21600,l21600,xe"><v:stroke joinstyle="miter"/><v:path gradientshapeok="t" o:connecttype="rect"/></v:shapetype>"#,
        sheet_index + 1
    );
    for (n, c) in comments.iter().enumerate() {
        let (col, row) = parse_a1(&c.cell);
        let _ = write!(
            out,
            r##"<v:shape id="_x0000_s{}" type="#_x0000_t202" style="position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:{};visibility:hidden" fillcolor="#ffffe1" o:insetmode="auto"><v:fill color2="#ffffe1"/><v:shadow on="t" color="black" obscured="t"/><v:path o:connecttype="none"/><v:textbox style="mso-direction-alt:auto"><div style="text-align:left"></div></v:textbox><x:ClientData ObjectType="Note"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{}, 15, {}, 10, {}, 15, {}, 4</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{}</x:Row><x:Column>{}</x:Column></x:ClientData></v:shape>"##,
            (sheet_index + 1) * 1024 + n + 1,
            n + 1,
            col,
            row.saturating_sub(1),
            col + 2,
            row + 3,
            row - 1,
            col - 1
        );
    }
    out.push_str("</xml>");
    out
}

impl Workbook {
    #[must_use]
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn sheet(&self, name: &str) -> &Sheet {
        self.sheets
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no sheet {name}"))
    }

    pub fn sheet_mut(&mut self, name: &str) -> &mut Sheet {
        self.sheets
            .iter_mut()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no sheet {name}"))
    }

    /// The sheets as written: with every pivot table's output cells.
    #[must_use]
    pub fn rendered_sheets(&self) -> Vec<Sheet> {
        let mut sheets = self.sheets.clone();
        crate::pivots::render(&mut sheets, &self.pivots, &self.pivot_caches, &self.slicers);
        sheets
    }

    /// The finished package bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let parts = self.parts();
        match self.zip {
            Zip::Stored => zip_writer::write_stored(&parts),
            Zip::Deflated => zip_writer::write_deflated(&parts, false),
        }
    }

    /// The package parts, in a fixed order.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn parts(&self) -> Vec<(String, Vec<u8>)> {
        let mut overrides: Vec<(String, String)> = Vec::new();
        let mut defaults: Vec<(&str, &str)> = vec![
            (
                "rels",
                "application/vnd.openxmlformats-package.relationships+xml",
            ),
            ("xml", "application/xml"),
        ];
        let main_type = if self.macro_enabled {
            "application/vnd.ms-excel.sheet.macroEnabled.main+xml".to_owned()
        } else {
            format!("{CT_SML}.sheet.main+xml")
        };
        overrides.push(("/xl/workbook.xml".to_owned(), main_type));

        let mut strings = Strings::default();
        let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
        let mut sheet_parts: Vec<(String, Vec<u8>)> = Vec::new();
        let mut wb_rels: Vec<RelRow> = Vec::new();
        let mut table_number = 0usize;
        let mut query_number = 0usize;
        let mut comment_number = 0usize;
        let mut slicer_sheet_number = 0usize;
        let mut pivot_number = 0usize;

        let rendered = self.rendered_sheets();
        let tab_ids: Vec<u32> = self.sheets.iter().map(|s| s.sheet_id).collect();

        for (index, sheet) in rendered.iter().enumerate() {
            let file = format!("sheet{}.xml", index + 1);
            wb_rels.push(rel(index + 1, "worksheet", &format!("worksheets/{file}")));
            overrides.push((
                format!("/xl/worksheets/{file}"),
                format!("{CT_SML}.worksheet+xml"),
            ));
            let mut sheet_rels: Vec<RelRow> = Vec::new();
            for (p_index, pivot) in self.pivots.iter().enumerate() {
                if pivot.sheet != index {
                    continue;
                }
                pivot_number += 1;
                let rid = sheet_rels.len() + 1;
                sheet_rels.push(rel(
                    rid,
                    "pivotTable",
                    &format!("../pivotTables/pivotTable{pivot_number}.xml"),
                ));
                overrides.push((
                    format!("/xl/pivotTables/pivotTable{pivot_number}.xml"),
                    format!("{CT_SML}.pivotTable+xml"),
                ));
                sheet_parts.push((
                    format!("xl/pivotTables/pivotTable{pivot_number}.xml"),
                    crate::pivots::table_xml(
                        pivot,
                        p_index,
                        pivot.cache + 1,
                        &self.pivot_caches,
                        &self.slicers,
                    )
                    .into_bytes(),
                ));
                sheet_parts.push((
                    format!("xl/pivotTables/_rels/pivotTable{pivot_number}.xml.rels"),
                    rels_xml(&[rel(
                        1,
                        "pivotCacheDefinition",
                        &format!("../pivotCache/pivotCacheDefinition{}.xml", pivot.cache + 1),
                    )])
                    .into_bytes(),
                ));
            }
            let on_sheet: Vec<&crate::pivots::Slicer> =
                self.slicers.iter().filter(|s| s.sheet == index).collect();
            let mut drawing_rid = None;
            let mut slicer_rid = None;
            if !on_sheet.is_empty() {
                slicer_sheet_number += 1;
                let n = slicer_sheet_number;
                let rid = sheet_rels.len() + 1;
                sheet_rels.push(rel(rid, "drawing", &format!("../drawings/drawing{n}.xml")));
                drawing_rid = Some(format!("rId{rid}"));
                let rid = sheet_rels.len() + 1;
                sheet_rels.push((
                    format!("rId{rid}"),
                    "http://schemas.microsoft.com/office/2007/relationships/slicer".to_owned(),
                    format!("../slicers/slicer{n}.xml"),
                ));
                slicer_rid = Some(format!("rId{rid}"));
                overrides.push((
                    format!("/xl/drawings/drawing{n}.xml"),
                    "application/vnd.openxmlformats-officedocument.drawing+xml".to_owned(),
                ));
                overrides.push((
                    format!("/xl/slicers/slicer{n}.xml"),
                    "application/vnd.ms-excel.slicer+xml".to_owned(),
                ));
                sheet_parts.push((
                    format!("xl/drawings/drawing{n}.xml"),
                    crate::pivots::slicer_drawing_xml(&on_sheet).into_bytes(),
                ));
                sheet_parts.push((
                    format!("xl/slicers/slicer{n}.xml"),
                    crate::pivots::slicers_xml(&on_sheet).into_bytes(),
                ));
            }
            let mut table_rids = Vec::new();
            for table in &sheet.tables {
                table_number += 1;
                let target = format!("../tables/table{table_number}.xml");
                let rid = sheet_rels.len() + 1;
                sheet_rels.push(rel(rid, "table", &target));
                table_rids.push(format!("rId{rid}"));
                overrides.push((
                    format!("/xl/tables/table{table_number}.xml"),
                    format!("{CT_SML}.table+xml"),
                ));
                let mut xml = format!(
                    r#"{DECL}<table xmlns="{NS_MAIN}" id="{table_number}" name="{}" displayName="{}" ref="{}" totalsRowShown="0"><autoFilter ref="{}"/><tableColumns count="{}">"#,
                    esc(&table.name),
                    esc(&table.name),
                    table.range,
                    table.range,
                    table.columns.len()
                );
                for (n, column) in table.columns.iter().enumerate() {
                    let _ = write!(
                        xml,
                        r#"<tableColumn id="{}" name="{}"/>"#,
                        n + 1,
                        esc(column)
                    );
                }
                xml.push_str(r#"</tableColumns><tableStyleInfo name="TableStyleMedium2" showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/></table>"#);
                sheet_parts.push((
                    format!("xl/tables/table{table_number}.xml"),
                    xml.into_bytes(),
                ));
            }
            // A query table the way Excel writes a text import: a sheet-level
            // part, named by a sheet-scoped ExternalData_N name.
            if let Some(query) = &sheet.query_table {
                query_number += 1;
                let rid = sheet_rels.len() + 1;
                sheet_rels.push(rel(
                    rid,
                    "queryTable",
                    &format!("../queryTables/queryTable{query_number}.xml"),
                ));
                let qt = format!(
                    r#"{DECL}<queryTable xmlns="{NS_MAIN}" name="{}" connectionId="{}" autoFormatId="0" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="1" applyPatternFormats="1" applyAlignmentFormats="0" applyWidthHeightFormats="0"/>"#,
                    esc(&query.name),
                    query.connection
                );
                sheet_parts.push((
                    format!("xl/queryTables/queryTable{query_number}.xml"),
                    qt.into_bytes(),
                ));
                overrides.push((
                    format!("/xl/queryTables/queryTable{query_number}.xml"),
                    format!("{CT_SML}.queryTable+xml"),
                ));
            }
            let mut legacy = None;
            if !sheet.comments.is_empty() {
                comment_number += 1;
                let rid = sheet_rels.len() + 1;
                sheet_rels.push(rel(
                    rid,
                    "comments",
                    &format!("../comments{comment_number}.xml"),
                ));
                let vml_rid = sheet_rels.len() + 1;
                sheet_rels.push(rel(
                    vml_rid,
                    "vmlDrawing",
                    &format!("../drawings/vmlDrawing{comment_number}.vml"),
                ));
                legacy = Some(format!("rId{vml_rid}"));
                overrides.push((
                    format!("/xl/comments{comment_number}.xml"),
                    format!("{CT_SML}.comments+xml"),
                ));
                if !defaults.iter().any(|(ext, _)| *ext == "vml") {
                    defaults.push((
                        "vml",
                        "application/vnd.openxmlformats-officedocument.vmlDrawing",
                    ));
                }
                sheet_parts.push((
                    format!("xl/comments{comment_number}.xml"),
                    comments_xml(&sheet.comments).into_bytes(),
                ));
                sheet_parts.push((
                    format!("xl/drawings/vmlDrawing{comment_number}.vml"),
                    vml_xml(&sheet.comments, index).into_bytes(),
                ));
            }
            let xml = self.sheet_xml(
                sheet,
                index,
                &mut strings,
                &table_rids,
                legacy.as_deref(),
                drawing_rid.as_deref(),
                slicer_rid.as_deref(),
            );
            parts.push((format!("xl/worksheets/{file}"), xml.into_bytes()));
            if !sheet_rels.is_empty() {
                parts.push((
                    format!("xl/worksheets/_rels/{file}.rels"),
                    rels_xml(&sheet_rels).into_bytes(),
                ));
            }
        }
        let mut next = self.sheets.len() + 1;
        wb_rels.push(rel(next, "styles", "styles.xml"));
        overrides.push(("/xl/styles.xml".to_owned(), format!("{CT_SML}.styles+xml")));
        next += 1;
        wb_rels.push(rel(next, "sharedStrings", "sharedStrings.xml"));
        overrides.push((
            "/xl/sharedStrings.xml".to_owned(),
            format!("{CT_SML}.sharedStrings+xml"),
        ));
        if !self.connections.is_empty() {
            next += 1;
            wb_rels.push(rel(next, "connections", "connections.xml"));
            overrides.push((
                "/xl/connections.xml".to_owned(),
                format!("{CT_SML}.connections+xml"),
            ));
        }
        for opaque in &self.opaque {
            next += 1;
            let target = opaque.name.trim_start_matches("xl/");
            wb_rels.push((
                format!("rId{next}"),
                opaque.rel_type.clone(),
                target.to_owned(),
            ));
            overrides.push((format!("/{}", opaque.name), opaque.content_type.clone()));
        }
        // Pivot caches and their records.
        let mut cache_rids = Vec::new();
        let mut late_parts: Vec<(String, Vec<u8>)> = Vec::new();
        for (k, cache) in self.pivot_caches.iter().enumerate() {
            let n = k + 1;
            next += 1;
            wb_rels.push(rel(
                next,
                "pivotCacheDefinition",
                &format!("pivotCache/pivotCacheDefinition{n}.xml"),
            ));
            cache_rids.push(format!("rId{next}"));
            overrides.push((
                format!("/xl/pivotCache/pivotCacheDefinition{n}.xml"),
                format!("{CT_SML}.pivotCacheDefinition+xml"),
            ));
            overrides.push((
                format!("/xl/pivotCache/pivotCacheRecords{n}.xml"),
                format!("{CT_SML}.pivotCacheRecords+xml"),
            ));
            late_parts.push((
                format!("xl/pivotCache/pivotCacheDefinition{n}.xml"),
                crate::pivots::cache_xml(cache, n).into_bytes(),
            ));
            late_parts.push((
                format!("xl/pivotCache/_rels/pivotCacheDefinition{n}.xml.rels"),
                rels_xml(&[rel(
                    1,
                    "pivotCacheRecords",
                    &format!("pivotCacheRecords{n}.xml"),
                )])
                .into_bytes(),
            ));
            late_parts.push((
                format!("xl/pivotCache/pivotCacheRecords{n}.xml"),
                crate::pivots::records_xml(cache).into_bytes(),
            ));
        }
        let mut slicer_cache_rids = Vec::new();
        for (k, slicer) in self.slicers.iter().enumerate() {
            let n = k + 1;
            next += 1;
            wb_rels.push((
                format!("rId{next}"),
                "http://schemas.microsoft.com/office/2007/relationships/slicerCache".to_owned(),
                format!("slicerCaches/slicerCache{n}.xml"),
            ));
            slicer_cache_rids.push(format!("rId{next}"));
            overrides.push((
                format!("/xl/slicerCaches/slicerCache{n}.xml"),
                "application/vnd.ms-excel.slicerCache+xml".to_owned(),
            ));
            late_parts.push((
                format!("xl/slicerCaches/slicerCache{n}.xml"),
                crate::pivots::slicer_cache_xml(slicer, &self.pivots, &self.pivot_caches, &tab_ids)
                    .into_bytes(),
            ));
        }
        // Extra parts and the relationships to them.
        let mut extra_rels: BTreeMap<String, Vec<RelRow>> = BTreeMap::new();
        for e in &self.extra {
            if let Some(ct) = &e.content_type {
                overrides.push((format!("/{}", e.name), ct.clone()));
            }
            if let Some((source, rel_type)) = &e.related_from {
                let dir = source.rsplit_once('/').map_or("", |(d, _)| d);
                let target = relative_to(dir, &e.name);
                if source == "xl/workbook.xml" {
                    next += 1;
                    wb_rels.push((format!("rId{next}"), rel_type.clone(), target));
                } else {
                    let list = extra_rels.entry(source.clone()).or_default();
                    let id = list.len() + 1;
                    list.push((format!("rId{id}"), rel_type.clone(), target));
                }
            }
            late_parts.push((e.name.clone(), e.bytes.clone()));
        }
        for (source, list) in &extra_rels {
            let (dir, file) = source.rsplit_once('/').unwrap_or(("", source));
            let name = if dir.is_empty() {
                format!("_rels/{file}.rels")
            } else {
                format!("{dir}/_rels/{file}.rels")
            };
            late_parts.push((name, rels_xml(list).into_bytes()));
        }

        overrides.push((
            "/docProps/core.xml".to_owned(),
            "application/vnd.openxmlformats-package.core-properties+xml".to_owned(),
        ));
        overrides.push((
            "/docProps/app.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.extended-properties+xml".to_owned(),
        ));
        let mut root_rels = vec![
            rel(1, "officeDocument", "xl/workbook.xml"),
            (
                "rId2".to_owned(),
                "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"
                    .to_owned(),
                "docProps/core.xml".to_owned(),
            ),
            rel(3, "extended-properties", "docProps/app.xml"),
        ];
        if !self.custom.is_empty() {
            root_rels.push(rel(4, "custom-properties", "docProps/custom.xml"));
            overrides.push((
                "/docProps/custom.xml".to_owned(),
                "application/vnd.openxmlformats-officedocument.custom-properties+xml".to_owned(),
            ));
        }

        let mut out: Vec<(String, Vec<u8>)> = Vec::new();
        let mut types = format!(
            r#"{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#
        );
        for (ext, content_type) in &defaults {
            let _ = write!(
                types,
                r#"<Default Extension="{ext}" ContentType="{content_type}"/>"#
            );
        }
        for (part, content_type) in &overrides {
            let _ = write!(
                types,
                r#"<Override PartName="{part}" ContentType="{content_type}"/>"#
            );
        }
        types.push_str("</Types>");
        out.push(("[Content_Types].xml".to_owned(), types.into_bytes()));
        out.push(("_rels/.rels".to_owned(), rels_xml(&root_rels).into_bytes()));
        out.push((
            "docProps/core.xml".to_owned(),
            core_xml(&self.title).into_bytes(),
        ));
        out.push(("docProps/app.xml".to_owned(), app_xml().into_bytes()));
        if !self.custom.is_empty() {
            out.push((
                "docProps/custom.xml".to_owned(),
                custom_xml(&self.custom).into_bytes(),
            ));
        }
        out.push((
            "xl/workbook.xml".to_owned(),
            self.workbook_xml(&cache_rids, &slicer_cache_rids)
                .into_bytes(),
        ));
        out.push((
            "xl/_rels/workbook.xml.rels".to_owned(),
            rels_xml(&wb_rels).into_bytes(),
        ));
        out.extend(parts);
        out.extend(sheet_parts);
        out.push(("xl/styles.xml".to_owned(), styles_xml().into_bytes()));
        out.push((
            "xl/sharedStrings.xml".to_owned(),
            strings.xml().into_bytes(),
        ));
        if !self.connections.is_empty() {
            out.push((
                "xl/connections.xml".to_owned(),
                self.connections_xml().into_bytes(),
            ));
        }
        for opaque in &self.opaque {
            out.push((opaque.name.clone(), opaque.bytes.clone()));
        }
        out.extend(late_parts);
        out
    }

    fn workbook_xml(&self, cache_rids: &[String], slicer_cache_rids: &[String]) -> String {
        let code_name = if self.macro_enabled {
            r#" codeName="ThisWorkbook""#
        } else {
            ""
        };
        let mut out = format!(
            r#"{DECL}<workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><workbookPr{code_name} defaultThemeVersion="166925"/><bookViews><workbookView xWindow="0" yWindow="0" windowWidth="28800" windowHeight="12300"/></bookViews><sheets>"#
        );
        for (index, sheet) in self.sheets.iter().enumerate() {
            let state = match sheet.state {
                SheetState::Visible => "",
                SheetState::Hidden => r#" state="hidden""#,
                SheetState::VeryHidden => r#" state="veryHidden""#,
            };
            let _ = write!(
                out,
                r#"<sheet name="{}" sheetId="{}"{state} r:id="rId{}"/>"#,
                esc(&sheet.name),
                sheet.sheet_id,
                index + 1
            );
        }
        out.push_str("</sheets>");
        let mut names: Vec<Name> = self.names.clone();
        let absolute = |range: &str| {
            range
                .split(':')
                .map(|cell| {
                    let (col, row) = parse_a1(cell);
                    format!("${}${row}", col_name(col))
                })
                .collect::<Vec<_>>()
                .join(":")
        };
        for (index, sheet) in self.sheets.iter().enumerate() {
            if let Some(range) = &sheet.auto_filter {
                names.push(Name {
                    name: "_xlnm._FilterDatabase".to_owned(),
                    refers_to: format!("{}!{}", quote_sheet(&sheet.name), absolute(range)),
                    local: Some(index),
                    hidden: true,
                });
            }
            if let Some(query) = &sheet.query_table {
                names.push(Name {
                    name: query.name.clone(),
                    refers_to: format!("{}!{}", quote_sheet(&sheet.name), absolute(&query.range)),
                    local: Some(index),
                    hidden: false,
                });
            }
        }
        // Excel names every slicer cache.
        for s in &self.slicers {
            names.push(Name {
                name: s.name.clone(),
                refers_to: "#N/A".to_owned(),
                local: None,
                hidden: false,
            });
        }
        if !names.is_empty() {
            // Excel sorts defined names; keep the same order so a re-save
            // doesn't reorder them.
            names.sort_by(|a, b| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
                    .then(a.local.cmp(&b.local))
            });
            out.push_str("<definedNames>");
            for n in &names {
                let local = n
                    .local
                    .map(|i| format!(r#" localSheetId="{i}""#))
                    .unwrap_or_default();
                let hidden = if n.hidden { r#" hidden="1""# } else { "" };
                let _ = write!(
                    out,
                    r#"<definedName name="{}"{local}{hidden}>{}</definedName>"#,
                    esc(&n.name),
                    esc(&n.refers_to)
                );
            }
            out.push_str("</definedNames>");
        }
        out.push_str(r#"<calcPr calcId="191029"/>"#);
        if !cache_rids.is_empty() {
            out.push_str("<pivotCaches>");
            for (k, rid) in cache_rids.iter().enumerate() {
                let _ = write!(out, r#"<pivotCache cacheId="{}" r:id="{rid}"/>"#, k + 1);
            }
            out.push_str("</pivotCaches>");
        }
        if !slicer_cache_rids.is_empty() {
            let _ = write!(
                out,
                r#"<extLst><ext uri="{{BBE1A952-AA13-448e-AADC-164F8A28A991}}" xmlns:x14="{}"><x14:slicerCaches>"#,
                crate::pivots::NS_X14
            );
            for rid in slicer_cache_rids {
                let _ = write!(out, r#"<x14:slicerCache r:id="{rid}"/>"#);
            }
            out.push_str("</x14:slicerCaches></ext></extLst>");
        }
        out.push_str("</workbook>");
        out
    }

    fn connections_xml(&self) -> String {
        let mut out = format!(r#"{DECL}<connections xmlns="{NS_MAIN}">"#);
        for c in &self.connections {
            if let Some(query) = &c.mashup {
                // As Excel writes a Power Query connection.
                let _ = write!(
                    out,
                    r#"<connection id="{}" keepAlive="1" name="{}" description="Connection to the '{}' query in the workbook." type="5" refreshedVersion="8" background="1" saveData="1"><dbPr connection="Provider=Microsoft.Mashup.OleDb.1;Data Source=$Workbook$;Location={};Extended Properties=&quot;&quot;" command="SELECT * FROM [{}]"/></connection>"#,
                    c.id,
                    esc(&c.name),
                    esc(query),
                    esc(query),
                    esc(query)
                );
                continue;
            }
            // As Excel writes a comma-separated text import.
            let _ = write!(
                out,
                r#"<connection id="{}" name="{}" type="6" refreshedVersion="8" background="1" saveData="1"><textPr prompt="0" sourceFile="{}" comma="1"><textFields><textField/></textFields></textPr></connection>"#,
                c.id,
                esc(&c.name),
                esc(&c.source_file)
            );
        }
        out.push_str("</connections>");
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn sheet_xml(
        &self,
        sheet: &Sheet,
        index: usize,
        strings: &mut Strings,
        table_rids: &[String],
        legacy: Option<&str>,
        drawing: Option<&str>,
        slicers: Option<&str>,
    ) -> String {
        let selected = if index == 0 {
            r#" tabSelected="1""#
        } else {
            ""
        };
        let pane = if sheet.freeze_header {
            r#"<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/><selection pane="bottomLeft" activeCell="A2" sqref="A2"/>"#
        } else {
            ""
        };
        let mut out = format!(
            r#"{DECL}<worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><dimension ref="{}"/><sheetViews><sheetView{selected} workbookViewId="0">{pane}</sheetView></sheetViews><sheetFormatPr defaultRowHeight="15"/>"#,
            sheet.dimension()
        );
        if !sheet.cols.is_empty() {
            out.push_str("<cols>");
            for (min, max, width) in &sheet.cols {
                let _ = write!(
                    out,
                    r#"<col min="{min}" max="{max}" width="{width}" customWidth="1"/>"#
                );
            }
            out.push_str("</cols>");
        }
        out.push_str("<sheetData>");
        for row in &sheet.rows {
            let hidden = if row.hidden { r#" hidden="1""# } else { "" };
            if row.cells.is_empty() {
                let _ = write!(out, r#"<row r="{}"{hidden}/>"#, row.r);
                continue;
            }
            let first = row.cells.first().map_or(1, |c| c.col);
            let last = row.cells.last().map_or(1, |c| c.col);
            let _ = write!(out, r#"<row r="{}" spans="{first}:{last}"{hidden}>"#, row.r);
            for cell in &row.cells {
                out.push_str(&cell_xml(cell, row.r, strings));
            }
            out.push_str("</row>");
        }
        out.push_str("</sheetData>");
        if let Some(range) = &sheet.auto_filter {
            let _ = write!(out, r#"<autoFilter ref="{range}"/>"#);
        }
        if !sheet.merged.is_empty() {
            let _ = write!(out, r#"<mergeCells count="{}">"#, sheet.merged.len());
            for range in &sheet.merged {
                let _ = write!(out, r#"<mergeCell ref="{range}"/>"#);
            }
            out.push_str("</mergeCells>");
        }
        for (n, (range, f)) in sheet.conditional.iter().enumerate() {
            let _ = write!(
                out,
                r#"<conditionalFormatting sqref="{range}"><cfRule type="expression" dxfId="0" priority="{}"><formula>{}</formula></cfRule></conditionalFormatting>"#,
                n + 1,
                esc(f)
            );
        }
        out.push_str(r#"<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>"#);
        if let Some(rid) = drawing {
            let _ = write!(out, r#"<drawing r:id="{rid}"/>"#);
        }
        if let Some(rid) = legacy {
            let _ = write!(out, r#"<legacyDrawing r:id="{rid}"/>"#);
        }
        if !table_rids.is_empty() {
            let _ = write!(out, r#"<tableParts count="{}">"#, table_rids.len());
            for rid in table_rids {
                let _ = write!(out, r#"<tablePart r:id="{rid}"/>"#);
            }
            out.push_str("</tableParts>");
        }
        if let Some(rid) = slicers {
            let _ = write!(
                out,
                r#"<extLst><ext uri="{{A8765BA9-456A-4dab-B4F3-ACF838C121DE}}" xmlns:x14="{}"><x14:slicerList><x14:slicer r:id="{rid}"/></x14:slicerList></ext></extLst>"#,
                crate::pivots::NS_X14
            );
        }
        out.push_str("</worksheet>");
        let _ = self;
        out
    }
}

/// A sheet name as a formula writes it: quoted when it needs to be.
#[must_use]
pub fn quote_sheet(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
    {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use super::*;

    fn sample() -> Workbook {
        let mut wb = Workbook::new("Sample");
        let mut sheet = Sheet::new("Data", 1);
        sheet.push_row(vec![
            Some((text("Name"), Style::Bold)),
            Some((text("Qty"), Style::Bold)),
        ]);
        sheet.push_row(vec![
            Some((text("Bolts"), Style::General)),
            Some((num("12"), Style::General)),
        ]);
        sheet.tables.push(Table {
            name: "Stock".to_owned(),
            range: "A1:B2".to_owned(),
            columns: vec!["Name".to_owned(), "Qty".to_owned()],
        });
        sheet.query_table = Some(QueryTable {
            name: "ExternalData_1".to_owned(),
            connection: 1,
            range: "D1:E2".to_owned(),
        });
        sheet.comments.push(Comment {
            cell: "B2".to_owned(),
            author: "Ari".to_owned(),
            text: "Counted on Monday".to_owned(),
        });
        wb.sheets.push(sheet);
        wb.connections.push(Connection {
            id: 1,
            name: "stock".to_owned(),
            source_file: r"C:\stock.csv".to_owned(),
            mashup: None,
        });
        wb.custom.push(("Client".to_owned(), "Example".to_owned()));
        wb
    }

    #[test]
    fn every_part_has_a_content_type_and_parses_as_utf8() {
        let wb = sample();
        let mut zip = zip::ZipArchive::new(Cursor::new(wb.to_bytes())).expect("a zip");
        let mut types = String::new();
        zip.by_name("[Content_Types].xml")
            .expect("types")
            .read_to_string(&mut types)
            .expect("utf-8");
        let names: Vec<String> = wb.parts().into_iter().map(|(n, _)| n).collect();
        for name in &names {
            if name.ends_with(".xml") && name != "[Content_Types].xml" {
                assert!(
                    types.contains(&format!("\"/{name}\"")),
                    "{name} has no override"
                );
            }
            let mut text = String::new();
            if !name.ends_with(".bin") {
                zip.by_name(name)
                    .expect("entry")
                    .read_to_string(&mut text)
                    .expect("utf-8");
                assert!(text.starts_with('<'), "{name}");
            }
        }
        for part in [
            "xl/tables/table1.xml",
            "xl/queryTables/queryTable1.xml",
            "xl/comments1.xml",
            "xl/drawings/vmlDrawing1.vml",
            "xl/connections.xml",
            "docProps/custom.xml",
        ] {
            assert!(names.contains(&part.to_owned()), "{part}");
        }
    }

    #[test]
    fn output_is_deterministic() {
        assert_eq!(sample().to_bytes(), sample().to_bytes());
    }

    #[test]
    fn columns_are_named_like_excel() {
        assert_eq!(col_name(1), "A");
        assert_eq!(col_name(26), "Z");
        assert_eq!(col_name(27), "AA");
        assert_eq!(col_name(702), "ZZ");
        assert_eq!(col_name(703), "AAA");
        assert_eq!(parse_a1("AA10"), (27, 10));
        assert_eq!(parse_a1(&a1(16384, 1_048_576)), (16384, 1_048_576));
    }

    #[test]
    fn sheet_names_are_quoted_when_needed() {
        assert_eq!(quote_sheet("Revenue"), "Revenue");
        assert_eq!(quote_sheet("P&L"), "'P&L'");
        assert_eq!(quote_sheet("Use of funds"), "'Use of funds'");
        assert_eq!(quote_sheet("Bob's"), "'Bob''s'");
    }
}
