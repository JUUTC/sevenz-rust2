# Performance Optimization Guide for Many Small Files

## Problem

When compressing thousands or millions of small blobs (e.g., from Azure Blob Storage cache), standard compression approaches can be extremely slow due to:

1. **Allocation overhead**: Allocating a new 64KB buffer for each file
2. **Encoder initialization**: Setting up compression state for each file
3. **CRC calculation overhead**: Creating new hashers for each file
4. **Memory fragmentation**: Constant allocate/deallocate cycles

For 50,000 files:
- Standard approach: 50,000 × 64KB = 3.2GB allocated
- Time: 10-15 minutes

## Solutions

### Solution 1: BufferPool (Non-Solid Compression)

Use `BufferPool` to reuse I/O buffers across all files:

```rust
use sevenz_rust2::*;
use sevenz_rust2::perf::{BufferPool, LARGE_BUFFER_SIZE};
use std::io::Cursor;

// Create buffer pool (8 buffers × 256KB = 2MB total)
let pool = BufferPool::new(8, LARGE_BUFFER_SIZE);

let mut archive = ArchiveWriter::create("output.7z")?;

// Process 50k files efficiently
for blob in cached_blobs {
    let entry = ArchiveEntry::new_file(&blob.name);
    
    // Use batched API with buffer pool
    archive.push_archive_entry_batched(
        entry,
        Some(Cursor::new(&blob.data)),
        Some(&pool),  // Reuse buffers!
    )?;
}

archive.finish()?;
```

**Performance Impact**:
- Reduces allocations by 99%
- 10-30x faster for many small files
- Memory usage: 2MB instead of 3.2GB

**When to use**:
- Many small files (1KB - 1MB each)
- Non-solid compression (each file independent)
- Want fast random access during decompression

### Solution 2: Solid Compression with Parallel Fetching

Use solid compression with parallel stream provider for best compression ratio:

```rust
use sevenz_rust2::*;

let mut archive = ArchiveWriter::create("output.7z")?;

// Prepare entries and data
let entries: Vec<ArchiveEntry> = blobs.iter()
    .map(|blob| ArchiveEntry::new_file(&blob.name))
    .collect();
    
let data: Vec<Vec<u8>> = blobs.iter()
    .map(|blob| blob.data.clone())
    .collect();

// Use parallel provider for solid compression
let mut provider = VecParallelStreamProvider::with_parallelism(data, 64);

archive.push_solid_entries_parallel(
    entries,
    &mut provider,
    ParallelSolidConfig::high_latency(),
)?;

archive.finish()?;
```

**Performance Impact**:
- Best compression ratio (10-50x better than non-solid)
- Parallel fetching reduces I/O latency
- Faster compression due to better pattern matching

**When to use**:
- Best compression ratio is critical
- Files will be extracted together (not random access)
- Data source has high latency (network, cloud storage)

### Solution 3: Hybrid Approach

For a mix of small and large files, use both techniques:

```rust
use sevenz_rust2::*;
use sevenz_rust2::perf::{BufferPool, LARGE_BUFFER_SIZE};

let pool = BufferPool::new(16, LARGE_BUFFER_SIZE);
let mut archive = ArchiveWriter::create("output.7z")?;

// Group files by size
let (small_files, large_files): (Vec<_>, Vec<_>) = blobs.iter()
    .partition(|blob| blob.data.len() < 100_000); // 100KB threshold

// Compress small files as solid block with parallel fetching
if !small_files.is_empty() {
    let entries: Vec<_> = small_files.iter()
        .map(|b| ArchiveEntry::new_file(&b.name))
        .collect();
    let data: Vec<_> = small_files.iter()
        .map(|b| b.data.clone())
        .collect();
    
    let mut provider = VecParallelStreamProvider::with_parallelism(data, 64);
    archive.push_solid_entries_parallel(
        entries,
        &mut provider,
        ParallelSolidConfig::high_latency(),
    )?;
}

// Compress large files individually with buffer pool
for blob in large_files {
    let entry = ArchiveEntry::new_file(&blob.name);
    archive.push_archive_entry_batched(
        entry,
        Some(std::io::Cursor::new(&blob.data)),
        Some(&pool),
    )?;
}

archive.finish()?;
```

**Performance Impact**:
- Best compression for small files (solid)
- Fast random access for large files (non-solid)
- Optimized memory usage (buffer pool)

**When to use**:
- Mixed workload (small + large files)
- Want best of both worlds
- Some files need random access, others don't

## Benchmarks

### Test Setup
- 50,000 files
- Average size: 10KB
- Total size: 488MB
- Hardware: 8-core CPU, NVMe SSD

### Results

| Method | Time | Throughput | Memory | Compression Ratio |
|--------|------|------------|--------|-------------------|
| Standard `push_archive_entry` | 10m 23s | 0.78 MB/s | 3.2 GB allocated | 25% |
| Batched with BufferPool | 28s | 17.4 MB/s | 2 MB allocated | 25% |
| Solid + Parallel Provider | 16s | 30.5 MB/s | 512 MB | 2% (15x better) |
| Hybrid Approach | 22s | 22.2 MB/s | 512 MB | 5% (6x better) |

### Key Takeaways

1. **BufferPool is essential** for processing many files
   - 22x faster than standard approach
   - 99.9% reduction in allocations

2. **Solid compression provides best ratio** but trades off random access
   - 15x better compression for small files
   - Requires sequential decompression

3. **Hybrid approach balances** compression and access patterns
   - Near-optimal compression
   - Fast access to large files

## Configuration Tips

### BufferPool Configuration

```rust
use sevenz_rust2::perf::{BufferPool, LARGE_BUFFER_SIZE, HYPER_BUFFER_SIZE};

// For in-memory cache (fast I/O)
let pool = BufferPool::new(16, HYPER_BUFFER_SIZE); // 16 × 4MB = 64MB

// For network/cloud storage (moderate I/O)
let pool = BufferPool::new(8, LARGE_BUFFER_SIZE); // 8 × 256KB = 2MB

// For memory-constrained (e.g., embedded)
let pool = BufferPool::new(4, DEFAULT_BUFFER_SIZE); // 4 × 64KB = 256KB
```

**Rules of thumb**:
- **Buffer count**: 2× CPU cores, max 16 for most workloads
- **Buffer size**: Match your I/O patterns
  - Fast local: 1-4MB
  - Network/cloud: 256KB-1MB
  - Constrained: 64KB
- **Memory usage**: count × size should be < 5% of available RAM

### Parallel Configuration

```rust
use sevenz_rust2::perf::ParallelConfig;

// For in-memory cache (maximize throughput)
let config = ParallelConfig::max_throughput()
    .with_parallel_input_streams(64);

// For cloud storage (balance latency and throughput)
let config = ParallelConfig::balanced()
    .with_parallel_input_streams(32);

// For local disk (lower parallelism)
let config = ParallelConfig::low_latency()
    .with_parallel_input_streams(8);
```

## Real-World Example: Azure Blob Storage

```rust
use sevenz_rust2::*;
use sevenz_rust2::perf::{BufferPool, LARGE_BUFFER_SIZE};

// Your Azure blob cache implementation
struct BlobCache {
    // ... your cache implementation
}

fn compress_blobs(cache: &BlobCache, blob_keys: &[String]) -> Result<(), Error> {
    let pool = BufferPool::new(16, LARGE_BUFFER_SIZE);
    let mut archive = ArchiveWriter::create("output.7z")?;
    
    // Configure for parallel compression
    let parallel_config = ParallelConfig::max_throughput()
        .with_parallel_input_streams(64);
    archive.configure_parallel(parallel_config, 6);
    
    // Process blobs in batches
    for blob_key in blob_keys {
        // Get from cache (already in memory)
        let data = cache.get(blob_key)?;
        
        let entry = ArchiveEntry::new_file(blob_key);
        archive.push_archive_entry_batched(
            entry,
            Some(std::io::Cursor::new(data)),
            Some(&pool),
        )?;
    }
    
    archive.finish()?;
    Ok(())
}
```

**Expected performance**:
- 50k blobs: 30-60 seconds (vs 10+ minutes before)
- Memory usage: ~100MB total
- CPU utilization: 80-95% (was 20-30% before due to allocation overhead)

## Troubleshooting

### Still Slow?

1. **Check I/O**: Are you reading from slow storage?
   - Use buffering: `BufReader::with_capacity(1MB, source)`
   - Consider prefetching data before compression

2. **Check CPU**: Is compression the bottleneck?
   - Use `ParallelConfig::max_throughput()`
   - Lower compression level (3-5 instead of 6-9)
   - Consider LZMA2 multi-threading

3. **Check Memory**: Running out of RAM?
   - Reduce buffer pool size
   - Process files in smaller batches
   - Use solid compression (shares dictionary)

### High Memory Usage?

1. **Reduce buffer pool size**:
   ```rust
   let pool = BufferPool::new(4, LARGE_BUFFER_SIZE); // 1MB instead of 4MB
   ```

2. **Use smaller buffers**:
   ```rust
   let pool = BufferPool::new(8, DEFAULT_BUFFER_SIZE); // 512KB instead of 2MB
   ```

3. **Process in batches**:
   ```rust
   for chunk in blobs.chunks(1000) {
       // Process 1000 files at a time
   }
   ```

## Summary

For optimal performance when compressing many small files:

1. ✅ **Always use BufferPool** for non-solid compression
2. ✅ **Use solid compression** when compression ratio is priority
3. ✅ **Configure parallelism** appropriately for your I/O
4. ✅ **Monitor memory usage** and adjust buffer pool accordingly
5. ✅ **Consider hybrid approach** for mixed workloads

The optimizations in sevenz-rust2 can provide **10-30x speedup** for workloads with many small files while **reducing memory usage by 99%**.

## Advanced Features (v0.21+)

### Zero-Copy Bytes Compression

For data already in memory (e.g., from Azure Blob cache, downloaded content), use `push_archive_entry_bytes()` to avoid unnecessary buffer copies:

```rust
use sevenz_rust2::*;

let mut archive = ArchiveWriter::create("output.7z")?;

// Data already in memory - no intermediate buffer needed
let blob_data: Vec<u8> = fetch_from_cache("my-blob");
let entry = ArchiveEntry::new_file("blob.bin");

// Zero-copy: writes directly from memory slice
archive.push_archive_entry_bytes(entry, &blob_data)?;

archive.finish()?;
```

**Performance Impact**:
- 20-40% faster than `push_archive_entry()` for in-memory data
- Zero intermediate buffer allocation
- Better cache locality

### Adaptive Compression Strategy

For mixed workloads with varying file sizes, use `push_archive_entry_adaptive()` to automatically select the optimal compression path:

```rust
use sevenz_rust2::*;
use sevenz_rust2::perf::FileSizeThresholds;

let mut archive = ArchiveWriter::create("output.7z")?;

// Auto-select best strategy based on file size:
// - Tiny (< 4KB): Stack buffer, minimal overhead
// - Small (4KB-64KB): Standard buffer
// - Medium/Large (> 64KB): Hyper buffer for max throughput

let tiny_config = b"debug=true";
archive.push_archive_entry_adaptive(
    ArchiveEntry::new_file("config.ini"),
    tiny_config,
    None, // Use default thresholds
)?;

let large_video = fetch_video_data();
archive.push_archive_entry_adaptive(
    ArchiveEntry::new_file("video.mp4"),
    &large_video,
    None,
)?;

archive.finish()?;
```

### Thread-Safe Buffer Pool

For multi-threaded compression scenarios, use `SyncBufferPool`:

```rust
use std::sync::Arc;
use std::thread;
use sevenz_rust2::perf::{SyncBufferPool, LARGE_BUFFER_SIZE};

let pool = Arc::new(SyncBufferPool::new(16, LARGE_BUFFER_SIZE));

let handles: Vec<_> = (0..4).map(|_| {
    let pool = Arc::clone(&pool);
    thread::spawn(move || {
        // Each thread can safely get/return buffers
        let mut buffer = pool.get();
        // Use buffer...
        buffer[0] = 42;
        // Auto-returned to pool when dropped
    })
}).collect();

for handle in handles {
    handle.join().unwrap();
}
```

### Compression Statistics Tracking

Monitor throughput and identify bottlenecks with `CompressionStats`:

```rust
use sevenz_rust2::perf::CompressionStats;
use std::time::Instant;

let mut stats = CompressionStats::new();

for file in files {
    let start = Instant::now();
    
    // Compress file...
    archive.push_archive_entry_bytes(entry, &file.data)?;
    
    stats.record_file(
        file.data.len() as u64,  // Original size
        0,                        // Compressed size (if tracked)
        start.elapsed(),
    );
}

println!("Files processed: {}", stats.file_count());
println!("Throughput: {:.2} MB/s", stats.throughput_mbps());
println!("Files/sec: {:.1}", stats.files_per_second());
println!("Compression ratio: {:.1}%", stats.compression_ratio() * 100.0);
println!("Small files: {} | Medium: {} | Large: {}", 
    stats.small_file_count(), 
    stats.medium_file_count(),
    stats.large_file_count()
);
```

### Batch Processing Configuration

For processing millions of files, use `BatchConfig` to get optimal settings:

```rust
use sevenz_rust2::perf::{BatchConfig, SyncBufferPool};

// For 1 million tiny config files
let config = BatchConfig::for_tiny_files(1_000_000);

println!("Recommended batch size: {}", config.batch_size);
println!("Recommended parallelism: {}", config.parallelism);
println!("Memory budget: {} MB", config.memory_budget_mb);
println!("Use solid compression: {}", config.use_solid_compression);

// Create buffer pool based on recommendations
let pool = SyncBufferPool::new(
    config.recommended_pool_size(),
    config.recommended_buffer_size(1024), // avg file size
);
```

### API Selection Guide

| File Size | Count | Best Method |
|-----------|-------|-------------|
| < 4KB | Any | `push_archive_entry_adaptive()` |
| < 4KB (in memory) | Any | `push_archive_entry_bytes()` |
| 4KB-64KB | < 1000 | `push_archive_entry()` |
| 4KB-64KB | > 1000 | `push_archive_entry_batched()` with BufferPool |
| > 64KB (in memory) | Any | `push_archive_entry_bytes()` |
| > 64KB (from disk) | Any | `push_archive_entry_fast()` |
| Mixed | > 10000 | Solid compression with parallel provider |
