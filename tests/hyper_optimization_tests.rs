//! Tests for hyper-optimization features.
//!
//! These tests verify the new performance features:
//! 1. SyncBufferPool for thread-safe buffer pooling
//! 2. CompressionStats for throughput tracking
//! 3. push_archive_entry_bytes for zero-copy compression
//! 4. push_archive_entry_adaptive for automatic strategy selection
//! 5. FileSizeThresholds and BatchConfig for workload optimization

#[cfg(all(feature = "compress", feature = "util"))]
use std::io::Cursor;

#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::perf::*;
#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::*;

// =============================================================================
// Zero-Copy Bytes Compression Tests
// =============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_push_archive_entry_bytes_basic() {
    let data = b"Hello, World! This is test data for zero-copy compression.";

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        let entry = ArchiveEntry::new_file("test.txt");

        writer
            .push_archive_entry_bytes(entry, data)
            .expect("compression should succeed");
        writer.finish().expect("finish should succeed");
    }

    // Verify decompression
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        let decompressed = reader.read_file("test.txt").expect("should read file");
        assert_eq!(decompressed, data.to_vec());
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_push_archive_entry_bytes_large() {
    // Test with larger data (1MB)
    let data: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        let entry = ArchiveEntry::new_file("large.bin");

        writer
            .push_archive_entry_bytes(entry, &data)
            .expect("compression should succeed");
        writer.finish().expect("finish should succeed");
    }

    // Verify decompression
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        let decompressed = reader.read_file("large.bin").expect("should read file");
        assert_eq!(decompressed.len(), data.len());
        assert_eq!(decompressed, data);
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_push_archive_entry_bytes_empty() {
    let data: &[u8] = &[];

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        let entry = ArchiveEntry::new_file("empty.txt");

        writer
            .push_archive_entry_bytes(entry, data)
            .expect("compression should succeed");
        writer.finish().expect("finish should succeed");
    }

    // Verify archive was created
    assert!(!archive_bytes.is_empty());
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_push_archive_entry_bytes_many_files() {
    // Test with many small files (simulating blob cache scenario)
    let file_count = 100;
    let files: Vec<(String, Vec<u8>)> = (0..file_count)
        .map(|i| {
            let name = format!("blob_{:04}.bin", i);
            let data: Vec<u8> = (0..(100 + i * 10)).map(|j| ((i + j) % 256) as u8).collect();
            (name, data)
        })
        .collect();

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();

        for (name, data) in &files {
            let entry = ArchiveEntry::new_file(name);
            writer
                .push_archive_entry_bytes(entry, data)
                .expect("compression should succeed");
        }

        writer.finish().expect("finish should succeed");
    }

    // Verify all files decompress correctly
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        for (name, original_data) in &files {
            let decompressed = reader.read_file(name).expect("should read file");
            assert_eq!(&decompressed, original_data, "Mismatch for {}", name);
        }
    }
}

// =============================================================================
// Adaptive Compression Tests
// =============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_push_archive_entry_adaptive_tiny() {
    // Test with tiny file (< 4KB)
    let data = b"tiny config file";

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        let entry = ArchiveEntry::new_file("config.ini");

        writer
            .push_archive_entry_adaptive(entry, data, None)
            .expect("compression should succeed");
        writer.finish().expect("finish should succeed");
    }

    // Verify decompression
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        let decompressed = reader.read_file("config.ini").expect("should read file");
        assert_eq!(decompressed, data.to_vec());
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_push_archive_entry_adaptive_medium() {
    // Test with medium file (64KB - 4MB)
    let data: Vec<u8> = (0..500_000).map(|i| (i % 256) as u8).collect();

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        let entry = ArchiveEntry::new_file("medium.bin");

        writer
            .push_archive_entry_adaptive(entry, &data, None)
            .expect("compression should succeed");
        writer.finish().expect("finish should succeed");
    }

    // Verify decompression
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        let decompressed = reader.read_file("medium.bin").expect("should read file");
        assert_eq!(decompressed.len(), data.len());
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_push_archive_entry_adaptive_mixed() {
    // Test with mixed file sizes in one archive
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("tiny.txt", b"small".to_vec()),
        ("small.bin", vec![0u8; 10_000]),
        ("medium.bin", vec![0u8; 100_000]),
        ("large.bin", vec![0u8; 5_000_000]),
    ];

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();

        for (name, data) in &files {
            let entry = ArchiveEntry::new_file(*name);
            writer
                .push_archive_entry_adaptive(entry, data, None)
                .expect("compression should succeed");
        }

        writer.finish().expect("finish should succeed");
    }

    // Verify all files
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        for (name, original_data) in &files {
            let decompressed = reader.read_file(*name).expect("should read file");
            assert_eq!(decompressed.len(), original_data.len(), "Size mismatch for {}", name);
        }
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_push_archive_entry_adaptive_custom_thresholds() {
    // Test with custom thresholds
    let thresholds = FileSizeThresholds::for_tiny_files();
    let data: Vec<u8> = vec![0u8; 10_000]; // 10KB - would be "small" with custom thresholds

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        let entry = ArchiveEntry::new_file("custom.bin");

        writer
            .push_archive_entry_adaptive(entry, &data, Some(&thresholds))
            .expect("compression should succeed");
        writer.finish().expect("finish should succeed");
    }

    // Verify decompression
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        let decompressed = reader.read_file("custom.bin").expect("should read file");
        assert_eq!(decompressed.len(), data.len());
    }
}

// =============================================================================
// SyncBufferPool Tests
// =============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_sync_buffer_pool_integration() {
    use std::sync::Arc;
    use std::thread;

    let pool = Arc::new(SyncBufferPool::new(8, LARGE_BUFFER_SIZE));

    // Simulate multi-threaded buffer usage
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let pool = Arc::clone(&pool);
            thread::spawn(move || {
                for _ in 0..10 {
                    let mut buffer = pool.get();
                    // Simulate work
                    buffer[0] = i as u8;
                    assert_eq!(buffer[0], i as u8);
                    // Buffer returns to pool on drop
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Pool should have buffers returned
    assert!(pool.available_count() >= 1);
}

// =============================================================================
// CompressionStats Tests
// =============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_compression_stats_integration() {
    let mut stats = CompressionStats::new();

    // Simulate compressing files of various sizes
    // SMALL_BUFFER_SIZE = 4KB, XLARGE_BUFFER_SIZE = 1MB (1,048,576 bytes)
    let sizes = [100, 1000, 10_000, 100_000, 2_000_000]; // 2MB is clearly large

    for size in sizes {
        let compressed_size = size / 2; // Simulate 50% compression
        let duration = std::time::Duration::from_micros(size as u64 / 10);
        stats.record_file(size as u64, compressed_size as u64, duration);
    }

    assert_eq!(stats.file_count(), 5);
    assert!(stats.compression_ratio() > 0.0 && stats.compression_ratio() < 1.0);
    assert!(stats.throughput_mbps() > 0.0);

    // Check file categorization
    // Files: 100, 1000 < 4KB = small (2 files)
    // Files: 10_000, 100_000 < 1MB = medium (2 files)
    // Files: 2_000_000 > 1MB = large (1 file)
    assert_eq!(stats.small_file_count(), 2); // 100, 1000 bytes
    assert_eq!(stats.medium_file_count(), 2); // 10KB, 100KB
    assert_eq!(stats.large_file_count(), 1); // 2MB
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_compression_stats_merge_threads() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let stats_vec: Arc<Mutex<Vec<CompressionStats>>> = Arc::new(Mutex::new(Vec::new()));

    // Simulate stats collection from multiple threads
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let stats_vec = Arc::clone(&stats_vec);
            thread::spawn(move || {
                let mut local_stats = CompressionStats::new();
                for j in 0..10 {
                    local_stats.record_file(
                        (i * 1000 + j * 100) as u64,
                        (i * 500 + j * 50) as u64,
                        std::time::Duration::from_micros(10),
                    );
                }
                stats_vec.lock().unwrap().push(local_stats);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Merge all stats
    let mut final_stats = CompressionStats::new();
    for stats in stats_vec.lock().unwrap().iter() {
        final_stats.merge(stats);
    }

    assert_eq!(final_stats.file_count(), 40); // 4 threads × 10 files
}

// =============================================================================
// BatchConfig Tests
// =============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_batch_config_tiny_files_workload() {
    let config = BatchConfig::for_tiny_files(1_000_000);

    // Verify recommendations
    assert!(config.batch_size >= 100);
    assert!(config.parallelism >= 1);
    assert!(config.use_solid_compression);

    let pool_size = config.recommended_pool_size();
    assert!(pool_size >= 2 && pool_size <= 64);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_batch_config_mixed_workload() {
    let config = BatchConfig::for_mixed_workload(50_000);

    assert!(!config.use_solid_compression);
    assert!(config.memory_budget_mb >= 256);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_batch_config_large_files_workload() {
    let config = BatchConfig::for_large_files(100);

    assert!(config.batch_size <= 100);
    assert!(!config.use_solid_compression);
    assert!(config.memory_budget_mb >= 1024);
}

// =============================================================================
// FileSizeThresholds Tests
// =============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_file_size_thresholds_categorization() {
    let thresholds = FileSizeThresholds::default();

    assert_eq!(thresholds.categorize(100), FileCategory::Tiny);
    assert_eq!(thresholds.categorize(5_000), FileCategory::Small);
    assert_eq!(thresholds.categorize(100_000), FileCategory::Medium);
    assert_eq!(thresholds.categorize(10_000_000), FileCategory::Large);

    // Buffer size recommendations
    assert_eq!(thresholds.buffer_size_for(100), SMALL_BUFFER_SIZE);
    assert_eq!(thresholds.buffer_size_for(5_000), DEFAULT_BUFFER_SIZE);
    assert_eq!(thresholds.buffer_size_for(100_000), LARGE_BUFFER_SIZE);
    assert_eq!(thresholds.buffer_size_for(10_000_000), HYPER_BUFFER_SIZE);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_file_size_thresholds_presets() {
    let tiny_optimized = FileSizeThresholds::for_tiny_files();
    assert!(tiny_optimized.tiny_threshold > SMALL_BUFFER_SIZE as u64);

    let large_optimized = FileSizeThresholds::for_large_files();
    assert!(large_optimized.tiny_threshold < SMALL_BUFFER_SIZE as u64);
}

// =============================================================================
// End-to-End Performance Scenario Tests
// =============================================================================

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_millions_of_tiny_files_simulation() {
    // Simulate a scenario with many tiny files (scaled down for test)
    let file_count = 1000;
    let mut archive_bytes = Vec::new();

    let mut stats = CompressionStats::new();

    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();

        for i in 0..file_count {
            let name = format!("config_{:06}.ini", i);
            let data = format!("key{0}=value{0}\nother{0}=data{0}", i);

            let start = std::time::Instant::now();

            writer
                .push_archive_entry_bytes(ArchiveEntry::new_file(&name), data.as_bytes())
                .expect("compression should succeed");

            stats.record_file(
                data.len() as u64,
                0, // We don't track compressed size per-file in this scenario
                start.elapsed(),
            );
        }

        writer.finish().expect("finish should succeed");
    }

    // Verify stats
    assert_eq!(stats.file_count(), file_count);
    assert_eq!(stats.small_file_count(), file_count as u64); // All should be tiny

    // Verify archive is valid
    {
        let reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        // Spot check a few files
        for i in [0, 100, 500, 999] {
            let name = format!("config_{:06}.ini", i);
            let archive = reader.archive();
            let entry = archive.files.iter().find(|e| e.name == name);
            assert!(entry.is_some(), "Entry {} should exist", name);
        }
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_mixed_workload_simulation() {
    // Simulate a realistic mixed workload
    let files: Vec<(&str, Vec<u8>)> = vec![
        // Tiny config files
        ("app.config", b"debug=true".to_vec()),
        ("settings.json", b"{\"version\":1}".to_vec()),
        // Small assets
        ("icon.png", vec![0x89; 1000]),
        ("style.css", vec![0x2E; 5000]),
        // Medium documents
        ("document.pdf", vec![0x25; 50_000]),
        ("report.xlsx", vec![0x50; 100_000]),
        // Large media
        ("video.mp4", vec![0x00; 1_000_000]),
    ];

    let mut archive_bytes = Vec::new();

    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();

        for (name, data) in &files {
            writer
                .push_archive_entry_adaptive(ArchiveEntry::new_file(*name), data, None)
                .expect("compression should succeed");
        }

        writer.finish().expect("finish should succeed");
    }

    // Verify all files
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("should open archive");

        for (name, original_data) in &files {
            let decompressed = reader.read_file(*name).expect("should read file");
            assert_eq!(
                decompressed.len(),
                original_data.len(),
                "Size mismatch for {}",
                name
            );
        }
    }
}
