// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The scenario series: one invented master resume, later versions of it, and
//! tailored versions with the edits a person would make in Word. Merge, change
//! state and harness tests are built on these. Avery Quinn, every company and
//! every school are fictional.
//!
//! Each document is a list of [`Line`]s, so tests can build their own variations
//! in memory from the same pieces.

use crate::docx::{Package, list_item, para, plain, run, run_with, styled};

/// One block of a resume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Title(String),
    Plain(String),
    H1(String),
    H2(String),
    Bullet(String),
    /// Raw `w:p` (or other block) XML, for edits that change markup rather than text.
    Raw(String),
}

impl Line {
    fn xml(&self) -> String {
        match self {
            Self::Title(text) => styled("Title", text),
            Self::Plain(text) => plain(text),
            Self::H1(text) => styled("Heading1", text),
            Self::H2(text) => styled("Heading2", text),
            Self::Bullet(text) => list_item(1, 0, text),
            Self::Raw(xml) => xml.clone(),
        }
    }
}

fn t(text: &str) -> String {
    text.to_owned()
}

/// The text of the new job added in master v2.
pub const NEW_JOB_TITLE: &str = "Senior data engineer, Halden Robotics, 2023 to now";
/// The bullet under the new job.
pub const NEW_JOB_BULLET: &str =
    "Moved nightly batch jobs to streaming, cutting report delays from hours to minutes.";
/// The Corvia job title in v1 and v2.
pub const CORVIA_TITLE: &str = "Data engineer, Corvia Freight, 2020 to 2023";
/// The Corvia job title after master v3.
pub const CORVIA_TITLE_V3: &str = "Lead data engineer, Corvia Freight, 2020 to 2023";

/// Master v1.
#[must_use]
pub fn master_v1_lines() -> Vec<Line> {
    vec![
        Line::Title(t("Avery Quinn")),
        Line::Plain(t(
            "Data engineer · Lakeside, Ontario · avery.quinn@example.com",
        )),
        Line::H1(t("Summary")),
        Line::Plain(t(
            "Data engineer with six years of experience building reliable pipelines for retail and logistics teams.",
        )),
        Line::H1(t("Experience")),
        Line::H2(t(CORVIA_TITLE)),
        Line::Bullet(t(
            "Built the shipment data warehouse used by forty analysts.",
        )),
        Line::Bullet(t("Cut nightly load times from six hours to forty minutes.")),
        Line::H2(t("Analyst, Brightwater Insurance, 2018 to 2020")),
        Line::Bullet(t("Automated weekly claims reports in Python.")),
        Line::H1(t("Skills")),
        Line::Plain(t("SQL, Python, dbt, Airflow, cloud warehouses")),
        Line::H1(t("Education")),
        Line::Plain(t("BSc Computer Science, Northbridge University, 2018")),
    ]
}

fn index_of(lines: &[Line], wanted: &Line) -> usize {
    lines
        .iter()
        .position(|line| line == wanted)
        .unwrap_or_else(|| panic!("line {wanted:?} is in the resume"))
}

/// Master v2: a new job at the top of Experience.
#[must_use]
pub fn master_v2_lines() -> Vec<Line> {
    let mut lines = master_v1_lines();
    let at = index_of(&lines, &Line::H1(t("Experience"))) + 1;
    lines.insert(at, Line::H2(t(NEW_JOB_TITLE)));
    lines.insert(at + 1, Line::Bullet(t(NEW_JOB_BULLET)));
    lines
}

/// Master v3: the Corvia title edited, and the name made bold (formatting only).
#[must_use]
pub fn master_v3_lines() -> Vec<Line> {
    let mut lines = master_v2_lines();
    let corvia = index_of(&lines, &Line::H2(t(CORVIA_TITLE)));
    lines[corvia] = Line::H2(t(CORVIA_TITLE_V3));
    lines[0] = Line::Raw(para(
        r#"<w:pStyle w:val="Title"/>"#,
        &run_with("<w:b/>", "Avery Quinn"),
    ));
    lines
}

/// A tailored version made from v1: the summary rewritten, Skills moved above
/// Experience and the Brightwater bullet removed. None of these touch what v2 or
/// v3 change.
#[must_use]
pub fn tailored_data_lines() -> Vec<Line> {
    let mut lines = master_v1_lines();
    let summary = index_of(&lines, &Line::H1(t("Summary"))) + 1;
    lines[summary] = Line::Plain(t(
        "Data engineer who builds analytics platforms that analysts trust, with six years in retail and logistics.",
    ));
    let bullet = index_of(
        &lines,
        &Line::Bullet(t("Automated weekly claims reports in Python.")),
    );
    lines.remove(bullet);
    let skills = index_of(&lines, &Line::H1(t("Skills")));
    let moved: Vec<Line> = lines.drain(skills..skills + 2).collect();
    let experience = index_of(&lines, &Line::H1(t("Experience")));
    for (offset, line) in moved.into_iter().enumerate() {
        lines.insert(experience + offset, line);
    }
    lines
}

/// A tailored version that edits the same paragraph master v3 edits.
#[must_use]
pub fn tailored_same_para_lines() -> Vec<Line> {
    let mut lines = master_v1_lines();
    let corvia = index_of(&lines, &Line::H2(t(CORVIA_TITLE)));
    lines[corvia] = Line::H2(t("Data engineer (logistics), Corvia Freight, 2020 to 2023"));
    lines
}

/// A tailored version that deletes the Corvia job, which master v3 edits.
#[must_use]
pub fn tailored_deleted_job_lines() -> Vec<Line> {
    let mut lines = master_v1_lines();
    let corvia = index_of(&lines, &Line::H2(t(CORVIA_TITLE)));
    lines.drain(corvia..corvia + 3);
    lines
}

/// A tailored version where the person typed v2's new job in by hand. The text is
/// the same, but Word split it into runs differently.
#[must_use]
pub fn tailored_copied_job_lines() -> Vec<Line> {
    let mut lines = tailored_data_lines();
    let at = index_of(&lines, &Line::H1(t("Experience"))) + 1;
    let (first, second) = NEW_JOB_TITLE.split_at(21);
    lines.insert(
        at,
        Line::Raw(para(
            r#"<w:pStyle w:val="Heading2"/>"#,
            &[run(first), run(second)].concat(),
        )),
    );
    lines.insert(at + 1, Line::Bullet(t(NEW_JOB_BULLET)));
    lines
}

/// Master v1 with Skills moved to the top and Education moved above Experience.
#[must_use]
pub fn reordered_lines() -> Vec<Line> {
    let mut lines = master_v1_lines();
    let skills = index_of(&lines, &Line::H1(t("Skills")));
    let skills_block: Vec<Line> = lines.drain(skills..skills + 2).collect();
    let education = index_of(&lines, &Line::H1(t("Education")));
    let education_block: Vec<Line> = lines.drain(education..education + 2).collect();
    let summary = index_of(&lines, &Line::H1(t("Summary")));
    for (offset, line) in skills_block.into_iter().enumerate() {
        lines.insert(summary + offset, line);
    }
    let experience = index_of(&lines, &Line::H1(t("Experience")));
    for (offset, line) in education_block.into_iter().enumerate() {
        lines.insert(experience + offset, line);
    }
    lines
}

/// A resume package from lines.
#[must_use]
pub fn package(title: &'static str, lines: &[Line]) -> Package {
    let body: String = lines.iter().map(Line::xml).collect();
    let mut package = Package::new(title, body);
    package.numbering = true;
    package
}

/// Every scenario fixture, as (file name, package).
#[must_use]
pub fn all() -> Vec<(&'static str, Package)> {
    vec![
        (
            "resume-master.docx",
            package("Avery Quinn master resume", &master_v1_lines()),
        ),
        (
            "resume-master-new-job.docx",
            package("Avery Quinn master resume: new job", &master_v2_lines()),
        ),
        (
            "resume-master-promoted.docx",
            package("Avery Quinn master resume: promoted", &master_v3_lines()),
        ),
        (
            "resume-tailored-data.docx",
            package("Avery Quinn resume: data roles", &tailored_data_lines()),
        ),
        (
            "resume-tailored-same-para.docx",
            package(
                "Avery Quinn resume: same paragraph edited",
                &tailored_same_para_lines(),
            ),
        ),
        (
            "resume-tailored-deleted-job.docx",
            package(
                "Avery Quinn resume: job deleted",
                &tailored_deleted_job_lines(),
            ),
        ),
        (
            "resume-tailored-copied-job.docx",
            package(
                "Avery Quinn resume: job copied by hand",
                &tailored_copied_job_lines(),
            ),
        ),
        (
            "resume-reordered.docx",
            package(
                "Avery Quinn master resume: sections reordered",
                &reordered_lines(),
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_differ_only_where_described() {
        let v1 = master_v1_lines();
        let v2 = master_v2_lines();
        assert_eq!(v2.len(), v1.len() + 2);
        let v3 = master_v3_lines();
        assert_eq!(v3.len(), v2.len());
        assert!(v3.contains(&Line::H2(t(CORVIA_TITLE_V3))));
        assert_eq!(tailored_data_lines().len(), v1.len() - 1);
        assert_eq!(tailored_deleted_job_lines().len(), v1.len() - 3);
        assert_eq!(reordered_lines().len(), v1.len());
    }

    #[test]
    fn copied_job_splits_the_title_between_words() {
        let (first, second) = NEW_JOB_TITLE.split_at(21);
        assert_eq!(first, "Senior data engineer,");
        assert_eq!(second, " Halden Robotics, 2023 to now");
    }
}
