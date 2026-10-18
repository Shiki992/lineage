// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Reading one worksheet part, streamed (decision 0025). Cells point into the
//! part's text; everything outside `<sheetData>` becomes keys.

use quick_xml::events::{BytesStart, Event};

use super::model::{
    Area, Cached, Cell, ColInfo, Content, Formula, FormulaKind, Row, SheetKeys, Span, Str,
};
use super::refs::{MAX_COL, MAX_ROW, is_number, parse_area, parse_cell};
use super::stream::{Pull, attr, rel_attr, span, truthy};
use crate::error::{Error, Result};
use crate::hash::Fnv;

/// Most cells one workbook may hold. Beyond this the engine refuses the
/// file rather than run the task pane out of memory.
pub const MAX_CELLS: usize = 40_000_000;

/// What a worksheet part holds.
#[derive(Debug, Default)]
pub(super) struct SheetRead {
    pub rows: Vec<Row>,
    pub cols: Vec<ColInfo>,
    pub merged: Vec<Area>,
    pub auto_filter: Option<Area>,
    pub keys: SheetKeys,
    pub sheet_data: Span,
    /// Relationship ids of drawings, notes and tables the sheet points at.
    pub drawing_rids: Vec<String>,
    pub legacy_rids: Vec<String>,
    pub table_rids: Vec<String>,
}

/// Reads a worksheet part. `cells` counts cells across the workbook.
///
/// # Errors
/// [`Error::BadXml`] for XML that isn't a worksheet, [`Error::Damaged`] for
/// cells out of order or out of range, [`Error::TooLarge`] past
/// [`MAX_CELLS`], [`Error::Unsupported`] for Strict Open XML.
pub(super) fn read(part: &str, text: &str, cells: &mut usize) -> Result<SheetRead> {
    let mut pull = Pull::new(part, text);
    let mut out = SheetRead::default();
    let mut depth = 0usize;
    let mut format = Fnv::new();
    let mut conditional = Fnv::new();
    let mut validation = Fnv::new();
    let mut protection = Fnv::new();
    let mut hyperlinks = Fnv::new();
    let mut filter = Fnv::new();
    let mut page = Fnv::new();
    let mut other = Fnv::new();
    let mut seen_root = false;
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                if depth == 0 {
                    if ev.strict {
                        return Err(Error::Unsupported("Strict Open XML".to_owned()));
                    }
                    if !ev.main || e.local_name().as_ref() != "worksheet" {
                        return Err(pull.bad("not a worksheet"));
                    }
                    seen_root = true;
                    if empty {
                        break;
                    }
                    depth = 1;
                    continue;
                }
                let local = e.local_name().as_ref().to_owned();
                let main = ev.main;
                if main {
                    match local.as_str() {
                        "sheetData" => {
                            if empty {
                                out.sheet_data = span(ev.at, ev.end)?;
                            } else {
                                read_rows(&mut pull, &mut out, cells, ev.at)?;
                            }
                            continue;
                        }
                        "cols" => {
                            if !empty {
                                read_cols(&mut pull, &mut out)?;
                            }
                            continue;
                        }
                        "mergeCells" => {
                            if !empty {
                                read_merged(&mut pull, &mut out)?;
                            }
                            continue;
                        }
                        "autoFilter" => {
                            out.auto_filter = attr(e, "ref").as_deref().and_then(parse_area);
                            // Only the criteria: the range grows with the
                            // rows, which are their own change.
                            if !empty {
                                pull.hash_rest(&mut filter)?;
                            }
                            continue;
                        }
                        "drawing" | "picture" => {
                            out.drawing_rids.extend(rel_attr(&pull, e, "id"));
                            if !empty {
                                pull.hash_rest(&mut Fnv::new())?;
                            }
                            continue;
                        }
                        "legacyDrawing" => {
                            out.legacy_rids.extend(rel_attr(&pull, e, "id"));
                            if !empty {
                                pull.hash_rest(&mut Fnv::new())?;
                            }
                            continue;
                        }
                        "tableParts" => {
                            if !empty {
                                read_table_parts(&mut pull, &mut out)?;
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
                let key: Option<&mut Fnv> = if !main {
                    Some(&mut other)
                } else {
                    match local.as_str() {
                        "sheetPr" | "sheetFormatPr" => Some(&mut format),
                        "conditionalFormatting" => Some(&mut conditional),
                        "dataValidations" => Some(&mut validation),
                        "sheetProtection" | "protectedRanges" => Some(&mut protection),
                        "hyperlinks" => Some(&mut hyperlinks),
                        "sortState" => Some(&mut filter),
                        "printOptions" | "pageMargins" | "pageSetup" | "headerFooter"
                        | "rowBreaks" | "colBreaks" => Some(&mut page),
                        // UI state, renewed by every save: where the cursor
                        // was, the zoom, the used range.
                        "sheetViews" | "dimension" | "sheetCalcPr" => None,
                        _ => Some(&mut other),
                    }
                };
                // The default row height follows Excel's font metrics (15
                // in one program, 14.5 in another) unless someone set it.
                // Whether a filter is on is the filter's own change.
                if main && (local == "sheetFormatPr" || local == "sheetPr") {
                    let custom = truthy(attr(e, "customHeight").as_deref());
                    // An element left with nothing in it is no setting.
                    let mut own = Fnv::new();
                    let mut any = !empty;
                    for a in e.attributes().flatten() {
                        let k = a.key.0;
                        if k.contains(':')
                            || (k == "defaultRowHeight" && !custom)
                            || k == "filterMode"
                        {
                            continue;
                        }
                        any = true;
                        own.str(k).str(&a.value);
                    }
                    if !empty {
                        pull.hash_rest(&mut own)?;
                    }
                    if any {
                        format.str(&local).u64(own.finish());
                    }
                    continue;
                }
                match key {
                    Some(h) => {
                        pull.hash_start(h, e);
                        if !empty {
                            pull.hash_rest(h)?;
                        }
                    }
                    None => {
                        if !empty {
                            pull.hash_rest(&mut Fnv::new())?;
                        }
                    }
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen_root {
        return Err(pull.bad("no worksheet"));
    }
    out.keys = SheetKeys {
        format: format.finish(),
        conditional: conditional.finish(),
        validation: validation.finish(),
        protection: protection.finish(),
        hyperlinks: hyperlinks.finish(),
        filter: filter.finish(),
        page: page.finish(),
        other: other.finish(),
    };
    Ok(out)
}

fn read_cols(pull: &mut Pull<'_>, out: &mut SheetRead) -> Result<()> {
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                if ev.main && e.local_name().as_ref() == "col" {
                    let num = |name| attr(e, name).and_then(|v| v.parse::<u32>().ok());
                    let (Some(min), Some(max)) = (num("min"), num("max")) else {
                        return Err(pull.bad("a column without min or max"));
                    };
                    if min == 0 || min > max || max > MAX_COL {
                        return Err(Error::Damaged("a column range is out of order".to_owned()));
                    }
                    let mut h = Fnv::new();
                    if truthy(attr(e, "customWidth").as_deref()) {
                        h.str("w").str(&attr(e, "width").unwrap_or_default());
                    }
                    for name in ["style", "outlineLevel", "collapsed"] {
                        h.str(name).str(&attr(e, name).unwrap_or_default());
                    }
                    out.cols.push(ColInfo {
                        min,
                        max,
                        hidden: truthy(attr(e, "hidden").as_deref()),
                        format: h.finish(),
                    });
                }
                if !empty {
                    pull.hash_rest(&mut Fnv::new())?;
                }
            }
            Event::End(_) => return Ok(()),
            Event::Eof => return Err(pull.bad("ends inside cols")),
            _ => {}
        }
    }
}

fn read_merged(pull: &mut Pull<'_>, out: &mut SheetRead) -> Result<()> {
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                if ev.main
                    && e.local_name().as_ref() == "mergeCell"
                    && let Some(area) = attr(e, "ref").as_deref().and_then(parse_area)
                {
                    out.merged.push(area);
                }
                if !empty {
                    pull.hash_rest(&mut Fnv::new())?;
                }
            }
            Event::End(_) => return Ok(()),
            Event::Eof => return Err(pull.bad("ends inside mergeCells")),
            _ => {}
        }
    }
}

fn read_table_parts(pull: &mut Pull<'_>, out: &mut SheetRead) -> Result<()> {
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                if ev.main && e.local_name().as_ref() == "tablePart" {
                    out.table_rids.extend(rel_attr(pull, e, "id"));
                }
                if !empty {
                    pull.hash_rest(&mut Fnv::new())?;
                }
            }
            Event::End(_) => return Ok(()),
            Event::Eof => return Err(pull.bad("ends inside tableParts")),
            _ => {}
        }
    }
}

fn row_format(e: &BytesStart<'_>) -> u64 {
    let mut h = Fnv::new();
    if truthy(attr(e, "customFormat").as_deref()) {
        h.str("s").str(&attr(e, "s").unwrap_or_default());
    }
    if truthy(attr(e, "customHeight").as_deref()) {
        h.str("ht").str(&attr(e, "ht").unwrap_or_default());
    }
    for name in ["outlineLevel", "collapsed", "thickTop", "thickBot"] {
        if let Some(v) = attr(e, name) {
            h.str(name).str(&v);
        }
    }
    h.finish()
}

fn read_rows(
    pull: &mut Pull<'_>,
    out: &mut SheetRead,
    cells: &mut usize,
    data_start: usize,
) -> Result<()> {
    let mut last_r = 0u32;
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                if !(ev.main && e.local_name().as_ref() == "row") {
                    if !empty {
                        pull.hash_rest(&mut Fnv::new())?;
                    }
                    continue;
                }
                let r = match attr(e, "r") {
                    Some(v) => v
                        .parse::<u32>()
                        .map_err(|_| Error::Damaged("a row number isn't a number".to_owned()))?,
                    None => last_r + 1,
                };
                if r == 0 || r > MAX_ROW || r <= last_r {
                    return Err(Error::Damaged("rows are out of order".to_owned()));
                }
                last_r = r;
                let hidden = truthy(attr(e, "hidden").as_deref());
                let format = row_format(e);
                let row_cells = if empty {
                    Vec::new()
                } else {
                    read_cells(pull, r, cells)?
                };
                let end = pull_pos(pull);
                out.rows.push(Row {
                    r,
                    span: span(ev.at, if empty { ev.end } else { end })?,
                    hidden,
                    format,
                    cells: row_cells,
                });
            }
            Event::End(_) => {
                out.sheet_data = span(data_start, ev.end)?;
                return Ok(());
            }
            Event::Eof => return Err(pull.bad("ends inside sheetData")),
            _ => {}
        }
    }
}

fn pull_pos(pull: &Pull<'_>) -> usize {
    usize::try_from(pull.reader.buffer_position()).unwrap_or(usize::MAX)
}

/// One `<c>` as read, before its type decides what it holds.
#[derive(Default)]
struct RawCell {
    value: Option<Str>,
    formula: Option<(Option<Str>, FormulaKind, bool, Span)>,
    inline: Option<String>,
}

fn read_cells(pull: &mut Pull<'_>, r: u32, cells: &mut usize) -> Result<Vec<Cell>> {
    let mut out: Vec<Cell> = Vec::new();
    let mut last_col = 0u32;
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                if !(ev.main && e.local_name().as_ref() == "c") {
                    if !empty {
                        pull.hash_rest(&mut Fnv::new())?;
                    }
                    continue;
                }
                // One pass over the attributes, borrowing: this runs for
                // every cell of a sheet.
                let (mut at_r, mut s_attr, mut t) = (None, None, None);
                let (mut dynamic, mut value_meta) = (false, false);
                for a in e.attributes().flatten() {
                    match a.key.0 {
                        "r" => at_r = Some(a.value),
                        "s" => s_attr = Some(a.value),
                        "t" => t = Some(a.value.into_owned()),
                        "cm" => dynamic = true,
                        "vm" => value_meta = true,
                        _ => {}
                    }
                }
                let col = match at_r {
                    Some(at) => {
                        let (col, row) = parse_cell(&at).ok_or_else(|| {
                            Error::Damaged("a cell reference is wrong".to_owned())
                        })?;
                        if row != r {
                            return Err(Error::Damaged("a cell is in the wrong row".to_owned()));
                        }
                        col
                    }
                    None => last_col + 1,
                };
                if col == 0 || col > MAX_COL || col <= last_col {
                    return Err(Error::Damaged("cells are out of order".to_owned()));
                }
                last_col = col;
                *cells += 1;
                if *cells > MAX_CELLS {
                    return Err(Error::TooLarge("too many cells".to_owned()));
                }
                let style = s_attr.and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
                let raw = if empty {
                    RawCell::default()
                } else {
                    read_cell_body(pull)?
                };
                let element = span(ev.at, if empty { ev.end } else { pull_pos(pull) })?;
                let content = content_of(pull, t.as_deref(), raw, dynamic)?;
                out.push(Cell {
                    col,
                    style,
                    content,
                    value_meta,
                    element,
                });
            }
            Event::End(_) => return Ok(out),
            Event::Eof => return Err(pull.bad("ends inside a row")),
            _ => {}
        }
    }
}

fn read_cell_body(pull: &mut Pull<'_>) -> Result<RawCell> {
    let mut raw = RawCell::default();
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                let name = e.local_name();
                let local: &str = name.as_ref();
                match (ev.main, local) {
                    (true, "v") => {
                        raw.value = Some(if empty {
                            Str::At(span(ev.end, ev.end)?)
                        } else {
                            pull.text_until_end(ev.end)?
                        });
                    }
                    (true, "f") => {
                        let kind = match attr(e, "t").as_deref() {
                            Some("array") => FormulaKind::Array {
                                range: attr(e, "ref").as_deref().and_then(parse_area),
                            },
                            Some("shared") => FormulaKind::Shared {
                                si: attr(e, "si")
                                    .and_then(|v| v.parse().ok())
                                    .ok_or_else(|| pull.bad("a shared formula without si"))?,
                                range: attr(e, "ref").as_deref().and_then(parse_area),
                            },
                            Some("dataTable") => FormulaKind::DataTable,
                            _ => FormulaKind::Normal,
                        };
                        let always = truthy(attr(e, "ca").as_deref());
                        let text = if empty {
                            None
                        } else {
                            let s = pull.text_until_end(ev.end)?;
                            match &s {
                                Str::At(span) if span.len == 0 => None,
                                Str::Own(o) if o.is_empty() => None,
                                _ => Some(s),
                            }
                        };
                        let element = span(ev.at, if empty { ev.end } else { pull_pos(pull) })?;
                        raw.formula = Some((text, kind, always, element));
                    }
                    (true, "is") => {
                        raw.inline = Some(if empty {
                            String::new()
                        } else {
                            read_inline(pull)?
                        });
                    }
                    _ => {
                        if !empty {
                            pull.hash_rest(&mut Fnv::new())?;
                        }
                    }
                }
            }
            Event::End(_) => return Ok(raw),
            Event::Eof => return Err(pull.bad("ends inside a cell")),
            _ => {}
        }
    }
}

/// The text of `<is>` or `<si>`: every `<t>`, directly or in runs, leaving
/// out phonetic guides (`<rPh>`).
pub(super) fn read_inline(pull: &mut Pull<'_>) -> Result<String> {
    let mut text = String::new();
    let mut depth = 1usize;
    let mut phonetic = 0usize;
    while depth > 0 {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) => {
                let local = e.local_name().as_ref().to_owned();
                if ev.main && local == "t" && phonetic == 0 {
                    let s = pull.text_until_end(ev.end)?;
                    text.push_str(s.get(pull.text));
                    continue;
                }
                if ev.main && local == "rPh" {
                    phonetic += 1;
                }
                depth += 1;
            }
            Event::End(ref e) => {
                if ev.main && e.local_name().as_ref() == "rPh" {
                    phonetic = phonetic.saturating_sub(1);
                }
                depth -= 1;
            }
            Event::Eof => return Err(pull.bad("ends inside text")),
            _ => {}
        }
    }
    Ok(text)
}

fn number_span(pull: &Pull<'_>, value: &Str) -> Result<Span> {
    match value {
        Str::At(span) if is_number(span.get(pull.text)) => Ok(*span),
        _ => Err(Error::Damaged(
            "a number cell holds something else".to_owned(),
        )),
    }
}

fn content_of(pull: &Pull<'_>, t: Option<&str>, raw: RawCell, dynamic: bool) -> Result<Content> {
    if let Some((text, kind, always, element)) = raw.formula {
        let cached = match (&raw.value, t) {
            (None, _) => None,
            (Some(v), Some("str")) => Some(Cached::Text(v.clone())),
            (Some(v), Some("b")) => Some(Cached::Bool(v.get(pull.text) == "1")),
            (Some(v), Some("e")) => Some(Cached::Error(v.clone())),
            (Some(v), _) => {
                if v.get(pull.text).is_empty() {
                    None
                } else {
                    Some(Cached::Number(Str::At(number_span(pull, v)?)))
                }
            }
        };
        return Ok(Content::Formula(Box::new(Formula {
            text,
            kind,
            dynamic,
            always,
            element,
            cached,
        })));
    }
    Ok(match t {
        Some("s") => match &raw.value {
            Some(v) => Content::Shared(
                v.get(pull.text)
                    .trim()
                    .parse()
                    .map_err(|_| Error::Damaged("a shared string index is wrong".to_owned()))?,
            ),
            None => Content::Blank,
        },
        Some("inlineStr") => Content::Inline(raw.inline.unwrap_or_default().into_boxed_str()),
        // Some programs write text as `t="str"` without a formula.
        Some("str") => match raw.value {
            Some(v) => Content::Inline(v.get(pull.text).into()),
            None => Content::Blank,
        },
        Some("b") => match &raw.value {
            Some(v) => Content::Bool(v.get(pull.text).trim() == "1"),
            None => Content::Blank,
        },
        Some("e") => raw.value.map_or(Content::Blank, Content::Error),
        Some("d") => match raw.value {
            Some(Str::At(span)) => Content::IsoDate(span),
            Some(Str::Own(_)) => return Err(Error::Damaged("a date cell is wrong".to_owned())),
            None => Content::Blank,
        },
        None | Some("n") => match &raw.value {
            Some(v) if !v.get(pull.text).is_empty() => Content::Number(number_span(pull, v)?),
            _ => Content::Blank,
        },
        Some(_) => return Err(Error::Damaged("a cell type is unknown".to_owned())),
    })
}
