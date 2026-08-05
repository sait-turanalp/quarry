#!/usr/bin/env python3
"""Mixed quantization: FFN INT4 + Attention INT8 + Embedding INT8 + rest FP16."""
import onnx, os, shutil, time, glob, random
import numpy as np, onnxruntime as ort
from onnx import numpy_helper, TensorProto
from onnxruntime.transformers.float16 import convert_float_to_float16
from scipy.stats import spearmanr, kendalltau
from transformers import AutoTokenizer

FP32_DIR = "./jina-reranker-v2-trim-onnx-fp32"
OUT_DIR = "./jina-reranker-v2-trim-mixed-onnx"

# Step 1: Load and convert to FP16
print("Loading FP32 model...")
model = onnx.load(os.path.join(FP32_DIR, "model.onnx"), load_external_data=True)
print("FP16 conversion...")
model = convert_float_to_float16(model, keep_io_types=True)

# Step 2: Identify MatMul nodes and classify as FFN vs Attention
# In XLM-RoBERTa: each layer has:
#   - Attention: Wqkv (768,2304), Wo (768,768) — "MatMul" feeding into attention
#   - FFN: W1 (768,3072), W2 (3072,768) — larger matrices
print("Classifying MatMul nodes...")
matmul_nodes = [n for n in model.graph.node if n.op_type == "MatMul"]

# Build initializer lookup
init_map = {}
for init in model.graph.initializer:
    init_map[init.name] = init

ffn_nodes = []
attn_nodes = []
for node in matmul_nodes:
    # Check weight tensor shape to classify
    for inp in node.input:
        if inp in init_map:
            dims = list(init_map[inp].dims)
            if len(dims) == 2:
                # FFN: one dim is 3072
                if 3072 in dims:
                    ffn_nodes.append(node.name)
                else:
                    attn_nodes.append(node.name)
            break

print(f"  FFN MatMul nodes: {len(ffn_nodes)}")
print(f"  Attention MatMul nodes: {len(attn_nodes)}")

# Step 3: Quantize embedding to INT8
print("Quantizing embedding to INT8...")
for init in model.graph.initializer:
    if "word_embedding" in init.name.lower() or "embeddings.word" in init.name.lower():
        arr = numpy_helper.to_array(init)
        orig_dtype = arr.dtype
        # Quantize to INT8: scale per-channel
        arr_f32 = arr.astype(np.float32)
        scale = np.max(np.abs(arr_f32), axis=1, keepdims=True) / 127.0
        scale = np.where(scale == 0, 1.0, scale)
        arr_int8 = np.clip(np.round(arr_f32 / scale), -127, 127).astype(np.int8)
        # Dequantize back to FP16
        arr_deq = (arr_int8.astype(np.float32) * scale).astype(np.float16)
        new_tensor = numpy_helper.from_array(arr_deq, name=init.name)
        init.CopyFrom(new_tensor)
        orig_mb = np.prod(arr.shape) * 2 / (1024 * 1024)  # FP16
        print(f"  {init.name}: {list(arr.shape)} — {orig_mb:.1f}MB (now dequantized INT8→FP16)")
        break

# Step 4: Apply INT4 to FFN nodes, INT8 to attention nodes
from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer

# First pass: INT4 on FFN only
print("INT4 quantization on FFN layers...")
all_matmul_names = [n.name for n in matmul_nodes]
exclude_from_int4 = attn_nodes  # Keep attention out of INT4

quantizer = MatMulNBitsQuantizer(
    model=model,
    bits=4,
    block_size=128,
    is_symmetric=True,
    nodes_to_exclude=exclude_from_int4,
)
quantizer.process()

# Second pass: INT8 on attention (the model now has FFN in INT4, need to apply INT8 to remaining)
print("INT8 quantization on attention layers...")
# The FFN nodes are already quantized (replaced with MatMulNBits),
# so a second pass with INT8 will only catch the remaining MatMul (attention)
quantizer2 = MatMulNBitsQuantizer(
    model=quantizer.model.model,
    bits=8,
    block_size=128,
    is_symmetric=True,
)
quantizer2.process()

# Save
if os.path.exists(OUT_DIR):
    shutil.rmtree(OUT_DIR)
os.makedirs(OUT_DIR)
out_path = os.path.join(OUT_DIR, "model.onnx")
quantizer2.model.save_model_to_file(out_path)
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
print(f"\nMixed model: {total/(1024*1024):.1f} MB")

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


def score_all(sess):
    inp = [i.name for i in sess.get_inputs()]
    scores = []
    t0 = time.perf_counter()
    for q, d in pairs:
        enc = tok(q, d, truncation=True, max_length=1024, return_tensors="np")
        out = sess.run(None, {k: enc[k] for k in inp if k in enc})
        scores.append(float(out[0].squeeze()))
    return scores, time.perf_counter() - t0


print(f"\n{len(pairs)} validation pairs")

print("FP32 baseline...")
s1 = ort.InferenceSession(os.path.join(FP32_DIR, "model.onnx"), providers=["CPUExecutionProvider"])
base, t_base = score_all(s1)
del s1

print("Mixed quant...")
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
print(f"  FP32:   {t_base:.1f}s ({t_base/len(pairs)*1000:.0f}ms/pair)")
print(f"  Mixed:  {t_qnt:.1f}s ({t_qnt/len(pairs)*1000:.0f}ms/pair)")
print(f"  Speedup: {t_base/t_qnt:.1f}x")
print()
for f in sorted(os.listdir(OUT_DIR)):
    fp = os.path.join(OUT_DIR, f)
    if os.path.isfile(fp):
        print(f"  {f:40s} {os.path.getsize(fp)/(1024*1024):8.1f} MB")
