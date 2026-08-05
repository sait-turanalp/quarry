#!/usr/bin/env python3
"""Export trimmed Jina reranker: FP16 + dynamic INT8 (ORT-compatible ops)."""
import onnx, os, shutil, glob, random, time
import numpy as np, onnxruntime as ort
from onnxruntime.quantization import quantize_dynamic, QuantType
from onnxruntime.transformers.float16 import convert_float_to_float16
from scipy.stats import spearmanr, kendalltau
from transformers import AutoTokenizer

FP32_DIR = "./jina-reranker-v2-trim-onnx-fp32"
OUT_DIR = "./jina-reranker-v2-trim-int8-onnx-compat"


def main():
    fp32_onnx = os.path.join(FP32_DIR, "model.onnx")

    # Dynamic INT8 quantization from FP32 (creates MatMulInteger ops, NOT MatMulNBits)
    print("Dynamic INT8 quantization (MatMulInteger ops)...")
    if os.path.exists(OUT_DIR):
        shutil.rmtree(OUT_DIR)
    os.makedirs(OUT_DIR)
    out_path = os.path.join(OUT_DIR, "model.onnx")

    quantize_dynamic(
        model_input=fp32_onnx,
        model_output=out_path,
        weight_type=QuantType.QInt8,
    )

    # Copy tokenizer + config
    for f in os.listdir(FP32_DIR):
        if not f.startswith("model.onnx"):
            src = os.path.join(FP32_DIR, f)
            if os.path.isfile(src):
                shutil.copy2(src, os.path.join(OUT_DIR, f))

    total = sum(
        os.path.getsize(os.path.join(OUT_DIR, f))
        for f in os.listdir(OUT_DIR)
        if f.startswith("model") and os.path.isfile(os.path.join(OUT_DIR, f))
    )
    print(f"INT8 dynamic: {total/(1024*1024):.1f} MB")

    # Verify op types (should be MatMulInteger, NOT MatMulNBits)
    m = onnx.load(out_path, load_external_data=False)
    ops = set(n.op_type for n in m.graph.node)
    print(f"Op types: {sorted(ops)}")
    has_nbits = any("NBits" in op for op in ops)
    has_matmulint = "MatMulInteger" in ops or "QLinearMatMul" in ops
    print(f"MatMulNBits: {'YES (BAD)' if has_nbits else 'NO (GOOD)'}")
    print(f"MatMulInteger/QLinear: {'YES (GOOD)' if has_matmulint else 'NO'}")
    del m

    # Validate
    tok = AutoTokenizer.from_pretrained(
        "Turbo-AI/jina-reranker-v2-base-multilingual__trim_vocab", trust_remote_code=True
    )

    # Quick smoke test
    sess = ort.InferenceSession(out_path, providers=["CPUExecutionProvider"])
    inp = [i.name for i in sess.get_inputs()]
    enc = tok("test query", "test document", return_tensors="np")
    out = sess.run(None, {k: enc[k] for k in inp if k in enc})
    print(f"Smoke test: {out[0].squeeze():.4f}")

    # Full validation
    random.seed(42)
    files = []
    for ext in ["*.rs", "*.toml", "*.md"]:
        files.extend(glob.glob(f"src/**/{ext}", recursive=True))
    files.extend(glob.glob("*.toml"))
    files.extend(glob.glob("*.md"))
    snippets = []
    for fp in files:
        try:
            lines = open(fp, encoding="utf-8", errors="ignore").read().split("\n")
            for i in range(0, len(lines), 200):
                ch = "\n".join(lines[i : i + 200]).strip()
                if len(ch) > 50:
                    snippets.append(ch)
        except Exception:
            pass
    random.shuffle(snippets)
    qs = ["How does indexing work?", "Find symbol search", "Show MCP server",
          "How are parsers registered?", "Find vector search", "Show config logic",
          "How does BM25 work?", "Find HTTP server", "Show chunking logic",
          "How does hybrid search work?", "Find reranking code", "Show pipeline stages",
          "What parsers exist?", "How does semantic search work?", "Find call graph",
          "Show visibility detection", "How does import resolution work?",
          "Find persistence layer", "Show transaction handling", "Find clustering code"]
    pairs = [(qs[i % len(qs)], snippets[128 + i][:2000]) for i in range(min(100, len(snippets) - 128))]

    def score_all(s):
        names = [i.name for i in s.get_inputs()]
        sc = []
        for qq, d in pairs:
            e = tok(qq, d, truncation=True, max_length=1024, return_tensors="np")
            sc.append(float(s.run(None, {k: e[k] for k in names if k in e})[0].squeeze()))
        return sc

    print(f"\n{len(pairs)} validation pairs")
    print("FP32 baseline...")
    s1 = ort.InferenceSession(fp32_onnx, providers=["CPUExecutionProvider"])
    base = score_all(s1)
    del s1

    print("INT8 dynamic...")
    qnt = score_all(sess)
    del sess

    b, q = np.array(base), np.array(qnt)
    sp, _ = spearmanr(b, q)
    kt, _ = kendalltau(b, q)
    mae = float(np.mean(np.abs(b - q)))
    inv = 0
    tot = 0
    for i in range(len(b)):
        for j in range(i + 1, len(b)):
            tot += 1
            if (b[i] > b[j]) != (q[i] > q[j]):
                inv += 1

    print(f"\n  Spearman:    {sp:.4f}  ({'PASS' if sp >= 0.98 else 'FAIL'})")
    print(f"  Kendall tau: {kt:.4f}  ({'PASS' if kt >= 0.95 else 'FAIL'})")
    print(f"  MAE:         {mae:.4f}  ({'PASS' if mae <= 0.5 else 'FAIL'})")
    print(f"  Inversions:  {inv/tot:.4f}  ({'PASS' if inv/tot <= 0.05 else 'FAIL'})")

    print(f"\nFiles:")
    for f in sorted(os.listdir(OUT_DIR)):
        fp = os.path.join(OUT_DIR, f)
        if os.path.isfile(fp):
            print(f"  {f:40s} {os.path.getsize(fp)/(1024*1024):8.1f} MB")


if __name__ == "__main__":
    main()
