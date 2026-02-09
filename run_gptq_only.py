#!/usr/bin/env python3
"""GPTQ INT4 only — 32 samples, 256 seq_len for speed."""

import json, glob, os, random, shutil, time
import numpy as np, onnx, onnxruntime as ort
from scipy.stats import kendalltau, spearmanr
from transformers import AutoTokenizer

MODEL_NAME = "jinaai/jina-reranker-v2-base-multilingual"
ONNX_FP32_DIR = "./jina-reranker-v2-onnx-fp32"
ONNX_INT4_DIR = "./jina-reranker-v2-int4-onnx"
SEED = 42

def log(m): print(f"\n{'='*60}\n  {m}\n{'='*60}\n")

# ── Calibration pairs: 32 short pairs ────────────────────────
def make_calib(tokenizer, input_names):
    random.seed(SEED)
    files = []
    for ext in ["*.rs", "*.toml"]:
        files.extend(glob.glob(f"src/**/{ext}", recursive=True))
    snippets = []
    for fp in files:
        try:
            c = open(fp, encoding="utf-8", errors="ignore").read()
            for i in range(0, len(c.split("\n")), 50):
                ch = "\n".join(c.split("\n")[i:i+50]).strip()
                if len(ch) > 30: snippets.append(ch[:500])
        except: pass
    random.shuffle(snippets)
    qs = ["search code","find function","show implementation","get symbol"]
    data = []
    for i in range(min(32, len(snippets))):
        enc = tokenizer(qs[i%4], snippets[i], truncation=True, max_length=256, return_tensors="np")
        data.append({k: enc[k] for k in input_names if k in enc})
    return data

class CalibReader:
    def __init__(self, data): self.data, self.idx = data, 0
    def get_next(self):
        if self.idx >= len(self.data): return None
        r = self.data[self.idx]; self.idx += 1; return r
    def __iter__(self): self.idx = 0; return self
    def __next__(self):
        r = self.get_next()
        if r is None: raise StopIteration
        return r
    def __len__(self): return len(self.data)
    def set_range(self, s, e): self.idx = s

# ── Validation pairs ─────────────────────────────────────────
def make_val():
    random.seed(SEED)
    files = []
    for ext in ["*.rs","*.toml","*.md"]:
        files.extend(glob.glob(f"src/**/{ext}", recursive=True))
    files.extend(glob.glob("*.toml")); files.extend(glob.glob("*.md"))
    snippets = []
    for fp in files:
        try:
            c = open(fp, encoding="utf-8", errors="ignore").read()
            for i in range(0, len(c.split("\n")), 200):
                ch = "\n".join(c.split("\n")[i:i+200]).strip()
                if len(ch) > 50: snippets.append(ch)
        except: pass
    random.shuffle(snippets)
    qs = ["How does indexing work?","Find symbol search","Show MCP server",
          "How are parsers registered?","Find vector search","Show config logic",
          "How does BM25 work?","Find HTTP server","Show chunking logic",
          "How does hybrid search work?","Find reranking code","Show pipeline stages",
          "What parsers exist?","How does semantic search work?","Find call graph",
          "Show visibility detection","How does import resolution work?",
          "Find persistence layer","Show transaction handling","Find clustering code"]
    pairs = []
    for i, s in enumerate(snippets[128:]):
        pairs.append((qs[i%len(qs)], s[:2000]))
        if len(pairs) >= 100: break
    return pairs

def score_all(session, tokenizer, pairs, inp_names):
    scores = []
    t0 = time.perf_counter()
    for i,(q,d) in enumerate(pairs):
        enc = tokenizer(q, d, truncation=True, max_length=1024, return_tensors="np")
        out = session.run(None, {k: enc[k] for k in inp_names if k in enc})
        scores.append(float(out[0].squeeze()))
        if (i+1)%25==0: print(f"  [{i+1}/{len(pairs)}] {scores[-1]:.4f}")
    el = time.perf_counter()-t0
    print(f"  {el:.1f}s ({el/len(pairs)*1000:.0f}ms/pair)")
    return scores, el

def check(base, qnt):
    b, q = np.array(base), np.array(qnt)
    sp,_ = spearmanr(b,q); kt,_ = kendalltau(b,q)
    mae = float(np.mean(np.abs(b-q)))
    inv = sum(1 for i in range(len(b)) for j in range(i+1,len(b)) if (b[i]>b[j])!=(q[i]>q[j]))
    tot = len(b)*(len(b)-1)//2; ir = inv/tot
    for name,ok,v in [("Spearman>0.98",sp>=0.98,sp),("Kendall>0.95",kt>=0.95,kt),
                       ("MAE<0.50",mae<=0.5,mae),("Inv<5%",ir<=0.05,ir)]:
        print(f"  [{'PASS' if ok else 'FAIL'}] {name} = {v:.4f}")
    return sp>=0.98 and kt>=0.95 and mae<=0.5 and ir<=0.05

def main():
    log("GPTQ INT4 — optimized (32 samples, 256 seq_len)")
    tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME, trust_remote_code=True)

    fp32 = os.path.join(ONNX_FP32_DIR, "model.onnx")
    sess = ort.InferenceSession(fp32, providers=["CPUExecutionProvider"])
    inp_names = [i.name for i in sess.get_inputs()]
    del sess

    # Baseline
    if os.path.exists("baseline_scores.json"):
        baseline = json.load(open("baseline_scores.json"))
        print(f"Cached baseline: {len(baseline)} scores")
    else:
        log("Computing baseline"); sess = ort.InferenceSession(fp32, providers=["CPUExecutionProvider"])
        baseline,_ = score_all(sess, tokenizer, make_val(), inp_names); del sess
        json.dump(baseline, open("baseline_scores.json","w"))

    val_pairs = make_val()
    calib_data = make_calib(tokenizer, inp_names)
    print(f"Calibration: {len(calib_data)} samples (max_len=256)")

    configs = [
        ("GPTQ bs=128", 128, True, False),
        ("GPTQ bs=128 actorder", 128, True, True),
        ("GPTQ bs=64", 64, True, False),
        ("GPTQ bs=64 actorder", 64, True, True),
    ]

    from onnxruntime.quantization.matmul_nbits_quantizer import GPTQWeightOnlyQuantConfig, MatMulNBitsQuantizer

    for desc, bs, sym, actorder in configs:
        log(desc)
        model = onnx.load(fp32, load_external_data=True)

        reader = CalibReader(list(calib_data))  # fresh copy
        cfg = GPTQWeightOnlyQuantConfig(calibration_data_reader=reader, block_size=bs, actorder=actorder)
        quant = MatMulNBitsQuantizer(model=model, bits=4, block_size=bs, is_symmetric=sym, algo_config=cfg)

        t0 = time.perf_counter()
        quant.process()
        print(f"  Quantization: {time.perf_counter()-t0:.1f}s")

        # Save
        if os.path.exists(ONNX_INT4_DIR): shutil.rmtree(ONNX_INT4_DIR)
        os.makedirs(ONNX_INT4_DIR)
        out = os.path.join(ONNX_INT4_DIR, "model.onnx")
        quant.model.save_model_to_file(out)
        for f in os.listdir(ONNX_FP32_DIR):
            if not f.startswith("model.onnx"):
                s = os.path.join(ONNX_FP32_DIR, f)
                if os.path.isfile(s): shutil.copy2(s, os.path.join(ONNX_INT4_DIR, f))
        sz = sum(os.path.getsize(os.path.join(ONNX_INT4_DIR,f)) for f in os.listdir(ONNX_INT4_DIR) if f.startswith("model") and os.path.isfile(os.path.join(ONNX_INT4_DIR,f)))
        print(f"  Size: {sz/(1024*1024):.1f} MB")
        del model, quant

        # Validate
        sess = ort.InferenceSession(out, providers=["CPUExecutionProvider"])
        scores, el = score_all(sess, tokenizer, val_pairs, [i.name for i in sess.get_inputs()])
        del sess
        json.dump(scores, open("int4_scores.json","w"))

        if check(baseline, scores):
            log(f"SUCCESS: {desc}")
            for f in sorted(os.listdir(ONNX_INT4_DIR)):
                fp = os.path.join(ONNX_INT4_DIR,f)
                if os.path.isfile(fp): print(f"  {f:40s} {os.path.getsize(fp)/(1024*1024):8.1f} MB")
            return
        print(f"  {desc} failed, next...\n")

    log("All failed. INT8 recommended.")

if __name__ == "__main__":
    main()
