//! Integration and end-to-end tests for parallel input stream optimizations.
//!
//! These tests verify:
//! 1. ParallelConfig presets and builder methods
//! 2. BufferConfig for high-performance I/O
//! 3. PrefetchQueue and PrefetchCallback for smart cache pre-population
//! 4. BackgroundReader for overlapping I/O with compression
//! 5. BufferedChunkReader for efficient chunk-based reading
//! 6. Integration with ArchiveWriter for parallel compression
//! 7. End-to-end compression/decompression with parallel settings

#[cfg(all(feature = "compress", feature = "util"))]
use std::{
    cell::RefCell,
    hash::{Hash, Hasher},
    io::{Cursor, Read},
    rc::Rc,
    thread,
};

#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::perf::*;
#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::*;

// ============================================================================
// Unit Tests - ParallelConfig
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_parallel_config_presets() {
    // Test max_throughput preset
    let max_config = ParallelConfig::max_throughput();
    assert!(max_config.threads >= 1);
    assert_eq!(max_config.chunk_size, LARGE_PARALLEL_CHUNK_SIZE);
    assert_eq!(max_config.buffer_size, HYPER_BUFFER_SIZE);

    // Test balanced preset
    let balanced_config = ParallelConfig::balanced();
    assert!(balanced_config.threads >= 1);
    assert_eq!(balanced_config.chunk_size, DEFAULT_PARALLEL_CHUNK_SIZE);
    assert_eq!(balanced_config.buffer_size, LARGE_BUFFER_SIZE);

    // Test low_latency preset
    let low_config = ParallelConfig::low_latency();
    assert!(low_config.threads >= 1);
    assert_eq!(low_config.chunk_size, SMALL_PARALLEL_CHUNK_SIZE);
    assert_eq!(low_config.buffer_size, DEFAULT_BUFFER_SIZE);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_parallel_config_builder_chain() {
    let config = ParallelConfig::new()
        .with_threads(8)
        .with_chunk_size(16 * 1024 * 1024)
        .with_buffer_size(2 * 1024 * 1024)
        .with_parallel_input_streams(4);

    assert_eq!(config.threads, 8);
    assert_eq!(config.chunk_size, 16 * 1024 * 1024);
    assert_eq!(config.buffer_size, 2 * 1024 * 1024);
    assert_eq!(config.parallel_input_streams, 4);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_parallel_config_boundary_clamping() {
    // Test minimum bounds
    let min_config = ParallelConfig::new()
        .with_threads(0)
        .with_chunk_size(0)
        .with_buffer_size(0)
        .with_parallel_input_streams(0);

    assert_eq!(min_config.threads, 1);
    assert_eq!(min_config.chunk_size, 64 * 1024); // Min 64KB
    assert!(min_config.buffer_size >= 4096); // Min 4KB
    assert_eq!(min_config.parallel_input_streams, 1);

    // Test maximum bounds
    let max_config = ParallelConfig::new()
        .with_threads(1000)
        .with_chunk_size(10 * 1024 * 1024 * 1024) // 10GB
        .with_parallel_input_streams(1000);

    assert_eq!(max_config.threads, 256);
    assert_eq!(max_config.chunk_size, 1024 * 1024 * 1024); // Max 1GB
    assert_eq!(max_config.parallel_input_streams, 64);
}

// ============================================================================
// Unit Tests - BufferConfig
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_buffer_config_presets() {
    let high_bandwidth = BufferConfig::high_bandwidth();
    assert_eq!(high_bandwidth.buffer_size, LARGE_BUFFER_SIZE);

    let low_memory = BufferConfig::low_memory();
    assert_eq!(low_memory.buffer_size, SMALL_BUFFER_SIZE);

    let default = BufferConfig::default();
    assert_eq!(default.buffer_size, DEFAULT_BUFFER_SIZE);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_buffer_size_constants() {
    assert_eq!(SMALL_BUFFER_SIZE, 4 * 1024);
    assert_eq!(DEFAULT_BUFFER_SIZE, 64 * 1024);
    assert_eq!(LARGE_BUFFER_SIZE, 256 * 1024);
    assert_eq!(XLARGE_BUFFER_SIZE, 1024 * 1024);
    assert_eq!(HYPER_BUFFER_SIZE, 4 * 1024 * 1024);
    assert_eq!(MAX_BUFFER_SIZE, 16 * 1024 * 1024);
}

// ============================================================================
// Unit Tests - PrefetchQueue
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_prefetch_queue_callback_invocation() {
    let invocation_count = Rc::new(RefCell::new(0));
    let count_clone = invocation_count.clone();

    let callback = FnPrefetchCallback::new(move |_hints: &[PrefetchHint<String>]| {
        *count_clone.borrow_mut() += 1;
    });

    let items: Vec<String> = (0..10).map(|i| format!("item{}", i)).collect();
    let mut queue = PrefetchQueue::new(items, 3, callback);

    // Initial callback on creation
    assert_eq!(*invocation_count.borrow(), 1);

    // Each next() should trigger callback
    for _ in 0..10 {
        queue.next();
    }

    // Should have 11 invocations: 1 initial + 10 next() calls
    // (last few calls won't have new hints but still call callback)
    assert!(*invocation_count.borrow() >= 10);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_prefetch_queue_empty() {
    let items: Vec<String> = vec![];
    let mut queue = PrefetchQueue::new(items, 5, NoopPrefetchCallback);

    assert_eq!(queue.remaining(), 0);
    assert_eq!(queue.total_count(), 0);
    assert!(queue.next().is_none());
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_prefetch_queue_single_item() {
    let received_hints: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let hints_clone = received_hints.clone();

    let callback = FnPrefetchCallback::new(move |hints: &[PrefetchHint<String>]| {
        for hint in hints {
            hints_clone.borrow_mut().push(hint.id.clone());
        }
    });

    let items = vec!["only_item".to_string()];
    let mut queue = PrefetchQueue::new(items, 5, callback);

    assert_eq!(queue.remaining(), 1);
    assert_eq!(received_hints.borrow().len(), 1);
    assert_eq!(received_hints.borrow()[0], "only_item");

    let item = queue.next().unwrap();
    assert_eq!(item, "only_item");
    assert!(queue.next().is_none());
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_prefetch_queue_lookahead_larger_than_items() {
    let received_hints: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let hints_clone = received_hints.clone();

    let callback = FnPrefetchCallback::new(move |hints: &[PrefetchHint<String>]| {
        *hints_clone.borrow_mut() = hints.len();
    });

    let items: Vec<String> = (0..5).map(|i| format!("item{}", i)).collect();
    let queue = PrefetchQueue::new(items, 100, callback);

    // Should only hint 5 items (all available)
    assert_eq!(*received_hints.borrow(), 5);
    assert_eq!(queue.remaining(), 5);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_prefetch_hint_with_size() {
    let hint = PrefetchHint::with_size("large_file.bin", 0, 1024 * 1024 * 100);

    assert_eq!(hint.id, "large_file.bin");
    assert_eq!(hint.lookahead_index, 0);
    assert_eq!(hint.estimated_size, Some(1024 * 1024 * 100));
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_prefetch_queue_extend_during_iteration() {
    let items = vec!["a".to_string(), "b".to_string()];
    let mut queue = PrefetchQueue::new(items, 10, NoopPrefetchCallback);

    // Get first item
    assert_eq!(queue.next(), Some("a".to_string()));

    // Extend while iterating
    queue.extend_items(vec!["c".to_string(), "d".to_string()]);

    // Should continue seamlessly
    assert_eq!(queue.next(), Some("b".to_string()));
    assert_eq!(queue.next(), Some("c".to_string()));
    assert_eq!(queue.next(), Some("d".to_string()));
    assert!(queue.next().is_none());
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_prefetch_queue_dynamic_lookahead() {
    let received_counts: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let counts_clone = received_counts.clone();

    let callback = FnPrefetchCallback::new(move |hints: &[PrefetchHint<String>]| {
        counts_clone.borrow_mut().push(hints.len());
    });

    let items: Vec<String> = (0..50).map(|i| format!("file{}.txt", i)).collect();
    let mut queue = PrefetchQueue::new(items, 5, callback);

    // Initially 5 hints
    assert_eq!(queue.lookahead_count(), 5);
    assert_eq!(*received_counts.borrow().last().unwrap(), 5);

    // Increase lookahead
    queue.set_lookahead_count(20);
    assert_eq!(queue.lookahead_count(), 20);
    assert_eq!(*received_counts.borrow().last().unwrap(), 20);

    // Decrease lookahead
    queue.set_lookahead_count(3);
    assert_eq!(queue.lookahead_count(), 3);
    assert_eq!(*received_counts.borrow().last().unwrap(), 3);
}

// ============================================================================
// Integration Tests - Buffered I/O
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_buffered_copy_various_sizes() {
    for size in [0, 1, 1000, 65536, 100000, 1024 * 1024] {
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let mut reader = Cursor::new(&data);
        let mut writer = Vec::new();

        let bytes = buffered_copy(&mut reader, &mut writer, DEFAULT_BUFFER_SIZE).unwrap();

        assert_eq!(bytes, size as u64, "Failed for size {}", size);
        assert_eq!(writer, data, "Data mismatch for size {}", size);
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_buffered_copy_with_crc_integrity() {
    let data: Vec<u8> = (0..100000).map(|i| (i % 256) as u8).collect();
    let expected_crc = crc32fast::hash(&data);

    let mut reader = Cursor::new(&data);
    let mut writer = Vec::new();

    let (bytes, crc) = buffered_copy_with_crc(&mut reader, &mut writer, LARGE_BUFFER_SIZE).unwrap();

    assert_eq!(bytes, data.len() as u64);
    assert_eq!(crc, expected_crc);
    assert_eq!(writer, data);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_buffered_copy_different_buffer_sizes() {
    let data: Vec<u8> = (0..100000).map(|i| (i % 256) as u8).collect();

    for buffer_size in [
        SMALL_BUFFER_SIZE,
        DEFAULT_BUFFER_SIZE,
        LARGE_BUFFER_SIZE,
        XLARGE_BUFFER_SIZE,
    ] {
        let mut reader = Cursor::new(&data);
        let mut writer = Vec::new();

        let bytes = buffered_copy(&mut reader, &mut writer, buffer_size).unwrap();

        assert_eq!(bytes, data.len() as u64);
        assert_eq!(writer, data);
    }
}

// ============================================================================
// Integration Tests - BackgroundReader
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_background_reader_empty_input() {
    let data: Vec<u8> = vec![];
    let mut reader = BackgroundReader::new(Cursor::new(data.clone()), 8192);

    let mut result = Vec::new();
    reader.read_to_end(&mut result).unwrap();

    assert_eq!(result, data);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_background_reader_small_input() {
    let data: Vec<u8> = vec![1, 2, 3, 4, 5];
    let mut reader = BackgroundReader::new(Cursor::new(data.clone()), 1024);

    let mut result = Vec::new();
    reader.read_to_end(&mut result).unwrap();

    assert_eq!(result, data);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_background_reader_large_input() {
    let data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect(); // 1MB
    let mut reader = BackgroundReader::new(Cursor::new(data.clone()), 32768);

    let mut result = Vec::new();
    reader.read_to_end(&mut result).unwrap();

    assert_eq!(result.len(), data.len());
    assert_eq!(result, data);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_background_reader_partial_reads() {
    let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    let mut reader = BackgroundReader::new(Cursor::new(data.clone()), 1024);

    // Read in small chunks
    let mut result = Vec::new();
    let mut buf = [0u8; 100];
    loop {
        let n = reader.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        result.extend_from_slice(&buf[..n]);
    }

    assert_eq!(result, data);
}

// ============================================================================
// Integration Tests - BufferedChunkReader
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_buffered_chunk_reader_exact_chunks() {
    let chunk_size = 1024;
    let data: Vec<u8> = (0..chunk_size * 5).map(|i| (i % 256) as u8).collect();
    let mut reader = BufferedChunkReader::new(Cursor::new(&data), chunk_size);

    let mut total = 0;
    let mut chunk_count = 0;
    loop {
        let chunk = reader.read_chunk().unwrap();
        if chunk.is_empty() {
            break;
        }
        total += chunk.len();
        chunk_count += 1;
    }

    assert_eq!(total, data.len());
    assert_eq!(chunk_count, 5);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_buffered_chunk_reader_uneven_chunks() {
    let chunk_size = 1000;
    let data: Vec<u8> = (0..3500).map(|i| (i % 256) as u8).collect(); // 3.5 chunks
    let mut reader = BufferedChunkReader::new(Cursor::new(&data), chunk_size);

    let mut total = 0;
    loop {
        let chunk = reader.read_chunk().unwrap();
        if chunk.is_empty() {
            break;
        }
        total += chunk.len();
    }

    assert_eq!(total, data.len());
    assert_eq!(reader.total_read(), data.len() as u64);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_buffered_chunk_reader_crc_matches() {
    let data: Vec<u8> = (0..50000).map(|i| (i % 256) as u8).collect();
    let expected_crc = crc32fast::hash(&data);

    let mut reader = BufferedChunkReader::new(Cursor::new(&data), 4096);

    loop {
        let chunk = reader.read_chunk().unwrap();
        if chunk.is_empty() {
            break;
        }
    }

    let crc = reader.finalize_crc();
    assert_eq!(crc, expected_crc);
}

// ============================================================================
// Integration Tests - ParallelPrefetchReader
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_parallel_prefetch_reader_multiple_sources() {
    let sources: Vec<Cursor<Vec<u8>>> = vec![
        Cursor::new((0..1000).map(|i| (i % 256) as u8).collect()),
        Cursor::new((0..2000).map(|i| ((i + 50) % 256) as u8).collect()),
        Cursor::new((0..500).map(|i| ((i + 100) % 256) as u8).collect()),
    ];

    let reader = ParallelPrefetchReader::new(sources);
    assert_eq!(reader.source_count(), 3);

    let sources = reader.into_sources();
    assert_eq!(sources.len(), 3);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_parallel_prefetch_reader_config() {
    let sources: Vec<Cursor<Vec<u8>>> = vec![
        Cursor::new(vec![1u8; 100]),
        Cursor::new(vec![2u8; 100]),
    ];

    let config = ParallelConfig::max_throughput();
    let reader = ParallelPrefetchReader::from_config(sources, &config);

    assert_eq!(reader.source_count(), 2);
}

// ============================================================================
// End-to-End Tests - Compression with Parallel Config
// ============================================================================

/// Helper to hash data for comparison
#[cfg(all(feature = "compress", feature = "util"))]
fn hash(data: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// Generate test data with varied patterns
#[cfg(all(feature = "compress", feature = "util"))]
fn generate_test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_compression_with_parallel_config() {
    let original_data = generate_test_data(1024 * 1024); // 1MB

    // Create archive with parallel configuration
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();

        // Configure for parallel compression
        let parallel_config = ParallelConfig::balanced();
        writer.configure_parallel(parallel_config, 6);

        let entry = ArchiveEntry::new_file("parallel_test.bin");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect("compression failed");
        writer.finish().expect("finish failed");
    }

    // Decompress and verify
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        let decompressed = reader
            .read_file("parallel_test.bin")
            .expect("decompression failed");

        assert_eq!(original_data.len(), decompressed.len());
        assert_eq!(hash(&original_data), hash(&decompressed));
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_compression_with_max_throughput_config() {
    let original_data = generate_test_data(512 * 1024); // 512KB

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.configure_parallel(ParallelConfig::max_throughput(), 5);

        let entry = ArchiveEntry::new_file("max_throughput_test.bin");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect("compression failed");
        writer.finish().expect("finish failed");
    }

    // Verify
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        let decompressed = reader
            .read_file("max_throughput_test.bin")
            .expect("decompression failed");

        assert_eq!(hash(&original_data), hash(&decompressed));
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_compression_with_fast_entry() {
    let original_data = generate_test_data(256 * 1024); // 256KB

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();

        let entry = ArchiveEntry::new_file("fast_entry_test.bin");
        writer
            .push_archive_entry_fast(entry, Some(original_data.as_slice()), HYPER_BUFFER_SIZE)
            .expect("fast compression failed");
        writer.finish().expect("finish failed");
    }

    // Verify
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        let decompressed = reader
            .read_file("fast_entry_test.bin")
            .expect("decompression failed");

        assert_eq!(hash(&original_data), hash(&decompressed));
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_multiple_files_with_parallel_config() {
    let files: Vec<(String, Vec<u8>)> = (0..10)
        .map(|i| {
            let name = format!("file_{}.bin", i);
            let data = generate_test_data(50000 + i * 10000);
            (name, data)
        })
        .collect();

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.configure_parallel(ParallelConfig::balanced(), 5);

        for (name, data) in &files {
            let entry = ArchiveEntry::new_file(name);
            writer
                .push_archive_entry(entry, Some(data.as_slice()))
                .expect("compression failed");
        }
        writer.finish().expect("finish failed");
    }

    // Verify all files
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        for (name, original_data) in &files {
            let decompressed = reader.read_file(name).expect("decompression failed");
            assert_eq!(
                hash(original_data),
                hash(&decompressed),
                "Data mismatch for {}",
                name
            );
        }
    }
}

// ============================================================================
// End-to-End Tests - PrefetchQueue with Simulated Cache
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_prefetch_queue_simulated_cache() {
    // Simulate a cache that pre-loads data based on hints
    let cache: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let cache_clone = cache.clone();

    let callback = FnPrefetchCallback::new(move |hints: &[PrefetchHint<String>]| {
        // "Pre-load" data into cache
        for hint in hints {
            if !cache_clone.borrow().contains(&hint.id) {
                cache_clone.borrow_mut().push(hint.id.clone());
            }
        }
    });

    // Simulate file list (like millions of files scenario)
    let files: Vec<String> = (0..100).map(|i| format!("blob_{:04}.dat", i)).collect();
    let mut queue = PrefetchQueue::new(files.clone(), 10, callback);

    // Verify initial cache has first 10 items
    assert!(cache.borrow().len() >= 10);

    // Process all files
    let mut processed = Vec::new();
    while let Some(file) = queue.next() {
        // Verify file was pre-cached before we requested it
        assert!(
            cache.borrow().contains(&file),
            "File {} was not pre-cached",
            file
        );
        processed.push(file);
    }

    assert_eq!(processed, files);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_prefetch_queue_incremental_discovery() {
    // Simulate incremental file discovery (like directory traversal)
    let processed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let processed_clone = processed.clone();

    let callback = FnPrefetchCallback::new(move |_hints: &[PrefetchHint<String>]| {
        // Callback received - would trigger cache pre-load in real scenario
    });

    // Start with initial batch
    let initial_files: Vec<String> = (0..5).map(|i| format!("initial_{}.txt", i)).collect();
    let mut queue = PrefetchQueue::new(initial_files, 10, callback);

    // Process first 3
    for _ in 0..3 {
        if let Some(file) = queue.next() {
            processed_clone.borrow_mut().push(file);
        }
    }

    // "Discover" more files
    let more_files: Vec<String> = (0..5).map(|i| format!("discovered_{}.txt", i)).collect();
    queue.extend_items(more_files);

    // Process remaining
    while let Some(file) = queue.next() {
        processed_clone.borrow_mut().push(file);
    }

    assert_eq!(processed.borrow().len(), 10);
}

// ============================================================================
// Stress Tests
// ============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_stress_prefetch_queue_large_lookahead() {
    // Test with large lookahead for millions-of-files scenario
    let items: Vec<String> = (0..1000).map(|i| format!("file_{:06}.bin", i)).collect();

    let hint_count: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let count_clone = hint_count.clone();

    let callback = FnPrefetchCallback::new(move |hints: &[PrefetchHint<String>]| {
        *count_clone.borrow_mut() = hints.len();
    });

    // Use large lookahead (simulating pre-fetching many files ahead)
    let mut queue = PrefetchQueue::new(items, 500, callback);

    // Initial hints should be 500
    assert_eq!(*hint_count.borrow(), 500);

    // Process all items
    let mut count = 0;
    while queue.next().is_some() {
        count += 1;
    }

    assert_eq!(count, 1000);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_stress_buffered_copy_large_data() {
    // 10MB test
    let data: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 256) as u8).collect();
    let mut reader = Cursor::new(&data);
    let mut writer = Vec::new();

    let bytes = buffered_copy(&mut reader, &mut writer, HYPER_BUFFER_SIZE).unwrap();

    assert_eq!(bytes, data.len() as u64);
    assert_eq!(writer, data);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_stress_background_reader_concurrent_reads() {
    // Test multiple background readers concurrently
    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || {
                let data: Vec<u8> = (0..100000).map(|j| ((i + j) % 256) as u8).collect();
                let mut reader = BackgroundReader::new(Cursor::new(data.clone()), 8192);

                let mut result = Vec::new();
                reader.read_to_end(&mut result).unwrap();

                assert_eq!(result, data);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
