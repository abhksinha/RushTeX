use pprof::ProfilerGuardBuilder;
use std::fs::File;

fn main() {
    let path = "examples/phase1_stress_100k.tex";
    let src = std::fs::read_to_string(path).expect("read tex file");

    println!("[*] Warming up pipeline on {}...", path);
    let _ = rushtex::compile_str(&src).unwrap();

    println!("[*] Starting profiler (1000 Hz, 80 iterations)...");
    let guard = ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "ld-"])
        .build()
        .unwrap();

    for i in 1..=80 {
        let _ = rushtex::compile_str(&src).unwrap();
        if i % 10 == 0 {
            print!(".");
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    }
    println!(" done.");

    let report = guard.report().build().expect("build profiler report");

    // Save SVG flamegraph
    let out_path = "flamegraph.svg";
    let file = File::create(out_path).expect("create flamegraph.svg");
    report.flamegraph(file).expect("write flamegraph.svg");
    println!("[+] Flamegraph saved to {}", out_path);

    // Collect top leaf symbols from report.data
    let mut counts: std::collections::HashMap<String, isize> = std::collections::HashMap::new();
    let mut total_samples = 0isize;

    for (frames, count) in report.data.iter() {
        total_samples += *count;
        if let Some(leaf) = frames.frames.first() {
            for symbol in leaf {
                let name = symbol.name();
                *counts.entry(name.to_string()).or_default() += *count;
            }
        }
    }

    let mut sorted: Vec<(String, isize)> = counts.into_iter().collect();
    sorted.sort_by_key(|&(_, c)| std::cmp::Reverse(c));

    println!("\n=== TOP CPU BOTTLENECKS (Total Samples: {}) ===", total_samples);
    for (name, count) in sorted.iter().take(20) {
        let pct = (*count as f64 / total_samples as f64) * 100.0;
        println!("{:>6.2}% ({:>4} samples) : {}", pct, count, name);
    }
}
