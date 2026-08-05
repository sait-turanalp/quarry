#!/usr/bin/env python3
"""
Jina Reranker v2 INT4 Quantization — Fast Pipeline

Strategy:
  1. HQQ (no calibration, seconds) — try first
  2. GPTQ optimized (32 samples, 256 seq_len) — fallback
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
from scipy.stats import kendalltau, spearmanr
from transformers import AutoTokenizer


MODEL_NAME = "jinaai/jina-reranker-v2-base-multilingual"
ONNX_FP32_DIR = "./jina-reranker-v2-onnx-fp32"
ONNX_INT4_DIR = "./jina-reranker-v2-int4-onnx"
SEED = 42
VAL_SIZE = 100
MAX_LENGTH = 1024

MIN_SPEARMAN = 0.98
MIN_KENDALL = 0.95
MAX_MAE = 0.5
MAX_INVERSION_RATE = 0.05


def log(msg):
    print(f"\n{'='*60}\n  {msg}\n{'='*60}\n")


def prepare_val_pairs():
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
                    snippets.append(chunk.strip())
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
        "Show the configuration loading logic",
        "How does the BM25 scoring work?",
        "What are the CLI commands available?",
        "Show me the document chunking logic",
        "Find the HTTP server implementation",
        "How does hybrid search combine results?",
        "Find the reranking implementation",
        "How does the pipeline stage system work?",
        "What language parsers are supported?",
        "How does semantic search work?",
        "Find the call graph analysis code",
        "Show the visibility detection logic",
        "How does import resolution work?",
        "Find the persistence layer code",
    ]
    # First 128 = calibration range, skip those for val
    pairs = []
    for i, snippet in enumerate(snippets[128:]):
        pairs.append((queries[i % len(queries)], snippet[:2000]))
        if len(pairs) >= VAL_SIZE:
            break
    return pairs


def prepare_calib_pairs(n=32, max_len=256):
    """Small, short calibration pairs for fast GPTQ."""
    random.seed(SEED)
    code_files = []
    for ext in ["*.rs", "*.toml"]:
        code_files.extend(glob.glob(f"src/**/{ext}", recursive=True))
    snippets = []
    for fpath in code_files:
        try:
            with open(fpath, "r", encoding="utf-8", errors="ignore") as f:
                content = f.read()
            lines = content.split("\n")
            for i in range(0, len(lines), 50):  # Smaller chunks
                chunk = "\n".join(lines[i : i + 50])
                if len(chunk.strip()) > 30:
                    snippets.append(chunk.strip()[:500])  # Short snippets
        except Exception:
            continue
    random.shuffle(snippets)
    queries = ["search code", "find function", "show implementation", "get symbol"]
    pairs = [(queries[i % len(queries)], snippets[i]) for i in range(min(n, len(snippets)))]
    return pairs


class CalibReader:
    def __init__(self, tokenizer, pairs, input_names, max_length=256):
        self.data = []
        for q, d in pairs:
            enc = tokenizer(q, d, truncation=True, max_length=max_length, return_tensors="np")
            self.data.append({k: enc[k] for k in input_names if k in enc})
        self.idx = 0

    def get_next(self):
        if self.idx >= len(self.data):
            return None
        r = self.data[self.idx]
        self.idx += 1
        return r

    def __iter__(self):
        self.idx = 0
        return self

    def __next__(self):
        r = self.get_next()
        if r is None:
            raise StopIteration
        return r

    def __len__(self):
        return len(self.data)

    def set_range(self, s, e):
        self.idx = s


def score_batch(session, tokenizer, pairs, input_names):
    scores = []
    t0 = time.perf_counter()
    for i, (q, d) in enumerate(pairs):
        enc = tokenizer(q, d, truncation=True, max_length=MAX_LENGTH, return_tensors="np")
        feed = {k: enc[k] for k in input_names if k in enc}
        out = session.run(None, feed)
        scores.append(float(out[0].squeeze()))
        if (i + 1) % 25 == 0:
            print(f"  [{i+1}/{len(pairs)}] score={scores[-1]:.4f}")
    elapsed = time.perf_counter() - t0
    print(f"  {elapsed:.1f}s ({elapsed/len(pairs)*1000:.0f}ms/pair)")
    return scores, elapsed


def check_quality(baseline, quantized):
    base, qnt = np.array(baseline), np.array(quantized)
    sp, _ = spearmanr(base, qnt)
    kt, _ = kendalltau(base, qnt)
    mae = float(np.mean(np.abs(base - qnt)))
    inv = sum(
        1 for i in range(len(base)) for j in range(i + 1, len(base))
        if (base[i] > base[j]) != (qnt[i] > qnt[j])
    )
    total = len(base) * (len(base) - 1) // 2
    inv_rate = inv / total

    checks = [
        ("Spearman > 0.98", sp >= MIN_SPEARMAN, sp),
        ("Kendall  > 0.95", kt >= MIN_KENDALL, kt),
        ("MAE      < 0.50", mae <= MAX_MAE, mae),
        ("Inversions < 5%", inv_rate <= MAX_INVERSION_RATE, inv_rate),
    ]
    all_pass = True
    for name, ok, val in checks:
        tag = "PASS" if ok else "FAIL"
        if not ok:
            all_pass = False
        print(f"  [{tag}] {name} (actual: {val:.4f})")
    return all_pass


def save_quantized(quantizer, fp32_dir, int4_dir):
    if os.path.exists(int4_dir):
        shutil.rmtree(int4_dir)
    os.makedirs(int4_dir)
    out = os.path.join(int4_dir, "model.onnx")
    quantizer.model.save_model_to_file(out)
    for f in os.listdir(fp32_dir):
        if not f.startswith("model.onnx"):
            src = os.path.join(fp32_dir, f)
            if os.path.isfile(src):
                shutil.copy2(src, os.path.join(int4_dir, f))
    sz = sum(
        os.path.getsize(os.path.join(int4_dir, f))
        for f in os.listdir(int4_dir)
        if os.path.isfile(os.path.join(int4_dir, f)) and f.startswith("model")
    ) / (1024 * 1024)
    print(f"  Model size: {sz:.1f} MB")
    return out


def try_quantize(name, fp32_onnx, algo_config, block_size, is_symmetric, tokenizer, val_pairs, baseline_scores):
    from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer

    log(f"{name}")
    model = onnx.load(fp32_onnx, load_external_data=True)
    print(f"  {len(model.graph.node)} nodes")

    quantizer = MatMulNBitsQuantizer(
        model=model,
        bits=4,
        block_size=block_size,
        is_symmetric=is_symmetric,
        algo_config=algo_config,
    )

    t0 = time.perf_counter()
    quantizer.process()
    qt = time.perf_counter() - t0
    print(f"  Quantization: {qt:.1f}s")

    out = save_quantized(quantizer, ONNX_FP32_DIR, ONNX_INT4_DIR)
    del model, quantizer

    # Validate
    session = ort.InferenceSession(out, providers=["CPUExecutionProvider"])
    inp_names = [i.name for i in session.get_inputs()]
    int4_scores, int4_time = score_batch(session, tokenizer, val_pairs, inp_names)
    del session

    with open("int4_scores.json", "w") as f:
        json.dump(int4_scores, f)

    passed = check_quality(baseline_scores, int4_scores)
    return passed, int4_time


def main():
    log("Jina Reranker v2 — Fast INT4 Quantization")

    tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME, trust_remote_code=True)
    val_pairs = prepare_val_pairs()

    fp32_onnx = os.path.join(ONNX_FP32_DIR, "model.onnx")
    if not os.path.exists(fp32_onnx):
        print(f"ERROR: {fp32_onnx} not found. Run quantize_jina_reranker.py first.")
        return

    # Baseline scores
    if os.path.exists("baseline_scores.json"):
        with open("baseline_scores.json") as f:
            baseline_scores = json.load(f)
        print(f"Loaded {len(baseline_scores)} cached baseline scores")
    else:
        log("Computing baseline (FP32 ONNX)")
        sess = ort.InferenceSession(fp32_onnx, providers=["CPUExecutionProvider"])
        inp = [i.name for i in sess.get_inputs()]
        baseline_scores, _ = score_batch(sess, tokenizer, val_pairs, inp)
        del sess
        with open("baseline_scores.json", "w") as f:
            json.dump(baseline_scores, f)

    fp32_session = ort.InferenceSession(fp32_onnx, providers=["CPUExecutionProvider"])
    input_names = [i.name for i in fp32_session.get_inputs()]
    del fp32_session

    # ═══════════════════════════════════════════════════════════════
    # Attempt 1: HQQ (no calibration, very fast)
    # ═══════════════════════════════════════════════════════════════
    from onnxruntime.quantization.matmul_nbits_quantizer import HQQWeightOnlyQuantConfig

    hqq_configs = [
        ("HQQ INT4 bs=128", 128, True, HQQWeightOnlyQuantConfig(block_size=128, bits=4)),
        ("HQQ INT4 bs=64",  64,  True, HQQWeightOnlyQuantConfig(block_size=64, bits=4)),
    ]

    for name, bs, sym, cfg in hqq_configs:
        passed, _ = try_quantize(name, fp32_onnx, cfg, bs, sym, tokenizer, val_pairs, baseline_scores)
        if passed:
            log(f"SUCCESS with {name}")
            print_final()
            return

    # ═══════════════════════════════════════════════════════════════
    # Attempt 2: GPTQ optimized (32 samples, 256 seq_len)
    # ═══════════════════════════════════════════════════════════════
    from onnxruntime.quantization.matmul_nbits_quantizer import GPTQWeightOnlyQuantConfig

    calib_pairs = prepare_calib_pairs(n=32, max_len=256)

    gptq_configs = [
        ("GPTQ INT4 bs=128 (32 samples)", 128, True, False),
        ("GPTQ INT4 bs=128 actorder (32 samples)", 128, True, True),
    ]

    for name, bs, sym, actorder in gptq_configs:
        reader = CalibReader(tokenizer, calib_pairs, input_names, max_length=256)
        cfg = GPTQWeightOnlyQuantConfig(
            calibration_data_reader=reader,
            block_size=bs,
            actorder=actorder,
        )

        # Monkey-patch to pass providers (stays CPU, but needed for the API)
        import onnxruntime.quantization.neural_compressor.weight_only as _wo
        _orig = _wo.gptq_quantize
        def _patched(*args, **kwargs):
            kwargs.setdefault("providers", ["CPUExecutionProvider"])
            return _orig(*args, **kwargs)
        _wo.gptq_quantize = _patched

        passed, _ = try_quantize(name, fp32_onnx, cfg, bs, sym, tokenizer, val_pairs, baseline_scores)
        _wo.gptq_quantize = _orig

        if passed:
            log(f"SUCCESS with {name}")
            print_final()
            return

    log("All INT4 configs failed. INT8 recommended.")


def print_final():
    print(f"\nFiles in {ONNX_INT4_DIR}/:")
    for f in sorted(os.listdir(ONNX_INT4_DIR)):
        fp = os.path.join(ONNX_INT4_DIR, f)
        if os.path.isfile(fp):
            print(f"  {f:40s} {os.path.getsize(fp) / (1024*1024):8.1f} MB")


if __name__ == "__main__":
    main()
