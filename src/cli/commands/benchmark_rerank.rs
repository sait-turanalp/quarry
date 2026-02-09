//! Persistent reranker benchmark command.
//!
//! Runs chunk-search quality/latency evaluation in-process so reranker/model
//! initialization is not repeated per query.

use crate::indexing::facade::IndexFacade;
use crate::IndexPersistence;
use crate::Settings;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct BenchmarkRerankArgs {
    pub queries: PathBuf,
    pub qrels: PathBuf,
    pub profiles: PathBuf,
    pub out: PathBuf,
    pub cold_runs: usize,
    pub warm_runs: usize,
    pub limit: usize,
    pub query_timeout_ms: u64,
    pub checkpoint_every: usize,
    pub skip_warm_on_timeout: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct QuerySpec {
    id: String,
    query: String,
}

#[derive(Debug, Clone, Deserialize)]
struct QrelSpec {
    query_id: String,
    chunk_id: u32,
    grade: u8,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct ProfileOverrides {
    model: Option<String>,
    top_n: Option<usize>,
    timeout_ms: Option<u64>,
    max_length: Option<usize>,
    runtime_tuning_enabled: Option<bool>,
    batch_size: Option<usize>,
    warmup_pairs: Option<usize>,
    prefilter_enabled: Option<bool>,
    prefilter_target_top_n: Option<usize>,
    prefilter_fallback_on_small_gap: Option<bool>,
    prefilter_small_gap_threshold: Option<f32>,
    prefilter_dual_source_tail_keep: Option<usize>,
    top_k_vector: Option<usize>,
    top_k_bm25: Option<usize>,
    top_k_fused: Option<usize>,
    post_rerank_heuristics_enabled: Option<bool>,
    rerank_score_normalization: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProfileSpec {
    name: String,
    #[serde(flatten)]
    overrides: ProfileOverrides,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct ProfileSpecFile {
    defaults: ProfileOverrides,
    profiles: Vec<ProfileSpec>,
}

#[derive(Debug, Clone)]
struct ResolvedProfile {
    name: String,
    settings: Arc<Settings>,
}

#[derive(Debug, Clone, Serialize)]
struct QueryRunReport {
    query_id: String,
    query: String,
    cold_ms: u64,
    warm_ms: u64,
    timed_out: bool,
    hit1: f64,
    rr10: f64,
    ndcg10: f64,
    recall5: f64,
    recall10: f64,
    top_chunk_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileSummary {
    profile: String,
    queries: usize,
    hit_at_1: f64,
    mrr_at_10: f64,
    ndcg_at_10: f64,
    recall_at_5: f64,
    recall_at_10: f64,
    cold_p50_ms: u64,
    cold_p95_ms: u64,
    warm_p50_ms: u64,
    warm_p95_ms: u64,
    warm_avg_ms: u64,
    timeout_queries: usize,
    timeout_miss: usize,
    true_miss: usize,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummaryFile {
    query_count: usize,
    profiles: Vec<ProfileSummary>,
    pareto_frontier: Vec<String>,
    recommended_profile: Option<String>,
    baseline_profile: String,
}

pub fn run(args: BenchmarkRerankArgs, base_config: &Settings) {
    if let Err(err) = run_inner(args, base_config) {
        eprintln!("benchmark-rerank failed: {err}");
        std::process::exit(1);
    }
}

fn run_inner(args: BenchmarkRerankArgs, base_config: &Settings) -> Result<(), String> {
    if args.limit == 0 {
        return Err("--limit must be >= 1".to_string());
    }
    if args.cold_runs == 0 {
        return Err("--cold-runs must be >= 1".to_string());
    }
    if args.checkpoint_every == 0 {
        return Err("--checkpoint-every must be >= 1".to_string());
    }

    let queries: Vec<QuerySpec> = load_jsonl(&args.queries)?;
    if queries.is_empty() {
        return Err(format!("query set is empty: {}", args.queries.display()));
    }
    let mut seen_query_ids = HashSet::new();
    for q in &queries {
        if q.id.trim().is_empty() {
            return Err("query id cannot be empty".to_string());
        }
        if q.query.trim().is_empty() {
            return Err(format!("query text cannot be empty (query_id='{}')", q.id));
        }
        if !seen_query_ids.insert(q.id.clone()) {
            return Err(format!("duplicate query id in query set: '{}'", q.id));
        }
    }

    let qrels_rows: Vec<QrelSpec> = load_jsonl(&args.qrels)?;
    if qrels_rows.is_empty() {
        return Err(format!("qrels is empty: {}", args.qrels.display()));
    }

    let qrels = build_qrels_map(&queries, &qrels_rows)?;
    let profiles = load_profiles(&args.profiles, base_config)?;

    fs::create_dir_all(&args.out).map_err(|e| {
        format!(
            "failed to create output directory '{}': {e}",
            args.out.display()
        )
    })?;

    let mut per_query: BTreeMap<String, Vec<QueryRunReport>> = BTreeMap::new();
    let mut summaries = Vec::new();
    let baseline_name = "baseline_current_default".to_string();
    let profile_count = profiles.len();

    for (idx, profile) in profiles.iter().enumerate() {
        let profile_name = profile.name.clone();
        println!(
            "[benchmark-rerank] profile {}/{}: {}",
            idx + 1,
            profile_count,
            profile_name
        );

        let (rows, summary) = run_profile(
            profile,
            &queries,
            &qrels,
            args.limit,
            args.cold_runs,
            args.warm_runs,
            args.query_timeout_ms,
            args.skip_warm_on_timeout,
            |completed, total, row, rows_so_far| {
                println!(
                    "[benchmark-rerank] {} query {}/{} id={} cold={}ms warm={}ms timeout={} hit1={:.0} rr10={:.3}",
                    profile_name,
                    completed,
                    total,
                    row.query_id,
                    row.cold_ms,
                    row.warm_ms,
                    row.timed_out,
                    row.hit1,
                    row.rr10
                );

                if completed % args.checkpoint_every == 0 || completed == total {
                    let mut partial_per_query = per_query.clone();
                    partial_per_query.insert(profile_name.clone(), rows_so_far.to_vec());

                    let mut partial_summaries = summaries.clone();
                    partial_summaries.push(summarize_profile(&profile_name, rows_so_far));

                    write_partial_outputs(
                        &args.out,
                        queries.len(),
                        &baseline_name,
                        &partial_per_query,
                        &partial_summaries,
                    )?;

                    println!(
                        "[benchmark-rerank] checkpoint {}: {}/{} queries done for {}",
                        args.out.join("summary.partial.json").display(),
                        completed,
                        total,
                        profile_name
                    );
                }

                Ok(())
            },
        )?;

        per_query.insert(profile_name, rows);
        summaries.push(summary);

        write_partial_outputs(
            &args.out,
            queries.len(),
            &baseline_name,
            &per_query,
            &summaries,
        )?;
    }

    let pareto = pareto_frontier(&summaries);
    let recommended = choose_recommended(&summaries, &baseline_name, 500);

    write_summary_table(&args.out.join("summary_table.md"), &summaries)?;
    write_decision_md(
        &args.out.join("decision.md"),
        &summaries,
        &pareto,
        &recommended,
        &baseline_name,
    )?;

    let summary_file = BenchmarkSummaryFile {
        query_count: queries.len(),
        profiles: summaries.clone(),
        pareto_frontier: pareto,
        recommended_profile: recommended,
        baseline_profile: baseline_name,
    };

    fs::write(
        args.out.join("summary.json"),
        serde_json::to_string_pretty(&summary_file)
            .map_err(|e| format!("failed to serialize summary.json: {e}"))?,
    )
    .map_err(|e| format!("failed to write summary.json: {e}"))?;

    fs::write(
        args.out.join("per_query.json"),
        serde_json::to_string_pretty(&per_query)
            .map_err(|e| format!("failed to serialize per_query.json: {e}"))?,
    )
    .map_err(|e| format!("failed to write per_query.json: {e}"))?;

    print_summary(&summaries);
    Ok(())
}

fn load_profiles(path: &Path, base_config: &Settings) -> Result<Vec<ResolvedProfile>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("failed to read profile matrix '{}': {e}", path.display()))?;
    let spec: ProfileSpecFile =
        toml::from_str(&raw).map_err(|e| format!("invalid profile matrix TOML: {e}"))?;

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    out.push(ResolvedProfile {
        name: "baseline_current_default".to_string(),
        settings: Arc::new(base_config.clone()),
    });
    seen.insert("baseline_current_default".to_string());

    for entry in spec.profiles {
        if entry.name.trim().is_empty() {
            return Err("profile name cannot be empty".to_string());
        }
        if seen.contains(&entry.name) {
            return Err(format!("duplicate profile name: {}", entry.name));
        }

        let mut cfg = base_config.clone();
        apply_overrides(&mut cfg, &spec.defaults);
        apply_overrides(&mut cfg, &entry.overrides);

        cfg.reranking.enabled = true;
        cfg.chunk_search.enabled = true;

        seen.insert(entry.name.clone());
        out.push(ResolvedProfile {
            name: entry.name,
            settings: Arc::new(cfg),
        });
    }

    if out.len() == 1 {
        return Err("profile matrix contains no sweep profiles".to_string());
    }

    Ok(out)
}

fn apply_overrides(cfg: &mut Settings, ov: &ProfileOverrides) {
    if let Some(v) = &ov.model {
        cfg.reranking.model = v.clone();
    }
    if let Some(v) = ov.top_n {
        cfg.reranking.top_n = v;
    }
    if let Some(v) = ov.timeout_ms {
        cfg.reranking.timeout_ms = v;
    }
    if let Some(v) = ov.max_length {
        cfg.reranking.max_length = v;
    }
    if let Some(v) = ov.runtime_tuning_enabled {
        cfg.reranking.runtime_tuning_enabled = v;
    }
    if let Some(v) = ov.batch_size {
        cfg.reranking.batch_size = v;
    }
    if let Some(v) = ov.warmup_pairs {
        cfg.reranking.warmup_pairs = v;
    }
    if let Some(v) = ov.prefilter_enabled {
        cfg.reranking.prefilter_enabled = v;
    }
    if let Some(v) = ov.prefilter_target_top_n {
        cfg.reranking.prefilter_target_top_n = v;
    }
    if let Some(v) = ov.prefilter_fallback_on_small_gap {
        cfg.reranking.prefilter_fallback_on_small_gap = v;
    }
    if let Some(v) = ov.prefilter_small_gap_threshold {
        cfg.reranking.prefilter_small_gap_threshold = v;
    }
    if let Some(v) = ov.prefilter_dual_source_tail_keep {
        cfg.reranking.prefilter_dual_source_tail_keep = v;
    }
    if let Some(v) = ov.top_k_vector {
        cfg.chunk_search.top_k_vector = v;
    }
    if let Some(v) = ov.top_k_bm25 {
        cfg.chunk_search.top_k_bm25 = v;
    }
    if let Some(v) = ov.top_k_fused {
        cfg.chunk_search.top_k_fused = v;
    }
    if let Some(v) = ov.post_rerank_heuristics_enabled {
        cfg.chunk_search.post_rerank_heuristics_enabled = v;
    }
    if let Some(v) = &ov.rerank_score_normalization {
        cfg.chunk_search.rerank_score_normalization = v.clone();
    }
}

fn run_profile(
    profile: &ResolvedProfile,
    queries: &[QuerySpec],
    qrels: &HashMap<String, HashMap<u32, u8>>,
    limit: usize,
    cold_runs: usize,
    warm_runs: usize,
    query_timeout_ms: u64,
    skip_warm_on_timeout: bool,
    mut on_progress: impl FnMut(usize, usize, &QueryRunReport, &[QueryRunReport]) -> Result<(), String>,
) -> Result<(Vec<QueryRunReport>, ProfileSummary), String> {
    let persistence = IndexPersistence::new(profile.settings.index_path.clone());
    if !persistence.exists() {
        return Err(format!(
            "index does not exist for profile '{}' at '{}'",
            profile.name,
            profile.settings.index_path.display()
        ));
    }

    let facade = persistence
        .load_facade_lite(Arc::clone(&profile.settings))
        .map_err(|e| format!("failed to load index for profile '{}': {e}", profile.name))?;

    let mut reports = Vec::with_capacity(queries.len());

    for (query_idx, q) in queries.iter().enumerate() {
        let relevant = qrels
            .get(&q.id)
            .ok_or_else(|| format!("missing qrels for query_id='{}'", q.id))?;

        let mut cold_samples = Vec::new();
        for _ in 0..cold_runs {
            let sample = run_single_query(&facade, &q.query, limit, query_timeout_ms)?;
            let sample_timed_out = sample.timed_out;
            cold_samples.push(sample);
            if sample_timed_out {
                break;
            }
        }

        let mut warm_samples = Vec::new();
        let cold_timed_out = cold_samples.iter().any(|s| s.timed_out);
        if !(skip_warm_on_timeout && cold_timed_out) {
            for _ in 0..warm_runs {
                let sample = run_single_query(&facade, &q.query, limit, query_timeout_ms)?;
                let sample_timed_out = sample.timed_out;
                warm_samples.push(sample);
                if sample_timed_out {
                    break;
                }
            }
        }

        let selected = if !warm_samples.is_empty() {
            pick_median_sample(&warm_samples)
        } else {
            pick_median_sample(&cold_samples)
        };

        let top10_ids: Vec<u32> = selected
            .outcome
            .results
            .iter()
            .take(10)
            .map(|r| r.chunk_id)
            .collect();

        let metrics = evaluate_single_query(&top10_ids, relevant);
        let timed_out = cold_samples
            .iter()
            .chain(warm_samples.iter())
            .any(|s| s.timed_out);

        let report = QueryRunReport {
            query_id: q.id.clone(),
            query: q.query.clone(),
            cold_ms: median_u64(cold_samples.iter().map(|s| s.latency_ms).collect()),
            warm_ms: if warm_samples.is_empty() {
                median_u64(cold_samples.iter().map(|s| s.latency_ms).collect())
            } else {
                median_u64(warm_samples.iter().map(|s| s.latency_ms).collect())
            },
            timed_out,
            hit1: metrics.hit1,
            rr10: metrics.rr10,
            ndcg10: metrics.ndcg10,
            recall5: metrics.recall5,
            recall10: metrics.recall10,
            top_chunk_ids: top10_ids,
        };
        reports.push(report);

        let last = reports
            .last()
            .ok_or_else(|| "internal error: missing last report after push".to_string())?;
        on_progress(query_idx + 1, queries.len(), last, &reports)?;
    }

    let summary = summarize_profile(&profile.name, &reports);
    Ok((reports, summary))
}

#[derive(Debug, Clone)]
struct QueryExecution {
    latency_ms: u64,
    timed_out: bool,
    outcome: crate::indexing::facade::ChunkSearchOutcome,
}

fn run_single_query(
    facade: &IndexFacade,
    query: &str,
    limit: usize,
    query_timeout_ms: u64,
) -> Result<QueryExecution, String> {
    let started = Instant::now();
    let outcome = facade
        .hybrid_chunk_search_detailed(query, limit, None)
        .map_err(|e| format!("chunk search failed for query '{query}': {e}"))?;
    let elapsed = started.elapsed().as_millis() as u64;
    let rerank_timeout = outcome.pruned_by.iter().any(|s| s == "rerank_timeout");
    let timed_out = is_query_timed_out(rerank_timeout, elapsed, query_timeout_ms);

    Ok(QueryExecution {
        latency_ms: elapsed,
        timed_out,
        outcome,
    })
}

fn is_query_timed_out(rerank_timeout: bool, latency_ms: u64, query_timeout_ms: u64) -> bool {
    rerank_timeout || (query_timeout_ms > 0 && latency_ms > query_timeout_ms)
}

fn pick_median_sample(samples: &[QueryExecution]) -> &QueryExecution {
    let mut idxs: Vec<usize> = (0..samples.len()).collect();
    idxs.sort_by_key(|i| samples[*i].latency_ms);
    &samples[idxs[samples.len() / 2]]
}

#[derive(Debug, Clone, Copy)]
struct QueryMetrics {
    hit1: f64,
    rr10: f64,
    ndcg10: f64,
    recall5: f64,
    recall10: f64,
}

fn evaluate_single_query(top10_ids: &[u32], relevant: &HashMap<u32, u8>) -> QueryMetrics {
    let mut rr10 = 0.0;
    let mut hit1 = 0.0;
    let mut dcg = 0.0;

    let relevant_set: HashSet<u32> = relevant
        .iter()
        .filter_map(|(k, v)| if *v > 0 { Some(*k) } else { None })
        .collect();

    for (rank, chunk_id) in top10_ids.iter().enumerate() {
        let pos = rank + 1;
        let grade = *relevant.get(chunk_id).unwrap_or(&0) as f64;
        if grade > 0.0 {
            if pos == 1 {
                hit1 = 1.0;
            }
            if rr10 == 0.0 {
                rr10 = 1.0 / pos as f64;
            }
            let gain = (2.0_f64).powf(grade) - 1.0;
            dcg += gain / ((pos as f64 + 1.0).log2());
        }
    }

    let mut ideal_grades: Vec<f64> = relevant
        .values()
        .filter(|g| **g > 0)
        .map(|g| *g as f64)
        .collect();
    ideal_grades.sort_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));
    ideal_grades.truncate(10);

    let mut idcg = 0.0;
    for (rank, grade) in ideal_grades.iter().enumerate() {
        let pos = rank + 1;
        let gain = (2.0_f64).powf(*grade) - 1.0;
        idcg += gain / ((pos as f64 + 1.0).log2());
    }

    let ndcg10 = if idcg > 0.0 { dcg / idcg } else { 0.0 };
    let recall5 = recall_at(top10_ids, &relevant_set, 5);
    let recall10 = recall_at(top10_ids, &relevant_set, 10);

    QueryMetrics {
        hit1,
        rr10,
        ndcg10,
        recall5,
        recall10,
    }
}

fn recall_at(top_ids: &[u32], relevant: &HashSet<u32>, k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let mut seen = HashSet::new();
    for chunk_id in top_ids.iter().take(k) {
        if relevant.contains(chunk_id) {
            seen.insert(*chunk_id);
        }
    }
    seen.len() as f64 / relevant.len() as f64
}

fn summarize_profile(profile: &str, rows: &[QueryRunReport]) -> ProfileSummary {
    let n = rows.len().max(1);

    let hit = rows.iter().map(|r| r.hit1).sum::<f64>() / n as f64;
    let mrr = rows.iter().map(|r| r.rr10).sum::<f64>() / n as f64;
    let ndcg = rows.iter().map(|r| r.ndcg10).sum::<f64>() / n as f64;
    let r5 = rows.iter().map(|r| r.recall5).sum::<f64>() / n as f64;
    let r10 = rows.iter().map(|r| r.recall10).sum::<f64>() / n as f64;

    let cold_ms: Vec<u64> = rows.iter().map(|r| r.cold_ms).collect();
    let warm_ms: Vec<u64> = rows.iter().map(|r| r.warm_ms).collect();

    let mut timeout_miss = 0usize;
    let mut true_miss = 0usize;
    let mut timeout_queries = 0usize;

    for row in rows {
        if row.timed_out {
            timeout_queries += 1;
        }
        if row.rr10 == 0.0 {
            if row.timed_out {
                timeout_miss += 1;
            } else {
                true_miss += 1;
            }
        }
    }

    ProfileSummary {
        profile: profile.to_string(),
        queries: rows.len(),
        hit_at_1: round3(hit),
        mrr_at_10: round3(mrr),
        ndcg_at_10: round3(ndcg),
        recall_at_5: round3(r5),
        recall_at_10: round3(r10),
        cold_p50_ms: percentile(&cold_ms, 50.0),
        cold_p95_ms: percentile(&cold_ms, 95.0),
        warm_p50_ms: percentile(&warm_ms, 50.0),
        warm_p95_ms: percentile(&warm_ms, 95.0),
        warm_avg_ms: avg_u64(&warm_ms),
        timeout_queries,
        timeout_miss,
        true_miss,
    }
}

fn pareto_frontier(summaries: &[ProfileSummary]) -> Vec<String> {
    let mut frontier = Vec::new();

    for candidate in summaries {
        let dominated = summaries.iter().any(|other| {
            if other.profile == candidate.profile {
                return false;
            }
            let quality_not_worse = other.ndcg_at_10 >= candidate.ndcg_at_10;
            let latency_not_worse = other.warm_p95_ms <= candidate.warm_p95_ms;
            let one_strict = other.ndcg_at_10 > candidate.ndcg_at_10
                || other.warm_p95_ms < candidate.warm_p95_ms;
            quality_not_worse && latency_not_worse && one_strict
        });

        if !dominated {
            frontier.push(candidate.profile.clone());
        }
    }

    frontier.sort();
    frontier
}

fn choose_recommended(
    summaries: &[ProfileSummary],
    baseline_name: &str,
    target_p95_ms: u64,
) -> Option<String> {
    let baseline = summaries.iter().find(|s| s.profile == baseline_name)?;

    let mut eligible: Vec<&ProfileSummary> = summaries
        .iter()
        .filter(|s| s.profile != baseline_name)
        .filter(|s| s.ndcg_at_10 >= baseline.ndcg_at_10)
        .filter(|s| s.mrr_at_10 >= baseline.mrr_at_10)
        .filter(|s| s.hit_at_1 >= baseline.hit_at_1)
        .collect();

    if eligible.is_empty() {
        eligible = summaries
            .iter()
            .filter(|s| s.profile != baseline_name)
            .collect();
    }

    let mut within_slo: Vec<&ProfileSummary> = eligible
        .iter()
        .copied()
        .filter(|s| s.warm_p95_ms <= target_p95_ms)
        .collect();

    let pool = if within_slo.is_empty() {
        &mut eligible
    } else {
        &mut within_slo
    };

    pool.sort_by(|a, b| {
        b.ndcg_at_10
            .partial_cmp(&a.ndcg_at_10)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                b.mrr_at_10
                    .partial_cmp(&a.mrr_at_10)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| a.warm_p95_ms.cmp(&b.warm_p95_ms))
    });

    pool.first().map(|s| s.profile.clone())
}

fn write_summary_table(path: &Path, summaries: &[ProfileSummary]) -> Result<(), String> {
    let mut lines = Vec::new();
    lines.push("| Profile | Hit@1 | MRR@10 | nDCG@10 | Recall@5 | Recall@10 | Cold p50 | Cold p95 | Warm p50 | Warm p95 | TimeoutQ | TimeoutMiss | TrueMiss |".to_string());
    lines.push("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|".to_string());

    for s in summaries {
        lines.push(format!(
            "| {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} ms | {} ms | {} ms | {} ms | {} | {} | {} |",
            s.profile,
            s.hit_at_1,
            s.mrr_at_10,
            s.ndcg_at_10,
            s.recall_at_5,
            s.recall_at_10,
            s.cold_p50_ms,
            s.cold_p95_ms,
            s.warm_p50_ms,
            s.warm_p95_ms,
            s.timeout_queries,
            s.timeout_miss,
            s.true_miss,
        ));
    }

    fs::write(path, lines.join("\n") + "\n")
        .map_err(|e| format!("failed to write '{}': {e}", path.display()))
}

fn write_decision_md(
    path: &Path,
    summaries: &[ProfileSummary],
    pareto: &[String],
    recommended: &Option<String>,
    baseline_name: &str,
) -> Result<(), String> {
    let baseline = summaries
        .iter()
        .find(|s| s.profile == baseline_name)
        .ok_or_else(|| format!("baseline profile '{baseline_name}' not found"))?;

    let mut out = String::new();
    out.push_str("# Reranker Benchmark Decision\n\n");
    out.push_str(&format!("- Baseline: `{}`\n", baseline_name));
    out.push_str("- SLO target: warm p95 <= 500ms\n\n");

    out.push_str("## Baseline Metrics\n\n");
    out.push_str(&format!(
        "- Hit@1: {:.3}\n- MRR@10: {:.3}\n- nDCG@10: {:.3}\n- Recall@5: {:.3}\n- Warm p95: {} ms\n\n",
        baseline.hit_at_1,
        baseline.mrr_at_10,
        baseline.ndcg_at_10,
        baseline.recall_at_5,
        baseline.warm_p95_ms
    ));

    out.push_str("## Pareto Frontier\n\n");
    if pareto.is_empty() {
        out.push_str("(none)\n\n");
    } else {
        for name in pareto {
            out.push_str(&format!("- `{}`\n", name));
        }
        out.push('\n');
    }

    out.push_str("## Recommendation\n\n");
    if let Some(name) = recommended {
        out.push_str(&format!("- Recommended profile: `{}`\n", name));
        if let Some(sel) = summaries.iter().find(|s| &s.profile == name) {
            out.push_str(&format!(
                "- Quality: Hit@1 {:.3}, MRR@10 {:.3}, nDCG@10 {:.3}\n",
                sel.hit_at_1, sel.mrr_at_10, sel.ndcg_at_10
            ));
            out.push_str(&format!(
                "- Latency: warm p50 {} ms, warm p95 {} ms\n",
                sel.warm_p50_ms, sel.warm_p95_ms
            ));
        }
    } else {
        out.push_str("- No recommendation could be selected.\n");
    }

    fs::write(path, out).map_err(|e| format!("failed to write '{}': {e}", path.display()))
}

fn write_partial_outputs(
    out_dir: &Path,
    query_count: usize,
    baseline_name: &str,
    per_query: &BTreeMap<String, Vec<QueryRunReport>>,
    summaries: &[ProfileSummary],
) -> Result<(), String> {
    fs::write(
        out_dir.join("per_query.partial.json"),
        serde_json::to_string_pretty(per_query)
            .map_err(|e| format!("failed to serialize per_query.partial.json: {e}"))?,
    )
    .map_err(|e| format!("failed to write per_query.partial.json: {e}"))?;

    let partial = BenchmarkSummaryFile {
        query_count,
        profiles: summaries.to_vec(),
        pareto_frontier: pareto_frontier(summaries),
        recommended_profile: choose_recommended(summaries, baseline_name, 500),
        baseline_profile: baseline_name.to_string(),
    };

    fs::write(
        out_dir.join("summary.partial.json"),
        serde_json::to_string_pretty(&partial)
            .map_err(|e| format!("failed to serialize summary.partial.json: {e}"))?,
    )
    .map_err(|e| format!("failed to write summary.partial.json: {e}"))?;

    Ok(())
}

fn print_summary(summaries: &[ProfileSummary]) {
    println!(
        "{:<28} {:>6} {:>7} {:>7} {:>8} {:>8} {:>9} {:>9} {:>7}",
        "Profile",
        "Hit@1",
        "MRR@10",
        "nDCG@10",
        "Recall@5",
        "Recall@10",
        "Warm p50",
        "Warm p95",
        "TmOut"
    );
    for s in summaries {
        println!(
            "{:<28} {:>6.3} {:>7.3} {:>7.3} {:>8.3} {:>8.3} {:>7}ms {:>7}ms {:>7}",
            s.profile,
            s.hit_at_1,
            s.mrr_at_10,
            s.ndcg_at_10,
            s.recall_at_5,
            s.recall_at_10,
            s.warm_p50_ms,
            s.warm_p95_ms,
            s.timeout_queries,
        );
    }
}

fn build_qrels_map(
    queries: &[QuerySpec],
    rows: &[QrelSpec],
) -> Result<HashMap<String, HashMap<u32, u8>>, String> {
    let query_ids: HashSet<&str> = queries.iter().map(|q| q.id.as_str()).collect();
    let mut map: HashMap<String, HashMap<u32, u8>> = HashMap::new();

    for row in rows {
        if row.grade > 2 {
            return Err(format!(
                "invalid grade={} for query_id='{}' chunk_id={} (expected 0..2)",
                row.grade, row.query_id, row.chunk_id
            ));
        }
        if !query_ids.contains(row.query_id.as_str()) {
            return Err(format!(
                "qrels has unknown query_id='{}' not present in query set",
                row.query_id
            ));
        }

        let entry = map.entry(row.query_id.clone()).or_default();
        let prev = entry.get(&row.chunk_id).copied().unwrap_or(0);
        if row.grade > prev {
            entry.insert(row.chunk_id, row.grade);
        }
    }

    for q in queries {
        let Some(entries) = map.get(&q.id) else {
            return Err(format!("missing qrels entries for query_id='{}'", q.id));
        };
        if !entries.values().any(|grade| *grade > 0) {
            return Err(format!(
                "qrels must contain at least one positive label (grade>0) for query_id='{}'",
                q.id
            ));
        }
    }

    Ok(map)
}

fn load_jsonl<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("failed to read JSONL '{}': {e}", path.display()))?;
    let mut out = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<T>(line)
            .map_err(|e| format!("invalid JSONL at {}:{}: {e}", path.display(), idx + 1))?;
        out.push(parsed);
    }
    Ok(out)
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn avg_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    (values.iter().sum::<u64>() as f64 / values.len() as f64).round() as u64
}

fn median_u64(mut values: Vec<u64>) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

fn percentile(values: &[u64], p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    if sorted.len() == 1 {
        return sorted[0];
    }

    let pos = (sorted.len() - 1) as f64 * (p / 100.0);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;

    if lo == hi {
        sorted[lo]
    } else {
        let weight = pos - lo as f64;
        (sorted[lo] as f64 * (1.0 - weight) + sorted[hi] as f64 * weight).round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_single_query_graded_ndcg() {
        let mut relevant = HashMap::new();
        relevant.insert(11, 2);
        relevant.insert(22, 1);

        let m = evaluate_single_query(&[11, 99, 22], &relevant);
        assert_eq!(m.hit1, 1.0);
        assert!(m.rr10 > 0.0);
        assert!(m.ndcg10 > 0.9);
        assert_eq!(m.recall5, 1.0);
    }

    #[test]
    fn test_recall_at_with_unique_hits() {
        let relevant = HashSet::from([1, 2]);
        let recall = recall_at(&[1, 1, 1, 2], &relevant, 4);
        assert!((recall - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_pareto_frontier_prefers_quality_latency_tradeoff() {
        let a = ProfileSummary {
            profile: "a".into(),
            queries: 10,
            hit_at_1: 0.7,
            mrr_at_10: 0.7,
            ndcg_at_10: 0.7,
            recall_at_5: 0.7,
            recall_at_10: 0.7,
            cold_p50_ms: 0,
            cold_p95_ms: 0,
            warm_p50_ms: 200,
            warm_p95_ms: 300,
            warm_avg_ms: 0,
            timeout_queries: 0,
            timeout_miss: 0,
            true_miss: 0,
        };
        let b = ProfileSummary {
            profile: "b".into(),
            ndcg_at_10: 0.75,
            warm_p95_ms: 400,
            ..a.clone()
        };
        let c = ProfileSummary {
            profile: "c".into(),
            ndcg_at_10: 0.6,
            warm_p95_ms: 500,
            ..a.clone()
        };

        let frontier = pareto_frontier(&[a, b, c]);
        assert!(frontier.contains(&"a".to_string()));
        assert!(frontier.contains(&"b".to_string()));
        assert!(!frontier.contains(&"c".to_string()));
    }

    #[test]
    fn test_is_query_timed_out_combines_rerank_and_wall_threshold() {
        assert!(is_query_timed_out(true, 10, 0));
        assert!(is_query_timed_out(false, 1501, 1500));
        assert!(!is_query_timed_out(false, 1500, 1500));
        assert!(!is_query_timed_out(false, 999, 0));
    }

    #[test]
    fn test_apply_overrides_applies_prefilter_fields() {
        let mut cfg = Settings::default();
        let ov = ProfileOverrides {
            prefilter_enabled: Some(true),
            prefilter_target_top_n: Some(19),
            prefilter_fallback_on_small_gap: Some(false),
            prefilter_small_gap_threshold: Some(0.02),
            prefilter_dual_source_tail_keep: Some(2),
            ..ProfileOverrides::default()
        };

        apply_overrides(&mut cfg, &ov);

        assert!(cfg.reranking.prefilter_enabled);
        assert_eq!(cfg.reranking.prefilter_target_top_n, 19);
        assert!(!cfg.reranking.prefilter_fallback_on_small_gap);
        assert!((cfg.reranking.prefilter_small_gap_threshold - 0.02).abs() < f32::EPSILON);
        assert_eq!(cfg.reranking.prefilter_dual_source_tail_keep, 2);
    }
}
