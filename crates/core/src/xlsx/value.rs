// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What a cell holds, as a value to compare: the typed content of an input
//! cell, or a formula's cached result. Text is compared exactly; numbers by
//! the double they stand for (decision 0020); cached numbers at the 15
//! significant digits Excel shows, because Excel's own recalculation can
//! land a unit in the last place away from another program's (decision
//! 0022).

use super::Workbook;
use super::model::{Cached, Cell, Content};
use super::refs::number_key;
use crate::hash::Fnv;

/// A value, borrowed from the workbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value<'a> {
    Blank,
    Text(&'a str),
    /// A number's lexeme as stored.
    Number(&'a str),
    Bool(bool),
    Error(&'a str),
    /// An ISO date as text (`t="d"`).
    Date(&'a str),
}

impl Value<'_> {
    /// A key that is equal for equal values.
    #[must_use]
    pub fn key(&self) -> u64 {
        let mut h = Fnv::new();
        match self {
            Self::Blank => return 0,
            Self::Text(t) => h.str("t").str(t),
            Self::Number(n) => h.str("n").u64(number_key(n).unwrap_or(0)),
            Self::Bool(b) => h.str("b").u64(u64::from(*b)),
            Self::Error(e) => h.str("e").str(&e.to_ascii_uppercase()),
            Self::Date(d) => h.str("d").str(d),
        };
        h.finish().max(1)
    }

    /// A key for a calculated result: numbers at 15 significant digits.
    #[must_use]
    pub fn shown_key(&self) -> u64 {
        match self {
            Self::Number(n) => {
                let mut h = Fnv::new();
                h.str("n").str(&shown(n));
                h.finish().max(1)
            }
            other => other.key(),
        }
    }

    #[must_use]
    pub fn is_blank(&self) -> bool {
        matches!(self, Self::Blank)
    }

    /// The value as plain text, for summaries and matching.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Blank => String::new(),
            Self::Text(t) | Self::Number(t) | Self::Error(t) | Self::Date(t) => (*t).to_owned(),
            Self::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
        }
    }
}

/// A number as Excel shows it at most: 15 significant digits. Used only to
/// compare calculated results.
#[must_use]
pub fn shown(lexeme: &str) -> String {
    match lexeme.parse::<f64>() {
        Ok(v) if v.is_finite() => {
            if v == 0.0 {
                "0".to_owned()
            } else {
                format!("{v:.14e}")
            }
        }
        _ => lexeme.to_owned(),
    }
}

/// The input value of a cell, or `None` for a formula cell.
#[must_use]
pub fn input<'a>(wb: &'a Workbook, text: &'a str, cell: &'a Cell) -> Option<Value<'a>> {
    Some(match &cell.content {
        Content::Blank => Value::Blank,
        Content::Shared(i) => Value::Text(wb.shared(*i).map_or("", |s| s.text.as_str())),
        Content::Inline(t) => Value::Text(t),
        Content::Number(s) => Value::Number(s.get(text)),
        Content::Bool(b) => Value::Bool(*b),
        Content::Error(e) => Value::Error(e.get(text)),
        Content::IsoDate(s) => Value::Date(s.get(text)),
        Content::Formula(_) => return None,
    })
}

/// A formula cell's cached result (`Blank` when it has none), or `None` for
/// an input cell.
#[must_use]
pub fn cached<'a>(text: &'a str, cell: &'a Cell) -> Option<Value<'a>> {
    let Content::Formula(f) = &cell.content else {
        return None;
    };
    Some(match &f.cached {
        None => Value::Blank,
        Some(Cached::Number(n)) => Value::Number(n.get(text)),
        Some(Cached::Text(t)) => Value::Text(t.get(text)),
        Some(Cached::Bool(b)) => Value::Bool(*b),
        Some(Cached::Error(e)) => Value::Error(e.get(text)),
    })
}

/// What a cell shows for matching rows: its input, or its cached result.
#[must_use]
pub fn either<'a>(wb: &'a Workbook, text: &'a str, cell: &'a Cell) -> Value<'a> {
    input(wb, text, cell)
        .or_else(|| cached(text, cell))
        .unwrap_or(Value::Blank)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_values_have_equal_keys() {
        assert_eq!(
            Value::Number("151.3").key(),
            Value::Number("151.30000000000001").key()
        );
        assert_ne!(Value::Number("1").key(), Value::Text("1").key());
        assert_eq!(Value::Error("#n/a").key(), Value::Error("#N/A").key());
        assert_eq!(Value::Blank.key(), 0);
        assert_ne!(Value::Text("").key(), 0);
        // Calculated results agree at 15 significant digits.
        assert_ne!(
            Value::Number("5762.4").key(),
            Value::Number("5762.4000000000005").key()
        );
        assert_eq!(
            Value::Number("5762.4").shown_key(),
            Value::Number("5762.4000000000005").shown_key()
        );
        assert_ne!(
            Value::Number("5762.4").shown_key(),
            Value::Number("5762.5").shown_key()
        );
    }
}
