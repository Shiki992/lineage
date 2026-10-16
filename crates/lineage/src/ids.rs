// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Identifiers and time. Both come from the host: the crate has no random
//! numbers and no clock, so every command is a pure function of its inputs.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A 32-hex-digit identifier (128 bits), as the host generates them. Ids name
/// folders in storage, so anything else is refused, including on read.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id(String);

impl TryFrom<String> for Id {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        Self::parse(&text)
    }
}

impl From<Id> for String {
    fn from(id: Id) -> Self {
        id.0
    }
}

impl Id {
    /// Checks and wraps an id.
    ///
    /// # Errors
    /// When the text is not 32 lower-case hex digits.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() == 32
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(text.to_owned()))
        } else {
            Err(format!("not an id: {text:?}"))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where new ids come from. The task pane uses `crypto.randomUUID`; tests use
/// [`SequentialIds`].
pub trait IdSource {
    fn next_id(&mut self) -> Id;
}

/// Ids 000…001, 000…002, … with a prefix digit, for reproducible tests.
#[derive(Clone, Debug, Default)]
pub struct SequentialIds {
    next: u64,
    prefix: u8,
}

impl SequentialIds {
    #[must_use]
    pub fn new(prefix: u8) -> Self {
        Self {
            next: 0,
            prefix: prefix % 16,
        }
    }
}

impl IdSource for SequentialIds {
    fn next_id(&mut self) -> Id {
        self.next += 1;
        Id(format!("{:x}{:031x}", self.prefix, self.next))
    }
}

/// Ids for the task pane: the first 128 bits of SHA-256(seed ‖ counter). The
/// host draws the 32-byte seed from a secure random source once, so the crate
/// itself stays free of randomness and ids never repeat within a session.
#[derive(Clone, Debug)]
pub struct SeededIds {
    seed: [u8; 32],
    counter: u64,
}

impl SeededIds {
    #[must_use]
    pub fn new(seed: [u8; 32]) -> Self {
        Self { seed, counter: 0 }
    }
}

impl IdSource for SeededIds {
    fn next_id(&mut self) -> Id {
        self.counter += 1;
        let mut input = Vec::with_capacity(40);
        input.extend_from_slice(&self.seed);
        input.extend_from_slice(&self.counter.to_le_bytes());
        let hex = lineage_core::hash::sha256_hex(&input);
        Id(hex[..32].to_owned())
    }
}

/// A point in time: milliseconds since 1970-01-01T00:00:00Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's civil-from-days.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
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

impl Timestamp {
    fn parts(self) -> (i64, u32, u32, i64, i64, i64) {
        let seconds = self.0.div_euclid(1000);
        let days = seconds.div_euclid(86_400);
        let rest = seconds.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        (y, m, d, rest / 3600, (rest % 3600) / 60, rest % 60)
    }

    /// ISO 8601 in UTC to the second, as Word's `w:date` wants: `2026-10-15T09:30:00Z`.
    #[must_use]
    pub fn iso(self) -> String {
        let (y, m, d, h, min, s) = self.parts();
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z")
    }

    /// "October 2026", for grouping sent copies.
    #[must_use]
    pub fn month_label(self) -> String {
        let (y, m, ..) = self.parts();
        format!("{} {y}", MONTHS[(m - 1) as usize])
    }

    /// "2 Oct", as on the panel.
    #[must_use]
    pub fn day_label(self) -> String {
        let (_, m, d, ..) = self.parts();
        format!("{d} {}", &MONTHS[(m - 1) as usize][..3])
    }

    /// "2 Oct 2026", where the year matters (a library of families).
    #[must_use]
    pub fn date_label(self) -> String {
        let (y, ..) = self.parts();
        format!("{} {y}", self.day_label())
    }

    /// The (year, month) pair, for grouping.
    #[must_use]
    pub fn year_month(self) -> (i64, u32) {
        let (y, m, ..) = self.parts();
        (y, m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_checked() {
        assert!(Id::parse("0123456789abcdef0123456789abcdef").is_ok());
        assert!(Id::parse("0123456789ABCDEF0123456789ABCDEF").is_err());
        assert!(Id::parse("short").is_err());
        let mut ids = SequentialIds::new(1);
        assert_eq!(ids.next_id().as_str(), "10000000000000000000000000000001");
        assert!(Id::parse(ids.next_id().as_str()).is_ok());
    }

    #[test]
    fn seeded_ids_are_well_formed_and_distinct() {
        let mut a = SeededIds::new([7; 32]);
        let mut b = SeededIds::new([8; 32]);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..1000 {
            for id in [a.next_id(), b.next_id()] {
                assert!(Id::parse(id.as_str()).is_ok(), "{id}");
                assert!(seen.insert(id), "ids repeat");
            }
        }
        let mut again = SeededIds::new([7; 32]);
        assert_eq!(
            again.next_id(),
            SeededIds::new([7; 32]).next_id(),
            "the same seed gives the same ids"
        );
    }

    #[test]
    fn dates() {
        assert_eq!(Timestamp(0).iso(), "1970-01-01T00:00:00Z");
        // 2026-10-15T09:30:00Z
        let t = Timestamp(1_792_056_600_000);
        assert_eq!(t.iso(), "2026-10-15T09:30:00Z");
        assert_eq!(t.month_label(), "October 2026");
        assert_eq!(t.day_label(), "15 Oct");
        assert_eq!(Timestamp(951_782_400_000).iso(), "2000-02-29T00:00:00Z");
    }
}
