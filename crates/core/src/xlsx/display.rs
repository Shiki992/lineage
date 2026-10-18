// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What a cell shows, for the panel's summaries: a value through its number
//! format. Never stored, never compared, never written (CLAUDE.md, "Excel
//! engine": the displayed value is derived only for display). The common
//! formats are covered (General, fixed decimals, thousands, percent,
//! currency, scientific, dates and times); anything else shows the number.

use super::value::Value;

/// The value as Excel would show it in a cell with `code`.
#[must_use]
pub fn display(value: &Value<'_>, code: &str, date1904: bool) -> String {
    match value {
        Value::Number(n) => match n.parse::<f64>() {
            Ok(v) if v.is_finite() => number(v, code, date1904),
            _ => (*n).to_owned(),
        },
        other => other.text(),
    }
}

/// Splits a format into its sections (`;` outside quotes and brackets).
fn sections(code: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut quoted = false;
    let mut bracket = false;
    let mut escaped = false;
    for c in code.chars() {
        if escaped {
            escaped = false;
            if let Some(s) = out.last_mut() {
                s.push(c);
            }
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            ';' if !quoted && !bracket => {
                out.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(s) = out.last_mut() {
            s.push(c);
        }
    }
    out
}

fn number(v: f64, code: &str, date1904: bool) -> String {
    let parts = sections(code);
    let (section, negative_section) = if v < 0.0 && parts.len() > 1 {
        (parts[1].as_str(), true)
    } else if v == 0.0 && parts.len() > 2 {
        (parts[2].as_str(), false)
    } else {
        (parts[0].as_str(), false)
    };
    let shown = if negative_section { -v } else { v };
    let body = strip_brackets(section);
    if body.trim().is_empty() || body.eq_ignore_ascii_case("general") {
        return general(shown);
    }
    if super::align::is_date_format(&body) {
        return date(shown, &body, date1904).unwrap_or_else(|| general(shown));
    }
    if body.contains('@') && !body.contains(['0', '#']) {
        return general(shown);
    }
    numeric(shown, &body)
}

/// Removes colours and conditions (`[Red]`, `[>100]`), keeping a currency
/// symbol from `[$€-407]`.
fn strip_brackets(section: &str) -> String {
    let mut out = String::new();
    let mut rest = section;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(']') else {
            rest = "";
            break;
        };
        let inside = &rest[start + 1..start + end];
        if let Some(symbol) = inside.strip_prefix('$') {
            out.push('"');
            out.push_str(symbol.split('-').next().unwrap_or(""));
            out.push('"');
        } else if inside.starts_with(['h', 'H', 'm', 'M', 's', 'S']) {
            // Elapsed time, [h]:mm: kept as a date part.
            out.push_str(inside);
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Excel's General format: whole numbers as they are, others with up to 15
/// significant digits, very large and very small ones in scientific form.
#[must_use]
pub fn general(v: f64) -> String {
    if v == 0.0 {
        return "0".to_owned();
    }
    let a = v.abs();
    if !(1e-9..1e15).contains(&a) {
        let s = format!("{v:.5E}");
        let (mantissa, exp) = s.split_once('E').unwrap_or((&s, "0"));
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        let exp: i32 = exp.parse().unwrap_or(0);
        return format!(
            "{mantissa}E{}{:02}",
            if exp < 0 { '-' } else { '+' },
            exp.abs()
        );
    }
    let s = format!("{:.*}", decimals_for_15(a), v);
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    }
}

fn decimals_for_15(a: f64) -> usize {
    #[allow(clippy::cast_possible_truncation)]
    let int_digits = if a < 1.0 {
        0
    } else {
        a.log10().floor() as i32 + 1
    };
    usize::try_from((15 - int_digits).clamp(0, 15)).unwrap_or(0)
}

/// Literal text in a format section, unquoted (`"$"`, `\x`), and the
/// placeholder part in the middle.
fn split_literals(body: &str) -> (String, String, String) {
    let mut prefix = String::new();
    let mut core = String::new();
    let mut suffix = String::new();
    let mut seen_digit = false;
    let mut chars = body.chars().peekable();
    let push = |c: char, seen: bool, prefix: &mut String, suffix: &mut String| {
        if seen {
            suffix.push(c);
        } else {
            prefix.push(c);
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                for d in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                    push(d, seen_digit, &mut prefix, &mut suffix);
                }
            }
            '\\' => {
                if let Some(d) = chars.next() {
                    push(d, seen_digit, &mut prefix, &mut suffix);
                }
            }
            // Padding and fill characters.
            '_' | '*' => {
                chars.next();
                if c == '_' {
                    push(' ', seen_digit, &mut prefix, &mut suffix);
                }
            }
            '0' | '#' | '?' | '.' | ',' | 'E' | 'e' | '+' | '-'
                if seen_digit || matches!(c, '0' | '#' | '?' | '.') =>
            {
                if !suffix.is_empty() && !matches!(c, ',') {
                    suffix.push(c);
                    continue;
                }
                seen_digit = true;
                core.push(c);
            }
            '%' => {
                push('%', seen_digit, &mut prefix, &mut suffix);
            }
            other => push(other, seen_digit, &mut prefix, &mut suffix),
        }
    }
    (prefix, core, suffix)
}

fn group_thousands(digits: &str) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn numeric(v: f64, body: &str) -> String {
    let (prefix, core, suffix) = split_literals(body);
    let percent = prefix.contains('%') || suffix.contains('%');
    let mut x = if percent { v * 100.0 } else { v };
    // Trailing commas scale by thousands ("#,##0,").
    let mut core = core;
    while core.ends_with(',') {
        core.pop();
        x /= 1000.0;
    }
    let (mantissa_code, exponent) = match core.find(['E', 'e']) {
        Some(i) => (&core[..i], Some(&core[i..])),
        None => (core.as_str(), None),
    };
    let decimals = mantissa_code.split_once('.').map_or(0, |(_, f)| {
        f.chars().filter(|c| matches!(c, '0' | '#' | '?')).count()
    });
    let thousands = mantissa_code.contains(',');
    let sign = if x < 0.0 { "-" } else { "" };
    let a = x.abs();
    let body = if let Some(exp_code) = exponent {
        let s = format!("{a:.decimals$E}");
        let (m, e) = s.split_once('E').unwrap_or((&s, "0"));
        let e: i32 = e.parse().unwrap_or(0);
        let digits = exp_code.chars().filter(|c| *c == '0').count().max(1);
        let esign = if e < 0 {
            "-"
        } else if exp_code.contains('+') {
            "+"
        } else {
            ""
        };
        format!("{m}E{esign}{:0digits$}", e.abs())
    } else {
        let s = format!("{a:.decimals$}");
        let (int, frac) = s.split_once('.').map_or((s.as_str(), ""), |(i, f)| (i, f));
        let int = if thousands {
            group_thousands(int)
        } else {
            int.to_owned()
        };
        // Optional decimals (#) drop trailing zeros.
        let required = mantissa_code
            .split_once('.')
            .map_or(0, |(_, f)| f.chars().filter(|c| *c == '0').count());
        let mut frac = frac.to_owned();
        while frac.len() > required && frac.ends_with('0') {
            frac.pop();
        }
        if frac.is_empty() {
            int
        } else {
            format!("{int}.{frac}")
        }
    };
    format!("{sign}{prefix}{body}{suffix}")
}

/// (year, month, day) of a day count from 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, u32::try_from(m).unwrap_or(1), d)
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A date serial through a date format.
fn date(serial: f64, body: &str, date1904: bool) -> Option<String> {
    if !(0.0..2_958_466.0).contains(&serial) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let whole = serial.floor() as i64;
    #[allow(clippy::cast_possible_truncation)]
    let seconds = ((serial - serial.floor()) * 86_400.0).round() as i64;
    // Days from 1970-01-01. Excel's 1900 system counts 29 February 1900,
    // which never was: serial 60.
    let days = if date1904 {
        whole - 24_107
    } else if whole >= 61 {
        whole - 25_569
    } else if whole == 60 {
        return Some("1900-02-29".to_owned());
    } else {
        whole - 25_568
    };
    let (y, m, d) = civil(days);
    let (h, min, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    let ampm = body.to_ascii_lowercase().contains("am/pm");
    let mut out = String::new();
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    let mut last_was_hour = false;
    while i < chars.len() {
        let c = chars[i];
        let run = chars[i..]
            .iter()
            .take_while(|x| x.eq_ignore_ascii_case(&c))
            .count();
        match c.to_ascii_lowercase() {
            'y' => {
                if run <= 2 {
                    out.push_str(&format!("{:02}", y.rem_euclid(100)));
                } else {
                    out.push_str(&y.to_string());
                }
                last_was_hour = false;
            }
            'm' => {
                let minutes = last_was_hour
                    || chars[i + run..]
                        .iter()
                        .find(|x| x.is_ascii_alphabetic())
                        .is_some_and(|x| x.eq_ignore_ascii_case(&'s'));
                if minutes && run <= 2 {
                    out.push_str(&format!("{min:02}"));
                } else {
                    let name = MONTHS[(m - 1) as usize];
                    match run {
                        1 => out.push_str(&m.to_string()),
                        2 => out.push_str(&format!("{m:02}")),
                        3 => out.push_str(&name[..3]),
                        _ => out.push_str(name),
                    }
                }
                last_was_hour = false;
            }
            'd' => {
                out.push_str(&if run == 1 {
                    d.to_string()
                } else {
                    format!("{d:02}")
                });
                last_was_hour = false;
            }
            'h' => {
                let hour = if ampm {
                    let x = h % 12;
                    if x == 0 { 12 } else { x }
                } else {
                    h
                };
                out.push_str(&if run == 1 {
                    hour.to_string()
                } else {
                    format!("{hour:02}")
                });
                last_was_hour = true;
            }
            's' => {
                out.push_str(&if run == 1 {
                    s.to_string()
                } else {
                    format!("{s:02}")
                });
                last_was_hour = false;
            }
            'a' if body[byte_index(&chars, i)..]
                .to_ascii_lowercase()
                .starts_with("am/pm") =>
            {
                out.push_str(if h < 12 { "AM" } else { "PM" });
                i += 5;
                continue;
            }
            '"' => {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '"' {
                    out.push(chars[j]);
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            '\\' => {
                if let Some(next) = chars.get(i + 1) {
                    out.push(*next);
                }
                i += 2;
                continue;
            }
            _ => {
                out.push(c);
                i += 1;
                continue;
            }
        }
        i += run;
    }
    Some(out)
}

fn byte_index(chars: &[char], i: usize) -> usize {
    chars[..i].iter().map(|c| c.len_utf8()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(lexeme: &str, code: &str) -> String {
        display(&Value::Number(lexeme), code, false)
    }

    #[test]
    fn numbers_show_through_their_formats() {
        assert_eq!(show("49", "\"$\"#,##0"), "$49");
        assert_eq!(show("1234567.891", "#,##0.00"), "1,234,567.89");
        assert_eq!(show("0.02", "0.00%"), "2.00%");
        assert_eq!(show("0.18", "0%"), "18%");
        assert_eq!(show("-1500", "#,##0 ;(#,##0)"), "(1,500)");
        assert_eq!(show("5762.4", "General"), "5762.4");
        assert_eq!(show("151.30000000000001", "General"), "151.3");
        assert_eq!(show("0.30000000000000004", "General"), "0.3");
        assert_eq!(show("12345678901234567000", "General"), "1.23457E+19");
        assert_eq!(show("1E-3", "General"), "0.001");
        assert_eq!(show("123456", "0.00E+00"), "1.23E+05");
        assert_eq!(show("2500", "[$€-407] #,##0"), "€ 2,500");
        assert_eq!(show("-7", "[Red]0"), "-7");
        assert_eq!(show("1.5", "0.0#"), "1.5");
        assert_eq!(show("1.25", "0.0#"), "1.25");
    }

    #[test]
    fn dates_show_as_dates() {
        assert_eq!(show("46023", "yyyy\\-mm\\-dd"), "2026-01-01");
        assert_eq!(show("46023", "m/d/yyyy"), "1/1/2026");
        assert_eq!(show("46054", "d-mmm-yy"), "1-Feb-26");
        assert_eq!(show("46023.75", "h:mm AM/PM"), "6:00 PM");
        assert_eq!(show("45658.5", "yyyy-mm-dd hh:mm"), "2025-01-01 12:00");
        assert_eq!(show("1", "yyyy-mm-dd"), "1900-01-01");
        assert_eq!(show("61", "yyyy-mm-dd"), "1900-03-01");
        assert_eq!(
            display(&Value::Number("0"), "yyyy-mm-dd", true),
            "1904-01-01"
        );
    }

    #[test]
    fn other_values_show_as_they_are() {
        assert_eq!(display(&Value::Text("00123"), "@", false), "00123");
        assert_eq!(display(&Value::Bool(true), "General", false), "TRUE");
        assert_eq!(display(&Value::Error("#N/A"), "0.00", false), "#N/A");
    }
}
