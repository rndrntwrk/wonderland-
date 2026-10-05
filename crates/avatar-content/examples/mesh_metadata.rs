//! Diagnose original mesh count metadata without changing source bytes.
use std::{collections::BTreeMap, fs, path::Path};
use wonderland_legacy_formats::{far, reader::Reader, Limits};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args().nth(1).ok_or("directory required")?;
    let limits = Limits::default();
    let mut counts = BTreeMap::new();
    for family in ["bodies", "heads", "hands"] {
        let bytes = fs::read(Path::new(&root).join(family).join("meshes/meshes.dat"))?;
        let index = far::index_v3(&bytes, &limits)?;
        for (i, entry) in index.entries().iter().enumerate() {
            let bytes = index.extract(i, &limits)?;
            let mut r = Reader::new(&bytes);
            r.u32_be()?;
            let n = r.u32_be()?;
            for _ in 0..n {
                let len = r.u8()? as usize;
                r.skip(len)?;
            }
            let faces = r.u32_be()? as usize;
            r.skip(faces.checked_mul(12).ok_or("face overflow")?)?;
            let binds = r.u32_be()? as usize;
            r.skip(binds.checked_mul(20).ok_or("binding overflow")?)?;
            let real = r.u32_be()?;
            r.skip((real as usize).checked_mul(8).ok_or("real overflow")?)?;
            let blend = r.u32_be()?;
            r.skip((blend as usize).checked_mul(8).ok_or("blend overflow")?)?;
            let repeat = r.u32_be()?;
            let label = if repeat == real {
                "real"
            } else if repeat == real + blend {
                "real+blend"
            } else {
                "other"
            };
            *counts.entry((family, label)).or_insert(0usize) += 1;
            if label != "real" && counts[&(family, label)] == 1 {
                println!(
                    "{family} {}: real{real},blend{blend},repeat{repeat}",
                    String::from_utf8_lossy(entry.name.as_deref().unwrap_or_default())
                );
            }
        }
    }
    println!("{counts:?}");
    Ok(())
}
