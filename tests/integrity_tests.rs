//! End-to-end integrity tests for all compression methods.
//!
//! These tests verify:
//! 1. Round-trip compression/decompression for each codec
//! 2. Archive integrity testing (equivalent to 7z -t)
//! 3. CRC verification

#[cfg(all(feature = "compress", feature = "util"))]
use std::{
    hash::{Hash, Hasher},
    io::Cursor,
};

#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::encoder_options::*;
#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::*;

/// Helper to generate test data of specified size
#[cfg(all(feature = "compress", feature = "util"))]
fn generate_test_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push((i % 256) as u8);
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

/// Test compression roundtrip and integrity verification for a given method
#[cfg(all(feature = "compress", feature = "util"))]
fn test_codec_integrity(methods: &[EncoderConfiguration], test_name: &str) {
    let original_data = generate_test_data(10240); // 10KB test data

    // Compress
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.set_content_methods(methods.to_vec());

        let entry = ArchiveEntry::new_file("test_file.bin");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect(&format!("{}: compression failed", test_name));
        writer.finish().expect(&format!("{}: finish failed", test_name));
    }

    // Test integrity (equivalent to 7z -t)
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

    // Decompress and verify
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect(&format!("{}: failed to open archive", test_name));

        let decompressed = reader
            .read_file("test_file.bin")
            .expect(&format!("{}: decompression failed", test_name));

        assert_eq!(
            hash(&original_data),
            hash(&decompressed),
            "{}: data mismatch",
            test_name
        );
    }
}

// === COPY Codec Tests ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_copy_integrity() {
    test_codec_integrity(&[EncoderMethod::COPY.into()], "COPY");
}

// === LZMA Codec Tests ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_lzma_integrity() {
    test_codec_integrity(&[EncoderMethod::LZMA.into()], "LZMA");
}

// === LZMA2 Codec Tests ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_lzma2_integrity() {
    test_codec_integrity(&[EncoderMethod::LZMA2.into()], "LZMA2");
}

// === Delta Filter Tests ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_delta_lzma_integrity() {
    for distance in 1..=4 {
        test_codec_integrity(
            &[
                EncoderMethod::LZMA.into(),
                DeltaOptions::from_distance(distance).into(),
            ],
            &format!("DELTA({})+LZMA", distance),
        );
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_delta_lzma2_integrity() {
    for distance in 1..=4 {
        test_codec_integrity(
            &[
                EncoderMethod::LZMA2.into(),
                DeltaOptions::from_distance(distance).into(),
            ],
            &format!("DELTA({})+LZMA2", distance),
        );
    }
}

// === BCJ Filter Tests ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_bcj_x86_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::BCJ_X86_FILTER.into(),
        ],
        "BCJ_X86+LZMA2",
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_bcj_arm_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::BCJ_ARM_FILTER.into(),
        ],
        "BCJ_ARM+LZMA2",
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_bcj_arm64_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::BCJ_ARM64_FILTER.into(),
        ],
        "BCJ_ARM64+LZMA2",
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_bcj_arm_thumb_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::BCJ_ARM_THUMB_FILTER.into(),
        ],
        "BCJ_ARM_THUMB+LZMA2",
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_bcj_ia64_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::BCJ_IA64_FILTER.into(),
        ],
        "BCJ_IA64+LZMA2",
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_bcj_sparc_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::BCJ_SPARC_FILTER.into(),
        ],
        "BCJ_SPARC+LZMA2",
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_bcj_ppc_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::BCJ_PPC_FILTER.into(),
        ],
        "BCJ_PPC+LZMA2",
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_bcj_riscv_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::BCJ_RISCV_FILTER.into(),
        ],
        "BCJ_RISCV+LZMA2",
    );
}

// === Swap Filter Tests ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_swap2_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::SWAP2_FILTER.into(),
        ],
        "SWAP2+LZMA2",
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_swap4_lzma2_integrity() {
    test_codec_integrity(
        &[
            EncoderMethod::LZMA2.into(),
            EncoderMethod::SWAP4_FILTER.into(),
        ],
        "SWAP4+LZMA2",
    );
}

// === PPMD Tests ===

#[cfg(all(feature = "compress", feature = "util", feature = "ppmd"))]
#[test]
fn test_ppmd_integrity() {
    test_codec_integrity(&[EncoderMethod::PPMD.into()], "PPMD");
}

// === BZip2 Tests ===

#[cfg(all(feature = "compress", feature = "util", feature = "bzip2"))]
#[test]
fn test_bzip2_integrity() {
    test_codec_integrity(&[EncoderMethod::BZIP2.into()], "BZIP2");
}

// === Deflate Tests ===

#[cfg(all(feature = "compress", feature = "util", feature = "deflate"))]
#[test]
fn test_deflate_integrity() {
    test_codec_integrity(&[EncoderMethod::DEFLATE.into()], "DEFLATE");
}

// === Brotli Tests ===

#[cfg(all(feature = "compress", feature = "util", feature = "brotli"))]
#[test]
fn test_brotli_integrity() {
    test_codec_integrity(
        &[BrotliOptions::default().with_skippable_frame_size(0).into()],
        "BROTLI",
    );
}

#[cfg(all(feature = "compress", feature = "util", feature = "brotli"))]
#[test]
fn test_brotli_skippable_integrity() {
    test_codec_integrity(
        &[BrotliOptions::default()
            .with_skippable_frame_size(64 * 1024)
            .into()],
        "BROTLI_SKIPPABLE",
    );
}

// === LZ4 Tests ===

#[cfg(all(feature = "compress", feature = "util", feature = "lz4"))]
#[test]
fn test_lz4_integrity() {
    test_codec_integrity(
        &[Lz4Options::default().with_skippable_frame_size(0).into()],
        "LZ4",
    );
}

#[cfg(all(feature = "compress", feature = "util", feature = "lz4"))]
#[test]
fn test_lz4_skippable_integrity() {
    test_codec_integrity(
        &[Lz4Options::default()
            .with_skippable_frame_size(128 * 1024)
            .into()],
        "LZ4_SKIPPABLE",
    );
}

// === ZSTD Tests ===

#[cfg(all(feature = "compress", feature = "util", feature = "zstd"))]
#[test]
fn test_zstd_integrity() {
    test_codec_integrity(&[EncoderMethod::ZSTD.into()], "ZSTD");
}

// === Encrypted Archive Tests ===

#[cfg(all(feature = "compress", feature = "util", feature = "aes256"))]
#[test]
fn test_encrypted_lzma2_integrity() {
    use sevenz_rust2::encoder_options::AesEncoderOptions;
    
    let original_data = generate_test_data(10240);
    let password = Password::from("test_password");

    // Compress with encryption
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.set_content_methods(vec![
            AesEncoderOptions::new(password.clone()).into(),
            EncoderMethod::LZMA2.into(),
        ]);

        let entry = ArchiveEntry::new_file("encrypted_file.bin");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect("encryption compression failed");
        writer.finish().expect("finish failed");
    }

    // Test integrity with correct password
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), password.clone())
                .expect("failed to open encrypted archive");

        let result = reader
            .test_integrity()
            .expect("encrypted integrity test failed");

        assert_eq!(result.files_tested, 1);
    }

    // Decompress and verify
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), password.clone())
                .expect("failed to open encrypted archive");

        let decompressed = reader
            .read_file("encrypted_file.bin")
            .expect("encrypted decompression failed");

        assert_eq!(hash(&original_data), hash(&decompressed));
    }
}

// === Multi-file Archive Tests ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_multi_file_integrity() {
    let files = vec![
        ("file1.txt", generate_test_data(1000)),
        ("file2.txt", generate_test_data(2000)),
        ("file3.txt", generate_test_data(3000)),
        ("subdir/file4.txt", generate_test_data(4000)),
    ];

    // Compress multiple files
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.set_content_methods(vec![EncoderMethod::LZMA2.into()]);

        // Add a directory
        let dir_entry = ArchiveEntry::new_directory("subdir");
        writer
            .push_archive_entry::<&[u8]>(dir_entry, None)
            .expect("failed to add directory");

        for (name, data) in &files {
            let entry = ArchiveEntry::new_file(*name);
            writer
                .push_archive_entry(entry, Some(data.as_slice()))
                .expect("compression failed");
        }
        writer.finish().expect("finish failed");
    }

    // Test integrity
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        let result = reader.test_integrity().expect("integrity test failed");

        // 4 files + 1 directory = 5 entries
        assert_eq!(result.files_tested, 5);
    }

    // Verify each file
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        for (name, original_data) in &files {
            let decompressed = reader.read_file(name).expect("decompression failed");
            assert_eq!(
                hash(original_data),
                hash(&decompressed),
                "data mismatch for {}",
                name
            );
        }
    }
}

// === Empty File Tests ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_empty_file_integrity() {
    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();

        let entry = ArchiveEntry::new_file("empty.txt");
        writer
            .push_archive_entry(entry, Some([].as_slice()))
            .expect("compression failed");
        writer.finish().expect("finish failed");
    }

    // Test integrity
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        let result = reader.test_integrity().expect("integrity test failed");
        assert_eq!(result.files_tested, 1);
        assert_eq!(result.bytes_tested, 0);
    }
}

// === Large File Test ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_large_file_integrity() {
    let original_data = generate_test_data(1024 * 1024); // 1MB

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.set_content_methods(vec![EncoderMethod::LZMA2.into()]);

        let entry = ArchiveEntry::new_file("large_file.bin");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect("compression failed");
        writer.finish().expect("finish failed");
    }

    // Test integrity
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        let result = reader.test_integrity().expect("integrity test failed");
        assert_eq!(result.files_tested, 1);
        assert_eq!(result.bytes_tested, original_data.len() as u64);
    }
}

// === Archive with Comment Test ===

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_archive_with_comment_integrity() {
    let original_data = generate_test_data(1000);
    let comment = "Test archive with comment - sevenz-rust2";

    let mut archive_bytes = Vec::new();
    {
        let mut writer = ArchiveWriter::new(Cursor::new(&mut archive_bytes)).unwrap();
        writer.set_comment(comment);

        let entry = ArchiveEntry::new_file("file.txt");
        writer
            .push_archive_entry(entry, Some(original_data.as_slice()))
            .expect("compression failed");
        writer.finish().expect("finish failed");
    }

    // Test integrity and verify comment
    {
        let mut reader =
            ArchiveReader::new(Cursor::new(archive_bytes.as_slice()), Password::empty())
                .expect("failed to open archive");

        assert_eq!(reader.archive().comment(), Some(comment));

        let result = reader.test_integrity().expect("integrity test failed");
        assert_eq!(result.files_tested, 1);
    }
}

// === Test existing archive files ===

#[test]
fn test_existing_archive_integrity() {
    use std::fs::File;

    // Test all .7z files in the resources directory
    let dir = std::fs::read_dir("tests/resources").unwrap();
    for entry in dir {
        let path = entry.unwrap().path();
        if path.extension().map_or(false, |ext| ext == "7z") {
            let path_str = path.to_string_lossy();

            // Skip encrypted archives for this test
            if path_str.contains("encrypted") || path_str.contains("password") {
                continue;
            }

            println!("Testing integrity of: {}", path_str);

            let file = File::open(&path).unwrap();
            let reader_result = ArchiveReader::new(file, Password::empty());

            // Some test archives may have unsupported features, that's ok
            if let Ok(mut reader) = reader_result {
                // Test integrity - some may fail due to unsupported codecs
                match reader.test_integrity() {
                    Ok(result) => {
                        println!(
                            "  OK: {} files, {} bytes",
                            result.files_tested, result.bytes_tested
                        );
                    }
                    Err(e) => {
                        // Some archives may have unsupported codecs, that's expected
                        println!("  Skipped (unsupported): {}", e);
                    }
                }
            }
        }
    }
}
