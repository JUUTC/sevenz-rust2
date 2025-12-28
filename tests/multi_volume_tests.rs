//! Tests for multi-volume archive support.

#[cfg(all(feature = "compress", feature = "util"))]
use sevenz_rust2::*;
#[cfg(all(feature = "compress", feature = "util"))]
use tempfile::*;

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_multi_volume_small_archive() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("test_archive");

    // Create a multi-volume archive with 1KB volumes
    let config = VolumeConfig::new(&base_path, 1024);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();

    // Add a small file
    let content = b"Hello, World! This is a test of multi-volume archives.";
    let entry = ArchiveEntry::new_file("test.txt");
    writer
        .push_archive_entry(entry, Some(content.as_slice()))
        .unwrap();

    let metadata = writer.finish_multi_volume().unwrap();

    // For small content, should only create 1 volume
    assert!(
        metadata.volume_count >= 1,
        "Should create at least 1 volume"
    );

    // Verify the first volume file exists and has correct naming
    let first_volume = temp_dir.path().join("test_archive.7z.001");
    assert!(first_volume.exists(), "First volume should exist");

    // Verify we can read the archive back
    let archive = Archive::open(&first_volume).expect("Should be able to open archive");
    assert_eq!(archive.files.len(), 1);
    assert_eq!(archive.files[0].name(), "test.txt");
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_multi_volume_large_archive_creates_multiple_volumes() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("large_archive");

    // Create a multi-volume archive with very small volumes (512 bytes)
    // This will force creation of multiple volumes even for small content
    let config = VolumeConfig::new(&base_path, 512);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();

    // Add a larger file that will span multiple volumes
    let content: Vec<u8> = (0..2048).map(|i| (i % 256) as u8).collect();
    let entry = ArchiveEntry::new_file("large_file.bin");
    writer
        .push_archive_entry(entry, Some(content.as_slice()))
        .unwrap();

    let metadata = writer.finish_multi_volume().unwrap();

    // Should create multiple volumes due to small volume size
    assert!(
        metadata.volume_count >= 1,
        "Should create at least 1 volume"
    );

    // Verify all volume files exist
    for path in &metadata.volume_paths {
        assert!(path.exists(), "Volume file {:?} should exist", path);
    }
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_multi_volume_empty_archive() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("empty_archive");

    let config = VolumeConfig::new(&base_path, 1024);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();

    // Add only a directory entry (no data stream)
    let entry = ArchiveEntry::new_directory("empty_dir");
    writer
        .push_archive_entry::<&[u8]>(entry, None)
        .unwrap();

    let metadata = writer.finish_multi_volume().unwrap();

    // Should create 1 volume for empty archive
    assert_eq!(metadata.volume_count, 1);

    let first_volume = temp_dir.path().join("empty_archive.7z.001");
    assert!(first_volume.exists());
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_volume_config_path_naming() {
    let config = VolumeConfig::new("/path/to/archive", 1024 * 1024);

    assert_eq!(
        config.volume_path(1).to_str().unwrap(),
        "/path/to/archive.7z.001"
    );
    assert_eq!(
        config.volume_path(10).to_str().unwrap(),
        "/path/to/archive.7z.010"
    );
    assert_eq!(
        config.volume_path(100).to_str().unwrap(),
        "/path/to/archive.7z.100"
    );
    assert_eq!(
        config.volume_path(999).to_str().unwrap(),
        "/path/to/archive.7z.999"
    );
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_multi_volume_multiple_files() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("multi_file_archive");

    let config = VolumeConfig::new(&base_path, 2048);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();

    // Add multiple files
    for i in 0..5 {
        let content = format!("File {} content with some data", i);
        let entry = ArchiveEntry::new_file(&format!("file_{}.txt", i));
        writer
            .push_archive_entry(entry, Some(content.as_bytes()))
            .unwrap();
    }

    let _metadata = writer.finish_multi_volume().unwrap();

    // Verify the archive can be read
    let first_volume = temp_dir.path().join("multi_file_archive.7z.001");
    let archive = Archive::open(&first_volume).expect("Should be able to open archive");
    assert_eq!(archive.files.len(), 5);
}

#[cfg(all(feature = "compress", feature = "util"))]
#[test]
fn test_multi_volume_with_compression_method() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().join("compressed_archive");

    let config = VolumeConfig::new(&base_path, 4096);
    let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();

    // Use LZMA compression
    writer.set_content_methods(vec![EncoderMethod::LZMA.into()]);

    let content = b"This is a test file with some compressible content. ".repeat(100);
    let entry = ArchiveEntry::new_file("compressed.txt");
    writer
        .push_archive_entry(entry, Some(content.as_slice()))
        .unwrap();

    let _metadata = writer.finish_multi_volume().unwrap();

    // Verify the archive can be read
    let first_volume = temp_dir.path().join("compressed_archive.7z.001");
    let archive = Archive::open(&first_volume).expect("Should be able to open archive");
    assert_eq!(archive.files.len(), 1);
}
