// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What changed in a VBA project, as code: modules added, removed or
//! renamed, the lines changed inside each, which macros (procedures) those
//! lines belong to, references, the signature, and a form's designer.
//!
//! Summaries are plain: "Macro RefreshForecast: 3 lines changed",
//! "Added module Helpers", "Renamed module Module1 to Forecast".

use super::{Module, ModuleKind, VbaProject};
use crate::code::{self, LineDiff};

pub use crate::code::{CodeLine, LineKind};

/// Two modules of the same kind whose code is at least this alike (0 to
/// 100) are one module renamed.
pub const RENAME_SIMILARITY: u8 = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleChangeKind {
    Added,
    Removed,
    Renamed { from: String },
    Changed,
}

/// What a procedure is called in the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcKind {
    Sub,
    Function,
    Property,
}

impl ProcKind {
    fn noun(self) -> &'static str {
        match self {
            Self::Sub => "Macro",
            Self::Function => "Function",
            Self::Property => "Property",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Procedure {
    pub name: String,
    pub kind: ProcKind,
    /// Its lines, from 0, end inclusive.
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcChange {
    Added,
    Removed,
    Changed { lines: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcedureChange {
    pub name: String,
    pub kind: ProcKind,
    pub change: ProcChange,
}

impl ProcedureChange {
    #[must_use]
    pub fn summary(&self) -> String {
        let noun = self.kind.noun();
        match self.change {
            ProcChange::Added => format!("Added {} {}", noun.to_lowercase(), self.name),
            ProcChange::Removed => format!("Removed {} {}", noun.to_lowercase(), self.name),
            ProcChange::Changed { lines } => format!(
                "{noun} {}: {lines} {} changed",
                self.name,
                if lines == 1 { "line" } else { "lines" }
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleChange {
    /// A stable id for the panel and the ledger.
    pub id: String,
    /// The name in the later version (the earlier one for a removed module).
    pub name: String,
    pub kind: ModuleKind,
    pub change: ModuleChangeKind,
    pub procedures: Vec<ProcedureChange>,
    /// Lines changed outside any procedure (the declarations).
    pub declarations: usize,
    /// Lines changed in all, as a person counts them.
    pub lines: usize,
    pub attributes_changed: bool,
    pub designer_changed: bool,
    /// The code view: removed and added lines with a little context.
    pub code: Vec<CodeLine>,
    /// "Macro RefreshForecast: 3 lines changed".
    pub summary: String,
    /// One line per procedure and anything else that changed.
    pub details: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureChange {
    Added { signer: Option<String> },
    Removed { signer: Option<String> },
    Changed { signer: Option<String> },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VbaDiff {
    pub modules: Vec<ModuleChange>,
    /// References added and removed, by what the References dialog shows.
    pub references_added: Vec<String>,
    pub references_removed: Vec<String>,
    pub signature: Option<SignatureChange>,
    /// Locked or unlocked for viewing.
    pub locked: Option<bool>,
    /// The later version has macros and the earlier one had none, or the
    /// other way round.
    pub project_added: bool,
    pub project_removed: bool,
}

impl VbaDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
            && self.references_added.is_empty()
            && self.references_removed.is_empty()
            && self.signature.is_none()
            && self.locked.is_none()
            && !self.project_added
            && !self.project_removed
    }

    /// Whether any code is new or different: what makes a returned file
    /// "come back with new macro code".
    #[must_use]
    pub fn new_code(&self) -> bool {
        self.modules.iter().any(|m| {
            !matches!(m.change, ModuleChangeKind::Removed)
                && (m.lines > 0
                    || m.attributes_changed
                    || m.designer_changed
                    || matches!(m.change, ModuleChangeKind::Added))
        }) || !self.references_added.is_empty()
    }
}

/// The procedures of a module's code, by their `Sub`/`Function`/`Property`
/// … `End` lines.
#[must_use]
pub fn procedures(lines: &[String]) -> Vec<Procedure> {
    let mut out = Vec::new();
    let mut open: Option<(String, ProcKind, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = strip_comment(line).trim().to_ascii_lowercase();
        if let Some((name, kind, start)) = open.as_ref() {
            if t.starts_with("end sub")
                || t.starts_with("end function")
                || t.starts_with("end property")
            {
                out.push(Procedure {
                    name: name.clone(),
                    kind: *kind,
                    start: *start,
                    end: i,
                });
                open = None;
            }
            continue;
        }
        if let Some((kind, name)) = header(line) {
            open = Some((name, kind, i));
        }
    }
    if let Some((name, kind, start)) = open {
        out.push(Procedure {
            name,
            kind,
            start,
            end: lines.len().saturating_sub(1),
        });
    }
    out
}

fn strip_comment(line: &str) -> &str {
    // A ' outside a string starts a comment.
    let mut in_string = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '\'' if !in_string => return &line[..i],
            _ => {}
        }
    }
    line
}

/// "Public Sub RefreshForecast()" → (Sub, "RefreshForecast").
fn header(line: &str) -> Option<(ProcKind, String)> {
    let mut words = strip_comment(line).split_whitespace().peekable();
    let mut word = words.next()?;
    for modifier in ["public", "private", "friend"] {
        if word.eq_ignore_ascii_case(modifier) {
            word = words.next()?;
            break;
        }
    }
    if word.eq_ignore_ascii_case("static") {
        word = words.next()?;
    }
    let kind = if word.eq_ignore_ascii_case("sub") {
        ProcKind::Sub
    } else if word.eq_ignore_ascii_case("function") {
        ProcKind::Function
    } else if word.eq_ignore_ascii_case("property") {
        let accessor = words.next()?;
        if !["get", "let", "set"]
            .iter()
            .any(|a| accessor.eq_ignore_ascii_case(a))
        {
            return None;
        }
        ProcKind::Property
    } else {
        return None;
    };
    let rest: String = words.collect::<Vec<_>>().join(" ");
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some((kind, name))
}

fn proc_at(procs: &[Procedure], line: usize) -> Option<usize> {
    procs.iter().position(|p| p.start <= line && line <= p.end)
}

fn same_proc(a: &Procedure, b: &Procedure) -> bool {
    a.name.eq_ignore_ascii_case(&b.name) && a.kind == b.kind
}

/// Which procedures a line diff touched.
fn procedure_changes(a: &[String], b: &[String], d: &LineDiff) -> (Vec<ProcedureChange>, usize) {
    let pa = procedures(a);
    let pb = procedures(b);
    let mut out: Vec<ProcedureChange> = Vec::new();
    for p in &pb {
        if !pa.iter().any(|q| same_proc(p, q)) {
            out.push(ProcedureChange {
                name: p.name.clone(),
                kind: p.kind,
                change: ProcChange::Added,
            });
        }
    }
    for p in &pa {
        if !pb.iter().any(|q| same_proc(p, q)) {
            out.push(ProcedureChange {
                name: p.name.clone(),
                kind: p.kind,
                change: ProcChange::Removed,
            });
        }
    }
    // For procedures on both sides: the lines changed inside them.
    let mut declarations = LineDiff::default();
    let mut per: Vec<LineDiff> = vec![LineDiff::default(); pb.len()];
    for &i in &d.removed {
        match proc_at(&pa, i).and_then(|k| pb.iter().position(|q| same_proc(&pa[k], q))) {
            Some(k) => per[k].removed.push(i),
            None if proc_at(&pa, i).is_none() => declarations.removed.push(i),
            None => {}
        }
    }
    for &j in &d.added {
        match proc_at(&pb, j) {
            Some(k) if pa.iter().any(|q| same_proc(&pb[k], q)) => per[k].added.push(j),
            Some(_) => {}
            None => declarations.added.push(j),
        }
    }
    for (k, p) in pb.iter().enumerate() {
        let lines = per[k].removed.len().max(per[k].added.len());
        if lines > 0 {
            out.push(ProcedureChange {
                name: p.name.clone(),
                kind: p.kind,
                change: ProcChange::Changed { lines },
            });
        }
    }
    let decl = declarations.removed.len().max(declarations.added.len());
    (out, decl)
}

fn module_change(
    a: Option<&Module>,
    b: Option<&Module>,
    change: ModuleChangeKind,
) -> Option<ModuleChange> {
    let empty: Vec<String> = Vec::new();
    let la = a.map_or(&empty, |m| &m.lines);
    let lb = b.map_or(&empty, |m| &m.lines);
    let m = b.or(a)?;
    let d = code::line_diff(la, lb);
    let attributes_changed = match (a, b) {
        (Some(a), Some(b)) => attributes_key(a) != attributes_key(b),
        _ => false,
    };
    let designer_changed = match (a, b) {
        (Some(a), Some(b)) => a.designer != b.designer,
        _ => false,
    };
    // A change only the code page hides still counts.
    let bytes_only = matches!((a, b), (Some(a), Some(b)) if a.key != b.key)
        && d.is_empty()
        && !attributes_changed;
    if change == ModuleChangeKind::Changed
        && d.is_empty()
        && !attributes_changed
        && !designer_changed
        && !bytes_only
    {
        return None;
    }
    let (procedures, declarations) = match change {
        ModuleChangeKind::Added | ModuleChangeKind::Removed => (Vec::new(), 0),
        _ => procedure_changes(la, lb, &d),
    };
    let lines = d.changed();
    let noun = m.kind.noun();
    let mut details: Vec<String> = procedures.iter().map(ProcedureChange::summary).collect();
    if declarations > 0 {
        details.push(format!(
            "Declarations: {declarations} {} changed",
            if declarations == 1 { "line" } else { "lines" }
        ));
    }
    if designer_changed {
        details.push("The form's layout or controls changed".to_owned());
    }
    if attributes_changed {
        details.push("Its settings changed".to_owned());
    }
    if bytes_only {
        details.push("Characters Lineage can't show changed".to_owned());
    }
    let summary = match &change {
        ModuleChangeKind::Added => format!(
            "Added {} {} ({} {})",
            noun.to_lowercase(),
            m.name,
            lb.len(),
            if lb.len() == 1 { "line" } else { "lines" }
        ),
        ModuleChangeKind::Removed => format!("Removed {} {}", noun.to_lowercase(), m.name),
        ModuleChangeKind::Renamed { from } => {
            if lines == 0 {
                format!("Renamed {} {from} to {}", noun.to_lowercase(), m.name)
            } else {
                format!(
                    "Renamed {} {from} to {}, and {lines} {} changed",
                    noun.to_lowercase(),
                    m.name,
                    if lines == 1 { "line" } else { "lines" }
                )
            }
        }
        ModuleChangeKind::Changed => {
            if procedures.len() == 1 && declarations == 0 && !designer_changed {
                procedures[0].summary()
            } else if lines > 0 {
                format!(
                    "{noun} {}: {lines} {} changed",
                    m.name,
                    if lines == 1 { "line" } else { "lines" }
                )
            } else if designer_changed {
                format!("Form {} changed", m.name)
            } else {
                format!("{noun} {} changed", m.name)
            }
        }
    };
    let id = crate::hash::sha256_hex(
        format!(
            "macro\u{1f}{}\u{1f}{summary}\u{1f}{}",
            m.name,
            lb.join("\n").len()
        )
        .as_bytes(),
    )[..16]
        .to_owned();
    Some(ModuleChange {
        id,
        name: m.name.clone(),
        kind: m.kind,
        change,
        procedures,
        declarations,
        lines,
        attributes_changed,
        designer_changed,
        code: code::view(la, lb, &d),
        summary,
        details,
    })
}

/// Attributes a person can change in the editor (not VB_Name, which the
/// rename already says).
fn attributes_key(m: &Module) -> Vec<String> {
    m.attributes
        .iter()
        .filter(|a| !a.trim_start().starts_with("Attribute VB_Name"))
        .map(|a| a.trim().to_owned())
        .collect()
}

/// What changed from project `a` to project `b`. Either may be absent (a
/// workbook without macros).
#[must_use]
pub fn diff(a: Option<&VbaProject>, b: Option<&VbaProject>) -> VbaDiff {
    let mut out = VbaDiff {
        project_added: a.is_none() && b.is_some(),
        project_removed: a.is_some() && b.is_none(),
        ..VbaDiff::default()
    };
    let none: Vec<Module> = Vec::new();
    let ma = a.map_or(&none, |p| &p.modules);
    let mb = b.map_or(&none, |p| &p.modules);
    let mut matched_a = vec![false; ma.len()];
    let mut matched_b = vec![false; mb.len()];
    let mut pairs: Vec<(usize, usize, ModuleChangeKind)> = Vec::new();
    for (j, m) in mb.iter().enumerate() {
        if let Some(i) = ma.iter().position(|n| n.name.eq_ignore_ascii_case(&m.name)) {
            matched_a[i] = true;
            matched_b[j] = true;
            pairs.push((i, j, ModuleChangeKind::Changed));
        }
    }
    // Renames: an unmatched module on each side, same kind, alike code.
    for (j, m) in mb.iter().enumerate() {
        if matched_b[j] {
            continue;
        }
        let best = ma
            .iter()
            .enumerate()
            .filter(|(i, n)| !matched_a[*i] && n.kind == m.kind)
            .map(|(i, n)| (i, code::similar(&n.lines, &m.lines)))
            .filter(|(_, s)| *s >= RENAME_SIMILARITY)
            .max_by_key(|(_, s)| *s);
        if let Some((i, _)) = best {
            matched_a[i] = true;
            matched_b[j] = true;
            pairs.push((
                i,
                j,
                ModuleChangeKind::Renamed {
                    from: ma[i].name.clone(),
                },
            ));
        }
    }
    for (i, j, kind) in pairs {
        if let Some(c) = module_change(Some(&ma[i]), Some(&mb[j]), kind) {
            out.modules.push(c);
        }
    }
    for (j, m) in mb.iter().enumerate() {
        if !matched_b[j] {
            out.modules
                .extend(module_change(None, Some(m), ModuleChangeKind::Added));
        }
    }
    for (i, m) in ma.iter().enumerate() {
        if !matched_a[i] {
            out.modules
                .extend(module_change(Some(m), None, ModuleChangeKind::Removed));
        }
    }
    // Keep the order of the later version, removed ones last.
    let order = |c: &ModuleChange| {
        mb.iter()
            .position(|m| m.name == c.name && c.change != ModuleChangeKind::Removed)
            .unwrap_or(usize::MAX)
    };
    out.modules.sort_by_key(|c| order(c));

    let refs = |p: Option<&VbaProject>| -> Vec<(String, String)> {
        p.map(|p| {
            p.references
                .iter()
                .map(|r| (r.identity(), r.label()))
                .collect()
        })
        .unwrap_or_default()
    };
    let (ra, rb) = (refs(a), refs(b));
    if a.is_some() && b.is_some() {
        out.references_added = rb
            .iter()
            .filter(|r| !ra.iter().any(|q| q.0 == r.0))
            .map(|r| r.1.clone())
            .collect();
        out.references_removed = ra
            .iter()
            .filter(|r| !rb.iter().any(|q| q.0 == r.0))
            .map(|r| r.1.clone())
            .collect();
    }
    let sa = a.and_then(|p| p.signature.as_ref());
    let sb = b.and_then(|p| p.signature.as_ref());
    out.signature = match (sa, sb) {
        (None, Some(s)) if a.is_some() => Some(SignatureChange::Added {
            signer: s.signer.name.clone(),
        }),
        (Some(s), None) if b.is_some() => Some(SignatureChange::Removed {
            signer: s.signer.name.clone(),
        }),
        (Some(x), Some(y)) if x.key != y.key => Some(SignatureChange::Changed {
            signer: y.signer.name.clone(),
        }),
        _ => None,
    };
    if let (Some(a), Some(b)) = (a, b)
        && a.locked != b.locked
    {
        out.locked = Some(b.locked);
    }
    out
}
