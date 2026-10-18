// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Lining up two versions of a workbook before any cell is compared
//! (decision 0021): sheets, then columns, then rows. A row that was sorted,
//! inserted, deleted or moved keeps its identity, so an edit is never put on
//! the wrong record. Rows that can't be matched with confidence are
//! reported as such, never guessed.
//!
//! Rows are matched, in order of trust:
//!
//! 1. by a key column (unique, filled in and stable on both sides; ID-like
//!    columns first);
//! 2. by a composite key of two columns;
//! 3. by their whole content, so a re-sorted sheet matches fully;
//! 4. by similarity, when one candidate is clearly best;
//! 5. by position between matched rows, for rows that hold only formulas.
//!
//! What is left is inserted, deleted, or "could not match".

use std::collections::{HashMap, HashSet};

use super::Workbook;
use super::model::{Content, Row, Sheet};
use super::value::{self, Value};
use crate::hash::Fnv;

/// How two versions' sheets pair up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetPairs {
    /// (sheet in a, sheet in b).
    pub both: Vec<(usize, usize)>,
    pub only_a: Vec<usize>,
    pub only_b: Vec<usize>,
}

/// One axis (rows or columns) of two sheets, lined up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matching {
    /// Matched (a, b), in a's order.
    pub pairs: Vec<(u32, u32)>,
    pub a_to_b: HashMap<u32, u32>,
    pub b_to_a: HashMap<u32, u32>,
    /// In a only: deleted.
    pub only_a: Vec<u32>,
    /// In b only: inserted.
    pub only_b: Vec<u32>,
    /// Rows that could not be matched with confidence, on each side. They
    /// are neither matched nor counted as inserted or deleted.
    pub unsure_a: Vec<u32>,
    pub unsure_b: Vec<u32>,
    /// b's matched entries that are out of the longest common order: moved.
    pub moved_b: Vec<u32>,
}

impl Matching {
    fn pair(&mut self, a: u32, b: u32) {
        self.a_to_b.insert(a, b);
        self.b_to_a.insert(b, a);
    }

    fn finish(&mut self, all_a: &[u32], all_b: &[u32]) {
        let unsure_a: HashSet<u32> = self.unsure_a.iter().copied().collect();
        let unsure_b: HashSet<u32> = self.unsure_b.iter().copied().collect();
        self.pairs = all_a
            .iter()
            .filter_map(|a| self.a_to_b.get(a).map(|b| (*a, *b)))
            .collect();
        self.only_a = all_a
            .iter()
            .copied()
            .filter(|a| !self.a_to_b.contains_key(a) && !unsure_a.contains(a))
            .collect();
        self.only_b = all_b
            .iter()
            .copied()
            .filter(|b| !self.b_to_a.contains_key(b) && !unsure_b.contains(b))
            .collect();
        self.unsure_a.sort_unstable();
        self.unsure_b.sort_unstable();
        let order: Vec<u32> = self.pairs.iter().map(|(_, b)| *b).collect();
        let keep = longest_increasing(&order);
        self.moved_b = order
            .iter()
            .enumerate()
            .filter(|(i, _)| !keep.contains(i))
            .map(|(_, b)| *b)
            .collect();
        self.moved_b.sort_unstable();
    }
}

/// How the rows were keyed, for the panel and the decision log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKey {
    /// A key column: its column in b and its header.
    Column(u32, String),
    /// Two columns together.
    Composite(u32, u32),
    /// No key: content and position.
    None,
}

/// Two sheets lined up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetAlignment {
    pub cols: Matching,
    pub rows: Matching,
    pub key: RowKey,
    /// The header row on each side, when there is one.
    pub header: (Option<u32>, Option<u32>),
    /// b's rows are sorted by this column of b (descending when true), and
    /// a's were not: the rows were re-sorted.
    pub sorted_by: Option<(u32, bool)>,
}

/// The longest increasing subsequence's positions in `seq`.
fn longest_increasing(seq: &[u32]) -> HashSet<usize> {
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; seq.len()];
    for (i, &x) in seq.iter().enumerate() {
        let pos = tails.partition_point(|&t| seq[t] < x);
        if pos > 0 {
            prev[i] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
    }
    let mut out = HashSet::new();
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.insert(i);
        at = prev[i];
    }
    out
}

/// The longest common subsequence of two key sequences, as index pairs.
/// Quadratic, so only for short sequences (columns, header rows).
fn lcs(a: &[u64], b: &[u64]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut t = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i][j] = if a[i] == b[j] && a[i] != 0 {
                t[i + 1][j + 1] + 1
            } else {
                t[i + 1][j].max(t[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] && a[i] != 0 {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t[i + 1][j] >= t[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Pairs sheets: by name, then by sheet id (a rename keeps it), then by
/// content for what is left with equal counts.
#[must_use]
pub fn pair_sheets(a: &Workbook, b: &Workbook) -> SheetPairs {
    let mut both = Vec::new();
    let mut used_b = HashSet::new();
    let mut used_a = HashSet::new();
    for (i, s) in a.sheets.iter().enumerate() {
        if let Some(j) = b.sheet_index(&s.name)
            && b.sheets[j].kind == s.kind
        {
            both.push((i, j));
            used_a.insert(i);
            used_b.insert(j);
        }
    }
    for (i, s) in a.sheets.iter().enumerate() {
        if used_a.contains(&i) || s.sheet_id == 0 {
            continue;
        }
        if let Some(j) = b
            .sheets
            .iter()
            .position(|t| t.sheet_id == s.sheet_id && t.kind == s.kind)
            .filter(|j| !used_b.contains(j))
        {
            both.push((i, j));
            used_a.insert(i);
            used_b.insert(j);
        }
    }
    both.sort_unstable();
    SheetPairs {
        both,
        only_a: (0..a.sheets.len())
            .filter(|i| !used_a.contains(i))
            .collect(),
        only_b: (0..b.sheets.len())
            .filter(|j| !used_b.contains(j))
            .collect(),
    }
}

/// One side of a sheet pair, with what alignment needs ready.
struct Side<'a> {
    wb: &'a Workbook,
    sheet: &'a Sheet,
    text: &'a str,
    header: Option<u32>,
}

impl<'a> Side<'a> {
    fn new(wb: &'a Workbook, index: usize) -> Self {
        let sheet = &wb.sheets[index];
        let text = wb.sheet_text(index);
        let mut side = Self {
            wb,
            sheet,
            text,
            header: None,
        };
        side.header = side.find_header(index);
        side
    }

    fn value(&self, row: &'a Row, col: u32) -> Value<'a> {
        row.cells
            .binary_search_by_key(&col, |c| c.col)
            .ok()
            .map_or(Value::Blank, |i| {
                value::either(self.wb, self.text, &row.cells[i])
            })
    }

    fn input(&self, row: &'a Row, col: u32) -> Option<Value<'a>> {
        let i = row.cells.binary_search_by_key(&col, |c| c.col).ok()?;
        value::input(self.wb, self.text, &row.cells[i])
    }

    /// A table's header row, or the first row near the top that is all text
    /// (two cells or more) with rows below it.
    fn find_header(&self, index: usize) -> Option<u32> {
        if let Some(t) = self
            .wb
            .tables
            .iter()
            .filter(|t| t.sheet == index && t.header_rows > 0)
            .min_by_key(|t| t.area.row1)
        {
            return Some(t.area.row1);
        }
        let last = self.sheet.rows.last()?.r;
        for row in self.sheet.rows.iter().take(10) {
            let filled: Vec<Value<'_>> = row
                .cells
                .iter()
                .map(|c| value::either(self.wb, self.text, c))
                .filter(|v| !v.is_blank())
                .collect();
            if filled.len() >= 2
                && filled.iter().all(|v| matches!(v, Value::Text(_)))
                && row
                    .cells
                    .iter()
                    .all(|c| !matches!(c.content, Content::Formula(_)))
                && row.r < last
            {
                return Some(row.r);
            }
        }
        None
    }

    fn header_text(&self, col: u32) -> Option<String> {
        let row = self.sheet.row(self.header?)?;
        match self.value(row, col) {
            Value::Text(t) if !t.trim().is_empty() => Some(t.trim().to_lowercase()),
            _ => None,
        }
    }

    fn max_col(&self) -> u32 {
        self.sheet
            .rows
            .iter()
            .filter_map(|r| r.cells.last().map(|c| c.col))
            .max()
            .unwrap_or(0)
    }

    fn data_rows(&self) -> impl Iterator<Item = &'a Row> + '_ {
        let header = self.header.unwrap_or(0);
        self.sheet.rows.iter().filter(move |r| r.r > header)
    }

    /// An order-free key of a column's data: the same values in any order
    /// give the same key.
    fn column_content(&self, col: u32) -> u64 {
        let mut sum = 0u64;
        let mut any = false;
        for row in self.data_rows() {
            let v = self.value(row, col);
            if !v.is_blank() {
                any = true;
                sum = sum.wrapping_add(v.key().wrapping_mul(0x9E37_79B9_7F4A_7C15));
            }
        }
        if any { sum.max(1) } else { 0 }
    }
}

/// Lines up columns: by header text where there are headers, by content
/// otherwise, then by position in the gaps between matched columns when
/// both gaps are the same size.
fn align_cols(a: &Side<'_>, b: &Side<'_>) -> Matching {
    let (na, nb) = (a.max_col(), b.max_col());
    let key = |side: &Side<'_>, c: u32| -> u64 {
        match side.header_text(c) {
            Some(h) => {
                let mut f = Fnv::new();
                f.str("h").str(&h);
                f.finish()
            }
            None => side.column_content(c),
        }
    };
    let ka: Vec<u64> = (1..=na).map(|c| key(a, c)).collect();
    let kb: Vec<u64> = (1..=nb).map(|c| key(b, c)).collect();
    let mut m = Matching::default();
    let anchors = lcs(&ka, &kb);
    let mut prev = (0usize, 0usize);
    let mut gaps = Vec::new();
    for &(i, j) in &anchors {
        gaps.push((prev.0..i, prev.1..j));
        m.pair(
            u32::try_from(i).unwrap_or(0) + 1,
            u32::try_from(j).unwrap_or(0) + 1,
        );
        prev = (i + 1, j + 1);
    }
    gaps.push((prev.0..ka.len(), prev.1..kb.len()));
    for (ga, gb) in gaps {
        // A column renamed or rewritten, where nothing else moved.
        if ga.len() == gb.len() {
            for (i, j) in ga.zip(gb) {
                m.pair(
                    u32::try_from(i).unwrap_or(0) + 1,
                    u32::try_from(j).unwrap_or(0) + 1,
                );
            }
        }
    }
    let all_a: Vec<u32> = (1..=na).collect();
    let all_b: Vec<u32> = (1..=nb).collect();
    m.finish(&all_a, &all_b);
    m
}

/// Words that mark an identifier column in a header.
fn id_like_header(h: &str) -> bool {
    let h = h.to_lowercase();
    h == "id"
        || h.ends_with(" id")
        || h.starts_with("id ")
        || h.contains("code")
        || h.contains("ref")
        || h.contains("number")
        || h.contains("no.")
        || h.contains('#')
        || h.contains("key")
        || h.contains("sku")
}

/// True for text with letters and digits ("APP-007", "ORD-000123"), or a
/// whole number.
fn id_like_value(v: &Value<'_>) -> bool {
    match v {
        Value::Text(t) => {
            t.chars().any(|c| c.is_ascii_digit()) && t.chars().any(char::is_alphabetic)
        }
        Value::Number(n) => n.bytes().all(|b| b.is_ascii_digit()),
        _ => false,
    }
}

struct Column {
    /// (row, value key) for filled rows.
    keys: Vec<(u32, u64)>,
    unique: bool,
    filled: usize,
    rows: usize,
    id_like: bool,
    text: bool,
    whole: bool,
    /// Formatted as dates: never a key on its own, since a corrected date
    /// would make the record look deleted and added.
    date: bool,
}

fn column(side: &Side<'_>, col: u32) -> Column {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    let mut unique = true;
    let mut rows = 0;
    let mut id_like = true;
    let mut text = true;
    let mut whole = true;
    let mut date = false;
    for row in side.data_rows() {
        if row.cells.is_empty() {
            continue;
        }
        rows += 1;
        if let Some(v) = side.input(row, col)
            && !v.is_blank()
        {
            let k = v.key();
            if !seen.insert(k) {
                unique = false;
            }
            id_like &= id_like_value(&v);
            text &= matches!(v, Value::Text(_));
            whole &= matches!(v, Value::Text(_))
                || matches!(v, Value::Number(n) if n.bytes().all(|b| b.is_ascii_digit()));
            keys.push((row.r, k));
            if let Ok(i) = row.cells.binary_search_by_key(&col, |c| c.col) {
                date |= matches!(v, Value::Number(_))
                    && is_date_format(side.wb.styles.format_code(row.cells[i].style));
            }
        }
    }
    let filled = keys.len();
    Column {
        keys,
        unique,
        filled,
        rows,
        id_like: id_like && filled > 0,
        text: text && filled > 0,
        whole,
        date,
    }
}

/// A number format that shows a date or time: d, m, y, h or s outside
/// quotes and brackets (`[Red]` is a colour).
#[must_use]
pub fn is_date_format(code: &str) -> bool {
    if code.eq_ignore_ascii_case("general") {
        return false;
    }
    let mut quoted = false;
    let mut bracket = false;
    let mut escaped = false;
    for c in code.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            'd' | 'D' | 'm' | 'M' | 'y' | 'Y' | 'h' | 'H' | 's' | 'S' if !quoted && !bracket => {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn overlap(a: &[(u32, u64)], b: &[(u32, u64)]) -> f64 {
    let set: HashSet<u64> = a.iter().map(|(_, k)| *k).collect();
    let common = b.iter().filter(|(_, k)| set.contains(k)).count();
    let least = a.len().min(b.len()).max(1);
    #[allow(clippy::cast_precision_loss)]
    let ratio = common as f64 / least as f64;
    ratio
}

/// Picks a key column: unique and nearly always filled on both sides, at
/// least half its values on both, and either ID-like or a text label column.
fn key_column(a: &Side<'_>, b: &Side<'_>, cols: &Matching) -> Option<(u32, u32)> {
    let mut best: Option<(u32, (u32, u32))> = None;
    for &(ca, cb) in &cols.pairs {
        let (x, y) = (column(a, ca), column(b, cb));
        let full = |c: &Column| c.rows >= 2 && c.unique && c.filled * 10 >= c.rows * 9;
        if !(full(&x) && full(&y)) || !(x.whole && y.whole) || x.date || y.date {
            continue;
        }
        let header = a.header_text(ca).unwrap_or_default();
        // An identifier may be half new (a list that grew); a label column
        // must be mostly the same labels.
        let ids = id_like_header(&header) || (x.id_like && y.id_like);
        if overlap(&x.keys, &y.keys) < if ids { 0.5 } else { 0.8 } {
            continue;
        }
        let mut score = 0;
        if id_like_header(&header) {
            score += 4;
        }
        if x.id_like && y.id_like {
            score += 2;
        }
        if x.text && y.text {
            score += 1;
        }
        if score == 0 {
            continue;
        }
        // Leftmost wins a tie.
        let score = score * 100 + (100 - ca.min(99));
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, (ca, cb)));
        }
    }
    best.map(|(_, pair)| pair)
}

/// A column in a and its match in b.
type ColPair = (u32, u32);

/// Two columns that are a key together.
fn composite_key(a: &Side<'_>, b: &Side<'_>, cols: &Matching) -> Option<(ColPair, ColPair)> {
    let candidates: Vec<(u32, u32)> = cols.pairs.iter().copied().take(10).collect();
    let mut best: Option<(f64, (ColPair, ColPair))> = None;
    let combined = |side: &Side<'_>, c1: u32, c2: u32| -> Option<Vec<(u32, u64)>> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut rows = 0;
        for row in side.data_rows() {
            if row.cells.is_empty() {
                continue;
            }
            rows += 1;
            let (v1, v2) = (side.input(row, c1)?, side.input(row, c2)?);
            if v1.is_blank() || v2.is_blank() {
                continue;
            }
            let k = v1
                .key()
                .wrapping_mul(31)
                .wrapping_add(v2.key().rotate_left(17));
            if !seen.insert(k) {
                return None;
            }
            out.push((row.r, k));
        }
        (rows >= 2 && out.len() * 10 >= rows * 9).then_some(out)
    };
    for (i, &p1) in candidates.iter().enumerate() {
        for &p2 in &candidates[i + 1..] {
            let (Some(x), Some(y)) = (combined(a, p1.0, p2.0), combined(b, p1.1, p2.1)) else {
                continue;
            };
            let o = overlap(&x, &y);
            if o >= 0.8 && best.is_none_or(|(s, _)| o > s) {
                best = Some((o, (p1, p2)));
            }
        }
    }
    best.map(|(_, k)| k)
}

/// What a row holds over the matched columns: input values, and for
/// formula cells only that there is a formula.
fn row_cells(side: &Side<'_>, row: &Row, cols: &[u32]) -> Vec<u64> {
    cols.iter()
        .map(|&c| match row.cells.binary_search_by_key(&c, |x| x.col) {
            Err(_) => 0,
            Ok(i) => match value::input(side.wb, side.text, &row.cells[i]) {
                Some(v) => v.key(),
                None => 2,
            },
        })
        .collect()
}

fn hash_cells(cells: &[u64]) -> u64 {
    let mut h = Fnv::new();
    for c in cells {
        h.u64(*c);
    }
    h.finish()
}

/// Lines up the rows of two sheets whose columns are already lined up.
fn align_rows(a: &Side<'_>, b: &Side<'_>, cols: &Matching) -> (Matching, RowKey) {
    let mut m = Matching::default();
    let header_a = a.header.unwrap_or(0);
    let header_b = b.header.unwrap_or(0);

    // Rows down to the header: titles and the header itself, by content then
    // position.
    let top_a: Vec<&Row> = a.sheet.rows.iter().filter(|r| r.r <= header_a).collect();
    let top_b: Vec<&Row> = b.sheet.rows.iter().filter(|r| r.r <= header_b).collect();
    let cols_a: Vec<u32> = cols.pairs.iter().map(|p| p.0).collect();
    let cols_b: Vec<u32> = cols.pairs.iter().map(|p| p.1).collect();
    let ka: Vec<u64> = top_a
        .iter()
        .map(|r| hash_cells(&row_cells(a, r, &cols_a)).max(1))
        .collect();
    let kb: Vec<u64> = top_b
        .iter()
        .map(|r| hash_cells(&row_cells(b, r, &cols_b)).max(1))
        .collect();
    let anchors = lcs(&ka, &kb);
    for &(i, j) in &anchors {
        m.pair(top_a[i].r, top_b[j].r);
    }
    if top_a.len() == top_b.len() {
        for (x, y) in top_a.iter().zip(&top_b) {
            if !m.a_to_b.contains_key(&x.r) && !m.b_to_a.contains_key(&y.r) {
                m.pair(x.r, y.r);
            }
        }
    }

    let data_a: Vec<&Row> = a.data_rows().filter(|r| !r.cells.is_empty()).collect();
    let data_b: Vec<&Row> = b.data_rows().filter(|r| !r.cells.is_empty()).collect();
    let mut key = RowKey::None;

    // 1 and 2: keys.
    if let Some((ca, cb)) = key_column(a, b, cols) {
        let x = column(a, ca);
        let y = column(b, cb);
        let by_key: HashMap<u64, u32> = y.keys.iter().map(|(r, k)| (*k, *r)).collect();
        for (r, k) in &x.keys {
            if let Some(rb) = by_key.get(k) {
                m.pair(*r, *rb);
            }
        }
        key = RowKey::Column(cb, b.header_text(cb).unwrap_or_default());
    } else if let Some(((a1, b1), (a2, b2))) = composite_key(a, b, cols) {
        let keys = |side: &Side<'_>, c1: u32, c2: u32| -> Vec<(u32, u64)> {
            side.data_rows()
                .filter_map(|row| {
                    let (v1, v2) = (side.input(row, c1)?, side.input(row, c2)?);
                    (!v1.is_blank() && !v2.is_blank()).then(|| {
                        (
                            row.r,
                            v1.key()
                                .wrapping_mul(31)
                                .wrapping_add(v2.key().rotate_left(17)),
                        )
                    })
                })
                .collect()
        };
        let by_key: HashMap<u64, u32> = keys(b, b1, b2).into_iter().map(|(r, k)| (k, r)).collect();
        for (r, k) in keys(a, a1, a2) {
            if let Some(rb) = by_key.get(&k) {
                m.pair(r, *rb);
            }
        }
        key = RowKey::Composite(b1, b2);
    }

    // 3: whole content, for rows the keys didn't place.
    let cells_a: HashMap<u32, Vec<u64>> = data_a
        .iter()
        .map(|r| (r.r, row_cells(a, r, &cols_a)))
        .collect();
    let cells_b: HashMap<u32, Vec<u64>> = data_b
        .iter()
        .map(|r| (r.r, row_cells(b, r, &cols_b)))
        .collect();
    let has_input = |cells: &Vec<u64>| cells.iter().any(|c| *c > 2);
    let keyed = !matches!(key, RowKey::None);
    let mut groups_b: HashMap<u64, Vec<u32>> = HashMap::new();
    for r in &data_b {
        if !m.b_to_a.contains_key(&r.r) && has_input(&cells_b[&r.r]) {
            groups_b
                .entry(hash_cells(&cells_b[&r.r]))
                .or_default()
                .push(r.r);
        }
    }
    for list in groups_b.values_mut() {
        list.reverse();
    }
    for r in &data_a {
        if m.a_to_b.contains_key(&r.r) || !has_input(&cells_a[&r.r]) {
            continue;
        }
        // A row whose key isn't in b is a different record, even if its
        // other cells match one: with a key, content only matches rows
        // without one.
        if keyed && key_value(a, r, &key, cols).is_some() {
            continue;
        }
        if let Some(list) = groups_b.get_mut(&hash_cells(&cells_a[&r.r]))
            && let Some(rb) = list.pop()
        {
            m.pair(r.r, rb);
        }
    }

    // 4: similarity, through values that are rare among what is left.
    let left_a: Vec<u32> = data_a
        .iter()
        .map(|r| r.r)
        .filter(|r| !m.a_to_b.contains_key(r))
        .collect();
    let left_b: Vec<u32> = data_b
        .iter()
        .map(|r| r.r)
        .filter(|r| !m.b_to_a.contains_key(r))
        .collect();
    if !left_a.is_empty() && !left_b.is_empty() && !keyed {
        similar(&mut m, &left_a, &left_b, &cells_a, &cells_b);
    }

    // 5: rows of formulas only, by position between matched rows.
    let gaps = gaps_between(&m, &data_a, &data_b);
    for (ga, gb) in gaps {
        let ga: Vec<u32> = ga.into_iter().filter(|r| !has_input(&cells_a[r])).collect();
        let gb: Vec<u32> = gb.into_iter().filter(|r| !has_input(&cells_b[r])).collect();
        if ga.len() == gb.len() {
            for (x, y) in ga.into_iter().zip(gb) {
                m.pair(x, y);
            }
        }
    }

    let all_a: Vec<u32> = a
        .sheet
        .rows
        .iter()
        .filter(|r| !r.cells.is_empty() || r.r <= header_a)
        .map(|r| r.r)
        .collect();
    let all_b: Vec<u32> = b
        .sheet
        .rows
        .iter()
        .filter(|r| !r.cells.is_empty() || r.r <= header_b)
        .map(|r| r.r)
        .collect();
    m.finish(&all_a, &all_b);
    (m, key)
}

fn key_value(side: &Side<'_>, row: &Row, key: &RowKey, cols: &Matching) -> Option<u64> {
    let to_a = |cb: u32| cols.b_to_a.get(&cb).copied();
    match key {
        RowKey::Column(cb, _) => {
            let v = side.input(row, to_a(*cb)?)?;
            (!v.is_blank()).then(|| v.key())
        }
        RowKey::Composite(b1, b2) => {
            let (v1, v2) = (side.input(row, to_a(*b1)?)?, side.input(row, to_a(*b2)?)?);
            (!v1.is_blank() && !v2.is_blank()).then(|| v1.key() ^ v2.key())
        }
        RowKey::None => None,
    }
}

/// How alike two rows are: the share of filled cells that are equal.
fn likeness(x: &[u64], y: &[u64]) -> f64 {
    let filled = x
        .iter()
        .zip(y)
        .filter(|(p, q)| **p != 0 || **q != 0)
        .count();
    if filled == 0 {
        return 0.0;
    }
    let same = x.iter().zip(y).filter(|(p, q)| **p != 0 && p == q).count();
    #[allow(clippy::cast_precision_loss)]
    let ratio = same as f64 / filled as f64;
    ratio
}

/// Rows at least this alike can be the same record, edited.
pub const LIKENESS: f64 = 0.5;

fn similar(
    m: &mut Matching,
    left_a: &[u32],
    left_b: &[u32],
    cells_a: &HashMap<u32, Vec<u64>>,
    cells_b: &HashMap<u32, Vec<u64>>,
) {
    // (column, value) → rows of b, for values at most 4 rows share.
    let mut index: HashMap<(usize, u64), Vec<u32>> = HashMap::new();
    for &r in left_b {
        for (c, &v) in cells_b[&r].iter().enumerate() {
            if v > 2 {
                index.entry((c, v)).or_default().push(r);
            }
        }
    }
    index.retain(|_, rows| rows.len() <= 4);
    let mut best_b: HashMap<u32, (f64, u32)> = HashMap::new();
    let mut proposals: Vec<(u32, u32, f64)> = Vec::new();
    let mut unsure: HashSet<u32> = HashSet::new();
    for &ra in left_a {
        let mut candidates: HashSet<u32> = HashSet::new();
        for (c, &v) in cells_a[&ra].iter().enumerate() {
            if let Some(rows) = index.get(&(c, v)) {
                candidates.extend(rows);
            }
        }
        let mut scored: Vec<(f64, u32)> = candidates
            .into_iter()
            .map(|rb| (likeness(&cells_a[&ra], &cells_b[&rb]), rb))
            .collect();
        scored.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)));
        let Some(&(top, rb)) = scored.first() else {
            continue;
        };
        if top < LIKENESS {
            continue;
        }
        if scored
            .get(1)
            .is_some_and(|(second, _)| (top - second).abs() < f64::EPSILON)
        {
            // Two rows equally like it: don't guess.
            unsure.insert(ra);
            for &(s, r) in &scored {
                if (top - s).abs() < f64::EPSILON {
                    m.unsure_b.push(r);
                }
            }
            continue;
        }
        proposals.push((ra, rb, top));
        let e = best_b.entry(rb).or_insert((top, ra));
        if top > e.0 || (top == e.0 && ra < e.1) {
            *e = (top, ra);
        }
    }
    for (ra, rb, _) in proposals {
        // Mutual best only.
        if best_b.get(&rb).is_some_and(|(_, best_a)| *best_a == ra) && !m.b_to_a.contains_key(&rb) {
            m.pair(ra, rb);
        }
    }
    m.unsure_a.extend(unsure);
    m.unsure_b.sort_unstable();
    m.unsure_b.dedup();
    let paired: Vec<u32> = m
        .unsure_b
        .iter()
        .copied()
        .filter(|r| m.b_to_a.contains_key(r))
        .collect();
    m.unsure_b.retain(|r| !paired.contains(r));
}

/// Unmatched rows of each side between consecutive matched rows (those in
/// order on both sides).
fn gaps_between(m: &Matching, data_a: &[&Row], data_b: &[&Row]) -> Vec<(Vec<u32>, Vec<u32>)> {
    let mut anchors: Vec<(u32, u32)> = data_a
        .iter()
        .filter_map(|r| m.a_to_b.get(&r.r).map(|b| (r.r, *b)))
        .collect();
    let order: Vec<u32> = anchors.iter().map(|p| p.1).collect();
    let keep = longest_increasing(&order);
    anchors = anchors
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep.contains(i))
        .map(|(_, p)| p)
        .collect();
    let mut out = Vec::new();
    let mut prev = (0u32, 0u32);
    let bounds: Vec<(u32, u32)> = anchors
        .iter()
        .copied()
        .chain(std::iter::once((u32::MAX, u32::MAX)))
        .collect();
    for (na, nb) in bounds {
        let ga: Vec<u32> = data_a
            .iter()
            .map(|r| r.r)
            .filter(|r| *r > prev.0 && *r < na && !m.a_to_b.contains_key(r))
            .collect();
        let gb: Vec<u32> = data_b
            .iter()
            .map(|r| r.r)
            .filter(|r| *r > prev.1 && *r < nb && !m.b_to_a.contains_key(r))
            .collect();
        if !ga.is_empty() || !gb.is_empty() {
            out.push((ga, gb));
        }
        prev = (na, nb);
    }
    out
}

/// b's matched data rows are in order of one column (and a's weren't).
fn sorted_by(a: &Side<'_>, b: &Side<'_>, rows: &Matching, cols: &Matching) -> Option<(u32, bool)> {
    if rows.moved_b.is_empty() {
        return None;
    }
    let header_b = b.header.unwrap_or(0);
    let matched_b: Vec<&Row> = b
        .sheet
        .rows
        .iter()
        .filter(|r| r.r > header_b && rows.b_to_a.contains_key(&r.r))
        .collect();
    for &(ca, cb) in &cols.pairs {
        let vb: Vec<Value<'_>> = matched_b.iter().map(|r| b.value(r, cb)).collect();
        // The same records, in a's own order.
        let header_a = a.header.unwrap_or(0);
        let va: Vec<Value<'_>> = a
            .sheet
            .rows
            .iter()
            .filter(|r| r.r > header_a && rows.a_to_b.contains_key(&r.r))
            .map(|r| a.value(r, ca))
            .collect();
        for descending in [false, true] {
            if in_order(&vb, descending) && !in_order(&va, descending) {
                return Some((cb, descending));
            }
        }
    }
    None
}

fn in_order(values: &[Value<'_>], descending: bool) -> bool {
    if values.len() < 2 || values.iter().any(Value::is_blank) {
        return false;
    }
    let cmp = |x: &Value<'_>, y: &Value<'_>| -> Option<std::cmp::Ordering> {
        match (x, y) {
            (Value::Number(p), Value::Number(q)) => {
                p.parse::<f64>().ok()?.partial_cmp(&q.parse::<f64>().ok()?)
            }
            (Value::Text(p), Value::Text(q)) => Some(p.to_lowercase().cmp(&q.to_lowercase())),
            _ => None,
        }
    };
    values.windows(2).all(|w| match cmp(&w[0], &w[1]) {
        Some(o) => {
            if descending {
                o != std::cmp::Ordering::Less
            } else {
                o != std::cmp::Ordering::Greater
            }
        }
        None => false,
    })
}

/// Lines up one sheet of `a` with one of `b`.
#[must_use]
pub fn align(a: &Workbook, sheet_a: usize, b: &Workbook, sheet_b: usize) -> SheetAlignment {
    let sa = Side::new(a, sheet_a);
    let sb = Side::new(b, sheet_b);
    let cols = align_cols(&sa, &sb);
    let (rows, key) = align_rows(&sa, &sb, &cols);
    let sorted = sorted_by(&sa, &sb, &rows, &cols);
    SheetAlignment {
        header: (sa.header, sb.header),
        cols,
        rows,
        key,
        sorted_by: sorted,
    }
}

#[cfg(test)]
mod tests;
