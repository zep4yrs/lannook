use std::io::Write;

use criterion::{criterion_group, criterion_main, Criterion};

use lannook_lib::transfer::{compute_sha256, sanitize_filename};

fn bench_sha256_32_mib_streaming(c: &mut Criterion) {
    let path = std::env::temp_dir().join("lannook-bench-32m.bin");
    {
        let mut file = std::fs::File::create(&path).expect("create bench file");
        let block = vec![0xABu8; 1024 * 1024];
        for _ in 0..32 {
            file.write_all(&block).expect("write bench block");
        }
    }

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    c.bench_function("sha256_32MiB_streaming", |b| {
        b.iter(|| rt.block_on(compute_sha256(&path)));
    });

    let _ = std::fs::remove_file(&path);
}

fn bench_sanitize_filename(c: &mut Criterion) {
    c.bench_function("sanitize_filename_typical", |b| {
        b.iter(|| {
            let _ = sanitize_filename("photo (2).jpg");
            let _ = sanitize_filename("C:\\Users\\nobody\\report<final>.pdf");
        });
    });
}

criterion_group!(
    benches,
    bench_sha256_32_mib_streaming,
    bench_sanitize_filename
);
criterion_main!(benches);
