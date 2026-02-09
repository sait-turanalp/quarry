#!/usr/bin/env python3
"""
Jina Reranker v2 GPTQ INT4 Quantization (calibration-aware)

Uses onnxruntime's built-in GPTQ algorithm with calibration data
to achieve INT4 quantization with minimal quality loss.
"""

import json
import glob
import os
import random
import shutil
import time

import numpy as np
import onnx
import onnxruntime as ort
import torch
from scipy.stats import kendalltau, spearmanr
from transformers import AutoTokenizer


# ─── Configuration ───────────────────────────────────────────────────────────

MODEL_NAME = "jinaai/jina-reranker-v2-base-multilingual"
ONNX_FP32_DIR = "./jina-reranker-v2-onnx-fp32"
ONNX_INT4_DIR = "./jina-reranker-v2-int4-onnx"
CALIB_SIZE = 128
VAL_SIZE = 100
MAX_LENGTH = 1024
SEED = 42

# Acceptance criteria
MIN_SPEARMAN = 0.98
MIN_KENDALL = 0.95
MAX_MAE = 0.5
MAX_INVERSION_RATE = 0.05


def log(msg: str):
    print(f"\n{'='*60}")
    print(f"  {msg}")
    print(f"{'='*60}\n")


# ─── Data preparation ────────────────────────────────────────────────────────

def prepare_pairs():
    """Generate query-code pairs from local codebase."""
    random.seed(SEED)

    code_files = []
    for ext in ["*.rs", "*.toml", "*.md"]:
        code_files.extend(glob.glob(f"src/**/{ext}", recursive=True))
    code_files.extend(glob.glob("*.toml"))
    code_files.extend(glob.glob("*.md"))

    snippets = []
    for fpath in code_files:
        try:
            with open(fpath, "r", encoding="utf-8", errors="ignore") as f:
                content = f.read()
            lines = content.split("\n")
            for i in range(0, len(lines), 200):
                chunk = "\n".join(lines[i : i + 200])
                if len(chunk.strip()) > 50:
                    snippets.append((fpath, chunk.strip()))
        except Exception:
            continue

    random.shuffle(snippets)

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

    pairs = []
    for i, (fpath, snippet) in enumerate(snippets):
        query = queries[i % len(queries)]
        pairs.append((query, snippet[:2000]))
        if len(pairs) >= CALIB_SIZE + VAL_SIZE:
            break

    while len(pairs) < CALIB_SIZE + VAL_SIZE:
        idx = len(pairs) % len(queries)
        pairs.append((queries[idx], f"fn example_{len(pairs)}() {{ /* code */ }}"))

    calib_pairs = pairs[:CALIB_SIZE]
    val_pairs = pairs[CALIB_SIZE : CALIB_SIZE + VAL_SIZE]
    print(f"Calibration pairs: {len(calib_pairs)}, Validation pairs: {len(val_pairs)}")
    return calib_pairs, val_pairs


# ─── CalibrationDataReader for GPTQ ──────────────────────────────────────────

class RerankerCalibrationReader:
    """Feeds tokenized query-document pairs to the GPTQ quantizer."""

    def __init__(self, tokenizer, pairs, input_names, max_length=1024):
        self.data = []
        for query, doc in pairs:
            enc = tokenizer(query, doc, truncation=True, max_length=max_length, return_tensors="np")
            self.data.append({k: enc[k] for k in input_names if k in enc})
        self.index = 0

    def get_next(self):
        if self.index >= len(self.data):
            return None
        result = self.data[self.index]
        self.index += 1
        return result

    def set_range(self, start_index, end_index):
        self.index = start_index
        self.end = end_index

    def __len__(self):
        return len(self.data)

    def __iter__(self):
        self.index = 0
        return self

    def __next__(self):
        result = self.get_next()
        if result is None:
            raise StopIteration
        return result


# ─── ONNX inference helper ───────────────────────────────────────────────────

def onnx_score(session, tokenizer, query, doc, input_names, max_length=1024):
    enc = tokenizer(query, doc, truncation=True, max_length=max_length, return_tensors="np")
    feed = {k: enc[k] for k in input_names if k in enc}
    out = session.run(None, feed)
    return float(out[0].squeeze())


def onnx_score_batch(session, tokenizer, pairs, input_names, label="ONNX"):
    scores = []
    start = time.perf_counter()
    for i, (q, d) in enumerate(pairs):
        s = onnx_score(session, tokenizer, q, d, input_names)
        scores.append(s)
        if (i + 1) % 20 == 0:
            print(f"  [{i+1}/{len(pairs)}] score={s:.4f}")
    elapsed = time.perf_counter() - start
    print(f"{label}: {elapsed:.2f}s ({elapsed/len(pairs)*1000:.1f}ms/pair)")
    return scores, elapsed


# ─── Quality comparison ──────────────────────────────────────────────────────

def compare(baseline, quantized):
    base = np.array(baseline)
    qnt = np.array(quantized)

    sp, _ = spearmanr(base, qnt)
    kt, _ = kendalltau(base, qnt)
    mae = np.mean(np.abs(base - qnt))
    max_err = np.max(np.abs(base - qnt))

    inversions = 0
    total = 0
    for i in range(len(base)):
        for j in range(i + 1, len(base)):
            total += 1
            if (base[i] > base[j]) != (qnt[i] > qnt[j]):
                inversions += 1
    inv_rate = inversions / total if total > 0 else 0

    print(f"Spearman:    {sp:.4f}")
    print(f"Kendall tau: {kt:.4f}")
    print(f"MAE:         {mae:.4f}")
    print(f"Max error:   {max_err:.4f}")
    print(f"Inversions:  {inv_rate:.4f} ({inversions}/{total})")

    checks = [
        ("Spearman > 0.98", sp >= MIN_SPEARMAN, sp),
        ("Kendall  > 0.95", kt >= MIN_KENDALL, kt),
        ("MAE      < 0.50", mae <= MAX_MAE, mae),
        ("Inversions < 5%", inv_rate <= MAX_INVERSION_RATE, inv_rate),
    ]

    all_pass = True
    print()
    for name, passed, val in checks:
        tag = "PASS" if passed else "FAIL"
        if not passed:
            all_pass = False
        print(f"  [{tag}] {name} (actual: {val:.4f})")

    return all_pass


# ─── Quantization configs to try ─────────────────────────────────────────────

CONFIGS = [
    # (description, block_size, is_symmetric, actorder)
    ("GPTQ INT4 bs=128 sym", 128, True, False),
    ("GPTQ INT4 bs=128 sym actorder", 128, True, True),
    ("GPTQ INT4 bs=64 sym", 64, True, False),
    ("GPTQ INT4 bs=64 sym actorder", 64, True, True),
]


# ─── Main ────────────────────────────────────────────────────────────────────

def main():
    log("Jina Reranker v2 — GPTQ INT4 Quantization (calibration-aware)")

    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME, trust_remote_code=True)

    # Prepare data
    calib_pairs, val_pairs = prepare_pairs()

    # Check FP32 ONNX exists
    fp32_onnx = os.path.join(ONNX_FP32_DIR, "model.onnx")
    if not os.path.exists(fp32_onnx):
        print(f"ERROR: FP32 ONNX model not found at {fp32_onnx}")
        print("Run quantize_jina_reranker.py first to export FP32 ONNX.")
        return

    # Get input names from FP32 model
    fp32_session = ort.InferenceSession(fp32_onnx, providers=["CPUExecutionProvider"])
    input_names = [inp.name for inp in fp32_session.get_inputs()]
    print(f"ONNX input names: {input_names}")

    # Load baseline scores
    log("Loading baseline scores")
    if os.path.exists("baseline_scores.json"):
        with open("baseline_scores.json") as f:
            baseline_scores = json.load(f)
        print(f"Loaded {len(baseline_scores)} cached baseline scores")
    else:
        print("Computing FP32 ONNX baseline...")
        baseline_scores, _ = onnx_score_batch(
            fp32_session, tokenizer, val_pairs, input_names, "FP32"
        )
        with open("baseline_scores.json", "w") as f:
            json.dump(baseline_scores, f)
    del fp32_session

    # Try each GPTQ config
    for desc, block_size, is_symmetric, actorder in CONFIGS:
        log(f"Quantizing: {desc}")

        from onnxruntime.quantization.matmul_nbits_quantizer import (
            GPTQWeightOnlyQuantConfig,
            MatMulNBitsQuantizer,
        )

        # Load fresh FP32 model
        model = onnx.load(fp32_onnx, load_external_data=True)
        print(f"Loaded FP32 model: {len(model.graph.node)} nodes")

        # Create calibration reader
        calib_reader = RerankerCalibrationReader(
            tokenizer, calib_pairs, input_names, MAX_LENGTH
        )

        # Configure GPTQ
        gptq_config = GPTQWeightOnlyQuantConfig(
            calibration_data_reader=calib_reader,
            block_size=block_size,
            actorder=actorder,
        )

        quantizer = MatMulNBitsQuantizer(
            model=model,
            bits=4,
            block_size=block_size,
            is_symmetric=is_symmetric,
            algo_config=gptq_config,
        )

        # Monkey-patch gptq_quantize to use CoreML (Metal GPU) provider
        import onnxruntime.quantization.neural_compressor.weight_only as _wo
        _orig_gptq = _wo.gptq_quantize
        def _patched_gptq(*args, **kwargs):
            kwargs["providers"] = ["CoreMLExecutionProvider", "CPUExecutionProvider"]
            return _orig_gptq(*args, **kwargs)
        _wo.gptq_quantize = _patched_gptq

        print(f"Running GPTQ quantization with CoreML/Metal GPU (block_size={block_size}, symmetric={is_symmetric}, actorder={actorder})...")
        t0 = time.perf_counter()
        quantizer.process()
        qt = time.perf_counter() - t0

        # Restore original
        _wo.gptq_quantize = _orig_gptq
        print(f"Quantization took {qt:.1f}s")

        # Save
        if os.path.exists(ONNX_INT4_DIR):
            shutil.rmtree(ONNX_INT4_DIR)
        os.makedirs(ONNX_INT4_DIR)

        output_path = os.path.join(ONNX_INT4_DIR, "model.onnx")
        quantizer.model.save_model_to_file(output_path)

        for fname in os.listdir(ONNX_FP32_DIR):
            if fname.startswith("model.onnx"):
                continue
            src = os.path.join(ONNX_FP32_DIR, fname)
            if os.path.isfile(src):
                shutil.copy2(src, os.path.join(ONNX_INT4_DIR, fname))

        total_size = sum(
            os.path.getsize(os.path.join(ONNX_INT4_DIR, f))
            for f in os.listdir(ONNX_INT4_DIR)
            if os.path.isfile(os.path.join(ONNX_INT4_DIR, f)) and f.startswith("model")
        )
        print(f"INT4 model size: {total_size / (1024*1024):.1f} MB")

        # Validate
        log(f"Validating: {desc}")
        int4_session = ort.InferenceSession(output_path, providers=["CPUExecutionProvider"])
        int4_input_names = [inp.name for inp in int4_session.get_inputs()]
        int4_scores, int4_time = onnx_score_batch(
            int4_session, tokenizer, val_pairs, int4_input_names, "INT4"
        )
        del int4_session

        with open("int4_scores.json", "w") as f:
            json.dump(int4_scores, f)

        passed = compare(baseline_scores, int4_scores)

        if passed:
            log(f"SUCCESS: {desc}")
            print(f"INT4 ONNX model: {ONNX_INT4_DIR}/model.onnx ({total_size / (1024*1024):.1f} MB)")
            print(f"Inference speed: {int4_time/len(val_pairs)*1000:.1f}ms/pair")

            # List files
            print(f"\nFiles in {ONNX_INT4_DIR}/:")
            for f in sorted(os.listdir(ONNX_INT4_DIR)):
                fp = os.path.join(ONNX_INT4_DIR, f)
                if os.path.isfile(fp):
                    sz = os.path.getsize(fp) / (1024 * 1024)
                    print(f"  {f:40s} {sz:8.1f} MB")
            return

        print(f"\n{desc} failed criteria, trying next config...\n")

    log("All GPTQ INT4 configs failed")
    print("Consider using INT8 quantization (previous run already passed).")


if __name__ == "__main__":
    main()
