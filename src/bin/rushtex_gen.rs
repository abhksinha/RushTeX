//! `rushtex-gen`: the build-time table generator (pillar 2). Dev only (`--features gen`).
//!
//!     rushtex-gen fonts --list FILE --use USE [--out assets/tables/fonts.rtdb]   # write, self-check
//!     rushtex-gen fonts --list FILE --use USE --check                           # self-check only
//!
//! `FILE` lists TFM paths, one per line, or `scripts/texlive/files.tsv` rows (path first).
//! `USE` (`scripts/texlive/font-use.tsv`) gives each font's popularity: `name<TAB>documents`.
//! IDs follow popularity, then name; the same inputs always give the same bytes. Tier 1 is
//! the fonts at least `TIER1_DOCS` documents load, tier 2 the other used fonts, tier 3 the rest.

use rushtex::data::data_blob::{Blob, BlobWriter};
use rushtex::data::data_tfm::{self, FontTable, GenFont};
use std::process::ExitCode;

/// Documents that must load a font for it to be in tier 1.
const TIER1_DOCS: u32 = 2;

fn read_fonts(list: &str, uses: &str) -> Result<Vec<GenFont>, String> {
    let text = std::fs::read_to_string(list).map_err(|e| format!("{list}: {e}"))?;
    let use_text = std::fs::read_to_string(uses).map_err(|e| format!("{uses}: {e}"))?;
    let docs: std::collections::HashMap<&str, u32> = use_text.lines().filter_map(|l| l.split_once('\t')).filter_map(|(n, c)| Some((n, c.trim().parse().ok()?))).collect();
    let mut fonts = Vec::new();
    for path in text.lines().filter_map(|l| l.split('\t').next()).filter(|p| p.ends_with(".tfm")) {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let parts = data_tfm::parse(&bytes).ok_or_else(|| format!("{path}: not a well-formed TFM"))?;
        let name = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).ok_or_else(|| format!("{path}: no name"))?;
        let n = docs.get(name).copied().unwrap_or(0);
        let tier = if n >= TIER1_DOCS { 1 } else if n > 0 { 2 } else { 3 };
        fonts.push((n, GenFont { name: name.to_string(), parts, tier }));
    }
    fonts.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    let mut names: Vec<&str> = fonts.iter().map(|f| f.1.name.as_str()).collect();
    names.sort_unstable();
    if let Some(d) = names.windows(2).find(|w| w[0] == w[1]) {
        return Err(format!("{}.tfm is listed twice", d[0]));
    }
    Ok(fonts.into_iter().map(|f| f.1).collect())
}

fn check(out: &str, fonts: &[GenFont]) -> Result<(), String> {
    let bytes = std::fs::read(out).map_err(|e| format!("{out}: {e}"))?;
    let blob = Blob::new(&bytes).ok_or("not a table blob")?;
    let table = FontTable::from_blob(&blob).ok_or("not a font table")?;
    data_tfm::check(&table, fonts)
}

fn fonts(args: &[String]) -> Result<String, String> {
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let list = arg("--list").ok_or("--list FILE is required")?;
    let uses = arg("--use").ok_or("--use FILE is required")?;
    let out = arg("--out").unwrap_or_else(|| "assets/tables/fonts.rtdb".into());
    let fonts = read_fonts(&list, &uses)?;
    let mut saved = 0;
    if !args.iter().any(|a| a == "--check") {
        let mut w = BlobWriter::default();
        saved = data_tfm::write(&mut w, &fonts)?;
        std::fs::write(&out, w.finish()).map_err(|e| format!("{out}: {e}"))?;
    }
    check(&out, &fonts)?;
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    let tiers: Vec<usize> = (1..=3).map(|t| fonts.iter().filter(|f| f.tier == t).count()).collect();
    Ok(format!("{out}: {} fonts (tiers {tiers:?}), {size} bytes ({saved} saved by shared arrays), every word checked against its TFM", fonts.len()))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("fonts") => fonts(&args[1..]),
        _ => Err("usage: rushtex-gen fonts --list FILE --use USE [--out PATH] [--check]".into()),
    };
    match result {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rushtex-gen: {e}");
            ExitCode::FAILURE
        }
    }
}
