//! Instructions `Kpse::new` retires (macOS), the file search's set-up cost.
//!
//!   cargo run --release --example kpse_cost
fn instructions() -> u64 {
    extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
    }
    let mut info = [0u64; 64];
    // SAFETY: the buffer is larger than `rusage_info_v4`, which the call fills.
    if unsafe { proc_pid_rusage(std::process::id() as i32, 4, info.as_mut_ptr()) } == 0 {
        info[31]
    } else {
        0
    }
}

fn main() {
    let cache = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".cache/rushtex/kpse-paths");
    for _ in 0..3 {
        let a = instructions();
        let k = rushtex::engine::engine_host::Kpse::new("pdflatex", Some(&cache), None);
        let b = instructions();
        eprintln!("Kpse::new {:.1}M instructions ({})", (b - a) as f64 / 1e6, if k.is_some() { "ok" } else { "none" });
        // Then one font program found through it, as a document outside the binary asks.
        if let Some(k) = k {
            let c = instructions();
            let got = k.read("sfrm2074.pfb").map_or(0, |v| v.len());
            eprintln!("  read sfrm2074.pfb {:.1}M instructions ({got} bytes)", (instructions() - c) as f64 / 1e6);
            for name in ["sfrm2074.pfb#clear", "sfrm2074.pfb#layout", "ecrm2074.vf"] {
                let c = instructions();
                let got = k.read(name).is_some();
                eprintln!("  read {name} {:.1}M instructions ({got})", (instructions() - c) as f64 / 1e6);
            }
        }
    }
}
