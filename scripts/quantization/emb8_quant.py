#!/usr/bin/env python3
"""INT8 all weights + FP16 others + Embedding INT8 dequant."""
import onnx, os, shutil, glob, random
import numpy as np, onnxruntime as ort
from onnx import numpy_helper
from onnxruntime.transformers.float16 import convert_float_to_float16
from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer
from scipy.stats import spearmanr, kendalltau
from transformers import AutoTokenizer

FP32_DIR = "./jina-reranker-v2-trim-onnx-fp32"
OUT = "./jina-reranker-v2-trim-int8fp16-emb8-onnx"

model = onnx.load(os.path.join(FP32_DIR, "model.onnx"), load_external_data=True)
model = convert_float_to_float16(model, keep_io_types=True)

for init in model.graph.initializer:
    if "embeddings.word" in init.name.lower():
        arr = numpy_helper.to_array(init).astype(np.float32)
        scale = np.max(np.abs(arr), axis=1, keepdims=True) / 127.0
        scale = np.where(scale == 0, 1.0, scale)
        arr_int8 = np.clip(np.round(arr / scale), -127, 127).astype(np.int8)
        arr_deq = (arr_int8.astype(np.float32) * scale).astype(np.float16)
        init.CopyFrom(numpy_helper.from_array(arr_deq, name=init.name))
        print(f"Embedding quantized: {list(arr.shape)}")
        break

q = MatMulNBitsQuantizer(model=model, bits=8, block_size=128, is_symmetric=True)
q.process()

if os.path.exists(OUT):
    shutil.rmtree(OUT)
os.makedirs(OUT)
out_path = os.path.join(OUT, "model.onnx")
q.model.save_model_to_file(out_path)
for f in os.listdir(FP32_DIR):
    if not f.startswith("model.onnx"):
        src = os.path.join(FP32_DIR, f)
        if os.path.isfile(src):
            shutil.copy2(src, os.path.join(OUT, f))

total = sum(
    os.path.getsize(os.path.join(OUT, f))
    for f in os.listdir(OUT)
    if f.startswith("model") and os.path.isfile(os.path.join(OUT, f))
)
print(f"Size: {total/(1024*1024):.1f} MB")

# Validate
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
qs = ["How does indexing work?", "Find symbol search", "Show MCP server",
      "How are parsers registered?", "Find vector search", "Show config logic",
      "How does BM25 work?", "Find HTTP server", "Show chunking logic",
      "How does hybrid search work?", "Find reranking code", "Show pipeline stages",
      "What parsers exist?", "How does semantic search work?", "Find call graph",
      "Show visibility detection", "How does import resolution work?",
      "Find persistence layer", "Show transaction handling", "Find clustering code"]
pairs = [(qs[i % len(qs)], snippets[128 + i][:2000]) for i in range(min(100, len(snippets) - 128))]

def sa(sess):
    inp = [i.name for i in sess.get_inputs()]
    sc = []
    for qq, d in pairs:
        enc = tok(qq, d, truncation=True, max_length=1024, return_tensors="np")
        sc.append(float(sess.run(None, {k: enc[k] for k in inp if k in enc})[0].squeeze()))
    return sc

s1 = ort.InferenceSession(os.path.join(FP32_DIR, "model.onnx"), providers=["CPUExecutionProvider"])
base = sa(s1)
del s1
s2 = ort.InferenceSession(out_path, providers=["CPUExecutionProvider"])
qnt = sa(s2)
del s2

b, q2 = np.array(base), np.array(qnt)
sp, _ = spearmanr(b, q2)
kt, _ = kendalltau(b, q2)
mae = float(np.mean(np.abs(b - q2)))
inv = 0
tot = 0
for i in range(len(b)):
    for j in range(i + 1, len(b)):
        tot += 1
        if (b[i] > b[j]) != (q2[i] > q2[j]):
            inv += 1

print(f"Spearman:   {sp:.4f} ({'PASS' if sp >= 0.98 else 'FAIL'})")
print(f"Kendall:    {kt:.4f} ({'PASS' if kt >= 0.95 else 'FAIL'})")
print(f"MAE:        {mae:.4f} ({'PASS' if mae <= 0.5 else 'FAIL'})")
print(f"Inversions: {inv/tot:.4f} ({'PASS' if inv/tot <= 0.05 else 'FAIL'})")
