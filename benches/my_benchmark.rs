use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use std::{fs, path::PathBuf};

fn merge(c: &mut Criterion) {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push("tests/dbfiles");

    let files: Vec<String> = fs::read_dir(d)
        .unwrap()
        .filter_map(|res| res.ok())
        .map(|dir| dir.path())
        .filter(|f| f.extension().map_or(false, |ext| ext == "db"))
        .map(|f| f.display().to_string())
        .collect();

    c.bench_function("merge_internal", |b| {
        b.iter(|| prometheus_client_python_speedups::merge_internal(black_box(&files)))
    });
}

criterion_group!(benches, merge);
criterion_main!(benches);
