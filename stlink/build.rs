//! Embeds `chips/stlink/*.chip` into the library as a static list.

use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("chips/stlink");
    println!("cargo::rerun-if-changed={}", dir.display());

    let mut files: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "chip"))
        .collect();
    files.sort();

    let mut out = String::from("pub(crate) static CHIP_FILES: &[(&str, &str)] = &[\n");
    for path in &files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("non-UTF-8 chip file name")?;
        out.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    out.push_str("];\n");

    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("chip_files.rs"),
        out,
    )?;
    Ok(())
}
