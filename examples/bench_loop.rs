//! In-process compile loop for profiling with macOS `sample` or `perf`:
//! `cargo run --release --example bench_loop -- FILE.tex [ITERATIONS]`.
//! Prints the median and minimum time of one `compile_str`.

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: bench_loop FILE.tex [ITERATIONS]");
    let n: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(200);
    let src = std::fs::read_to_string(&path).expect("read tex file");
    let _ = rushtex::compile_str(&src).expect("compile");
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        let t = std::time::Instant::now();
        let pdf = rushtex::compile_str(&src).expect("compile");
        std::hint::black_box(pdf);
        times.push(t.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    println!("{path}: median {:.3} ms, min {:.3} ms over {n} runs", times[n / 2], times[0]);
}
