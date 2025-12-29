//! End-to-end tests with 100MB test files.
//!
//! These tests verify complete compress -> encrypt -> decompress -> test -> validate workflows
//! with large 100MB files to ensure the library handles large data correctly.
//!
//! Tests cover:
//! - All supported compression algorithms
//! - Encryption with compression
//! - Multi-volume archives with compression and encryption
//! - Full integrity testing and data validation

#[cfg(all(feature = "compress", feature = "util"))]
use std::{
    hash::{Hash, Hasher},
    io::Cursor,
};

#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::encoder_options::*;
#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::*;
#[cfg(all(feature = "compress", feature = "util"))]
use tempfile::*;

/// Generate test data of specified size with varied patterns
#[cfg(all(feature = "compress", feature = "util"))]
fn generate_large_test_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    
    // Create patterns that are somewhat compressible but not trivial
    // Mix of repeated patterns and varied data
    let pattern_size = 1024;
    let num_patterns = size / pattern_size;
    
    for i in 0..num_patterns {
        // Create a pattern that changes per chunk
        let base = (i % 256) as u8;
        for j in 0..pattern_size {
            data.push(base.wrapping_add((j % 256) as u8));
        }
    }
    
    // Fill remaining bytes
    let remaining = size % pattern_size;
    for j in 0..remaining {
        data.push((j % 256) as u8);
    }
    
    data
}

/// Helper to hash data for comparison
#[cfg(all(feature = "compress", feature = "util"))]
fn hash(data: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// Complete end-to-end test: compress -> test integrity -> decompress -> validate
#[cfg(all(feature = "compress", feature = "util"))]
fn test_e2e_large_file(methods: &[EncoderConfiguration], test_name: &str) {
    println!("Starting test: {}", test_name);
    
    let data_size = 100 * 1024 * 1024; // 100MB
    let original_data = generate_large_test_data(data_size);
    println!("  Generated {} bytes of test data", original_data.len());
    
    // Step 1: Compress
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.set_content_methods(methods.to_vec());
        
        let entry = ArchiveEntry::new_file("large_test_file.bin");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect(&format!("{}: compression failed", test_name));
        writer.finish().expect(&format!("{}: finish failed", test_name));
    }
    println!("  Compressed to {} bytes", archive_bytes.len());
    
    // Step 2: Test integrity (equivalent to 7z -t)
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect(&format!("{}: failed to open archive", test_name));
        
        let result = reader
            .test_integrity()
            .expect(&format!("{}: integrity test failed", test_name));
        
        assert_eq!(result.files_tested, 1, "{}: wrong file count", test_name);
        assert_eq!(
            result.bytes_tested,
            original_data.len() as u64,
            "{}: wrong byte count",
            test_name
        );
    }
    println!("  Integrity test passed");
    
    // Step 3: Decompress
    let decompressed = {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect(&format!("{}: failed to open archive for decompression", test_name));
        
        reader
            .read_file("large_test_file.bin")
            .expect(&format!("{}: decompression failed", test_name))
    };
    println!("  Decompressed {} bytes", decompressed.len());
    
    // Step 4: Validate
    assert_eq!(
        original_data.len(),
        decompressed.len(),
        "{}: size mismatch",
        test_name
    );
    assert_eq!(
        hash(&original_data),
        hash(&decompressed),
        "{}: data mismatch",
        test_name
    );
    println!("  Validation passed - test complete!");
}

// === Core Compression Algorithms ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_100mb_copy() {
    test_e2e_large_file(&[EncoderMethod::COPY.into()], "E2E_100MB_COPY");
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_100mb_lzma() {
    test_e2e_large_file(&[EncoderMethod::LZMA.into()], "E2E_100MB_LZMA");
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_100mb_lzma2() {
    test_e2e_large_file(&[EncoderMethod::LZMA2.into()], "E2E_100MB_LZMA2");
}

// === Optional Compression Algorithms ===

#[cfg(all(feature = "compress", feature = "util", feature = "bzip2"))]
#[test]
fn test_e2e_100mb_bzip2() {
    test_e2e_large_file(&[EncoderMethod::BZIP2.into()], "E2E_100MB_BZIP2");
}

#[cfg(all(feature = "compress", feature = "util", feature = "ppmd"))]
#[test]
fn test_e2e_100mb_ppmd() {
    test_e2e_large_file(&[EncoderMethod::PPMD.into()], "E2E_100MB_PPMD");
}

#[cfg(all(feature = "compress", feature = "util", feature = "deflate"))]
#[test]
fn test_e2e_100mb_deflate() {
    test_e2e_large_file(&[EncoderMethod::DEFLATE.into()], "E2E_100MB_DEFLATE");
}

#[cfg(all(feature = "compress", feature = "util", feature = "brotli"))]
#[test]
fn test_e2e_100mb_brotli() {
    test_e2e_large_file(
        &[BrotliOptions::default().with_skippable_frame_size(0).into()],
        "E2E_100MB_BROTLI",
    );
}

#[cfg(all(feature = "compress", feature = "util", feature = "lz4"))]
#[test]
fn test_e2e_100mb_lz4() {
    test_e2e_large_file(
        &[Lz4Options::default().with_skippable_frame_size(0).into()],
        "E2E_100MB_LZ4",
    );
}

#[cfg(all(feature = "compress", feature = "util", feature = "zstd"))]
#[test]
fn test_e2e_100mb_zstd() {
    test_e2e_large_file(&[EncoderMethod::ZSTD.into()], "E2E_100MB_ZSTD");
}

// === Encrypted Archives ===

#[cfg(all(feature = "compress", feature = "util", feature = "aes256"))]
#[test]
fn test_e2e_100mb_encrypted_lzma2() {
    println!("Starting test: E2E_100MB_ENCRYPTED_LZMA2");
    
    let data_size = 100 * 1024 * 1024; // 100MB
    let original_data = generate_large_test_data(data_size);
    let password = Password::from("test_e2e_password_secure");
    println!("  Generated {} bytes of test data", original_data.len());
    
    // Step 1: Compress with encryption
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.set_content_methods(vec![
            AesEncoderOptions::new(password.clone()).into(),
            EncoderMethod::LZMA2.into(),
        ]);
        
        let entry = ArchiveEntry::new_file("encrypted_large_file.bin");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect("encrypted compression failed");
        writer.finish().expect("finish failed");
    }
    println!("  Compressed and encrypted to {} bytes", archive_bytes.len());
    
    // Step 2: Test integrity with password
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), password.clone())
                .expect("failed to open encrypted archive");
        
        let result = reader
            .test_integrity()
            .expect("encrypted integrity test failed");
        
        assert_eq!(result.files_tested, 1);
        assert_eq!(result.bytes_tested, original_data.len() as u64);
    }
    println!("  Integrity test passed");
    
    // Step 3: Decompress with password
    let decompressed = {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), password.clone())
                .expect("failed to open encrypted archive for decompression");
        
        reader
            .read_file("encrypted_large_file.bin")
            .expect("encrypted decompression failed")
    };
    println!("  Decompressed {} bytes", decompressed.len());
    
    // Step 4: Validate
    assert_eq!(original_data.len(), decompressed.len());
    assert_eq!(hash(&original_data), hash(&decompressed));
    println!("  Validation passed - test complete!");
}

#[cfg(all(feature = "compress", feature = "util", feature = "aes256"))]
#[test]
fn test_e2e_100mb_encrypted_lzma() {
    println!("Starting test: E2E_100MB_ENCRYPTED_LZMA");
    
    let data_size = 100 * 1024 * 1024; // 100MB
    let original_data = generate_large_test_data(data_size);
    let password = Password::from("test_e2e_password_lzma");
    println!("  Generated {} bytes of test data", original_data.len());
    
    // Step 1: Compress with encryption
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.set_content_methods(vec![
            AesEncoderOptions::new(password.clone()).into(),
            EncoderMethod::LZMA.into(),
        ]);
        
        let entry = ArchiveEntry::new_file("encrypted_large_file.bin");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect("encrypted compression failed");
        writer.finish().expect("finish failed");
    }
    println!("  Compressed and encrypted to {} bytes", archive_bytes.len());
    
    // Step 2: Test integrity
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), password.clone())
                .expect("failed to open encrypted archive");
        
        let result = reader
            .test_integrity()
            .expect("encrypted integrity test failed");
        
        assert_eq!(result.files_tested, 1);
    }
    println!("  Integrity test passed");
    
    // Step 3: Decompress and validate
    let decompressed = {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), password.clone())
                .expect("failed to open encrypted archive for decompression");
        
        reader
            .read_file("encrypted_large_file.bin")
            .expect("encrypted decompression failed")
    };
    
    assert_eq!(hash(&original_data), hash(&decompressed));
    println!("  Validation passed - test complete!");
}

// === Multi-Volume Archives ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_100mb_multi_volume_lzma2() {
    println!("Starting test: E2E_100MB_MULTI_VOLUME_LZMA2");
    
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("mv_lzma2");
    
    let data_size = 100 * 1024 * 1024; // 100MB
    let original_data = generate_large_test_data(data_size);
    println!("  Generated {} bytes of test data", original_data.len());
    
    // Step 1: Create multi-volume archive with 10MB volumes
    let volume_size = 10 * 1024 * 1024; // 10MB per volume
    let config = VolumeConfig::new(&base_path, volume_size);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();
    writer.set_content_methods(vec![EncoderMethod::LZMA2.into()]);
    
    let entry = ArchiveEntry::new_file("large_mv_file.bin");
    writer
        .push_archive_entry(entry, Some(original_data.as_slice()))
        .expect("multi-volume compression failed");
    
    let metadata = writer.finish_multi_volume().expect("finish multi-volume failed");
    println!(
        "  Created {} volumes: {:?}",
        metadata.volume_count, metadata.volume_paths
    );
    
    // Step 2: Test integrity using first volume
    let first_volume = temp_dir.path().join("mv_lzma2.7z.001");
    assert!(first_volume.exists(), "First volume should exist");
    
    {
        let mut reader = ArchiveReader::open(&first_volume, Password::empty())
            .expect("failed to open multi-volume archive");
        
        let result = reader
            .test_integrity()
            .expect("multi-volume integrity test failed");
        
        assert_eq!(result.files_tested, 1);
        assert_eq!(result.bytes_tested, original_data.len() as u64);
    }
    println!("  Integrity test passed");
    
    // Step 3: Decompress from multi-volume archive
    let decompressed = {
        let mut reader = ArchiveReader::open(&first_volume, Password::empty())
            .expect("failed to open multi-volume archive for decompression");
        
        reader
            .read_file("large_mv_file.bin")
            .expect("multi-volume decompression failed")
    };
    println!("  Decompressed {} bytes", decompressed.len());
    
    // Step 4: Validate
    assert_eq!(original_data.len(), decompressed.len());
    assert_eq!(hash(&original_data), hash(&decompressed));
    println!("  Validation passed - test complete!");
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_e2e_100mb_multi_volume_lzma() {
    println!("Starting test: E2E_100MB_MULTI_VOLUME_LZMA");
    
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("mv_lzma");
    
    let data_size = 100 * 1024 * 1024; // 100MB
    let original_data = generate_large_test_data(data_size);
    println!("  Generated {} bytes of test data", original_data.len());
    
    // Step 1: Create multi-volume archive
    let volume_size = 10 * 1024 * 1024; // 10MB per volume
    let config = VolumeConfig::new(&base_path, volume_size);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();
    writer.set_content_methods(vec![EncoderMethod::LZMA.into()]);
    
    let entry = ArchiveEntry::new_file("large_mv_file.bin");
    writer
        .push_archive_entry(entry, Some(original_data.as_slice()))
        .expect("multi-volume compression failed");
    
    let metadata = writer.finish_multi_volume().expect("finish multi-volume failed");
    println!("  Created {} volumes", metadata.volume_count);
    
    // Step 2: Test integrity
    let first_volume = temp_dir.path().join("mv_lzma.7z.001");
    {
        let mut reader = ArchiveReader::open(&first_volume, Password::empty())
            .expect("failed to open multi-volume archive");
        
        let result = reader
            .test_integrity()
            .expect("multi-volume integrity test failed");
        
        assert_eq!(result.files_tested, 1);
    }
    println!("  Integrity test passed");
    
    // Step 3: Decompress and validate
    let decompressed = {
        let mut reader = ArchiveReader::open(&first_volume, Password::empty())
            .expect("failed to open multi-volume archive for decompression");
        
        reader
            .read_file("large_mv_file.bin")
            .expect("multi-volume decompression failed")
    };
    
    assert_eq!(hash(&original_data), hash(&decompressed));
    println!("  Validation passed - test complete!");
}

#[cfg(all(feature = "compress", feature = "util", feature = "aes256"))]
#[test]
fn test_e2e_100mb_multi_volume_encrypted_lzma2() {
    println!("Starting test: E2E_100MB_MULTI_VOLUME_ENCRYPTED_LZMA2");
    
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("mv_enc_lzma2");
    let password = Password::from("mv_encrypted_password");
    
    let data_size = 100 * 1024 * 1024; // 100MB
    let original_data = generate_large_test_data(data_size);
    println!("  Generated {} bytes of test data", original_data.len());
    
    // Step 1: Create encrypted multi-volume archive
    let volume_size = 10 * 1024 * 1024; // 10MB per volume
    let config = VolumeConfig::new(&base_path, volume_size);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();
    writer.set_content_methods(vec![
        AesEncoderOptions::new(password.clone()).into(),
        EncoderMethod::LZMA2.into(),
    ]);
    
    let entry = ArchiveEntry::new_file("encrypted_mv_file.bin");
    writer
        .push_archive_entry(entry, Some(original_data.as_slice()))
        .expect("encrypted multi-volume compression failed");
    
    let metadata = writer.finish_multi_volume().expect("finish multi-volume failed");
    println!("  Created {} encrypted volumes", metadata.volume_count);
    
    // Step 2: Test integrity with password
    let first_volume = temp_dir.path().join("mv_enc_lzma2.7z.001");
    {
        let mut reader = ArchiveReader::open(&first_volume, password.clone())
            .expect("failed to open encrypted multi-volume archive");
        
        let result = reader
            .test_integrity()
            .expect("encrypted multi-volume integrity test failed");
        
        assert_eq!(result.files_tested, 1);
        assert_eq!(result.bytes_tested, original_data.len() as u64);
    }
    println!("  Integrity test passed");
    
    // Step 3: Decompress with password
    let decompressed = {
        let mut reader = ArchiveReader::open(&first_volume, password.clone())
            .expect("failed to open encrypted multi-volume archive for decompression");
        
        reader
            .read_file("encrypted_mv_file.bin")
            .expect("encrypted multi-volume decompression failed")
    };
    println!("  Decompressed {} bytes", decompressed.len());
    
    // Step 4: Validate
    assert_eq!(original_data.len(), decompressed.len());
    assert_eq!(hash(&original_data), hash(&decompressed));
    println!("  Validation passed - test complete!");
}

#[cfg(all(feature = "compress", feature = "util", feature = "aes256"))]
#[test]
fn test_e2e_100mb_multi_volume_encrypted_lzma() {
    println!("Starting test: E2E_100MB_MULTI_VOLUME_ENCRYPTED_LZMA");
    
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("mv_enc_lzma");
    let password = Password::from("mv_encrypted_password_lzma");
    
    let data_size = 100 * 1024 * 1024; // 100MB
    let original_data = generate_large_test_data(data_size);
    println!("  Generated {} bytes of test data", original_data.len());
    
    // Step 1: Create encrypted multi-volume archive
    let volume_size = 10 * 1024 * 1024; // 10MB per volume
    let config = VolumeConfig::new(&base_path, volume_size);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();
    writer.set_content_methods(vec![
        AesEncoderOptions::new(password.clone()).into(),
        EncoderMethod::LZMA.into(),
    ]);
    
    let entry = ArchiveEntry::new_file("encrypted_mv_file.bin");
    writer
        .push_archive_entry(entry, Some(original_data.as_slice()))
        .expect("encrypted multi-volume compression failed");
    
    let metadata = writer.finish_multi_volume().expect("finish multi-volume failed");
    println!("  Created {} encrypted volumes", metadata.volume_count);
    
    // Step 2: Test integrity
    let first_volume = temp_dir.path().join("mv_enc_lzma.7z.001");
    {
        let mut reader = ArchiveReader::open(&first_volume, password.clone())
            .expect("failed to open encrypted multi-volume archive");
        
        let result = reader
            .test_integrity()
            .expect("encrypted multi-volume integrity test failed");
        
        assert_eq!(result.files_tested, 1);
    }
    println!("  Integrity test passed");
    
    // Step 3: Decompress and validate
    let decompressed = {
        let mut reader = ArchiveReader::open(&first_volume, password.clone())
            .expect("failed to open encrypted multi-volume archive for decompression");
        
        reader
            .read_file("encrypted_mv_file.bin")
            .expect("encrypted multi-volume decompression failed")
    };
    
    assert_eq!(hash(&original_data), hash(&decompressed));
    println!("  Validation passed - test complete!");
}
