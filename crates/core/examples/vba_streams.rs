// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Prints the streams of a workbook's VBA project and the modules' source
//! offsets: what Excel wrote when it saved it (the Phase 5B spike).
//! Usage: cargo run -p lineage-core --example vba_streams -- <file.xlsm>

fn main() {
    let path = std::env::args().nth(1).expect("a workbook");
    let bytes = std::fs::read(&path).expect("reads");
    let package = lineage_core::Package::read(&bytes).expect("a package");
    let bin = package.part("xl/vbaProject.bin").expect("a VBA project");
    let cfb = lineage_core::vba::cfb::read(bin).expect("a compound file");
    for e in &cfb.entries {
        let kind = if e.storage { "storage" } else { "stream" };
        println!(
            "{kind:8} {:>7} {}",
            e.data.len(),
            e.path.replace(|c: char| c.is_control(), "?")
        );
    }
    let dir = cfb.stream("VBA/dir").expect("dir");
    let dir = lineage_core::vba::compress::decompress(dir, 1 << 24).expect("decompresses");
    for m in lineage_core::vba::dir::parse(&dir).expect("parses").modules {
        println!(
            "module {} offset {}",
            String::from_utf8_lossy(&m.name),
            m.offset
        );
    }
}
