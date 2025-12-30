//! Integration tests for small file fast path optimization
//!
//! These tests verify that push_archive_entry_small() works correctly
//! and provides performance benefits for tiny files.

#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::*;
#[cfg(all(feature = "compress", feature = "util"))]
use std::io::Cursor;

// Test single small file
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_small_file_single() {
    let data = b"Hello, World!";
    let expected_crc = crc32fast::hash(data);
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        let entry = ArchiveEntry::new_file("hello.txt");
        writer.push_archive_entry_small(entry, Some(Cursor::new(data))).unwrap();
        writer.finish().unwrap();
    }
    
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    let decompressed = reader.read_file("hello.txt").unwrap();
    assert_eq!(decompressed.as_slice(), data);
    assert_eq!(crc32fast::hash(&decompressed), expected_crc);
}

// Test empty file with small path
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_small_file_empty() {
    let data: &[u8] = b"";
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        let entry = ArchiveEntry::new_file("empty.txt");
        writer.push_archive_entry_small(entry, Some(Cursor::new(data))).unwrap();
        writer.finish().unwrap();
    }
    
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    let decompressed = reader.read_file("empty.txt").unwrap();
    assert_eq!(decompressed.len(), 0);
}

// Test many tiny files (simulates config files, metadata, etc.)
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_small_file_many_tiny() {
    let files: Vec<(String, Vec<u8>)> = (0..100)
        .map(|i| {
            let name = format!("config_{:03}.ini", i);
            let data = format!("key{}=value{}", i, i).into_bytes();
            (name, data)
        })
        .collect();
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        for (name, data) in &files {
            let entry = ArchiveEntry::new_file(name);
            writer.push_archive_entry_small(entry, Some(Cursor::new(data))).unwrap();
        }
        writer.finish().unwrap();
    }
    
    // Verify random sample
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    for i in [0, 25, 50, 75, 99] {
        let (name, expected_data) = &files[i];
        let decompressed = reader.read_file(name).unwrap();
        assert_eq!(decompressed, *expected_data, "Mismatch for {}", name);
    }
}

// Test files at 4KB boundary (edge case for small path)
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_small_file_4kb_boundary() {
    let test_cases = vec![
        ("3kb.bin", vec![42u8; 3 * 1024]),
        ("4kb.bin", vec![43u8; 4 * 1024]),
        ("4kb_plus_1.bin", vec![44u8; 4 * 1024 + 1]),
    ];
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        for (name, data) in &test_cases {
            let entry = ArchiveEntry::new_file(name);
            writer.push_archive_entry_small(entry, Some(Cursor::new(data))).unwrap();
        }
        writer.finish().unwrap();
    }
    
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    for (name, expected_data) in &test_cases {
        let decompressed = reader.read_file(name).unwrap();
        assert_eq!(decompressed, *expected_data, "Mismatch for {}", name);
    }
}

// Test various small sizes
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_small_file_various_sizes() {
    let sizes = vec![1, 10, 100, 500, 1000, 2048, 3000, 4095];
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        for &size in &sizes {
            let name = format!("file_{}b.dat", size);
            let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
            let entry = ArchiveEntry::new_file(&name);
            writer.push_archive_entry_small(entry, Some(Cursor::new(&data))).unwrap();
        }
        writer.finish().unwrap();
    }
    
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    for &size in &sizes {
        let name = format!("file_{}b.dat", size);
        let expected_data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let decompressed = reader.read_file(&name).unwrap();
        assert_eq!(decompressed.len(), size, "Size mismatch for {}", name);
        assert_eq!(decompressed, expected_data, "Data mismatch for {}", name);
    }
}

// Test CRC integrity for small files
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_small_file_crc_integrity() {
    let test_data = vec![
        b"a".to_vec(),
        b"ab".to_vec(),
        b"abc".to_vec(),
        (0..1000).map(|i| (i % 256) as u8).collect::<Vec<u8>>(),
    ];
    
    for (i, data) in test_data.iter().enumerate() {
        let expected_crc = crc32fast::hash(data);
        
        let mut archive_bytes = Vec::new();
        {
            let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
            let entry = ArchiveEntry::new_file(&format!("test_{}.bin", i));
            writer.push_archive_entry_small(entry, Some(Cursor::new(data))).unwrap();
            writer.finish().unwrap();
        }
        
        let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
        let decompressed = reader.read_file(&format!("test_{}.bin", i)).unwrap();
        let actual_crc = crc32fast::hash(&decompressed);
        
        assert_eq!(actual_crc, expected_crc, "CRC mismatch for test case {}", i);
        assert_eq!(decompressed, *data, "Data mismatch for test case {}", i);
    }
}

// Stress test: Many tiny files rapidly
#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_small_file_stress() {
    let file_count = 500;
    let files: Vec<Vec<u8>> = (0..file_count)
        .map(|i| vec![(i % 256) as u8; 10 + (i % 20)])
        .collect();
    
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        for (i, data) in files.iter().enumerate() {
            let entry = ArchiveEntry::new_file(&format!("tiny_{:04}.dat", i));
            writer.push_archive_entry_small(entry, Some(Cursor::new(data))).unwrap();
        }
        writer.finish().unwrap();
    }
    
    // Spot check
    let mut reader = ArchiveReader::new(Cursor::new(&archive_bytes), Password::empty()).unwrap();
    for i in [0, 100, 250, 400, 499] {
        let name = format!("tiny_{:04}.dat", i);
        let decompressed = reader.read_file(&name).unwrap();
        assert_eq!(decompressed, files[i], "Mismatch for file {}", i);
    }
}
