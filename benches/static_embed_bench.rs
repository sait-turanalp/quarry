//! Benchmark: OptimizedStaticModel (int8 runtime) — sequential vs rayon.
//!
//! Requires a local model at `~/.codanna/models/potion-retrieval-32M-int8/`.
//! Run with: `cargo bench --bench static_embed_bench`

use criterion::Criterion;

const MODEL_PATH: &str = concat!(
    env!("HOME"),
    "/.codanna/models/potion-retrieval-32M-int8"
);

/// Read current process RSS in bytes (macOS) via `ps`.
fn rss_bytes() -> usize {
    use std::process::Command;
    let pid = std::process::id();
    Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0)
        * 1024 // ps reports in KB
}

fn bench_single_short(c: &mut Criterion) {
    let rss_before = rss_bytes();
    let model = codanna::semantic::OptimizedStaticModel::from_local(MODEL_PATH)
        .expect("optimized model not found");
    let rss_after = rss_bytes();
    let delta_mb = (rss_after.saturating_sub(rss_before)) as f64 / 1_048_576.0;
    eprintln!("[optimized] RSS delta after load: {delta_mb:.1} MB");

    c.bench_function("optimized/single_short", |b| {
        b.iter(|| model.encode_single("parse_json_data"));
    });
}

fn bench_single_long(c: &mut Criterion) {
    let model = codanna::semantic::OptimizedStaticModel::from_local(MODEL_PATH)
        .expect("optimized model not found");
    let long_text = "fn process_data(input: &[u8]) -> Result<Vec<OutputRecord>, ProcessingError> { \
        let mut results = Vec::with_capacity(input.len() / 64); \
        for chunk in input.chunks(64) { \
            let parsed = Parser::new(chunk).parse_record()?; \
            if parsed.is_valid() { \
                let transformed = transform_record(&parsed, &self.config)?; \
                results.push(transformed); \
            } \
        } \
        Ok(results) \
    }"
    .repeat(5);
    c.bench_function("optimized/single_long", |b| {
        b.iter(|| model.encode_single(&long_text));
    });
}

fn bench_batch_100_sequential(c: &mut Criterion) {
    let model = codanna::semantic::OptimizedStaticModel::from_local(MODEL_PATH)
        .expect("optimized model not found");
    let texts: Vec<String> = (0..100)
        .map(|i| format!("fn symbol_{i}(arg: &str) -> Result<(), Error> {{ process(arg) }}"))
        .collect();
    c.bench_function("optimized/batch_100_sequential", |b| {
        b.iter(|| model.encode_batch(&texts, Some(512), 1024));
    });
}

fn bench_batch_100_rayon(c: &mut Criterion) {
    let model = codanna::semantic::OptimizedStaticModel::from_local(MODEL_PATH)
        .expect("optimized model not found");
    let texts: Vec<String> = (0..100)
        .map(|i| format!("fn symbol_{i}(arg: &str) -> Result<(), Error> {{ process(arg) }}"))
        .collect();
    c.bench_function("optimized/batch_100_rayon", |b| {
        b.iter(|| model.encode_batch_parallel(&texts, Some(512)));
    });
}

fn bench_summary(c: &mut Criterion) {
    // Theoretical embedding table size.
    const VOCAB: usize = 63091;
    const DIM: usize = 512;
    let i8_table_mb = (VOCAB * DIM) as f64 / 1_048_576.0;

    eprintln!();
    eprintln!("╔═══════════════════════════════════════════╗");
    eprintln!("║      OptimizedStaticModel (int8)          ║");
    eprintln!("╠═══════════════════════════════════════════╣");
    eprintln!("║ Embedding table: {i8_table_mb:.1} MB (int8)         ║");
    eprintln!("║ Vocab: {VOCAB}  Dim: {DIM}               ║");
    eprintln!("╚═══════════════════════════════════════════╝");
    eprintln!();

    // Dummy bench so criterion is satisfied.
    c.bench_function("summary/noop", |b| b.iter(|| 1 + 1));
}

fn main() {
    // Normal criterion entry point.
    let mut criterion = Criterion::default().configure_from_args();
    bench_single_short(&mut criterion);
    bench_single_long(&mut criterion);
    bench_batch_100_sequential(&mut criterion);
    bench_batch_100_rayon(&mut criterion);
    bench_summary(&mut criterion);

    criterion.final_summary();
}
