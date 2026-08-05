#!/usr/bin/env python3
"""
Jina Reranker v2 INT4 Quantization Pipeline

BF16 model -> ONNX export -> INT4 block-wise quantization (MatMulNBits)
Validates quality with Spearman, Kendall, MAE, and inversion rate metrics.
"""

import json
import os
import sys
import time

import numpy as np
import torch
from scipy.stats import kendalltau, spearmanr


# ─── Configuration ───────────────────────────────────────────────────────────

MODEL_NAME = "jinaai/jina-reranker-v2-base-multilingual"
ONNX_FP32_DIR = "./jina-reranker-v2-onnx-fp32"
ONNX_INT4_DIR = "./jina-reranker-v2-int4-onnx"
CALIB_SIZE = 128
VAL_SIZE = 100
MAX_LENGTH = 1024
SEED = 42

# INT4 quantization parameters
BLOCK_SIZE = 128  # group_size equivalent
BITS = 4
SYMMETRIC = True

# Acceptance criteria
MIN_SPEARMAN = 0.98
MIN_KENDALL = 0.95
MAX_MAE = 0.5
MAX_INVERSION_RATE = 0.05


def log(msg: str):
    print(f"\n{'='*60}")
    print(f"  {msg}")
    print(f"{'='*60}\n")


# ─── Step 1: Download model and tokenizer ────────────────────────────────────

def download_model():
    log("Step 1: Downloading model and tokenizer")
    from transformers import AutoModelForSequenceClassification, AutoTokenizer

    tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME, trust_remote_code=True)
    model = AutoModelForSequenceClassification.from_pretrained(
        MODEL_NAME, trust_remote_code=True
    )
    print(f"Model loaded: {MODEL_NAME}")
    print(f"Parameters: {sum(p.numel() for p in model.parameters()):,}")
    return model, tokenizer


# ─── Step 2: Prepare calibration and validation data ─────────────────────────

def prepare_data():
    log("Step 2: Preparing calibration and validation data")

    # Use code snippets from the local codebase as calibration data
    # This is more representative for our use case (code reranking)
    import glob
    import random

    random.seed(SEED)

    # Collect code files from the quarry codebase
    code_files = []
    for ext in ["*.rs", "*.toml", "*.md"]:
        code_files.extend(glob.glob(f"src/**/{ext}", recursive=True))
    code_files.extend(glob.glob("*.toml"))
    code_files.extend(glob.glob("*.md"))

    # Read file contents and create query-document pairs
    snippets = []
    for fpath in code_files:
        try:
            with open(fpath, "r", encoding="utf-8", errors="ignore") as f:
                content = f.read()
            # Split into chunks of ~200 lines
            lines = content.split("\n")
            for i in range(0, len(lines), 200):
                chunk = "\n".join(lines[i : i + 200])
                if len(chunk.strip()) > 50:
                    snippets.append((fpath, chunk.strip()))
        except Exception:
            continue

    random.shuffle(snippets)

    # Generate query-document pairs using function signatures and code
    pairs = []
    queries = [
        "How does the indexing pipeline work?",
        "Find the symbol search implementation",
        "What is the embedding generation process?",
        "Show me the MCP server setup",
        "How are tree-sitter parsers registered?",
        "Find the vector search engine code",
        "What is the relationship tracking system?",
        "Show the configuration loading logic",
        "How does the BM25 scoring work?",
        "Find the ONNX runtime integration",
        "What are the CLI commands available?",
        "Show me the document chunking logic",
        "How does the file watcher work?",
        "Find the HTTP server implementation",
        "What is the symbol resolution process?",
        "Show the Tantivy index schema",
        "How are embeddings stored?",
        "Find the clustering algorithm",
        "What is the compact symbol format?",
        "Show the project resolver logic",
        "How does hybrid search combine results?",
        "Find the reranking implementation",
        "What is the string table optimization?",
        "Show me the error handling patterns",
        "How does the pipeline stage system work?",
        "Find the walker file discovery code",
        "What language parsers are supported?",
        "Show the token counting logic",
        "How does semantic search work?",
        "Find the call graph analysis code",
        "What is the impact analysis feature?",
        "Show the visibility detection logic",
        "How does import resolution work?",
        "Find the scope tracking system",
        "What is the audit coverage tracking?",
        "Show me the transaction handling",
        "How are symbols deduplicated?",
        "Find the persistence layer code",
        "What is the metadata tracking?",
        "Show the guidance engine output",
    ]

    # Create pairs: each query matched with multiple code snippets
    for i, (fpath, snippet) in enumerate(snippets):
        query = queries[i % len(queries)]
        pairs.append((query, snippet[:2000]))  # Truncate long snippets
        if len(pairs) >= CALIB_SIZE + VAL_SIZE:
            break

    # If we don't have enough pairs, pad with synthetic ones
    while len(pairs) < CALIB_SIZE + VAL_SIZE:
        idx = len(pairs) % len(queries)
        pairs.append((queries[idx], f"fn example_{len(pairs)}() {{ /* code */ }}"))

    calib_pairs = pairs[:CALIB_SIZE]
    val_pairs = pairs[CALIB_SIZE : CALIB_SIZE + VAL_SIZE]

    print(f"Collected {len(snippets)} code snippets from {len(code_files)} files")
    print(f"Calibration pairs: {len(calib_pairs)}")
    print(f"Validation pairs: {len(val_pairs)}")
    return calib_pairs, val_pairs


# ─── Step 3: Get baseline scores from original model ─────────────────────────

def get_baseline_scores(model, tokenizer, val_pairs):
    log("Step 3: Computing baseline scores from BF16 model")
    model.eval()
    scores = []

    start = time.perf_counter()
    with torch.no_grad():
        for i, (query, doc) in enumerate(val_pairs):
            inputs = tokenizer(
                query, doc, truncation=True, max_length=MAX_LENGTH, return_tensors="pt"
            )
            outputs = model(**inputs)
            score = outputs.logits.squeeze().item()
            scores.append(score)
            if (i + 1) % 20 == 0:
                print(f"  [{i+1}/{len(val_pairs)}] score={score:.4f}")

    elapsed = time.perf_counter() - start
    print(f"\nBaseline inference: {elapsed:.2f}s ({elapsed/len(val_pairs)*1000:.1f}ms/pair)")

    with open("baseline_scores.json", "w") as f:
        json.dump(scores, f)

    print(f"Saved baseline_scores.json ({len(scores)} scores)")
    return scores, elapsed


# ─── Step 4: Export to ONNX ──────────────────────────────────────────────────

def export_to_onnx(model, tokenizer):
    log("Step 4: Exporting model to ONNX (FP32)")

    os.makedirs(ONNX_FP32_DIR, exist_ok=True)
    onnx_path = os.path.join(ONNX_FP32_DIR, "model.onnx")

    # Jina reranker uses custom architecture — manual torch.onnx.export
    model.eval()
    model.float()  # Ensure FP32 for ONNX export

    # Create dummy inputs matching the model's expected input
    dummy_input = tokenizer(
        "query text", "document text",
        truncation=True, max_length=128, return_tensors="pt",
    )

    input_names = list(dummy_input.keys())
    print(f"Input names: {input_names}")

    # Build dynamic axes for variable sequence length and batch size
    dynamic_axes = {}
    for name in input_names:
        dynamic_axes[name] = {0: "batch_size", 1: "sequence_length"}
    dynamic_axes["logits"] = {0: "batch_size"}

    torch.onnx.export(
        model,
        tuple(dummy_input[k] for k in input_names),
        onnx_path,
        input_names=input_names,
        output_names=["logits"],
        dynamic_axes=dynamic_axes,
        opset_version=17,
        do_constant_folding=True,
    )

    # Save tokenizer alongside ONNX model
    tokenizer.save_pretrained(ONNX_FP32_DIR)

    # Also save config.json for ORTModel loading
    model.config.save_pretrained(ONNX_FP32_DIR)

    size_mb = os.path.getsize(onnx_path) / (1024 * 1024)
    print(f"ONNX model exported: {onnx_path} ({size_mb:.1f} MB)")

    return onnx_path


# ─── Step 5: INT4 block-wise quantization ────────────────────────────────────

def quantize_int4(onnx_path, block_size=BLOCK_SIZE, bits=BITS, symmetric=SYMMETRIC):
    log(f"Step 5: INT4 quantization (block_size={block_size}, bits={bits}, symmetric={symmetric})")
    from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer
    import onnx

    # Load with external data (weights stored in .data file)
    input_model = onnx.load(onnx_path, load_external_data=True)
    print(f"Loaded ONNX model: {len(input_model.graph.node)} nodes")

    quantizer = MatMulNBitsQuantizer(
        model=input_model,
        block_size=block_size,
        is_symmetric=symmetric,
        bits=bits,
    )

    print("Running quantization...")
    quantizer.process()

    # Save quantized model
    os.makedirs(ONNX_INT4_DIR, exist_ok=True)
    output_path = os.path.join(ONNX_INT4_DIR, "model.onnx")
    quantizer.model.save_model_to_file(output_path)

    # Copy tokenizer and config files
    import shutil
    for fname in os.listdir(ONNX_FP32_DIR):
        if fname.startswith("model.onnx"):
            continue  # Skip FP32 model files
        src = os.path.join(ONNX_FP32_DIR, fname)
        dst = os.path.join(ONNX_INT4_DIR, fname)
        if os.path.isfile(src):
            shutil.copy2(src, dst)

    # Report size (model might have external data)
    total_size = 0
    for f in os.listdir(ONNX_INT4_DIR):
        fp = os.path.join(ONNX_INT4_DIR, f)
        if os.path.isfile(fp) and f.startswith("model"):
            total_size += os.path.getsize(fp)
    size_mb = total_size / (1024 * 1024)
    print(f"INT4 ONNX model saved: {output_path} ({size_mb:.1f} MB)")
    return output_path


# ─── Step 6: Validate INT4 model ─────────────────────────────────────────────

def run_onnx_inference(onnx_dir, tokenizer, pairs, label="ONNX"):
    """Run inference using onnxruntime InferenceSession directly."""
    import onnxruntime as ort

    onnx_path = os.path.join(onnx_dir, "model.onnx")
    session = ort.InferenceSession(onnx_path, providers=["CPUExecutionProvider"])

    # Get expected input names from ONNX model
    onnx_input_names = [inp.name for inp in session.get_inputs()]
    print(f"ONNX inputs: {onnx_input_names}")

    # Quick sanity check
    test_enc = tokenizer("test query", "test document", return_tensors="np")
    test_feed = {k: test_enc[k] for k in onnx_input_names if k in test_enc}
    test_out = session.run(None, test_feed)
    print(f"Sanity check score: {test_out[0].squeeze():.4f}")

    scores = []
    start = time.perf_counter()

    for i, (query, doc) in enumerate(pairs):
        enc = tokenizer(
            query, doc, truncation=True, max_length=MAX_LENGTH, return_tensors="np"
        )
        feed = {k: enc[k] for k in onnx_input_names if k in enc}
        out = session.run(None, feed)
        score = float(out[0].squeeze())
        scores.append(score)
        if (i + 1) % 20 == 0:
            print(f"  [{i+1}/{len(pairs)}] score={score:.4f}")

    elapsed = time.perf_counter() - start
    print(f"\n{label} inference: {elapsed:.2f}s ({elapsed/len(pairs)*1000:.1f}ms/pair)")
    return scores, elapsed


def validate_int4(tokenizer, val_pairs):
    log("Step 6: Validating INT4 model")

    scores, elapsed = run_onnx_inference(ONNX_INT4_DIR, tokenizer, val_pairs, "INT4")

    with open("int4_scores.json", "w") as f:
        json.dump(scores, f)

    print(f"Saved int4_scores.json ({len(scores)} scores)")
    return scores, elapsed


# ─── Step 7: Quality comparison ──────────────────────────────────────────────

def compare_quality(baseline_scores, int4_scores):
    log("Step 7: Quality comparison")

    base = np.array(baseline_scores)
    int4 = np.array(int4_scores)

    # Spearman correlation
    spearman_corr, spearman_p = spearmanr(base, int4)
    print(f"Spearman correlation: {spearman_corr:.4f} (p={spearman_p:.2e})")

    # Kendall tau
    kendall_corr, kendall_p = kendalltau(base, int4)
    print(f"Kendall tau:          {kendall_corr:.4f} (p={kendall_p:.2e})")

    # MAE
    mae = np.mean(np.abs(base - int4))
    print(f"Mean absolute error:  {mae:.4f}")

    # Max absolute error
    max_err = np.max(np.abs(base - int4))
    print(f"Max absolute error:   {max_err:.4f}")

    # Inversion rate
    inversions = 0
    total_pairs = 0
    for i in range(len(base)):
        for j in range(i + 1, len(base)):
            total_pairs += 1
            base_order = base[i] > base[j]
            int4_order = int4[i] > int4[j]
            if base_order != int4_order:
                inversions += 1

    inversion_rate = inversions / total_pairs if total_pairs > 0 else 0
    print(f"Inversion rate:       {inversion_rate:.4f} ({inversions}/{total_pairs})")

    # Check acceptance criteria
    print(f"\n{'─'*40}")
    print("Acceptance Criteria:")
    results = {}

    checks = [
        ("Spearman > 0.98", spearman_corr >= MIN_SPEARMAN, spearman_corr),
        ("Kendall  > 0.95", kendall_corr >= MIN_KENDALL, kendall_corr),
        ("MAE      < 0.50", mae <= MAX_MAE, mae),
        ("Inversions < 5%", inversion_rate <= MAX_INVERSION_RATE, inversion_rate),
    ]

    all_pass = True
    for name, passed, value in checks:
        status = "PASS" if passed else "FAIL"
        if not passed:
            all_pass = False
        print(f"  [{status}] {name} (actual: {value:.4f})")

    results = {
        "spearman": spearman_corr,
        "kendall": kendall_corr,
        "mae": mae,
        "max_error": max_err,
        "inversion_rate": inversion_rate,
        "all_pass": all_pass,
    }

    return results


# ─── Step 8: Speed and size benchmarks ───────────────────────────────────────

def benchmark_report(baseline_time, int4_time, val_count):
    log("Step 8: Performance report")

    # Speed comparison
    print(f"Original BF16:  {baseline_time:.2f}s ({baseline_time/val_count*1000:.1f}ms/pair)")
    print(f"INT4 ONNX:      {int4_time:.2f}s ({int4_time/val_count*1000:.1f}ms/pair)")
    speedup = baseline_time / int4_time if int4_time > 0 else float("inf")
    print(f"Speedup:        {speedup:.1f}x")

    # Size comparison
    def dir_size(path):
        total = 0
        for f in os.listdir(path):
            fp = os.path.join(path, f)
            if os.path.isfile(fp):
                total += os.path.getsize(fp)
        return total / (1024 * 1024)

    fp32_size = dir_size(ONNX_FP32_DIR)
    int4_size = dir_size(ONNX_INT4_DIR)
    compression = fp32_size / int4_size if int4_size > 0 else float("inf")

    print(f"\nFP32 ONNX size: {fp32_size:.1f} MB")
    print(f"INT4 ONNX size: {int4_size:.1f} MB")
    print(f"Compression:    {compression:.1f}x")


# ─── Fallback: retry with different parameters ──────────────────────────────

FALLBACK_CONFIGS = [
    {"block_size": 64, "bits": 4, "symmetric": True, "desc": "block_size=64"},
    {"block_size": 128, "bits": 4, "symmetric": False, "desc": "asymmetric"},
    {"block_size": 64, "bits": 4, "symmetric": False, "desc": "block_size=64 + asymmetric"},
    {"block_size": 128, "bits": 8, "symmetric": True, "desc": "INT8 fallback"},
]


# ─── Main pipeline ──────────────────────────────────────────────────────────

def main():
    log("Jina Reranker v2 INT4 Quantization Pipeline")

    # Step 1: Download model
    model, tokenizer = download_model()

    # Step 2: Prepare data
    calib_pairs, val_pairs = prepare_data()

    # Step 3: Baseline scores (skip if cached)
    onnx_fp32_path = os.path.join(ONNX_FP32_DIR, "model.onnx")
    if os.path.exists("baseline_scores.json") and os.path.exists(onnx_fp32_path):
        log("Step 3: Loading cached baseline scores")
        with open("baseline_scores.json") as f:
            baseline_scores = json.load(f)
        baseline_time = 263.0  # approximate from previous run
        print(f"Loaded {len(baseline_scores)} cached baseline scores")
    else:
        baseline_scores, baseline_time = get_baseline_scores(model, tokenizer, val_pairs)

    # Step 4: Export to ONNX (skip if cached)
    if os.path.exists(onnx_fp32_path):
        log("Step 4: Using cached FP32 ONNX model")
        onnx_path = onnx_fp32_path
        size_mb = os.path.getsize(onnx_path) / (1024 * 1024)
        print(f"Cached: {onnx_path} ({size_mb:.1f} MB)")
    else:
        onnx_path = export_to_onnx(model, tokenizer)

    # Free GPU/CPU memory from original model
    del model
    if torch.cuda.is_available():
        torch.cuda.empty_cache()

    # Step 5: INT4 quantization
    quantize_int4(onnx_path)

    # Step 6: Validate
    int4_scores, int4_time = validate_int4(tokenizer, val_pairs)

    # Step 7: Quality check
    results = compare_quality(baseline_scores, int4_scores)

    # Step 8: Benchmarks
    benchmark_report(baseline_time, int4_time, len(val_pairs))

    # Handle failures with fallback configs
    if not results["all_pass"]:
        log("Quality check FAILED - trying fallback configurations")
        for i, config in enumerate(FALLBACK_CONFIGS):
            print(f"\n--- Fallback {i+1}: {config['desc']} ---")

            # Re-quantize with different params
            import shutil
            shutil.rmtree(ONNX_INT4_DIR, ignore_errors=True)

            quantize_int4(
                onnx_path,
                block_size=config["block_size"],
                bits=config["bits"],
                symmetric=config["symmetric"],
            )

            int4_scores, int4_time = validate_int4(tokenizer, val_pairs)
            results = compare_quality(baseline_scores, int4_scores)
            benchmark_report(baseline_time, int4_time, len(val_pairs))

            if results["all_pass"]:
                print(f"\nFallback {i+1} ({config['desc']}) PASSED!")
                break
        else:
            print("\nAll fallback configurations failed.")
            print("Manual review of the scores is recommended.")
    else:
        log("All quality checks PASSED!")

    # Final summary
    log("DONE - Final output")
    print(f"INT4 ONNX model: {ONNX_INT4_DIR}/")
    print(f"Baseline scores: baseline_scores.json")
    print(f"INT4 scores:     int4_scores.json")

    # List output files
    if os.path.isdir(ONNX_INT4_DIR):
        print(f"\nFiles in {ONNX_INT4_DIR}/:")
        for f in sorted(os.listdir(ONNX_INT4_DIR)):
            fp = os.path.join(ONNX_INT4_DIR, f)
            if os.path.isfile(fp):
                size = os.path.getsize(fp) / (1024 * 1024)
                print(f"  {f:40s} {size:8.1f} MB")


if __name__ == "__main__":
    main()
