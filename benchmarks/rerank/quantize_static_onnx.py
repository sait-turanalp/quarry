#!/usr/bin/env python3
"""Build a static INT8 ONNX reranker model from real query/snippet calibration pairs.

Expected input dataset row (JSONL), typically candidates.raw.jsonl:
  {"query":"...", "snippet":"...", "query_id":"q1", ...}
"""

from __future__ import annotations

import argparse
import json
import random
import re
import shutil
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Iterable

import numpy as np
import onnx
import onnxruntime as ort
from onnxruntime.quantization import (
    CalibrationDataReader,
    CalibrationMethod,
    QuantFormat,
    QuantType,
    quantize_static,
)


def _err(msg: str) -> None:
    raise SystemExit(msg)


def _load_tokenizer(model_dir: Path):
    try:
        from transformers import AutoTokenizer  # type: ignore
    except Exception as e:
        _err(
            "Missing dependency: transformers.\n"
            f"Import error: {e}\n"
            "Install with: pip install 'transformers>=4.40'"
        )
    try:
        return AutoTokenizer.from_pretrained(
            str(model_dir), local_files_only=True, use_fast=True
        )
    except Exception as e:
        _err(f"Failed to load tokenizer from '{model_dir}': {e}")


def _resolve_model_onnx(model_dir: Path) -> Path:
    cands = [model_dir / "model.onnx", model_dir / "onnx" / "model.onnx"]
    for p in cands:
        if p.exists():
            return p
    _err(
        "Missing ONNX model. Expected one of:\n"
        f"- {cands[0]}\n- {cands[1]}"
    )
    return cands[0]


def _copy_metadata_files(src_model_dir: Path, dst_model_dir: Path) -> None:
    dst_model_dir.mkdir(parents=True, exist_ok=True)
    required = [
        "config.json",
        "tokenizer.json",
        "tokenizer_config.json",
        "special_tokens_map.json",
    ]
    missing = []
    for name in required:
        src = src_model_dir / name
        if not src.exists():
            missing.append(str(src))
            continue
        shutil.copy2(src, dst_model_dir / name)
    if missing:
        _err("Missing required metadata files:\n" + "\n".join(missing))


def _load_pairs(calibration_jsonl: Path) -> list[dict]:
    rows = []
    with calibration_jsonl.open("r", encoding="utf-8") as f:
        for i, line in enumerate(f, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as e:
                _err(f"Invalid JSON at {calibration_jsonl}:{i}: {e}")
            q = str(row.get("query", "")).strip()
            s = str(row.get("snippet", "")).strip()
            if not q or not s:
                continue
            rows.append(
                {
                    "query": q,
                    "snippet": s,
                    "query_id": str(row.get("query_id", "")).strip(),
                    "rank": int(row.get("rank", 0) or 0),
                }
            )
    if not rows:
        _err(f"No usable (query,snippet) rows found in {calibration_jsonl}")
    return rows


def _sample_pairs(rows: list[dict], sample_size: int, seed: int) -> list[dict]:
    if sample_size <= 0:
        _err("--sample-size must be > 0")

    by_query: dict[str, list[dict]] = defaultdict(list)
    for r in rows:
        qid = r["query_id"] or "__unknown__"
        by_query[qid].append(r)

    rng = random.Random(seed)
    for bucket in by_query.values():
        bucket.sort(key=lambda x: x["rank"] if x["rank"] > 0 else 999_999)

    query_ids = sorted(by_query.keys())
    rng.shuffle(query_ids)
    min_per_query = 2

    picked: list[dict] = []
    seen = set()
    for qid in query_ids:
        for row in by_query[qid][:min_per_query]:
            key = (row["query"], row["snippet"])
            if key in seen:
                continue
            seen.add(key)
            picked.append(row)
            if len(picked) >= sample_size:
                return picked

    pool = []
    for r in rows:
        key = (r["query"], r["snippet"])
        if key in seen:
            continue
        pool.append(r)
    rng.shuffle(pool)
    for r in pool:
        picked.append(r)
        if len(picked) >= sample_size:
            break
    return picked


def _count_ops(onnx_path: Path) -> Counter:
    m = onnx.load(str(onnx_path))
    counts: Counter = Counter()
    for node in m.graph.node:
        counts[node.op_type] += 1
    return counts


class PairCalibrationReader(CalibrationDataReader):
    def __init__(
        self,
        pairs: list[dict],
        tokenizer,
        input_names: list[str],
        max_length: int,
    ) -> None:
        self._items = self._encode_pairs(pairs, tokenizer, input_names, max_length)
        self._idx = 0

    @staticmethod
    def _encode_pairs(
        pairs: Iterable[dict],
        tokenizer,
        input_names: list[str],
        max_length: int,
    ) -> list[dict]:
        out = []
        need_token_type_ids = "token_type_ids" in set(input_names)
        for row in pairs:
            enc = tokenizer(
                row["query"],
                row["snippet"],
                truncation=True,
                max_length=max_length,
                padding="max_length",
                return_tensors="np",
            )
            item = {}
            if "input_ids" in input_names:
                item["input_ids"] = enc["input_ids"].astype(np.int64)
            if "attention_mask" in input_names:
                item["attention_mask"] = enc["attention_mask"].astype(np.int64)
            if need_token_type_ids:
                if "token_type_ids" in enc:
                    item["token_type_ids"] = enc["token_type_ids"].astype(np.int64)
                else:
                    item["token_type_ids"] = np.zeros_like(enc["input_ids"], dtype=np.int64)
            out.append(item)
        return out

    def get_next(self):  # type: ignore[override]
        if self._idx >= len(self._items):
            return None
        item = self._items[self._idx]
        self._idx += 1
        return item

    def rewind(self):  # type: ignore[override]
        self._idx = 0


def _quantize(
    src_model: Path,
    out_model: Path,
    reader: CalibrationDataReader,
    fmt: QuantFormat,
    calibration_method: CalibrationMethod,
    nodes_to_exclude: list[str] | None = None,
) -> None:
    quantize_static(
        model_input=str(src_model),
        model_output=str(out_model),
        calibration_data_reader=reader,
        quant_format=fmt,
        activation_type=QuantType.QUInt8,
        weight_type=QuantType.QInt8,
        op_types_to_quantize=["MatMul", "Gemm"],
        per_channel=True,
        nodes_to_exclude=nodes_to_exclude or None,
        calibrate_method=calibration_method,
    )


def _parse_csv_patterns(text: str | None) -> list[str]:
    if not text:
        return []
    out = []
    for part in text.split(","):
        p = part.strip()
        if p:
            out.append(p)
    return out


def _preset_patterns(name: str) -> list[str]:
    n = name.strip().lower()
    if n in {"", "none"}:
        return []
    if n == "jina_v1_mixed":
        return [
            # Keep attention Q/K projections in FP32 (sensitive outliers)
            r"/attention/self/query/MatMul$",
            r"/attention/self/key/MatMul$",
            # Keep attention output projection in FP32
            r"/attention/output/dense/MatMul$",
            # Keep final pooling/head in FP32
            r"/bert/pooler/dense/Gemm$",
            r"/classifier/Gemm$",
        ]
    if n == "jina_v1_mixed_v2":
        return [
            # Keep attention Q/K/V projections in FP32.
            r"/attention/self/query/MatMul$",
            r"/attention/self/key/MatMul$",
            r"/attention/self/value/MatMul$",
            # Keep attention output projection in FP32.
            r"/attention/output/dense/MatMul$",
            # Keep MLP output projection in FP32.
            r"/mlp/wo/MatMul$",
            # Keep final pooling/head in FP32.
            r"/bert/pooler/dense/Gemm$",
            r"/classifier/Gemm$",
        ]
    _err(
        f"Unsupported --exclude-preset '{name}'. "
        "Use: none|jina_v1_mixed|jina_v1_mixed_v2"
    )
    return []


def _resolve_nodes_to_exclude(src_model: Path, patterns: list[str]) -> list[str]:
    if not patterns:
        return []
    model = onnx.load(str(src_model))
    compiled = [re.compile(p) for p in patterns]
    excluded = []
    for node in model.graph.node:
        name = node.name or ""
        if not name:
            continue
        if any(rx.search(name) for rx in compiled):
            excluded.append(name)
    # Dedup while preserving order
    seen = set()
    uniq = []
    for n in excluded:
        if n in seen:
            continue
        seen.add(n)
        uniq.append(n)
    return uniq


def _verify_session(onnx_path: Path) -> tuple[bool, str]:
    try:
        ort.InferenceSession(str(onnx_path), providers=["CPUExecutionProvider"])
        return True, ""
    except Exception as e:
        return False, str(e)


def _calibration_method_from_str(name: str) -> CalibrationMethod:
    n = name.strip().lower()
    if n == "percentile":
        return CalibrationMethod.Percentile
    if n == "entropy":
        return CalibrationMethod.Entropy
    if n == "minmax":
        return CalibrationMethod.MinMax
    _err(f"Unsupported --calibration-method '{name}'. Use: percentile|entropy|minmax")
    return CalibrationMethod.Percentile


def main() -> None:
    ap = argparse.ArgumentParser(description="Quantize reranker ONNX into static INT8")
    ap.add_argument("--model-dir", required=True, help="Source model directory")
    ap.add_argument("--calibration-jsonl", required=True, help="Calibration rows JSONL")
    ap.add_argument("--out-dir", required=True, help="Output model directory")
    ap.add_argument("--sample-size", type=int, default=500, help="Calibration pair count")
    ap.add_argument("--seed", type=int, default=42, help="Sampling seed")
    ap.add_argument("--max-length", type=int, default=1024, help="Tokenizer max length")
    ap.add_argument(
        "--calibration-method",
        default="percentile",
        help="percentile|entropy|minmax (default: percentile)",
    )
    ap.add_argument(
        "--prefer-format",
        default="qoperator",
        help="qoperator|qdq (default: qoperator)",
    )
    ap.add_argument(
        "--exclude-preset",
        default="none",
        help="none|jina_v1_mixed|jina_v1_mixed_v2 (default: none)",
    )
    ap.add_argument(
        "--exclude-node-patterns",
        default="",
        help="Comma-separated regex patterns matched against node names",
    )
    args = ap.parse_args()

    model_dir = Path(args.model_dir).expanduser().resolve()
    out_dir = Path(args.out_dir).expanduser().resolve()
    calibration_jsonl = Path(args.calibration_jsonl).expanduser().resolve()
    if not model_dir.exists():
        _err(f"Missing --model-dir: {model_dir}")
    if not calibration_jsonl.exists():
        _err(f"Missing --calibration-jsonl: {calibration_jsonl}")

    src_model = _resolve_model_onnx(model_dir)
    tokenizer = _load_tokenizer(model_dir)
    rows = _load_pairs(calibration_jsonl)
    sampled = _sample_pairs(rows, args.sample_size, args.seed)
    pattern_list = _preset_patterns(args.exclude_preset) + _parse_csv_patterns(
        args.exclude_node_patterns
    )
    nodes_to_exclude = _resolve_nodes_to_exclude(src_model, pattern_list)

    # Build destination layout expected by quarry custom model loader.
    _copy_metadata_files(model_dir, out_dir)
    out_onnx_dir = out_dir / "onnx"
    out_onnx_dir.mkdir(parents=True, exist_ok=True)
    out_model = out_onnx_dir / "model.onnx"

    # Discover model input names from source graph.
    sess = ort.InferenceSession(str(src_model), providers=["CPUExecutionProvider"])
    input_names = [i.name for i in sess.get_inputs()]
    reader = PairCalibrationReader(
        sampled, tokenizer, input_names=input_names, max_length=args.max_length
    )
    calibration_method = _calibration_method_from_str(args.calibration_method)

    prefer = args.prefer_format.strip().lower()
    if prefer not in {"qoperator", "qdq"}:
        _err("Unsupported --prefer-format. Use qoperator|qdq")
    formats = (
        [QuantFormat.QOperator, QuantFormat.QDQ]
        if prefer == "qoperator"
        else [QuantFormat.QDQ, QuantFormat.QOperator]
    )

    format_used = None
    load_error = None
    for fmt in formats:
        if out_model.exists():
            out_model.unlink()
        reader.rewind()
        _quantize(
            src_model,
            out_model,
            reader,
            fmt,
            calibration_method,
            nodes_to_exclude=nodes_to_exclude,
        )
        ok, err = _verify_session(out_model)
        if ok:
            format_used = "qoperator" if fmt == QuantFormat.QOperator else "qdq"
            load_error = ""
            break
        load_error = err

    if not format_used:
        _err(
            "Static INT8 model was generated but failed to load in ORT.\n"
            f"Last error: {load_error}"
        )

    src_ops = _count_ops(src_model)
    dst_ops = _count_ops(out_model)
    meta = {
        "source_model": str(src_model),
        "output_model": str(out_model),
        "sample_size": len(sampled),
        "sample_seed": args.seed,
        "max_length": args.max_length,
        "calibration_method": args.calibration_method.lower(),
        "format_used": format_used,
        "exclude_preset": args.exclude_preset,
        "exclude_node_patterns": pattern_list,
        "excluded_node_count": len(nodes_to_exclude),
        "excluded_node_sample": nodes_to_exclude[:20],
        "source_ops": dict(src_ops),
        "quantized_ops": dict(dst_ops),
        "dynamic_quantize_linear_count": int(dst_ops.get("DynamicQuantizeLinear", 0)),
        "qlinear_matmul_count": int(dst_ops.get("QLinearMatMul", 0)),
        "matmul_integer_count": int(dst_ops.get("MatMulInteger", 0)),
    }
    meta_path = out_dir / "model.meta.json"
    meta_path.write_text(json.dumps(meta, indent=2), encoding="utf-8")
    print(json.dumps(meta, indent=2))
    print(f"\nWrote: {meta_path}")


if __name__ == "__main__":
    main()
