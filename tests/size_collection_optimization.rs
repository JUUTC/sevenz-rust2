//! Integration tests specifically for the optimized size collection path
//!
//! These tests verify that the stack-allocated fast path works correctly
//! for the common case of 1-4 coders.

#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::*;
#[cfg(all(feature = "compress", feature = "util"))]
use std::io::Cursor;

// Test single coder (most common: just LZMA2)
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_single_coder_optimization() {
    let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    let expected_crc = crc32fast::hash(&data);
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        // Default uses LZMA2 (single coder)
        let entry = ArchiveEntry::new_file("test.bin");
        writer.push_archive_entry(entry, Some(Cursor::new(&data))).unwrap();
        writer.finish().unwrap();
    }
    
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    let decompressed = reader.read_file("test.bin").unwrap();
    assert_eq!(crc32fast::hash(&decompressed), expected_crc);
}

// Test with many files to exercise optimization repeatedly
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_optimization_with_many_files() {
    let file_count = 100;
    let files: Vec<Vec<u8>> = (0..file_count)
        .map(|i| (0..1000 + (i * 10)).map(|j| ((i + j) % 256) as u8).collect())
        .collect();
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        for (i, data) in files.iter().enumerate() {
            let entry = ArchiveEntry::new_file(&format!("file_{:03}.bin", i));
            writer.push_archive_entry(entry, Some(Cursor::new(data))).unwrap();
        }
        writer.finish().unwrap();
    }
    
    // Verify a sample of files
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    for i in [0, 25, 50, 75, 99] {
        let name = format!("file_{:03}.bin", i);
        let decompressed = reader.read_file(&name).unwrap();
        assert_eq!(decompressed, files[i], "Mismatch for {}", name);
    }
}

// Stress test: exercise size collection with many small files
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_size_collection_stress() {
    let file_count = 500;
    let files: Vec<Vec<u8>> = (0..file_count)
        .map(|i| vec![(i % 256) as u8; 100 + (i % 50)])
        .collect();
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        for (i, data) in files.iter().enumerate() {
            let entry = ArchiveEntry::new_file(&format!("tiny_{:04}.dat", i));
            writer.push_archive_entry(entry, Some(Cursor::new(data))).unwrap();
        }
        writer.finish().unwrap();
    }
    
    // Spot check files
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    for i in [0, 100, 250, 400, 499] {
        let name = format!("tiny_{:04}.dat", i);
        let decompressed = reader.read_file(&name).unwrap();
        assert_eq!(decompressed, files[i], "Mismatch for file {}", i);
    }
}

// Test empty and single-byte files
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_size_collection_edge_cases() {
    let test_cases = vec![
        ("empty.bin", vec![]),
        ("single.bin", vec![42]),
        ("two.bin", vec![1, 2]),
    ];
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        for (name, data) in &test_cases {
            let entry = ArchiveEntry::new_file(name);
            writer.push_archive_entry(entry, Some(Cursor::new(data))).unwrap();
        }
        writer.finish().unwrap();
    }
    
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    for (name, expected_data) in &test_cases {
        let decompressed = reader.read_file(name).unwrap();
        assert_eq!(decompressed, *expected_data, "Edge case failed for {}", name);
    }
}
