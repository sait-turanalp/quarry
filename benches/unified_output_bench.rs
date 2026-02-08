//! Performance benchmarks for UnifiedOutput schema
//!
//! Verifies zero-cost abstractions and measures allocation overhead

use codanna::io::{EntityType, UnifiedOutputBuilder};
use codanna::symbol::Symbol;
use codanna::types::{FileId, Range, SymbolId, SymbolKind};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn create_test_symbols(count: usize) -> Vec<Symbol> {
    (0..count)
        .map(|i| {
            Symbol::new(
                SymbolId::new(i as u32 + 1).unwrap(),
                format!("symbol_{i}"),
                SymbolKind::Function,
                FileId::new(1).unwrap(),
                Range::new(i as u32, 0, i as u32, 10),
            )
        })
        .collect()
}

fn bench_unified_output_creation(c: &mut Criterion) {
    c.bench_function("create_unified_output_100_symbols", |b| {
        let symbols = create_test_symbols(100);
        b.iter(|| {
            let output =
                UnifiedOutputBuilder::items(black_box(symbols.clone()), EntityType::Symbol).build();
            black_box(output);
        });
    });

    c.bench_function("create_unified_output_1000_symbols", |b| {
        let symbols = create_test_symbols(1000);
        b.iter(|| {
            let output =
                UnifiedOutputBuilder::items(black_box(symbols.clone()), EntityType::Symbol).build();
            black_box(output);
        });
    });
}

fn bench_json_serialization(c: &mut Criterion) {
    c.bench_function("serialize_unified_output_100_symbols", |b| {
        let symbols = create_test_symbols(100);
        let output = UnifiedOutputBuilder::items(symbols, EntityType::Symbol).build();
        b.iter(|| {
            let json = serde_json::to_string(&output).unwrap();
            black_box(json);
        });
    });
}

fn bench_display_formatting(c: &mut Criterion) {
    c.bench_function("display_unified_output_100_symbols", |b| {
        let symbols = create_test_symbols(100);
        let output = UnifiedOutputBuilder::items(symbols, EntityType::Symbol).build();
        b.iter(|| {
            let text = format!("{output}");
            black_box(text);
        });
    });
}

criterion_group!(
    benches,
    bench_unified_output_creation,
    bench_json_serialization,
    bench_display_formatting
);
criterion_main!(benches);
