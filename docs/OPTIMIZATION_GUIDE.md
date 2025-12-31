# Performance Optimization Guide

This guide covers how to achieve maximum performance with sevenz-rust2, including
release profile settings and best practices for various workloads.

## Table of Contents

1. [Release Build Optimization](#release-build-optimization)
2. [Compression Performance Tips](#compression-performance-tips)
3. [Decompression Performance Tips](#decompression-performance-tips)
4. [Multi-Threading](#multi-threading)
5. [Benchmarking](#benchmarking)
6. [Performance vs C++ Reference](#performance-vs-c-reference)

## Release Build Optimization

### Required Cargo.toml Settings

For maximum performance, ensure your `Cargo.toml` includes these settings:

```toml
[profile.release]
lto = "fat"           # Link Time Optimization - crucial for cross-crate inlining
codegen-units = 1     # Single codegen unit for better optimization
opt-level = 3         # Maximum optimization level
panic = "abort"       # Slightly faster, removes unwinding code

# For profiling (releases with debug symbols)
[profile.release-with-debug]
inherits = "release"
debug = true
strip = "none"
```

### Build Commands

```bash
# Maximum performance build
cargo build --release

# Build with native CPU optimizations (best performance, not portable)
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Build for profiling
cargo build --profile release-with-debug
```

### Expected Impact

| Setting | Impact |
|---------|--------|
| `lto = "fat"` | 10-30% faster (enables cross-crate inlining) |
| `codegen-units = 1` | 5-10% faster (better global optimization) |
| `target-cpu=native` | 5-20% faster (uses AVX2, etc.) |

## Compression Performance Tips

### 1. Use Solid Compression for Many Small Files

```rust
use sevenz_rust2::*;
use std::io::Cursor;

// Bad: Non-solid compression (one block per file)
let mut archive = ArchiveWriter::create("output.7z")?;
for file in files {
    archive.push_archive_entry(entry, Some(Cursor::new(&file.data)))?;
}

// Good: Solid compression (all files in one block)
let mut archive = ArchiveWriter::create("output.7z")?;
let mut provider = VecParallelStreamProvider::new(file_data);
archive.push_solid_entries_parallel(entries, &mut provider, ParallelSolidConfig::default())?;
```

### 2. Use BufferPool for High-Volume Workloads

```rust
use sevenz_rust2::perf::{BufferPool, LARGE_BUFFER_SIZE};

// Reuse buffers to avoid allocation overhead
let pool = BufferPool::new(8, LARGE_BUFFER_SIZE);

for blob in blobs {
    archive.push_archive_entry_batched(entry, Some(reader), Some(&pool))?;
}
```

### 3. Use Zero-Copy for In-Memory Data

```rust
// Data already in memory - avoid intermediate buffer
let data: Vec<u8> = get_data();
archive.push_archive_entry_bytes(entry, &data)?;
```

### 4. Choose Appropriate Compression Level

```rust
use sevenz_rust2::encoder_options::Lzma2Options;

// Fast compression (lower CPU, larger output)
let mut options = Lzma2Options::default();
options.options.lzma_options.level = 1;

// Maximum compression (higher CPU, smaller output)
options.options.lzma_options.level = 9;
```

## Decompression Performance Tips

### 1. Enable Multi-Threading for LZMA2

```rust
let mut reader = ArchiveReader::open("archive.7z", Password::empty())?;

// Use all available cores for LZMA2 decompression
reader.set_thread_count(std::thread::available_parallelism()?.get() as u32);
```

### 2. Use Memory-Mapped Files (mmap feature)

For large archives, memory mapping can be faster:

```toml
[dependencies]
sevenz-rust2 = { version = "0.20", features = ["mmap"] }
```

```rust
use sevenz_rust2::mmap::MmapArchiveReader;

let reader = MmapArchiveReader::open("large_archive.7z")?;
```

### 3. Process Files in Block Order

For solid archives, always process files in their stored order:

```rust
// Efficient: Process in order
reader.for_each_entries(|entry, reader| {
    let mut data = Vec::with_capacity(entry.size as usize);
    reader.read_to_end(&mut data)?;
    Ok(true)
})?;

// Inefficient: Random access in solid archives
for name in random_names {
    let data = reader.read_file(&name)?; // Re-decompresses previous data each time!
}
```

## Multi-Threading

### Parallel Solid Compression

```rust
use sevenz_rust2::*;

let config = ParallelSolidConfig::new()
    .with_batch_size(32)         // Process 32 files per batch
    .with_buffer_size(256 * 1024); // 256KB buffers

let mut provider = VecParallelStreamProvider::with_parallelism(file_data, 8);
archive.push_solid_entries_parallel(entries, &mut provider, config)?;
```

### Thread-Safe Buffer Pool

```rust
use std::sync::Arc;
use sevenz_rust2::perf::SyncBufferPool;

let pool = Arc::new(SyncBufferPool::new(16, 256 * 1024));

// Share across threads
std::thread::scope(|s| {
    for i in 0..4 {
        let pool = Arc::clone(&pool);
        s.spawn(move || {
            let mut buf = pool.get();
            // Use buffer...
        });
    }
});
```

## Benchmarking

### Creating Benchmarks

```rust
use std::time::Instant;
use sevenz_rust2::perf::CompressionStats;

let mut stats = CompressionStats::new();

for file in files {
    let start = Instant::now();
    
    // Compression code...
    archive.push_archive_entry_bytes(entry, &file.data)?;
    
    stats.record_file(file.data.len() as u64, 0, start.elapsed());
}

println!("Throughput: {:.2} MB/s", stats.throughput_mbps());
println!("Files/sec: {:.0}", stats.files_per_second());
```

### Profiling

```bash
# With perf (Linux)
RUSTFLAGS="-C target-cpu=native" cargo build --profile release-with-debug
perf record --call-graph dwarf ./target/release-with-debug/myapp
perf report

# With flamegraph
cargo install flamegraph
cargo flamegraph --release -- [args]
```

## Performance vs C++ Reference

The Rust implementation may show different performance characteristics compared
to the C++ 7-Zip reference implementation:

### Where Rust is Comparable or Faster

1. **CRC32 calculation**: Uses SIMD-accelerated `crc32fast` crate
2. **Header parsing**: Comparable speed
3. **Multi-threaded decompression**: LZMA2 MT is well-optimized

### Where Rust May Be Slower

1. **LZMA/LZMA2 core algorithm**: The `lzma-rust2` crate is pure Rust and may
   not match the hand-optimized assembly in liblzma.

2. **Dynamic dispatch overhead**: The decoder/encoder stack uses trait objects
   (`Box<dyn Read/Write>`), adding some indirection.

### Optimization Checklist

- [ ] Build with `--release` and LTO enabled
- [ ] Use `RUSTFLAGS="-C target-cpu=native"` for local benchmarks
- [ ] Profile with `perf` or `flamegraph` to identify bottlenecks
- [ ] Use solid compression for many small files
- [ ] Use buffer pools for high-volume workloads
- [ ] Enable multi-threading where supported
- [ ] Consider memory-mapped I/O for large archives

### Future Optimizations

Potential improvements that could close the gap with C++:

1. SIMD-optimized LZMA match finder
2. Assembly-optimized range coder
3. Static dispatch for common codec configurations
4. Reduced allocations in hot paths
