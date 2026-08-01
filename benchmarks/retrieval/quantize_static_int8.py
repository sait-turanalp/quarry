#!/usr/bin/env python3
"""Quantize a model2vec safetensors embedding table to int8 for OptimizedStaticModel.

Global symmetric scale: the loader mean-pools int8 rows into i32 and then L2-normalises,
so a single global scale cancels out exactly. Per-row scales would NOT cancel — do not use them.
"""
import json
import pathlib
import shutil
import sys

import numpy as np
from safetensors.numpy import load_file, save_file

src = pathlib.Path(sys.argv[1])
dst = pathlib.Path(sys.argv[2]).expanduser()
dst.mkdir(parents=True, exist_ok=True)

tensors = load_file(src / "model.safetensors")
key = "embeddings" if "embeddings" in tensors else next(iter(tensors))
w = tensors[key].astype(np.float32)

scale = np.abs(w).max() / 127.0
q = np.clip(np.rint(w / scale), -127, 127).astype(np.int8)

save_file({"embeddings": q}, str(dst / "model.safetensors"))
for f in ("tokenizer.json", "config.json"):
    shutil.copy(src / f, dst / f)

# Fidelity check: cosine between f32 and dequantized rows (what actually matters after L2 norm).
deq = q.astype(np.float32)
num = (w * deq).sum(1)
den = np.linalg.norm(w, axis=1) * np.linalg.norm(deq, axis=1)
cos = num / np.where(den == 0, 1, den)
print(f"tensor_key={key} shape={w.shape} scale={scale:.6g}")
print(f"int8 table = {q.nbytes / 1e6:.1f} MB (f32 was {w.nbytes / 1e6:.1f} MB)")
print(f"row cosine f32-vs-int8: min={cos.min():.5f} mean={cos.mean():.5f} p1={np.percentile(cos, 1):.5f}")
print(f"normalize={json.loads((dst / 'config.json').read_text()).get('normalize')}")
print(f"written -> {dst}")
