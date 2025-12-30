[![Crate](https://img.shields.io/crates/v/sevenz-rust2.svg)](https://crates.io/crates/sevenz-rust2)
[![Documentation](https://docs.rs/sevenz-rust2/badge.svg)](https://docs.rs/sevenz-rust2)

This project is a 7z compressor/decompressor written in pure Rust.

This is a fork of the original, unmaintained sevenz-rust crate to continue the development and maintenance.

## Supported Codecs & filters

| Codec       | Decompression | Compression |
|-------------|---------------|-------------|
| COPY        | ✓             | ✓           |
| LZMA        | ✓             | ✓           |
| LZMA2       | ✓             | ✓           |
| BROTLI (*)  | ✓             | ✓           |
| BZIP2       | ✓             | ✓           |
| DEFLATE (*) | ✓             | ✓           |
| PPMD        | ✓             | ✓           |
| LZ4 (*)     | ✓             | ✓           |
| ZSTD (*)    | ✓             | ✓           |

(*) Require optional cargo feature.

| Filter        | Decompression | Compression |
|---------------|---------------|-------------|
| BCJ X86       | ✓             | ✓           |
| BCJ ARM       | ✓             | ✓           |
| BCJ ARM64     | ✓             | ✓           |
| BCJ ARM_THUMB | ✓             | ✓           |
| BCJ RISC_V    | ✓             | ✓           |
| BCJ PPC       | ✓             | ✓           |
| BCJ SPARC     | ✓             | ✓           |
| BCJ IA64      | ✓             | ✓           |
| BCJ2          | ✓             |             |
| DELTA         | ✓             | ✓           |

### Usage

```toml
[dependencies]
sevenz-rust2 = { version = "0.19" }
```

Decompress source file "data/sample.7z" to destination path "data/sample":

```rust
sevenz_rust2::decompress_file("data/sample.7z", "data/sample").expect("complete");
```

#### Decompress an encrypted 7z file

Use the helper function to encrypt and decompress source file "path/to/encrypted.7z" to destination path "data/sample":

```rust
sevenz_rust2::decompress_file_with_password("path/to/encrypted.7z", "data/sample", "password".into()).expect("complete");
```

## Compression

Use the helper function to create a 7z file with source path:

```rust
sevenz_rust2::compress_to_path("examples/data/sample", "examples/data/sample.7z").expect("compress ok");
```

### Compress with AES encryption

Use the helper function to create a 7z file with source path and password:

```rust
sevenz_rust2::compress_to_path_encrypted("examples/data/sample", "examples/data/sample.7z", "password".into()).expect("compress ok");
```

### Advanced Usage

#### Multi-Volume Archives

Create multi-volume archives that split across multiple files when they exceed a size limit:

```rust
use sevenz_rust2::*;

// Create a multi-volume archive with 10MB volume size limit
let config = VolumeConfig::new("path/to/archive", 10 * 1024 * 1024);
let mut writer = ArchiveWriter::create_multi_volume(config).expect("create writer ok");

// Add files as normal
writer.push_source_path("path/to/compress", |_| true).expect("pack ok");

// Finish and get metadata about created volumes
let metadata = writer.finish_multi_volume().expect("compress ok");
println!("Created {} volumes", metadata.volume_count);
// Files created: archive.7z.001, archive.7z.002, etc.
```

#### High-Performance Batch Compression

For applications processing thousands of small files (e.g., downloading blobs into memory cache), use `BufferPool` and batched compression for 10-30x speedup:

```rust
use sevenz_rust2::*;
use sevenz_rust2::perf::{BufferPool, LARGE_BUFFER_SIZE};

// Create buffer pool to reuse allocations (reduces 3.2GB to 2MB for 50k files)
let pool = BufferPool::new(16, LARGE_BUFFER_SIZE);

let mut archive = ArchiveWriter::create("output.7z").expect("create writer ok");

// Process many files efficiently
for blob in cached_blobs {
    let entry = ArchiveEntry::new_file(&blob.name);
    archive.push_archive_entry_batched(
        entry,
        Some(blob.reader()),
        Some(&pool),  // Reuse buffers!
    ).expect("compress ok");
}

archive.finish().expect("done");
```

**Performance**: For 50k files, reduces compression time from 10+ minutes to 30-60 seconds.

See [Performance Guide](docs/PERFORMANCE_GUIDE.md) for detailed optimization techniques.

#### Solid compression

Solid archives can in theory provide better compression rates, but decompressing a file needs all previous data to also
be decompressed.

```rust
use sevenz_rust2::*;

let mut writer = ArchiveWriter::create("dest.7z").expect("create writer ok");
writer.push_source_path("path/to/compress", | _ | true).expect("pack ok");
writer.finish().expect("compress ok");
```

#### Configure the compression methods

With encryption and lzma2 options:

```rust
use sevenz_rust2::*;

let mut writer = ArchiveWriter::create("dest.7z").expect("create writer ok");
writer.set_content_methods(vec![
    encoder_options::AesEncoderOptions::new("sevenz-rust".into()).into(),
    encoder_options::Lzma2Options::from_level(9).into(),
]);
writer.push_source_path("path/to/compress", | _ | true).expect("pack ok");
writer.finish().expect("compress ok");
```

#### High-Performance Parallel Compression

For maximum compression throughput with fast I/O sources (in-memory cache, NVMe, high-speed networks),
use parallel compression configuration:

```rust
use sevenz_rust2::*;
use sevenz_rust2::perf::ParallelConfig;

let mut writer = ArchiveWriter::create("dest.7z").expect("create writer ok");

// Configure for maximum throughput - uses all CPU cores and large buffers
writer.configure_parallel(ParallelConfig::max_throughput(), 6);

// Or use balanced settings for a good compression/speed trade-off
writer.configure_parallel(ParallelConfig::balanced(), 6);

// Add files with high-performance buffer sizes
use sevenz_rust2::perf::HYPER_BUFFER_SIZE;
let entry = ArchiveEntry::from_path("path/to/file", "file.txt".to_string());
writer.push_archive_entry_fast(entry, Some(std::fs::File::open("path/to/file").unwrap()), HYPER_BUFFER_SIZE).expect("ok");

writer.finish().expect("compress ok");
```

Available configurations:
- `ParallelConfig::max_throughput()` - Maximum I/O throughput for very fast sources (4MB buffers, 16MB chunks)
- `ParallelConfig::balanced()` - Good balance between speed and memory (256KB buffers, 4MB chunks)
- `ParallelConfig::low_latency()` - Lower latency for smaller files (64KB buffers, 1MB chunks)

Custom configuration:
```rust
use sevenz_rust2::perf::ParallelConfig;

let config = ParallelConfig::new()
    .with_threads(8)           // Use 8 compression threads
    .with_chunk_size(8 * 1024 * 1024)  // 8MB chunks
    .with_buffer_size(1024 * 1024);     // 1MB I/O buffers

let mut writer = ArchiveWriter::create("dest.7z").expect("create writer ok");
writer.configure_parallel(config, 6);  // Level 6 compression
```

#### Smart Cache Prefetch Hints

For callers with smart caches that want to pre-populate data just-in-time, use the `PrefetchQueue` to get hints about upcoming blobs:

```rust
use sevenz_rust2::perf::{PrefetchQueue, FnPrefetchCallback, PrefetchHint};

// Create a callback to receive prefetch hints
let callback = FnPrefetchCallback::new(|hints: &[PrefetchHint<String>]| {
    for hint in hints {
        println!("Upcoming blob: {} (lookahead: {})", hint.id, hint.lookahead_index);
        // Pre-populate your cache here for hint.id
    }
});

// Create a queue with your file list and lookahead count
let files = vec!["file1.txt".to_string(), "file2.txt".to_string(), "file3.txt".to_string()];
let mut queue = PrefetchQueue::new(files, 3, callback);

// Process items - each call to next() triggers prefetch hints for upcoming items
while let Some(filename) = queue.next() {
    // The callback has already been notified about the next 3 files
    // so your cache can pre-load them in parallel
    println!("Processing: {}", filename);
}
```

The `PrefetchQueue` supports:
- Configurable lookahead count (1-16 items ahead)
- `peek_upcoming()` to see what's coming without advancing
- `remaining()` to check how many items are left
- Works with any `Clone` type for blob identifiers

### WASM support

WASM is supported, but you can't use the default features. We provide a "default_wasm" feature that contains
all default features with the needed changes to support WASM:

```bash
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build --target wasm32-unknown-unknown --no-default-features --features=default_wasm
```

## Licence

Licensed under the [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0).
