// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! How alike two pieces of text are, as a whole percentage (no floats).
//!
//! Dice's coefficient over word pairs (bigrams) of the lower-cased words. Short
//! texts with fewer than two words use character pairs instead, so one-word
//! headings can still be compared.

use crate::hash::{Fnv, fnv};

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn grams(text: &str) -> Vec<u64> {
    let words = words(text);
    let mut out: Vec<u64> = if words.len() >= 2 {
        words
            .windows(2)
            .map(|pair| Fnv::new().str(&pair[0]).str(&pair[1]).finish())
            .collect()
    } else {
        let chars: Vec<char> = text
            .to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if chars.len() < 2 {
            return chars.iter().map(|c| fnv(&c.to_string())).collect();
        }
        chars
            .windows(2)
            .map(|pair| fnv(&pair.iter().collect::<String>()))
            .collect()
    };
    out.sort_unstable();
    out
}

/// Similarity of two texts, 0 to 100. Two empty texts are 100% alike.
#[must_use]
pub fn similarity(a: &str, b: &str) -> u8 {
    let (ga, gb) = (grams(a), grams(b));
    if ga.is_empty() && gb.is_empty() {
        return if a.trim() == b.trim() { 100 } else { 0 };
    }
    if ga.is_empty() || gb.is_empty() {
        return 0;
    }
    let (mut i, mut j, mut common) = (0, 0, 0_usize);
    while i < ga.len() && j < gb.len() {
        match ga[i].cmp(&gb[j]) {
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    let percent = 200 * common / (ga.len() + gb.len());
    u8::try_from(percent).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_and_unrelated() {
        assert_eq!(
            similarity("Data engineer at Corvia", "Data engineer at Corvia"),
            100
        );
        assert_eq!(similarity("", ""), 100);
        assert_eq!(similarity("Data engineer", "Kitchen garden"), 0);
        assert_eq!(similarity("Words here", ""), 0);
    }

    #[test]
    fn small_edits_stay_similar() {
        let a = "Data engineer, Corvia Freight, 2020 to 2023";
        let b = "Lead data engineer, Corvia Freight, 2020 to 2023";
        assert!(similarity(a, b) >= 80, "{}", similarity(a, b));
        assert_eq!(similarity(a, b), similarity(b, a));
    }

    #[test]
    fn case_and_punctuation_do_not_matter() {
        assert_eq!(similarity("Data, engineer!", "data engineer"), 100);
    }

    #[test]
    fn single_words_use_characters() {
        assert!(similarity("Experience", "Experiences") > 80);
        assert!(similarity("Skills", "Education") < 30);
    }
}
