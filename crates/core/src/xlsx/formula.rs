// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Formulas as the file stores them (en-US, A1 style, no `=`), split into
//! tokens that know which parts are references. This finds what a formula
//! refers to; it never calculates (CLAUDE.md, "Excel engine": Excel
//! calculates, Lineage records what Excel stored).
//!
//! Every token keeps its source text, so a formula renders back exactly.
//! References can be shifted (to expand a shared formula) or rendered by
//! identity (to compare formulas across versions whose rows moved).

use super::refs::{MAX_COL, MAX_ROW, col_name, parse_col};

/// A row or column number, and whether it is anchored with `$`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Coord {
    pub n: u32,
    pub abs: bool,
}

/// Which sheet a reference is on, when it says.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SheetPrefix {
    /// `[1]`: another workbook, by its external link number.
    pub book: Option<String>,
    /// The sheet's name, unquoted. Empty for `[1]!Name`.
    pub first: String,
    /// The last sheet of a 3D reference (`Jan:Mar!B2`).
    pub last: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefKind {
    Cell {
        col: Coord,
        row: Coord,
    },
    Area {
        c1: Coord,
        r1: Coord,
        c2: Coord,
        r2: Coord,
    },
    /// Whole columns, `A:C`.
    Cols {
        c1: Coord,
        c2: Coord,
    },
    /// Whole rows, `3:5`.
    Rows {
        r1: Coord,
        r2: Coord,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Reference {
    pub sheet: Option<SheetPrefix>,
    pub kind: RefKind,
    /// `A1#`: the whole spill range of a dynamic array.
    pub spill: bool,
}

impl Reference {
    /// The rectangle it covers, 1-based and inclusive.
    #[must_use]
    pub fn bounds(&self) -> (u32, u32, u32, u32) {
        match self.kind {
            RefKind::Cell { col, row } => (col.n, row.n, col.n, row.n),
            RefKind::Area { c1, r1, c2, r2 } => (
                c1.n.min(c2.n),
                r1.n.min(r2.n),
                c1.n.max(c2.n),
                r1.n.max(r2.n),
            ),
            RefKind::Cols { c1, c2 } => (c1.n.min(c2.n), 1, c1.n.max(c2.n), MAX_ROW),
            RefKind::Rows { r1, r2 } => (1, r1.n.min(r2.n), MAX_COL, r1.n.max(r2.n)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    Ref(Reference),
    /// A defined name, possibly sheet-qualified or in another workbook.
    Name {
        sheet: Option<SheetPrefix>,
        name: String,
    },
    /// A structured reference into a table, such as `Transactions[Amount]`
    /// or `[@Amount]`, as written.
    Structured(String),
    /// A function's name, as written (`SUM`, `_xlfn.XLOOKUP`), before `(`.
    Function(String),
    /// A text literal, quotes included.
    Text(String),
    /// An error literal such as `#REF!`.
    Error(String),
    /// Everything else: operators, numbers, booleans, spaces, array
    /// constants, as written.
    Other(String),
}

/// A tokenized formula. Each token keeps its source text in `src`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formula {
    pub tokens: Vec<(Token, String)>,
}

const ERRORS: [&str; 15] = [
    "#NULL!",
    "#DIV/0!",
    "#VALUE!",
    "#REF!",
    "#NAME?",
    "#NUM!",
    "#N/A",
    "#GETTING_DATA",
    "#SPILL!",
    "#CALC!",
    "#FIELD!",
    "#BLOCKED!",
    "#CONNECT!",
    "#BUSY!",
    "#UNKNOWN!",
];

/// Functions whose result can change without any input changing: Excel
/// recalculates them every time (CLAUDE.md, refreshed data).
pub const VOLATILE: [&str; 8] = [
    "NOW",
    "TODAY",
    "RAND",
    "RANDBETWEEN",
    "RANDARRAY",
    "OFFSET",
    "INDIRECT",
    "INFO",
];

/// Functions that make their references at run time, so what they depend on
/// can't be known from the formula.
pub const DYNAMIC: [&str; 3] = ["OFFSET", "INDIRECT", "CELL"];

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '\\' | '$' | '?') || !c.is_ascii()
}

/// `$?[A-Z]{1,3}` → column.
fn parse_col_coord(word: &str) -> Option<Coord> {
    let (abs, rest) = word.strip_prefix('$').map_or((false, word), |r| (true, r));
    Some(Coord {
        n: parse_col(rest)?,
        abs,
    })
}

/// `$?\d+` → row.
fn parse_row_coord(word: &str) -> Option<Coord> {
    let (abs, rest) = word.strip_prefix('$').map_or((false, word), |r| (true, r));
    if rest.is_empty() || rest.len() > 7 || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = rest.parse().ok()?;
    (1..=MAX_ROW).contains(&n).then_some(Coord { n, abs })
}

/// `$?[A-Z]{1,3}$?\d+` → (column, row).
fn parse_cell_coords(word: &str) -> Option<(Coord, Coord)> {
    let body = word.strip_prefix('$').unwrap_or(word);
    let letters = body.bytes().take_while(u8::is_ascii_alphabetic).count();
    if letters == 0 {
        return None;
    }
    let split = word.len() - body.len() + letters;
    let col = parse_col_coord(&word[..split])?;
    let row = parse_row_coord(&word[split..])?;
    Some((col, row))
}

struct Scanner<'a> {
    s: &'a str,
    i: usize,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn word(&mut self) -> &str {
        let start = self.i;
        while let Some(c) = self.peek() {
            if is_word_char(c) {
                self.i += c.len_utf8();
            } else {
                break;
            }
        }
        &self.s[start..self.i]
    }

    /// A quoted run ending in `quote`, with doubled quotes inside.
    fn quoted(&mut self, quote: char) -> Option<String> {
        let start = self.i;
        self.i += quote.len_utf8();
        let mut out = String::new();
        loop {
            let c = self.peek()?;
            self.i += c.len_utf8();
            if c == quote {
                if self.peek() == Some(quote) {
                    self.i += quote.len_utf8();
                    out.push(quote);
                } else {
                    return Some(out);
                }
            } else {
                out.push(c);
            }
            if self.i > self.s.len() {
                self.i = start;
                return None;
            }
        }
    }

    /// Balanced square brackets, as written.
    fn brackets(&mut self) -> Option<()> {
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            self.i += c.len_utf8();
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(());
                    }
                }
                // A literal bracket inside a structured reference is
                // escaped with an apostrophe.
                '\'' => {
                    if let Some(next) = self.peek() {
                        self.i += next.len_utf8();
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// Splits formula text into tokens. Never fails: anything it can't place is
/// kept as `Other`, so the formula still renders back exactly.
#[must_use]
pub fn tokenize(text: &str) -> Formula {
    let mut sc = Scanner { s: text, i: 0 };
    let mut tokens: Vec<(Token, String)> = Vec::new();
    let push_other = |tokens: &mut Vec<(Token, String)>, s: &str| {
        if let Some((Token::Other(prev), src)) = tokens.last_mut() {
            prev.push_str(s);
            src.push_str(s);
        } else {
            tokens.push((Token::Other(s.to_owned()), s.to_owned()));
        }
    };
    while let Some(c) = sc.peek() {
        let start = sc.i;
        match c {
            '"' => {
                if sc.quoted('"').is_none() {
                    sc.i = text.len();
                }
                let src = &text[start..sc.i];
                tokens.push((Token::Text(src.to_owned()), src.to_owned()));
            }
            '{' => {
                // An array constant: numbers and text only.
                let mut in_text = false;
                sc.i += 1;
                while let Some(d) = sc.peek() {
                    sc.i += d.len_utf8();
                    if d == '"' {
                        in_text = !in_text;
                    } else if d == '}' && !in_text {
                        break;
                    }
                }
                push_other(&mut tokens, &text[start..sc.i]);
            }
            '#' => {
                let rest = &text[sc.i..];
                if let Some(e) = ERRORS.iter().find(|e| {
                    rest.get(..e.len())
                        .is_some_and(|head| head.eq_ignore_ascii_case(e))
                }) {
                    sc.i += e.len();
                    let src = &text[start..sc.i];
                    tokens.push((Token::Error(src.to_ascii_uppercase()), src.to_owned()));
                } else {
                    sc.i += 1;
                    push_other(&mut tokens, "#");
                }
            }
            '\'' | '[' => {
                if let Some(token) = prefixed(&mut sc) {
                    tokens.push((token, text[start..sc.i].to_owned()));
                } else {
                    sc.i = start + 1;
                    push_other(&mut tokens, &text[start..sc.i]);
                }
            }
            c if c.is_ascii_digit() || c == '.' => {
                // A row range ("3:5") or a number.
                if let Some(token) = reference_or_name(&mut sc, None) {
                    tokens.push((token, text[start..sc.i].to_owned()));
                } else {
                    sc.i = start;
                    number(&mut sc);
                    push_other(&mut tokens, &text[start..sc.i]);
                }
            }
            c if is_word_char(c) => {
                if let Some(token) = reference_or_name(&mut sc, None) {
                    tokens.push((token, text[start..sc.i].to_owned()));
                } else {
                    sc.i = start;
                    let w = sc.word().to_owned();
                    push_other(&mut tokens, &w);
                }
            }
            _ => {
                sc.i += c.len_utf8();
                push_other(&mut tokens, &text[start..sc.i]);
            }
        }
    }
    Formula { tokens }
}

fn number(sc: &mut Scanner<'_>) {
    while let Some(c) = sc.peek() {
        if c.is_ascii_digit() || c == '.' {
            sc.i += 1;
        } else if matches!(c, 'e' | 'E') {
            sc.i += 1;
            if matches!(sc.peek(), Some('+' | '-')) {
                sc.i += 1;
            }
        } else {
            break;
        }
    }
}

/// A token starting with `'` (a quoted sheet) or `[` (another workbook, or
/// a structured reference inside a table).
fn prefixed(sc: &mut Scanner<'_>) -> Option<Token> {
    let start = sc.i;
    if sc.peek() == Some('[') {
        // `[1]Sheet1!A1`, `[1]!Name`: an external link number.
        let rest = &sc.s[sc.i + 1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest.as_bytes().get(digits) == Some(&b']') {
            let book = rest[..digits].to_owned();
            sc.i += digits + 2;
            if sc.peek() == Some('!') {
                sc.i += 1;
                let name = sc.word().to_owned();
                if name.is_empty() {
                    sc.i = start;
                    return None;
                }
                return Some(Token::Name {
                    sheet: Some(SheetPrefix {
                        book: Some(book),
                        first: String::new(),
                        last: None,
                    }),
                    name,
                });
            }
            let sheet = sc.word().to_owned();
            return finish_prefix(sc, Some(book), sheet, start);
        }
        // `[@Amount]`, `[#Headers]`: a structured reference in a table.
        sc.brackets()?;
        return Some(Token::Structured(sc.s[start..sc.i].to_owned()));
    }
    // `'Sheet name'!`, `'[1]Sheet name'!`, `'Jan 2026:Mar 2026'!`
    let inner = sc.quoted('\'')?;
    if sc.peek() != Some('!') {
        sc.i = start;
        return None;
    }
    let (book, rest) = match inner.strip_prefix('[') {
        Some(after) => match after.split_once(']') {
            Some((b, r)) if !b.is_empty() && b.bytes().all(|x| x.is_ascii_digit()) => {
                (Some(b.to_owned()), r.to_owned())
            }
            _ => (None, inner.clone()),
        },
        None => (None, inner.clone()),
    };
    let (first, last) = match rest.split_once(':') {
        Some((a, b)) => (a.to_owned(), Some(b.to_owned())),
        None => (rest, None),
    };
    sc.i += 1;
    let prefix = SheetPrefix { book, first, last };
    match reference_or_name(sc, Some(prefix)) {
        Some(t) => Some(t),
        None => {
            sc.i = start;
            None
        }
    }
}

/// After `[1]Sheet` or `Sheet` (unquoted): an optional `:Last` and the `!`.
fn finish_prefix(
    sc: &mut Scanner<'_>,
    book: Option<String>,
    first: String,
    start: usize,
) -> Option<Token> {
    let mut last = None;
    if sc.peek() == Some(':') {
        let save = sc.i;
        sc.i += 1;
        let w = sc.word().to_owned();
        if sc.peek() == Some('!') && !w.is_empty() {
            last = Some(w);
        } else {
            sc.i = save;
        }
    }
    if sc.peek() != Some('!') {
        sc.i = start;
        return None;
    }
    sc.i += 1;
    let prefix = SheetPrefix { book, first, last };
    let token = reference_or_name(sc, Some(prefix));
    if token.is_none() {
        sc.i = start;
    }
    token
}

/// A reference, function, structured reference or name at the cursor, with
/// an optional sheet already read.
fn reference_or_name(sc: &mut Scanner<'_>, prefix: Option<SheetPrefix>) -> Option<Token> {
    let start = sc.i;
    let word = sc.word().to_owned();
    if word.is_empty() {
        return None;
    }
    // An unquoted sheet name: `Sheet1!A1`, `Jan:Mar!B2`.
    if prefix.is_none() && (sc.peek() == Some('!') || looks_3d(sc)) {
        return finish_prefix(sc, None, word, start);
    }
    if prefix.is_none() && sc.peek() == Some('(') {
        return Some(Token::Function(word));
    }
    if prefix.is_none() && sc.peek() == Some('[') {
        sc.brackets()?;
        return Some(Token::Structured(sc.s[start..sc.i].to_owned()));
    }
    // A cell, possibly the start of an area, or a spill reference.
    if let Some((col, row)) = parse_cell_coords(&word) {
        if sc.peek() == Some(':') {
            let save = sc.i;
            sc.i += 1;
            let second = sc.word().to_owned();
            if let Some((c2, r2)) = parse_cell_coords(&second) {
                return Some(Token::Ref(Reference {
                    sheet: prefix,
                    kind: RefKind::Area {
                        c1: col,
                        r1: row,
                        c2,
                        r2,
                    },
                    spill: false,
                }));
            }
            sc.i = save;
        }
        let spill = sc.peek() == Some('#') && !sc.s[sc.i..].starts_with("#REF!");
        if spill {
            sc.i += 1;
        }
        return Some(Token::Ref(Reference {
            sheet: prefix,
            kind: RefKind::Cell { col, row },
            spill,
        }));
    }
    // Whole columns or rows: both ends must be there.
    if sc.peek() == Some(':') {
        let save = sc.i;
        sc.i += 1;
        let second = sc.word().to_owned();
        if let (Some(c1), Some(c2)) = (parse_col_coord(&word), parse_col_coord(&second)) {
            return Some(Token::Ref(Reference {
                sheet: prefix,
                kind: RefKind::Cols { c1, c2 },
                spill: false,
            }));
        }
        if let (Some(r1), Some(r2)) = (parse_row_coord(&word), parse_row_coord(&second)) {
            return Some(Token::Ref(Reference {
                sheet: prefix,
                kind: RefKind::Rows { r1, r2 },
                spill: false,
            }));
        }
        sc.i = save;
    }
    if prefix.is_some() && word.eq_ignore_ascii_case("#REF!") {
        return None;
    }
    let first = word.chars().next().unwrap_or('0');
    if first.is_ascii_digit() || first == '.' || first == '$' {
        sc.i = start;
        return None;
    }
    if prefix.is_none() && (word.eq_ignore_ascii_case("TRUE") || word.eq_ignore_ascii_case("FALSE"))
    {
        sc.i = start;
        return None;
    }
    Some(Token::Name {
        sheet: prefix,
        name: word,
    })
}

/// `Jan:Mar!`: a 3D prefix after an unquoted first sheet.
fn looks_3d(sc: &Scanner<'_>) -> bool {
    if sc.peek() != Some(':') {
        return false;
    }
    let rest = &sc.s[sc.i + 1..];
    let n = rest
        .chars()
        .take_while(|c| is_word_char(*c))
        .map(char::len_utf8)
        .sum::<usize>();
    n > 0 && rest[n..].starts_with('!')
}

/// Writes a sheet prefix as a formula does, quoting when needed.
#[must_use]
pub fn render_prefix(prefix: &SheetPrefix) -> String {
    let mut name = String::new();
    if let Some(book) = &prefix.book {
        name.push('[');
        name.push_str(book);
        name.push(']');
    }
    name.push_str(&prefix.first);
    if let Some(last) = &prefix.last {
        name.push(':');
        name.push_str(last);
    }
    let plain = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            && !s.starts_with(|c: char| c.is_ascii_digit())
            && parse_cell_coords(s).is_none()
    };
    let needs_quotes = !plain(&prefix.first) || prefix.last.as_deref().is_some_and(|l| !plain(l));
    if prefix.first.is_empty() && prefix.book.is_some() {
        return format!("{name}!");
    }
    if needs_quotes {
        format!("'{}'!", name.replace('\'', "''"))
    } else {
        format!("{name}!")
    }
}

fn render_col(c: Coord) -> String {
    format!("{}{}", if c.abs { "$" } else { "" }, col_name(c.n))
}

fn render_row(r: Coord) -> String {
    format!("{}{}", if r.abs { "$" } else { "" }, r.n)
}

/// Writes a reference in A1 style.
#[must_use]
pub fn render_reference(r: &Reference) -> String {
    let mut out = r.sheet.as_ref().map(render_prefix).unwrap_or_default();
    match r.kind {
        RefKind::Cell { col, row } => {
            out.push_str(&render_col(col));
            out.push_str(&render_row(row));
        }
        RefKind::Area { c1, r1, c2, r2 } => {
            out.push_str(&format!(
                "{}{}:{}{}",
                render_col(c1),
                render_row(r1),
                render_col(c2),
                render_row(r2)
            ));
        }
        RefKind::Cols { c1, c2 } => out.push_str(&format!("{}:{}", render_col(c1), render_col(c2))),
        RefKind::Rows { r1, r2 } => out.push_str(&format!("{}:{}", render_row(r1), render_row(r2))),
    }
    if r.spill {
        out.push('#');
    }
    out
}

fn shift_coord(c: Coord, by: i64, max: u32) -> Option<Coord> {
    if c.abs {
        return Some(c);
    }
    let n = i64::from(c.n) + by;
    (1..=i64::from(max)).contains(&n).then(|| Coord {
        n: u32::try_from(n).unwrap_or(1),
        abs: false,
    })
}

/// The sheet prefix of a reference token as it was written, `!` included.
fn written_prefix<'s>(src: &'s str, r: &Reference) -> &'s str {
    if r.sheet.is_none() {
        return "";
    }
    src.rfind('!').map_or("", |i| &src[..=i])
}

impl Formula {
    /// The formula as written.
    #[must_use]
    pub fn render(&self) -> String {
        self.tokens.iter().map(|(_, src)| src.as_str()).collect()
    }

    /// The formula with every token rendered by `f`, or as written when `f`
    /// returns `None`.
    pub fn render_with(&self, mut f: impl FnMut(&Token) -> Option<String>) -> String {
        let mut out = String::new();
        for (token, src) in &self.tokens {
            match f(token) {
                Some(s) => out.push_str(&s),
                None => out.push_str(src),
            }
        }
        out
    }

    /// The formula as it reads in a cell `rows` down and `cols` across from
    /// where it is written: relative references move, `$` ones stay. A
    /// reference pushed off the sheet becomes `#REF!`, as Excel does. This is
    /// how a shared formula's other cells read.
    #[must_use]
    pub fn shifted(&self, rows: i64, cols: i64) -> Self {
        let tokens = self
            .tokens
            .iter()
            .map(|(token, src)| {
                let Token::Ref(r) = token else {
                    return (token.clone(), src.clone());
                };
                let kind = match r.kind {
                    RefKind::Cell { col, row } => shift_coord(col, cols, MAX_COL)
                        .zip(shift_coord(row, rows, MAX_ROW))
                        .map(|(col, row)| RefKind::Cell { col, row }),
                    RefKind::Area { c1, r1, c2, r2 } => {
                        match (
                            shift_coord(c1, cols, MAX_COL),
                            shift_coord(r1, rows, MAX_ROW),
                            shift_coord(c2, cols, MAX_COL),
                            shift_coord(r2, rows, MAX_ROW),
                        ) {
                            (Some(c1), Some(r1), Some(c2), Some(r2)) => {
                                Some(RefKind::Area { c1, r1, c2, r2 })
                            }
                            _ => None,
                        }
                    }
                    RefKind::Cols { c1, c2 } => shift_coord(c1, cols, MAX_COL)
                        .zip(shift_coord(c2, cols, MAX_COL))
                        .map(|(c1, c2)| RefKind::Cols { c1, c2 }),
                    RefKind::Rows { r1, r2 } => shift_coord(r1, rows, MAX_ROW)
                        .zip(shift_coord(r2, rows, MAX_ROW))
                        .map(|(r1, r2)| RefKind::Rows { r1, r2 }),
                };
                match kind {
                    // Unmoved: exactly as written.
                    Some(kind) if kind == r.kind => (token.clone(), src.clone()),
                    Some(kind) => {
                        let moved = Reference {
                            sheet: r.sheet.clone(),
                            kind,
                            spill: r.spill,
                        };
                        // The sheet prefix stays exactly as written.
                        let prefix = written_prefix(src, r);
                        let unprefixed = Reference {
                            sheet: None,
                            ..moved.clone()
                        };
                        let text = format!("{prefix}{}", render_reference(&unprefixed));
                        (Token::Ref(moved), text)
                    }
                    None => {
                        let prefix = written_prefix(src, r);
                        let text = format!("{prefix}#REF!");
                        (Token::Error("#REF!".to_owned()), text)
                    }
                }
            })
            .collect();
        Self { tokens }
    }

    pub fn references(&self) -> impl Iterator<Item = &Reference> {
        self.tokens.iter().filter_map(|(t, _)| match t {
            Token::Ref(r) => Some(r),
            _ => None,
        })
    }

    /// Function names, upper-cased, without Excel's `_xlfn.` and `_xlws.`
    /// prefixes for newer functions.
    pub fn functions(&self) -> impl Iterator<Item = String> + '_ {
        self.tokens.iter().filter_map(|(t, _)| match t {
            Token::Function(name) => {
                let upper = name.to_ascii_uppercase();
                let bare = upper
                    .strip_prefix("_XLFN.")
                    .map(|s| s.strip_prefix("_XLWS.").unwrap_or(s))
                    .unwrap_or(&upper)
                    .to_owned();
                Some(bare)
            }
            _ => None,
        })
    }

    /// Calls a function Excel recalculates every time.
    #[must_use]
    pub fn is_volatile(&self) -> bool {
        self.functions().any(|f| VOLATILE.contains(&f.as_str()))
    }

    /// Makes references at run time, so its precedents aren't all known.
    #[must_use]
    pub fn has_dynamic_references(&self) -> bool {
        self.functions().any(|f| DYNAMIC.contains(&f.as_str()))
    }

    /// Refers to another workbook.
    #[must_use]
    pub fn is_external(&self) -> bool {
        self.tokens.iter().any(|(t, _)| match t {
            Token::Ref(r) => r.sheet.as_ref().is_some_and(|s| s.book.is_some()),
            Token::Name { sheet, .. } => sheet.as_ref().is_some_and(|s| s.book.is_some()),
            _ => false,
        })
    }

    pub fn names(&self) -> impl Iterator<Item = (Option<&SheetPrefix>, &str)> {
        self.tokens.iter().filter_map(|(t, _)| match t {
            Token::Name { sheet, name } => Some((sheet.as_ref(), name.as_str())),
            _ => None,
        })
    }

    pub fn structured(&self) -> impl Iterator<Item = &str> {
        self.tokens.iter().filter_map(|(t, _)| match t {
            Token::Structured(s) => Some(s.as_str()),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(f: &str) -> Vec<String> {
        tokenize(f).references().map(render_reference).collect()
    }

    #[test]
    fn formulas_render_back_exactly() {
        for f in [
            "SUM(Revenue!C2:C13)",
            "'P&L'!B2",
            "B2*Price*(1-Churn)",
            "IF(A1>0,\"yes \"\"really\"\"\",\"no\")",
            "SUMIFS(Transactions[Amount],Transactions[Category],A2)",
            "VLOOKUP(\"EUR\",Rates!A:B,2,FALSE)",
            "COUNTIF(Applications!E:E,A2)",
            "SUM(Jan:Mar!B2)",
            "[1]Prices!$B$3*2",
            "'[2]Old prices'!A1",
            "[1]!TaxRate",
            "SUM(3:5)",
            "{1,2;3,\"a}b\"}",
            "_xlfn.XLOOKUP(A2,B:B,C:C)",
            "A1#",
            "LOG10(100)+ATAN2(1,1)",
            "Table1[[#This Row],[Amount]]*[@Qty]",
            "1E+5+.5-3.25E-2",
            "#REF!+Sheet1!#REF!",
            "TRUE+FALSE",
            "A1 B1",
            "  ",
            "\"unterminated",
            "'unterminated",
        ] {
            assert_eq!(tokenize(f).render(), f, "{f}");
        }
    }

    #[test]
    fn references_are_found() {
        assert_eq!(refs("SUM(Revenue!C2:C13)"), ["Revenue!C2:C13"]);
        assert_eq!(refs("'P&L'!B2+'P&L'!$C$6"), ["'P&L'!B2", "'P&L'!$C$6"]);
        assert_eq!(refs("VLOOKUP(\"EUR\",Rates!A:B,2,FALSE)"), ["Rates!A:B"]);
        assert_eq!(refs("SUM(3:5)"), ["3:5"]);
        assert_eq!(refs("SUM(Jan:Mar!B2)"), ["Jan:Mar!B2"]);
        assert_eq!(refs("[1]Prices!$B$3*2"), ["[1]Prices!$B$3"]);
        assert_eq!(refs("LOG10(100)"), Vec::<String>::new());
        assert_eq!(refs("\"A1\"&B1"), ["B1"]);
        assert_eq!(refs("{1,2}+A1"), ["A1"]);
        assert_eq!(refs("A1#"), ["A1#"]);
        assert_eq!(refs("XFD1048576+XFE1"), ["XFD1048576"]);
    }

    #[test]
    fn names_functions_and_tables() {
        let f = tokenize("SUMIFS(Transactions[Amount],Transactions[Category],A2)*Price");
        assert_eq!(f.functions().collect::<Vec<_>>(), ["SUMIFS"]);
        assert_eq!(
            f.structured().collect::<Vec<_>>(),
            ["Transactions[Amount]", "Transactions[Category]"]
        );
        assert_eq!(f.names().map(|(_, n)| n).collect::<Vec<_>>(), ["Price"]);
        assert!(tokenize("TODAY()").is_volatile());
        assert!(tokenize("_xlfn.RANDARRAY(3)").is_volatile());
        assert!(!tokenize("SUM(A1)").is_volatile());
        assert!(tokenize("INDIRECT(\"A\"&B1)").has_dynamic_references());
        assert!(tokenize("[1]Prices!$B$3").is_external());
        assert!(tokenize("[1]!TaxRate").is_external());
        assert!(!tokenize("Prices!$B$3").is_external());
    }

    #[test]
    fn shared_formulas_shift_relative_references_only() {
        let f = tokenize("B2*Price*(1-Churn)+$C$1+C$1+$D2+SUM(A:A)+SUM(2:2)");
        assert_eq!(
            f.shifted(3, 1).render(),
            "C5*Price*(1-Churn)+$C$1+D$1+$D5+SUM(B:B)+SUM(5:5)"
        );
        assert_eq!(tokenize("A1").shifted(-1, 0).render(), "#REF!");
        assert_eq!(tokenize("Data!A1").shifted(0, -1).render(), "Data!#REF!");
        assert_eq!(tokenize("'P&L'!B2").shifted(1, 0).render(), "'P&L'!B3");
    }

    #[test]
    fn prefixes_render_with_quotes_when_needed() {
        let p = |first: &str| SheetPrefix {
            book: None,
            first: first.to_owned(),
            last: None,
        };
        assert_eq!(render_prefix(&p("Revenue")), "Revenue!");
        assert_eq!(render_prefix(&p("P&L")), "'P&L'!");
        assert_eq!(render_prefix(&p("Use of funds")), "'Use of funds'!");
        assert_eq!(render_prefix(&p("Bob's")), "'Bob''s'!");
        assert_eq!(render_prefix(&p("2026")), "'2026'!");
        assert_eq!(render_prefix(&p("A1")), "'A1'!");
    }
}
