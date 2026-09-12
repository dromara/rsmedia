//! Single-threaded encoding benchmark (criterion): each job encodes one clip
//! with `with_thread_count(1)`, no worker pool — one job per iteration. This is
//! the per-job baseline that `encode_pipeline.rs` (task-parallel pools)
//! compares against, and the right configuration when jobs are already
//! parallelized upstream.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench encode_single_thread
//! ```

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::time::Duration;

mod common;
use common::{
    AUDIO_MEDIA_SECS, SUBTITLE_MEDIA_SECS, VIDEO_MEDIA_SECS, bench_dir, encode_audio,
    encode_subtitle, encode_video, remove_file,
};

fn bench_single_thread(c: &mut Criterion) {
    rsmedia::init().expect("rsmedia init failed");
    let dir = bench_dir();

    let mut group = c.benchmark_group("encode_single_thread");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.warm_up_time(Duration::from_millis(500));

    // Video: 60 frames (2 s), pts left unset so the encoder's automatic
    // numbering is exercised.
    let video_path = dir.join("single_video.mp4");
    group.throughput(Throughput::Elements(VIDEO_MEDIA_SECS as u64));
    group.bench_function("video 320x240@30, 2 s", |b| {
        b.iter(|| encode_video(&video_path, 1).unwrap())
    });
    remove_file(&video_path);

    // Audio: variable input frame sizes so the encoder's sample FIFO splits
    // and merges them into fixed 1024-sample AAC frames.
    let audio_path = dir.join("single_audio.mp4");
    group.throughput(Throughput::Elements(AUDIO_MEDIA_SECS as u64));
    group.bench_function("audio 44.1kHz stereo aac, ~3 s", |b| {
        b.iter(|| encode_audio(&audio_path, 1).unwrap())
    });
    remove_file(&audio_path);

    // Subtitle: 30 mov_text segments written through the dedicated subtitle
    // pipeline (StreamWriter + encode_subtitle_segments_to_file).
    let subtitle_path = dir.join("single_subtitle.mp4");
    group.throughput(Throughput::Elements(SUBTITLE_MEDIA_SECS as u64));
    group.bench_function("subtitle 30x mov_text, 60 s", |b| {
        b.iter(|| encode_subtitle(&subtitle_path, 1).unwrap())
    });
    remove_file(&subtitle_path);

    group.finish();
}

criterion_group!(benches, bench_single_thread);
criterion_main!(benches);
