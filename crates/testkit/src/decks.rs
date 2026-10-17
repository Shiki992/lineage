// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The synthetic .pptx fixtures. All content is invented: "Lumen Ledger", its
//! founders, its numbers and every investor named here are fictional. Never put
//! real decks or real people's data in this file.
//!
//! Each fixture is listed in `fixtures/pptx/README.md`. Add one here, run
//! `cargo xtask fixtures`, and add its row to the README.

use crate::png;
use crate::pptx::{
    ChartData, Deck, Layout, Placed, Shape, Slide, Zip, creation_id_for, p, p_at, p_runs, r,
    r_bold, r_link, rect,
};

/// Where generated deck fixtures live, relative to the repository root.
pub const OUT_DIR: &str = "fixtures/pptx/synthetic";

/// Slide ids in the pitch deck.
pub mod ids {
    pub const TITLE: u32 = 256;
    pub const PROBLEM: u32 = 257;
    pub const SOLUTION: u32 = 258;
    pub const MARKET: u32 = 259;
    pub const PRODUCT: u32 = 260;
    pub const TRACTION: u32 = 261;
    pub const TEAM: u32 = 262;
    pub const ASK: u32 = 263;
}

fn title(text: &str) -> Shape {
    Shape::Title(vec![p(text)])
}

fn bullets(lines: &[&str]) -> Shape {
    Shape::Body {
        idx: 1,
        paras: lines.iter().map(|l| p(l)).collect(),
    }
}

/// The traction chart's data.
pub fn traction_data(values: [i64; 4]) -> ChartData {
    ChartData {
        series: "ARR ($k)".to_owned(),
        categories: ["Q1", "Q2", "Q3", "Q4"]
            .iter()
            .map(|q| format!("{q} 2026"))
            .collect(),
        values: values.to_vec(),
    }
}

/// The text box beside the traction chart.
pub fn traction_box(text: &str) -> Shape {
    Shape::TextBox {
        name: "TextBox 3".to_owned(),
        at: rect(7_315_200, 1_825_625, 4_038_600, 1_200_000),
        paras: vec![p_runs(vec![r_bold(text)]), p("Net revenue retention 128%")],
    }
}

/// The master pitch deck: eight slides, Traction is slide 6.
pub fn pitch_deck() -> Deck {
    let slides = vec![
        Slide::new(
            ids::TITLE,
            Layout::Title,
            vec![
                Shape::Title(vec![p("Lumen Ledger")]),
                Shape::Subtitle(vec![p("Seed round · 2026")]),
            ],
        )
        .with_notes(&["Thank them for the time. Two minutes on the problem."]),
        Slide::new(
            ids::PROBLEM,
            Layout::TitleAndContent,
            vec![
                title("Problem"),
                bullets(&[
                    "Small lenders reconcile loan books by hand",
                    "Month-end close takes nine days on average",
                    "Errors surface only at audit",
                ]),
            ],
        ),
        Slide::new(
            ids::SOLUTION,
            Layout::TitleAndContent,
            vec![
                title("Solution"),
                Shape::Body {
                    idx: 1,
                    paras: vec![
                        p("A ledger that reconciles itself every night"),
                        p_at(1, "Connects to the core banking system in a day"),
                        p_at(1, "Flags breaks before they reach the books"),
                    ],
                },
            ],
        ),
        Slide::new(
            ids::MARKET,
            Layout::TitleAndContent,
            vec![
                title("Market"),
                bullets(&[
                    "4,800 community lenders in North America",
                    "$4.2B serviceable market",
                ]),
            ],
        )
        .with_notes(&["Bottom-up: lenders times average contract."]),
        Slide::new(
            ids::PRODUCT,
            Layout::TitleOnly,
            vec![
                title("Product"),
                Shape::Picture {
                    name: "Picture 2".to_owned(),
                    descr: "The reconciliation dashboard".to_owned(),
                    at: rect(2_286_000, 1_825_625, 7_620_000, 4_286_250),
                    png: png::solid_square(16, [14, 124, 123]),
                },
            ],
        ),
        Slide::new(
            ids::TRACTION,
            Layout::TitleOnly,
            vec![
                title("Traction"),
                Shape::Chart {
                    name: "Chart 2".to_owned(),
                    at: rect(838_200, 1_825_625, 6_096_000, 4_351_338),
                    data: traction_data([310, 560, 840, 1200]),
                },
                traction_box("$1.2M ARR"),
            ],
        )
        .with_notes(&["Numbers as of 30 September."]),
        Slide::new(
            ids::TEAM,
            Layout::TwoContent,
            vec![
                title("Team"),
                Shape::Body {
                    idx: 1,
                    paras: vec![
                        p("Rowan Ellis, CEO"),
                        p_at(1, "Ran lending ops at a regional bank"),
                    ],
                },
                Shape::Body {
                    idx: 2,
                    paras: vec![
                        p("Imani Okafor, CTO"),
                        p_at(1, "Built ledgers at two payment firms"),
                    ],
                },
            ],
        ),
        Slide::new(
            ids::ASK,
            Layout::TitleAndContent,
            vec![
                title("The ask"),
                bullets(&[
                    "Raising $2.5M seed",
                    "18 months of runway",
                    "Hire four engineers",
                ]),
            ],
        ),
    ];
    Deck::new("Lumen Ledger pitch deck", slides)
}

/// The master after "New traction numbers": text and chart changed on slide 6.
pub fn traction_update(mut deck: Deck) -> Deck {
    let slide = deck.slide_mut(ids::TRACTION);
    for placed in &mut slide.shapes {
        match &mut placed.shape {
            Shape::Chart { data, .. } => *data = traction_data([310, 560, 840, 2000]),
            Shape::TextBox { .. } => placed.shape = traction_box("$2.0M ARR"),
            _ => {}
        }
    }
    deck
}

/// The tailored version for Northgate Capital: a new subtitle and one more slide.
pub fn northgate(mut deck: Deck) -> Deck {
    deck.slide_mut(ids::TITLE).shapes[1] =
        Placed::from(Shape::Subtitle(vec![p("Prepared for Northgate Capital")]));
    let why = Slide::new(
        264,
        Layout::TitleAndContent,
        vec![
            title("Why Northgate"),
            bullets(&["Your portfolio lenders are our first customers"]),
        ],
    );
    let at = deck
        .slides
        .iter()
        .position(|s| s.id == ids::ASK)
        .unwrap_or(deck.slides.len());
    deck.slides.insert(at, why);
    deck
}

fn move_slide(deck: &mut Deck, id: u32, after: u32) {
    let from = deck.slides.iter().position(|s| s.id == id).expect("slide");
    let slide = deck.slides.remove(from);
    let to = deck
        .slides
        .iter()
        .position(|s| s.id == after)
        .expect("slide")
        + 1;
    deck.slides.insert(to, slide);
}

/// Market moved after Traction.
pub fn reordered(mut deck: Deck) -> Deck {
    move_slide(&mut deck, ids::MARKET, ids::TRACTION);
    deck
}

/// Traction duplicated, as PowerPoint's Duplicate Slide does: a new id, a new
/// creation id and a new file, with the same content, right after the original.
pub fn duplicated(mut deck: Deck) -> Deck {
    let at = deck
        .slides
        .iter()
        .position(|s| s.id == ids::TRACTION)
        .expect("traction");
    let mut copy = deck.slides[at].clone();
    copy.id = 264;
    copy.creation_id = Some(creation_id_for(264));
    copy.file = 9;
    deck.slides.insert(at + 1, copy);
    deck
}

fn edit_chart(mut deck: Deck) -> Deck {
    for placed in &mut deck.slide_mut(ids::TRACTION).shapes {
        if let Shape::Chart { data, .. } = &mut placed.shape {
            *data = traction_data([300, 560, 900, 1200]);
        }
    }
    deck
}

fn edit_notes(mut deck: Deck) -> Deck {
    deck.slide_mut(ids::TRACTION).notes = Some(vec![
        "Numbers as of 30 September.".to_owned(),
        "Mention the two pilots starting in November.".to_owned(),
    ]);
    deck
}

fn appendix(hidden: bool) -> Slide {
    let slide = Slide::new(
        264,
        Layout::TitleAndContent,
        vec![
            title("Appendix: unit economics"),
            bullets(&["CAC payback 11 months", "Gross margin 81%"]),
        ],
    );
    if hidden { slide.hidden() } else { slide }
}

fn with_hidden_appendix(mut deck: Deck) -> Deck {
    deck.slides.push(appendix(true));
    deck
}

fn layout_changed(mut deck: Deck) -> Deck {
    let slide = deck.slide_mut(ids::MARKET);
    slide.layout = Layout::TitleOnly;
    slide.shapes = vec![
        Placed::from(title("Market")),
        Placed::from(Shape::TextBox {
            name: "TextBox 2".to_owned(),
            at: rect(838_200, 1_825_625, 10_515_600, 1_500_000),
            paras: vec![
                p("4,800 community lenders in North America"),
                p("$4.2B serviceable market"),
            ],
        }),
    ];
    deck
}

fn picture_moved(mut deck: Deck) -> Deck {
    for placed in &mut deck.slide_mut(ids::PRODUCT).shapes {
        if let Shape::Picture { at, .. } = &mut placed.shape {
            at.x += 914_400;
        }
    }
    deck
}

fn made_bold(mut deck: Deck) -> Deck {
    deck.slide_mut(ids::MARKET).shapes[1] = Placed::from(Shape::Body {
        idx: 1,
        paras: vec![
            p("4,800 community lenders in North America"),
            p_runs(vec![r_bold("$4.2B"), r(" serviceable market")]),
        ],
    });
    deck
}

fn team_removed(mut deck: Deck) -> Deck {
    deck.slides.retain(|s| s.id != ids::TEAM);
    deck
}

fn with_sections(mut deck: Deck) -> Deck {
    deck.sections = vec![
        (
            "Story".to_owned(),
            vec![ids::TITLE, ids::PROBLEM, ids::SOLUTION, ids::MARKET],
        ),
        (
            "Business".to_owned(),
            vec![ids::PRODUCT, ids::TRACTION, ids::TEAM, ids::ASK],
        ),
    ];
    deck
}

fn with_comments(mut deck: Deck) -> Deck {
    deck.authors = vec![
        ("Jordan Vale".to_owned(), "JV".to_owned()),
        ("Sam Okoro".to_owned(), "SO".to_owned()),
    ];
    deck.slide_mut(ids::TRACTION).comments = vec![
        (0, "Is Q4 booked or signed?".to_owned()),
        (1, "Add logos of the first customers.".to_owned()),
    ];
    deck.slide_mut(ids::ASK).comments = vec![(0, "Say what the money buys.".to_owned())];
    deck
}

fn with_custom_properties(mut deck: Deck) -> Deck {
    deck.custom = vec![
        ("Round".to_owned(), "Seed".to_owned()),
        ("Owner".to_owned(), "Rowan Ellis".to_owned()),
    ];
    deck
}

fn with_empty_properties(mut deck: Deck) -> Deck {
    deck.empty_custom = true;
    deck
}

fn zipped(mut deck: Deck, zip: Zip) -> Deck {
    deck.zip = zip;
    deck
}

/// Both decks add a different slide with the same id, 264.
fn added_a(mut deck: Deck) -> Deck {
    let at = deck.slides.len() - 1;
    deck.slides.insert(
        at,
        Slide::new(
            264,
            Layout::TitleAndContent,
            vec![title("Pricing"), bullets(&["$1,800 per month per lender"])],
        ),
    );
    deck
}

fn added_b(mut deck: Deck) -> Deck {
    let at = deck.slides.len() - 1;
    let mut slide = Slide::new(
        264,
        Layout::TitleAndContent,
        vec![
            title("Competition"),
            bullets(&["Spreadsheets and consultants"]),
        ],
    );
    // PowerPoint gives every new slide a random creation id.
    slide.creation_id = Some(1_987_654_321);
    deck.slides.insert(at, slide);
    deck
}

fn without_creation_ids(mut deck: Deck) -> Deck {
    for slide in &mut deck.slides {
        slide.creation_id = None;
    }
    deck
}

fn with_table(mut deck: Deck) -> Deck {
    deck.slides.insert(
        6,
        Slide::new(
            264,
            Layout::TitleOnly,
            vec![
                title("Use of funds"),
                Shape::Table {
                    name: "Table 2".to_owned(),
                    at: rect(838_200, 1_825_625, 10_515_600, 1_483_360),
                    rows: vec![
                        vec!["Area".to_owned(), "Share".to_owned()],
                        vec!["Engineering".to_owned(), "60%".to_owned()],
                        vec!["Sales".to_owned(), "30%".to_owned()],
                        vec!["Operations".to_owned(), "10%".to_owned()],
                    ],
                },
            ],
        ),
    );
    deck
}

fn with_group(mut deck: Deck) -> Deck {
    deck.slide_mut(ids::SOLUTION)
        .shapes
        .push(Placed::from(Shape::Group {
            name: "Group 3".to_owned(),
            shapes: vec![
                Shape::TextBox {
                    name: "TextBox 4".to_owned(),
                    at: rect(838_200, 5_200_000, 3_000_000, 500_000),
                    paras: vec![p("Nightly")],
                },
                Shape::TextBox {
                    name: "TextBox 5".to_owned(),
                    at: rect(4_200_000, 5_200_000, 3_000_000, 500_000),
                    paras: vec![p("Self-checking")],
                },
            ],
        }));
    deck
}

fn with_links(mut deck: Deck) -> Deck {
    deck.slide_mut(ids::ASK)
        .shapes
        .push(Placed::from(Shape::TextBox {
            name: "TextBox 3".to_owned(),
            at: rect(838_200, 5_600_000, 10_515_600, 500_000),
            paras: vec![p_runs(vec![
                r("Data room: "),
                r_link(
                    "lumenledger.example/room",
                    "https://lumenledger.example/room?ref=deck&v=2",
                ),
            ])],
        }));
    deck
}

/// An entrance animation on the traction text box (shape id 4).
pub const APPEAR_ON_CLICK: &str = r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst><p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="5" presetID="1" presetClass="entr" presetSubtype="0" fill="hold" nodeType="clickEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:cTn id="6" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="4"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#;

fn with_animation(mut deck: Deck) -> Deck {
    deck.slide_mut(ids::TRACTION).timing = Some(APPEAR_ON_CLICK.to_owned());
    deck
}

/// Every synthetic deck fixture, as (file name, deck).
pub fn all() -> Vec<(&'static str, Deck)> {
    let base = pitch_deck;
    vec![
        ("pitch-deck.pptx", base()),
        ("pitch-deck-traction-update.pptx", traction_update(base())),
        ("pitch-deck-northgate.pptx", northgate(base())),
        ("pitch-deck-reordered.pptx", reordered(base())),
        ("pitch-deck-duplicated.pptx", duplicated(base())),
        ("pitch-deck-chart-edited.pptx", edit_chart(base())),
        ("pitch-deck-notes-edited.pptx", edit_notes(base())),
        (
            "pitch-deck-hidden-appendix.pptx",
            with_hidden_appendix(base()),
        ),
        ("pitch-deck-layout-changed.pptx", layout_changed(base())),
        ("pitch-deck-picture-moved.pptx", picture_moved(base())),
        ("pitch-deck-bold.pptx", made_bold(base())),
        ("pitch-deck-team-removed.pptx", team_removed(base())),
        ("pitch-deck-sections.pptx", with_sections(base())),
        ("pitch-deck-comments.pptx", with_comments(base())),
        (
            "pitch-deck-custom-properties.pptx",
            with_custom_properties(base()),
        ),
        (
            "pitch-deck-empty-properties.pptx",
            with_empty_properties(base()),
        ),
        ("pitch-deck-deflated.pptx", zipped(base(), Zip::Deflated)),
        ("pitch-deck-streamed.pptx", zipped(base(), Zip::Streamed)),
        ("pitch-deck-added-a.pptx", added_a(base())),
        ("pitch-deck-added-b.pptx", added_b(base())),
        (
            "pitch-deck-no-creation-ids.pptx",
            without_creation_ids(base()),
        ),
        ("pitch-deck-table.pptx", with_table(base())),
        ("pitch-deck-group.pptx", with_group(base())),
        ("pitch-deck-links.pptx", with_links(base())),
        ("pitch-deck-animation.pptx", with_animation(base())),
    ]
}

/// Named builders tests use to compose scenarios.
pub mod edits {
    pub use super::{duplicated, northgate, reordered, traction_update};

    pub fn chart_edited(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::edit_chart(deck)
    }
    pub fn notes_edited(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::edit_notes(deck)
    }
    pub fn hidden_appendix(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::with_hidden_appendix(deck)
    }
    pub fn layout_changed(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::layout_changed(deck)
    }
    pub fn picture_moved(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::picture_moved(deck)
    }
    pub fn made_bold(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::made_bold(deck)
    }
    pub fn team_removed(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::team_removed(deck)
    }
    pub fn sections(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::with_sections(deck)
    }
    pub fn comments(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::with_comments(deck)
    }
    pub fn added_a(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::added_a(deck)
    }
    pub fn added_b(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::added_b(deck)
    }
    pub fn table(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::with_table(deck)
    }
    pub fn group(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::with_group(deck)
    }
    pub fn links(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::with_links(deck)
    }
    pub fn animation(deck: crate::pptx::Deck) -> crate::pptx::Deck {
        super::with_animation(deck)
    }
    pub fn appendix(hidden: bool) -> crate::pptx::Slide {
        super::appendix(hidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_decks_differ_from_the_base() {
        let all = all();
        let base = pitch_deck().to_bytes();
        for (i, (name, deck)) in all.iter().enumerate() {
            assert!(all[i + 1..].iter().all(|(n, _)| n != name), "{name} twice");
            if *name != "pitch-deck.pptx" {
                assert_ne!(deck.to_bytes(), base, "{name} is the base deck");
            }
        }
    }

    #[test]
    fn traction_is_slide_six() {
        let deck = pitch_deck();
        assert_eq!(deck.slides[5].title().as_deref(), Some("Traction"));
    }
}
