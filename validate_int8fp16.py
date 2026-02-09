#!/usr/bin/env python3
"""INT8+FP16 on trimmed model + quality validation."""
import onnx, os, shutil, time, glob, random
import numpy as np, onnxruntime as ort
from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer
from onnxruntime.transformers.float16 import convert_float_to_float16
from scipy.stats import spearmanr, kendalltau
from transformers import AutoTokenizer

FP32_DIR = "./jina-reranker-v2-trim-onnx-fp32"
OUT_DIR = "./jina-reranker-v2-trim-int8fp16-onnx"

# Build model
model = onnx.load(os.path.join(FP32_DIR, "model.onnx"), load_external_data=True)
print("FP16 conversion...")
model_fp16 = convert_float_to_float16(model, keep_io_types=True)
print("INT8 quantization...")
quantizer = MatMulNBitsQuantizer(model=model_fp16, bits=8, block_size=128, is_symmetric=True)
quantizer.process()

if os.path.exists(OUT_DIR):
    shutil.rmtree(OUT_DIR)
os.makedirs(OUT_DIR)
out_path = os.path.join(OUT_DIR, "model.onnx")
quantizer.model.save_model_to_file(out_path)
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
print(f"INT8+FP16: {total/(1024*1024):.1f} MB")

# Validation
tok = AutoTokenizer.from_pretrained(
    "Turbo-AI/jina-reranker-v2-base-multilingual__trim_vocab", trust_remote_code=True
)
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
qs = [
    "How does indexing work?", "Find symbol search", "Show MCP server",
    "How are parsers registered?", "Find vector search", "Show config logic",
    "How does BM25 work?", "Find HTTP server", "Show chunking logic",
    "How does hybrid search work?", "Find reranking code", "Show pipeline stages",
    "What parsers exist?", "How does semantic search work?", "Find call graph",
    "Show visibility detection", "How does import resolution work?",
    "Find persistence layer", "Show transaction handling", "Find clustering code",
]
pairs = [(qs[i % len(qs)], snippets[128 + i][:2000]) for i in range(min(100, len(snippets) - 128))]
print(f"{len(pairs)} validation pairs")


def score_all(sess):
    inp = [i.name for i in sess.get_inputs()]
    scores = []
    t0 = time.perf_counter()
    for q, d in pairs:
        enc = tok(q, d, truncation=True, max_length=1024, return_tensors="np")
        out = sess.run(None, {k: enc[k] for k in inp if k in enc})
        scores.append(float(out[0].squeeze()))
    return scores, time.perf_counter() - t0


print("FP32 baseline...")
s1 = ort.InferenceSession(os.path.join(FP32_DIR, "model.onnx"), providers=["CPUExecutionProvider"])
base, t_base = score_all(s1)
del s1

print("INT8+FP16...")
s2 = ort.InferenceSession(out_path, providers=["CPUExecutionProvider"])
qnt, t_qnt = score_all(s2)
del s2

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
ir = inv / tot

print()
print(f"  Spearman:    {sp:.4f}  ({'PASS' if sp >= 0.98 else 'FAIL'})")
print(f"  Kendall tau: {kt:.4f}  ({'PASS' if kt >= 0.95 else 'FAIL'})")
print(f"  MAE:         {mae:.4f}  ({'PASS' if mae <= 0.5 else 'FAIL'})")
print(f"  Inversions:  {ir:.4f}  ({'PASS' if ir <= 0.05 else 'FAIL'})")
print()
print(f"  FP32:      {t_base:.1f}s ({t_base/len(pairs)*1000:.0f}ms/pair)")
print(f"  INT8+FP16: {t_qnt:.1f}s ({t_qnt/len(pairs)*1000:.0f}ms/pair)")
print(f"  Speedup:   {t_base/t_qnt:.1f}x")
