#!/usr/bin/env python3
"""Export Turbo-AI trimmed Jina reranker to ONNX + INT8 static quantization."""

import os
import shutil
import time

import torch
from transformers import AutoModelForSequenceClassification, AutoTokenizer

MODEL_NAME = "Turbo-AI/jina-reranker-v2-base-multilingual__trim_vocab"
ONNX_FP32_DIR = "./jina-reranker-v2-trim-onnx-fp32"
ONNX_INT8_DIR = "./jina-reranker-v2-trim-int8-onnx"


def log(m):
    print(f"\n{'='*60}\n  {m}\n{'='*60}\n")


def main():
    # Step 1: Download
    log("Downloading trimmed model")
    tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME, trust_remote_code=True)
    model = AutoModelForSequenceClassification.from_pretrained(MODEL_NAME, trust_remote_code=True)
    model.eval().float()
    print(f"Params: {sum(p.numel() for p in model.parameters()):,}")

    # Step 2: ONNX export
    log("Exporting to ONNX (FP32)")
    os.makedirs(ONNX_FP32_DIR, exist_ok=True)
    onnx_path = os.path.join(ONNX_FP32_DIR, "model.onnx")

    dummy = tokenizer("query", "document", truncation=True, max_length=128, return_tensors="pt")
    input_names = list(dummy.keys())
    print(f"Inputs: {input_names}")

    dynamic_axes = {}
    for name in input_names:
        dynamic_axes[name] = {0: "batch_size", 1: "seq_len"}
    dynamic_axes["logits"] = {0: "batch_size"}

    torch.onnx.export(
        model,
        tuple(dummy[k] for k in input_names),
        onnx_path,
        input_names=input_names,
        output_names=["logits"],
        dynamic_axes=dynamic_axes,
        opset_version=17,
        do_constant_folding=True,
    )
    tokenizer.save_pretrained(ONNX_FP32_DIR)
    model.config.save_pretrained(ONNX_FP32_DIR)

    fp32_size = os.path.getsize(onnx_path) / (1024 * 1024)
    # Check for external data file
    data_file = onnx_path + ".data"
    if os.path.exists(data_file):
        fp32_size += os.path.getsize(data_file) / (1024 * 1024)
    print(f"FP32 ONNX: {fp32_size:.1f} MB")

    # Step 3: INT8 static quantization
    log("INT8 quantization (MatMulNBits)")
    import onnx
    from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer

    fp32_model = onnx.load(onnx_path, load_external_data=True)
    print(f"Nodes: {len(fp32_model.graph.node)}")

    quantizer = MatMulNBitsQuantizer(
        model=fp32_model,
        bits=8,
        block_size=128,
        is_symmetric=True,
    )
    quantizer.process()

    os.makedirs(ONNX_INT8_DIR, exist_ok=True)
    int8_path = os.path.join(ONNX_INT8_DIR, "model.onnx")
    quantizer.model.save_model_to_file(int8_path)

    # Copy tokenizer + config
    for f in os.listdir(ONNX_FP32_DIR):
        if not f.startswith("model.onnx"):
            src = os.path.join(ONNX_FP32_DIR, f)
            if os.path.isfile(src):
                shutil.copy2(src, os.path.join(ONNX_INT8_DIR, f))

    int8_size = sum(
        os.path.getsize(os.path.join(ONNX_INT8_DIR, f))
        for f in os.listdir(ONNX_INT8_DIR)
        if os.path.isfile(os.path.join(ONNX_INT8_DIR, f)) and f.startswith("model")
    ) / (1024 * 1024)
    print(f"INT8 ONNX: {int8_size:.1f} MB")

    # Step 4: Verify
    log("Verification")
    import onnxruntime as ort

    session = ort.InferenceSession(int8_path, providers=["CPUExecutionProvider"])
    inp_names = [i.name for i in session.get_inputs()]
    enc = tokenizer("test query", "test document", return_tensors="np")
    feed = {k: enc[k] for k in inp_names if k in enc}
    out = session.run(None, feed)
    print(f"Test score: {out[0].squeeze():.4f}")
    print("Model works!")

    # Final report
    log("DONE")
    print(f"FP32: {fp32_size:.1f} MB")
    print(f"INT8: {int8_size:.1f} MB")
    print(f"Compression: {fp32_size/int8_size:.1f}x\n")
    for f in sorted(os.listdir(ONNX_INT8_DIR)):
        fp = os.path.join(ONNX_INT8_DIR, f)
        if os.path.isfile(fp):
            print(f"  {f:40s} {os.path.getsize(fp)/(1024*1024):8.1f} MB")


if __name__ == "__main__":
    main()
