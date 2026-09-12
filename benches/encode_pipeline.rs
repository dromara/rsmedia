//! Task-parallel encoding benchmark (criterion): a fixed pool of
//! `core count` worker threads drains batches of video / audio / subtitle
//! encode jobs, every encoder running single-threaded so that the **total
//! parallelism equals the core count** (no oversubscription).
//!
//! Per-job single-threaded baselines live in `encode_single_thread.rs`.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench encode_pipeline
//! ```

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::time::Duration;

mod common;
use common::{JOBS_PER_KIND, bench_dir, cores, media_secs, remove_file, run_pool, unique_path};
use rsmedia::MediaType;

fn bench_pool(c: &mut Criterion) {
    rsmedia::init().expect("rsmedia init failed");
    let cores = cores();
    let dir = bench_dir();

    let mut group = c.benchmark_group("encode_pool");
    // Full pool runs are tens of milliseconds to seconds per iteration; a small
    // sample size keeps the whole bench under a minute while staying stable.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(4));
    group.warm_up_time(Duration::from_millis(500));

    for kind in [MediaType::VIDEO, MediaType::AUDIO, MediaType::SUBTITLE] {
        let jobs: Vec<_> = (0..JOBS_PER_KIND)
            .map(|i| (kind, unique_path(&dir, &format!("pool_{kind:?}_{i}"))))
            .collect();
        group.throughput(Throughput::Elements(
            (media_secs(kind) * JOBS_PER_KIND as f64) as u64,
        ));
        group.bench_function(
            format!("{kind} x{JOBS_PER_KIND} jobs, {cores} workers x 1 thread"),
            |b| b.iter(|| run_pool(&jobs, cores, 1).unwrap()),
        );
        for (_, path) in &jobs {
            remove_file(path);
        }
    }

    // Mixed batch: the three kinds compete for the same pool, which is what a
    // real transcoding service looks like.
    let mut mixed = Vec::new();
    for kind in [MediaType::VIDEO, MediaType::AUDIO, MediaType::SUBTITLE] {
        for i in 0..JOBS_PER_KIND {
            mixed.push((kind, unique_path(&dir, &format!("pool_mixed_{kind:?}_{i}"))));
        }
    }
    let media_secs_total: f64 = mixed.iter().map(|(k, _)| media_secs(*k)).sum();
    group.throughput(Throughput::Elements(media_secs_total as u64));
    group.bench_function(
        format!("mixed x{} jobs, {cores} workers x 1 thread", mixed.len()),
        |b| b.iter(|| run_pool(&mixed, cores, 1).unwrap()),
    );
    for (_, path) in &mixed {
        remove_file(path);
    }

    group.finish();
}

criterion_group!(benches, bench_pool);
criterion_main!(benches);
