//! Quick reranker latency benchmark.
//!
//! Runs 10 built-in queries against the chunk search pipeline and prints a
//! per-stage latency table.  No qrels or profile TOML files needed — just a
//! pre-built index.

use crate::IndexPersistence;
use crate::Settings;
use crate::chunks::ChunkSearchBackend;
use crate::indexing::facade::{IndexFacade, SearchTimingMs};
use std::sync::Arc;

const QUERIES: &[(&str, &str)] = &[
    ("q01", "parse json data"),
    ("q02", "database connection pool"),
    ("q03", "error handling middleware"),
    ("q04", "authentication token validation"),
    ("q05", "file read write operations"),
    ("q06", "http request handler"),
    ("q07", "string formatting template"),
    ("q08", "sort array algorithm"),
    ("q09", "cache invalidation strategy"),
    ("q10", "websocket message handler"),
];

#[derive(Debug, Clone)]
pub struct BenchmarkRerankQuickArgs {
    pub warm_runs: usize,
    pub limit: usize,
}

pub fn run(args: BenchmarkRerankQuickArgs, config: &Settings) {
    if let Err(err) = run_inner(args, config) {
        eprintln!("benchmark-rerank-quick failed: {err}");
        std::process::exit(1);
    }
}

fn run_inner(args: BenchmarkRerankQuickArgs, config: &Settings) -> Result<(), String> {
    if args.limit == 0 {
        return Err("--limit must be >= 1".to_string());
    }
    if args.warm_runs == 0 {
        return Err("--warm-runs must be >= 1".to_string());
    }

    let persistence = IndexPersistence::new(config.index_path.clone());
    if !persistence.exists() {
        return Err(format!(
            "index does not exist at '{}'",
            config.index_path.display()
        ));
    }

    eprintln!("Loading index...");
    let facade = persistence
        .load_facade_lite(Arc::new(config.clone()))
        .map_err(|e| format!("failed to load index: {e}"))?;

    // Count chunks
    let chunk_root = facade.index_base().join("code_chunks");
    let chunk_count = ChunkSearchBackend::open(&chunk_root)
        .map(|b| b.doc_count())
        .unwrap_or(0);

    eprintln!("Running cold query (includes reranker model init)...");
    let cold_ms = run_cold(&facade, args.limit)?;

    eprintln!(
        "Running {} warm runs × {} queries...",
        args.warm_runs,
        QUERIES.len()
    );
    let timings = run_warm(&facade, args.limit, args.warm_runs)?;

    print_table(&args, &timings, cold_ms, chunk_count, config);
    Ok(())
}

fn run_cold(facade: &IndexFacade, limit: usize) -> Result<u64, String> {
    let (_, query) = QUERIES[0];
    let outcome = facade
        .hybrid_chunk_search_detailed(query, limit, None)
        .map_err(|e| format!("cold query failed: {e}"))?;
    Ok(outcome.timing.total)
}

fn run_warm(
    facade: &IndexFacade,
    limit: usize,
    warm_runs: usize,
) -> Result<Vec<(String, SearchTimingMs)>, String> {
    // Collect all samples: for each query, run warm_runs times and pick median by total.
    let mut results = Vec::with_capacity(QUERIES.len());

    for (qid, query) in QUERIES {
        let mut samples = Vec::with_capacity(warm_runs);
        for _ in 0..warm_runs {
            let outcome = facade
                .hybrid_chunk_search_detailed(query, limit, None)
                .map_err(|e| format!("warm query '{qid}' failed: {e}"))?;
            samples.push(outcome.timing);
        }
        // Pick median by total latency
        samples.sort_by_key(|t| t.total);
        let median = samples.swap_remove(samples.len() / 2);
        results.push((qid.to_string(), median));
    }

    Ok(results)
}

fn print_table(
    args: &BenchmarkRerankQuickArgs,
    timings: &[(String, SearchTimingMs)],
    cold_ms: u64,
    chunk_count: usize,
    config: &Settings,
) {
    let model = &config.reranking.model;
    let symbol_count_str = if chunk_count > 0 {
        format!("{chunk_count}")
    } else {
        "n/a".to_string()
    };

    println!();
    println!(
        "Reranker Quick Benchmark ({} queries × {} warm runs)",
        QUERIES.len(),
        args.warm_runs
    );
    println!(
        "Model: {} | Chunks: {} | Limit: {}",
        model, symbol_count_str, args.limit
    );
    println!();

    // Header
    println!(
        "{:>3}  {:<30} {:>6} {:>6} {:>6} {:>6} {:>6}",
        "#", "Query", "BM25", "Vector", "RRF", "Rerank", "Total"
    );

    // Rows
    for (i, (_qid, timing)) in timings.iter().enumerate() {
        let (_, query_text) = QUERIES[i];
        let label = format_query_label(query_text, 30);
        println!(
            "{:>3}  {:<30} {:>5}ms {:>5}ms {:>5}ms {:>5}ms {:>5}ms",
            i + 1,
            label,
            timing.bm25,
            timing.vector,
            timing.rrf,
            timing.rerank,
            timing.total
        );
    }

    // Separator
    println!("{}", "─".repeat(78));

    // Aggregate stats
    let bm25s: Vec<u64> = timings.iter().map(|(_, t)| t.bm25).collect();
    let vectors: Vec<u64> = timings.iter().map(|(_, t)| t.vector).collect();
    let rrfs: Vec<u64> = timings.iter().map(|(_, t)| t.rrf).collect();
    let reranks: Vec<u64> = timings.iter().map(|(_, t)| t.rerank).collect();
    let totals: Vec<u64> = timings.iter().map(|(_, t)| t.total).collect();

    println!(
        "{:>3}  {:<30} {:>5}ms {:>5}ms {:>5}ms {:>5}ms {:>5}ms",
        "",
        "AVG (warm)",
        avg(&bm25s),
        avg(&vectors),
        avg(&rrfs),
        avg(&reranks),
        avg(&totals),
    );
    println!(
        "{:>3}  {:<30} {:>5}ms {:>5}ms {:>5}ms {:>5}ms {:>5}ms",
        "",
        "P50 (warm median)",
        percentile(&bm25s, 50),
        percentile(&vectors, 50),
        percentile(&rrfs, 50),
        percentile(&reranks, 50),
        percentile(&totals, 50),
    );
    println!(
        "{:>3}  {:<30} {:>5}ms {:>5}ms {:>5}ms {:>5}ms {:>5}ms",
        "",
        "P95 (warm)",
        percentile(&bm25s, 95),
        percentile(&vectors, 95),
        percentile(&rrfs, 95),
        percentile(&reranks, 95),
        percentile(&totals, 95),
    );

    println!();
    println!("Cold (first query, model init): {cold_ms}ms");
    println!();
}

fn format_query_label(query: &str, max_len: usize) -> String {
    if query.len() <= max_len {
        query.to_string()
    } else {
        format!("{}…", &query[..max_len - 1])
    }
}

fn avg(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.iter().sum::<u64>() / values.len() as u64
}

fn percentile(values: &[u64], pct: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let idx = ((pct as f64 / 100.0) * (sorted.len() as f64 - 1.0)).ceil() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
