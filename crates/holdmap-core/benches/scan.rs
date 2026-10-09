//! Performance budget: a full warm scan should take under 50 ms.
//! Run with `cargo bench -p holdmap-core`.

use criterion::{criterion_group, criterion_main, Criterion};
use holdmap_core::topology::TopologyBuilder;
use holdmap_core::{remote, scan, Engine, ScanOptions};
use std::hint::black_box;

fn opts() -> ScanOptions {
    ScanOptions {
        all_states: false,
        docker: false,
    }
}

fn benches(c: &mut Criterion) {
    c.bench_function("scan (listeners + PID map, warm)", |b| {
        b.iter(|| black_box(scan::scan(&opts()).expect("scan")))
    });
    let s = scan::scan(&opts()).expect("scan");
    c.bench_function("topology build", |b| {
        b.iter(|| black_box(TopologyBuilder::new(&s).build()))
    });
    let engine = Engine::from_scan(scan::scan(&opts()).expect("scan"));
    c.bench_function("explain one port", |b| {
        b.iter(|| black_box(engine.explain(1, &Default::default())))
    });
    let text: String = (0..2000)
        .map(|i| {
            format!(
                "tcp ESTAB 0 0 127.0.0.1:{} 127.0.0.1:5432 users:((\"node\",pid={},fd=20))\n",
                10000 + i,
                1000 + i
            )
        })
        .collect();
    c.bench_function("parse 2000 ss lines", |b| {
        b.iter(|| black_box(remote::parse_ss(&text)))
    });
}

criterion_group!(scan_benches, benches);
criterion_main!(scan_benches);
