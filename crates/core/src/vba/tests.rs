// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::vba::{self as kit, CfbStream, Module as KitModule, ModuleKind as K};
use proptest::prelude::*;

use super::diff::{ModuleChangeKind, SignatureChange, diff};
use super::*;

fn forecast() -> KitModule {
    KitModule::new(
        "Forecast",
        K::Standard,
        "Option Explicit\n\nPublic Sub RefreshForecast()\n    Dim months As Integer\n    months = 12\n    Range(\"Forecast!B2\").Value = months\nEnd Sub\n\nFunction Growth(rate As Double) As Double\n    Growth = 1 + rate\nEnd Function",
    )
}

fn project() -> kit::Project {
    let mut form = KitModule::new(
        "OrderForm",
        K::Form,
        "Private Sub OkButton_Click()\n    Me.Hide\nEnd Sub",
    );
    form.layout = "two buttons".to_owned();
    kit::Project::new(vec![
        KitModule::new(
            "ThisWorkbook",
            K::Workbook,
            "Private Sub Workbook_Open()\n    ' Nothing runs in Lineage.\nEnd Sub",
        ),
        KitModule::new("Sheet1", K::Sheet, ""),
        forecast(),
        KitModule::new(
            "Scenario",
            K::Class,
            "Public Name As String\nPublic Growth As Double",
        ),
        form,
    ])
}

fn read_kit(p: &kit::Project) -> VbaProject {
    read(&p.to_bytes(), &[]).unwrap()
}

#[test]
fn reads_every_kind_of_module() {
    let p = read_kit(&project());
    assert_eq!(p.name, "VBAProject");
    assert_eq!(p.codepage, 1252);
    let kinds: Vec<(&str, ModuleKind)> = p
        .modules
        .iter()
        .map(|m| (m.name.as_str(), m.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("ThisWorkbook", ModuleKind::Workbook),
            ("Sheet1", ModuleKind::Sheet),
            ("Forecast", ModuleKind::Standard),
            ("Scenario", ModuleKind::Class),
            ("OrderForm", ModuleKind::Form),
        ]
    );
    let f = p.module("forecast").unwrap();
    assert_eq!(f.attributes, ["Attribute VB_Name = \"Forecast\""]);
    assert_eq!(f.lines[2], "Public Sub RefreshForecast()");
    assert_eq!(f.lines.len(), 11);
    assert!(p.module("OrderForm").unwrap().designer.is_some());
    assert!(p.module("Forecast").unwrap().designer.is_none());
    assert!(!p.locked);
    assert!(!p.lossy);
    assert_eq!(p.signature, None);
    let refs: Vec<String> = p.references.iter().map(Reference::label).collect();
    assert_eq!(
        refs,
        ["OLE Automation", "Microsoft Office 16.0 Object Library"]
    );
}

#[test]
fn a_locked_project_is_still_read() {
    let mut k = project();
    k.locked = true;
    let p = read_kit(&k);
    assert!(p.locked);
    assert_eq!(p.modules.len(), 5);
}

#[test]
fn reads_who_signed() {
    let sig = kit::signature_part("Fabrikam Test Signing", "Fabrikam Test CA");
    let p = read(&project().to_bytes(), &[&sig]).unwrap();
    let s = p.signature.unwrap();
    assert_eq!(s.signer.name.as_deref(), Some("Fabrikam Test Signing"));
    assert_eq!(s.signer.issuer.as_deref(), Some("Fabrikam Test CA"));
}

#[test]
fn characters_outside_ascii_follow_the_code_page() {
    let k = kit::Project::new(vec![KitModule::new(
        "Prices",
        K::Standard,
        "Const Label = \"Price in \u{20AC} \u{2013} caf\u{E9}\"",
    )]);
    let p = read_kit(&k);
    assert_eq!(
        p.modules[0].lines[0],
        "Const Label = \"Price in \u{20AC} \u{2013} caf\u{E9}\""
    );
    assert!(!p.lossy);
    // Another code page: ASCII exact, the rest as "?".
    let (s, lossy) = decode(b"caf\xE9", 1251);
    assert_eq!(s, "caf?");
    assert!(lossy);
}

#[test]
fn a_long_module_reads() {
    let code: String = (0..20_000)
        .map(|n| format!("    total = total + {n} ' line {n}\n"))
        .collect();
    let k = kit::Project::new(vec![KitModule::new(
        "Long",
        K::Standard,
        &format!("Sub Sum()\n{code}End Sub"),
    )]);
    let p = read_kit(&k);
    assert_eq!(p.modules[0].lines.len(), 20_002);
}

#[test]
fn malformed_compound_files_are_refused_not_panicked() {
    let good = project().to_bytes();
    assert!(read(&good[..600], &[]).is_err());
    assert!(read(b"not a compound file at all, but long enough", &[]).is_err());
    // A FAT entry that points at itself: a loop.
    let mut looping = good.clone();
    let fat_at = 512;
    let dir_start = u32::from_le_bytes([good[48], good[49], good[50], good[51]]) as usize;
    looping[fat_at + dir_start * 4..fat_at + dir_start * 4 + 4]
        .copy_from_slice(&u32::try_from(dir_start).unwrap().to_le_bytes());
    assert!(matches!(read(&looping, &[]), Err(Error::Damaged(_))));
    // A wrong sector size.
    let mut bad = good.clone();
    bad[30] = 10;
    assert!(read(&bad, &[]).is_err());
    // Every truncation is an error or a project, never a panic.
    for cut in (0..good.len()).step_by(97) {
        let _ = read(&good[..cut], &[]);
    }
}

#[test]
fn raw_chunks_decompress() {
    // Bytes that don't compress, two full chunks: written as raw chunks.
    let mut x: u32 = 12_345;
    let noise: Vec<u8> = (0..8192)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (x >> 16) as u8
        })
        .collect();
    let packed = kit::compress(&noise);
    assert_eq!(packed[2] & 0x80, 0, "a raw chunk");
    assert_eq!(compress::decompress(&packed, 1 << 20).unwrap(), noise);
}

/// Found by the `vba_project` fuzz target in CI: the testkit compressor
/// wrote an empty buffer as a chunk whose size underflowed. MS-OVBA
/// 2.4.1.3.6 compresses an empty buffer to the signature byte alone.
#[test]
fn an_empty_buffer_is_the_signature_alone() {
    assert_eq!(kit::compress(&[]), [0x01]);
    assert!(
        compress::decompress(&[0x01], MAX_MODULE_BYTES)
            .unwrap()
            .is_empty()
    );
    // The fuzz target's shortest inputs, whose halves are empty or one byte.
    for data in [&b""[..], b"x", b"xy", b"\x01"] {
        as_the_fuzz_target(data);
    }
}

/// What `fuzz/fuzz_targets/vba_project.rs` does with bytes that aren't a
/// compound file: half become the dir stream, half the module. An error or
/// a project, never a panic.
fn as_the_fuzz_target(data: &[u8]) {
    let (dir, module) = data.split_at(data.len() / 2);
    let bin = kit::write_cfb(&[
        CfbStream {
            path: "PROJECT".to_owned(),
            data: b"Module=Module1\r\n".to_vec(),
        },
        CfbStream {
            path: "VBA/dir".to_owned(),
            data: kit::compress(dir),
        },
        CfbStream {
            path: "VBA/Module1".to_owned(),
            data: kit::compress(module),
        },
    ]);
    if let Ok(project) = read(&bin, &[data]) {
        assert!(diff(Some(&project), Some(&project)).is_empty());
    }
}

#[test]
fn a_missing_dir_stream_is_damaged() {
    let bin = kit::write_cfb(&[CfbStream {
        path: "PROJECT".to_owned(),
        data: b"Name=\"X\"".to_vec(),
    }]);
    assert!(matches!(read(&bin, &[]), Err(Error::Damaged(_))));
}

#[test]
fn compound_files_hold_big_and_small_streams() {
    let big: Vec<u8> = (0..20_000u32).map(|n| (n % 251) as u8).collect();
    let bin = kit::write_cfb(&[
        CfbStream {
            path: "A/B/small".to_owned(),
            data: b"tiny".to_vec(),
        },
        CfbStream {
            path: "A/big".to_owned(),
            data: big.clone(),
        },
        CfbStream {
            path: "top".to_owned(),
            data: Vec::new(),
        },
    ]);
    let c = cfb::read(&bin).unwrap();
    assert_eq!(c.stream("a/b/SMALL"), Some(&b"tiny"[..]));
    assert_eq!(c.stream("A/big"), Some(big.as_slice()));
    assert_eq!(c.stream("top"), Some(&[][..]));
    assert!(c.has_storage("A/B"));
    assert_eq!(c.under("A").count(), 2);
}

// --- What changed ----------------------------------------------------------

#[test]
fn an_edited_macro_names_the_macro() {
    let a = read_kit(&project());
    let mut k = project();
    k.module_mut("Forecast").unwrap().code = forecast()
        .code
        .replace("months = 12", "months = 18")
        .replace("Value = months", "Value = months * 2");
    let b = read_kit(&k);
    let d = diff(Some(&a), Some(&b));
    assert_eq!(d.modules.len(), 1);
    let m = &d.modules[0];
    assert_eq!(m.change, ModuleChangeKind::Changed);
    assert_eq!(m.summary, "Macro RefreshForecast: 2 lines changed");
    assert_eq!(m.lines, 2);
    assert!(
        m.code
            .iter()
            .any(|l| l.kind == LineKind::Removed && l.text.contains("12"))
    );
    assert!(
        m.code
            .iter()
            .any(|l| l.kind == LineKind::Added && l.text.contains("18"))
    );
    assert!(d.new_code());
}

#[test]
fn added_removed_and_renamed_modules() {
    let a = read_kit(&project());
    let mut k = project();
    k.modules.retain(|m| m.name != "Scenario");
    k.module_mut("Forecast").unwrap().name = "Planning".to_owned();
    k.modules.push(KitModule::new(
        "Helpers",
        K::Standard,
        "Sub ExportPdf()\nEnd Sub",
    ));
    let b = read_kit(&k);
    let d = diff(Some(&a), Some(&b));
    let summaries: Vec<&str> = d.modules.iter().map(|m| m.summary.as_str()).collect();
    assert_eq!(
        summaries,
        [
            "Renamed module Forecast to Planning",
            "Added module Helpers (2 lines)",
            "Removed class Scenario"
        ]
    );
}

#[test]
fn procedures_added_and_removed_in_a_module() {
    let a = read_kit(&project());
    let mut k = project();
    let m = k.module_mut("Forecast").unwrap();
    m.code = m.code.replace(
        "Function Growth(rate As Double) As Double\n    Growth = 1 + rate\nEnd Function",
        "Private Sub ExportPdf()\n    ' later\nEnd Sub",
    );
    let b = read_kit(&k);
    let d = diff(Some(&a), Some(&b));
    assert_eq!(
        d.modules[0].details,
        ["Added macro ExportPdf", "Removed function Growth"]
    );
    assert_eq!(d.modules[0].summary, "Module Forecast: 3 lines changed");
}

#[test]
fn a_form_layout_change_shows_without_code() {
    let a = read_kit(&project());
    let mut k = project();
    k.module_mut("OrderForm").unwrap().layout = "three buttons".to_owned();
    let d = diff(Some(&a), Some(&read_kit(&k)));
    assert_eq!(d.modules.len(), 1);
    assert_eq!(d.modules[0].summary, "Form OrderForm changed");
    assert_eq!(
        d.modules[0].details,
        ["The form's layout or controls changed"]
    );
}

#[test]
fn references_signature_and_lock() {
    let a = read_kit(&project());
    let mut k = project();
    k.references.push(kit::Reference::scripting());
    k.locked = true;
    let sig = kit::signature_part("Fabrikam Test Signing", "Fabrikam Test CA");
    let b = read(&k.to_bytes(), &[&sig]).unwrap();
    let d = diff(Some(&a), Some(&b));
    assert!(d.modules.is_empty());
    assert_eq!(d.references_added, ["Microsoft Scripting Runtime"]);
    assert_eq!(
        d.signature,
        Some(SignatureChange::Added {
            signer: Some("Fabrikam Test Signing".to_owned())
        })
    );
    assert_eq!(d.locked, Some(true));
    assert!(d.new_code(), "a new reference is new macro code");
}

#[test]
fn the_same_project_has_no_changes() {
    let a = read_kit(&project());
    let d = diff(Some(&a), Some(&read_kit(&project())));
    assert!(d.is_empty());
    assert!(!d.new_code());
}

#[test]
fn reordering_modules_is_no_change() {
    let a = read_kit(&project());
    let mut k = project();
    k.modules.reverse();
    assert!(diff(Some(&a), Some(&read_kit(&k))).is_empty());
}

#[test]
fn macros_appearing_and_going() {
    let a = read_kit(&project());
    let d = diff(None, Some(&a));
    assert!(d.project_added);
    assert_eq!(d.modules.len(), 5);
    assert!(d.new_code());
    let d = diff(Some(&a), None);
    assert!(d.project_removed);
    assert!(!d.new_code());
}

#[test]
fn procedure_headers() {
    let lines: Vec<String> = [
        "Option Explicit",
        "Private Static Function Rate() As Double ' cached",
        "End Function",
        "Public Property Get Label() As String",
        "End Property",
        "Sub Main()",
        "    MsgBox \"it's End Sub inside a string\"",
        "End Sub",
        "Dim SubTotal As Long",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let names: Vec<(String, usize, usize)> = diff::procedures(&lines)
        .into_iter()
        .map(|p| (p.name, p.start, p.end))
        .collect();
    assert_eq!(
        names,
        [
            ("Rate".to_owned(), 1, 2),
            ("Label".to_owned(), 3, 4),
            ("Main".to_owned(), 5, 7)
        ]
    );
}

proptest! {
    // Code-like text: a short last chunk must compress (a raw chunk is
    // always 4096 bytes), which text like this always does.
    #[test]
    fn decompress_undoes_compress(text in "[a-f =()\r\n]{0,9000}") {
        let packed = kit::compress(text.as_bytes());
        prop_assert_eq!(
            compress::decompress(&packed, MAX_MODULE_BYTES).unwrap(),
            text.as_bytes()
        );
    }

    // The fuzz target's path, natively: libFuzzer doesn't build on Windows.
    #[test]
    fn any_bytes_as_the_fuzz_target_never_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..400)
    ) {
        as_the_fuzz_target(&bytes);
    }

    #[test]
    fn decompress_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
        let mut b = bytes;
        if let Some(first) = b.first_mut() {
            *first = 1;
        }
        let _ = compress::decompress(&b, 1 << 16);
    }

    #[test]
    fn the_code_view_rebuilds_both_sides(
        a in proptest::collection::vec("[a-c]{0,2}", 0..40),
        b in proptest::collection::vec("[a-c]{0,2}", 0..40),
    ) {
        let d = crate::code::line_diff(&a, &b);
        // Removed and Same lines are `a`; Added and Same lines are `b`.
        let mut ra = Vec::new();
        let mut rb = Vec::new();
        let (mut i, mut j) = (0, 0);
        for &(pi, pj) in d.pairs.iter().chain(std::iter::once(&(a.len(), b.len()))) {
            while i < pi { ra.push(a[i].clone()); i += 1; }
            while j < pj { rb.push(b[j].clone()); j += 1; }
            if pi < a.len() { ra.push(a[pi].clone()); rb.push(b[pj].clone()); i += 1; j += 1; }
        }
        prop_assert_eq!(ra, a);
        prop_assert_eq!(rb, b);
    }
}
