// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! A1 references: "B7", "$B$7", "A1:C3", "A:A", "3:5", and numbers as the
//! file stores them.

use super::model::Area;

/// Excel's last column (XFD) and row.
pub const MAX_COL: u32 = 16_384;
pub const MAX_ROW: u32 = 1_048_576;

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

/// The column of "AB" (case-insensitive), if it is one Excel has.
#[must_use]
pub fn parse_col(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut col: u32 = 0;
    for b in letters.bytes() {
        if !b.is_ascii_alphabetic() {
            return None;
        }
        col = col * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1);
    }
    (col <= MAX_COL).then_some(col)
}

fn parse_row(digits: &str) -> Option<u32> {
    if digits.is_empty() || digits.len() > 7 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let row: u32 = digits.parse().ok()?;
    (1..=MAX_ROW).contains(&row).then_some(row)
}

/// (column, row) of "B7" or "$B$7".
#[must_use]
pub fn parse_cell(text: &str) -> Option<(u32, u32)> {
    let text = text.strip_prefix('$').unwrap_or(text);
    let split = text.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, rest) = text.split_at(split);
    let rest = rest.strip_prefix('$').unwrap_or(rest);
    Some((parse_col(letters)?, parse_row(rest)?))
}

/// An area: "A1:C3", "B7", "A:C" (whole columns) or "3:5" (whole rows).
#[must_use]
pub fn parse_area(text: &str) -> Option<Area> {
    let text = text.trim();
    let (a, b) = text.split_once(':').unwrap_or((text, text));
    if let (Some((c1, r1)), Some((c2, r2))) = (parse_cell(a), parse_cell(b)) {
        return Some(Area {
            col1: c1.min(c2),
            row1: r1.min(r2),
            col2: c1.max(c2),
            row2: r1.max(r2),
        });
    }
    let strip = |s: &str| s.trim_start_matches('$').to_owned();
    let (a, b) = (strip(a), strip(b));
    if let (Some(c1), Some(c2)) = (parse_col(&a), parse_col(&b)) {
        return Some(Area {
            col1: c1.min(c2),
            row1: 1,
            col2: c1.max(c2),
            row2: MAX_ROW,
        });
    }
    if let (Some(r1), Some(r2)) = (parse_row(&a), parse_row(&b)) {
        return Some(Area {
            col1: 1,
            row1: r1.min(r2),
            col2: MAX_COL,
            row2: r1.max(r2),
        });
    }
    None
}

/// "A1:C3", or "B7" for one cell.
#[must_use]
pub fn area_name(area: &Area) -> String {
    if area.col1 == area.col2 && area.row1 == area.row2 {
        a1(area.col1, area.row1)
    } else {
        format!("{}:{}", a1(area.col1, area.row1), a1(area.col2, area.row2))
    }
}

/// True when `lexeme` is a number as SpreadsheetML stores one: an
/// `xsd:double` without INF or NaN.
#[must_use]
pub fn is_number(lexeme: &str) -> bool {
    let b = lexeme.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - frac_start;
    }
    if digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return false;
        }
    }
    i == b.len()
}

/// The number a lexeme stands for, as the bits of an IEEE 754 double.
///
/// Excel stores every number as a double and writes it with up to 17
/// significant digits, so a typed 151.3 is saved as "151.30000000000001",
/// while another program may write "151.3". Both are the same double, and the
/// same number. This is used only to compare: lexemes are never rewritten.
/// Negative zero is zero.
#[must_use]
pub fn number_key(lexeme: &str) -> Option<u64> {
    if !is_number(lexeme) {
        return None;
    }
    let value: f64 = lexeme.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some(if value == 0.0 { 0 } else { value.to_bits() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_and_areas() {
        assert_eq!(parse_cell("B7"), Some((2, 7)));
        assert_eq!(parse_cell("$AB$10"), Some((28, 10)));
        assert_eq!(parse_cell("XFD1048576"), Some((16_384, 1_048_576)));
        assert_eq!(parse_cell("XFE1"), None);
        assert_eq!(parse_cell("A0"), None);
        assert_eq!(parse_cell("A1048577"), None);
        assert_eq!(parse_cell("7B"), None);
        assert_eq!(
            parse_area("C3:A1"),
            Some(Area {
                col1: 1,
                row1: 1,
                col2: 3,
                row2: 3
            })
        );
        assert_eq!(
            parse_area("B:B").map(|a| (a.col1, a.row2)),
            Some((2, MAX_ROW))
        );
        assert_eq!(
            parse_area("$2:$4").map(|a| (a.row1, a.col2)),
            Some((2, MAX_COL))
        );
        assert_eq!(area_name(&Area::cell(3, 9)), "C9");
        assert_eq!(col_name(703), "AAA");
    }

    #[test]
    fn numbers_compare_by_the_double_they_stand_for() {
        assert_eq!(number_key("151.3"), number_key("151.30000000000001"));
        assert_eq!(number_key("1"), number_key("1.0"));
        assert_eq!(number_key("1E0"), number_key("1"));
        assert_eq!(number_key("-0"), number_key("0"));
        assert_ne!(number_key("0.3"), number_key("0.30000000000000004"));
        for bad in [
            "", "-", ".", "1e", "inf", "NaN", "1,5", " 1", "0x10", "1e400",
        ] {
            assert_eq!(number_key(bad), None, "{bad}");
        }
        assert!(is_number(".5"));
        assert!(is_number("5."));
        assert!(is_number("-1.25E-3"));
    }
}
