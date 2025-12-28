//! Multi-volume archive support through segmented writing.
//!
//! This module provides the `SegmentedWriter` type that enables creating
//! multi-volume 7z archives that span multiple files.

use std::{
    fs::File,
    io::{self, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

/// Configuration for multi-volume archive creation.
#[derive(Debug, Clone)]
pub struct VolumeConfig {
    /// Maximum size of each volume in bytes.
    pub volume_size: u64,
    /// Base path for the archive (without extension).
    pub base_path: PathBuf,
}

impl VolumeConfig {
    /// Creates a new volume configuration.
    ///
    /// # Arguments
    /// * `base_path` - The base path for the archive files. Volume files will be named
    ///   as `base_path.7z.001`, `base_path.7z.002`, etc.
    /// * `volume_size` - Maximum size of each volume in bytes.
    pub fn new(base_path: impl AsRef<Path>, volume_size: u64) -> Self {
        Self {
            volume_size,
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Returns the path for a specific volume number.
    pub fn volume_path(&self, volume_number: u32) -> PathBuf {
        let filename = format!("{}.7z.{:03}", self.base_path.display(), volume_number);
        PathBuf::from(filename)
    }
}

/// A writer that segments output across multiple volume files.
///
/// `SegmentedWriter` automatically creates new volume files when the current
/// volume reaches its size limit, enabling creation of multi-volume archives.
pub struct SegmentedWriter {
    config: VolumeConfig,
    current_volume: u32,
    current_file: File,
    current_volume_bytes: u64,
    total_bytes: u64,
    volume_boundaries: Vec<u64>,
}

impl SegmentedWriter {
    /// Creates a new `SegmentedWriter` with the given configuration.
    ///
    /// # Arguments
    /// * `config` - The volume configuration specifying size limits and file paths.
    ///
    /// # Returns
    /// A new `SegmentedWriter` instance ready to write the first volume.
    pub fn new(config: VolumeConfig) -> io::Result<Self> {
        let first_volume_path = config.volume_path(1);
        if let Some(parent) = first_volume_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = File::create(&first_volume_path)?;

        Ok(Self {
            config,
            current_volume: 1,
            current_file: file,
            current_volume_bytes: 0,
            total_bytes: 0,
            volume_boundaries: vec![0],
        })
    }

    /// Returns the current volume number (1-indexed).
    pub fn current_volume(&self) -> u32 {
        self.current_volume
    }

    /// Returns the total number of bytes written across all volumes.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Returns the number of bytes written to the current volume.
    pub fn current_volume_bytes(&self) -> u64 {
        self.current_volume_bytes
    }

    /// Returns the boundaries (cumulative byte positions) for each volume.
    ///
    /// The first element is always 0. Each subsequent element represents
    /// the cumulative byte position at the start of that volume.
    pub fn volume_boundaries(&self) -> &[u64] {
        &self.volume_boundaries
    }

    /// Returns the volume configuration.
    pub fn config(&self) -> &VolumeConfig {
        &self.config
    }

    /// Switches to a new volume file.
    fn switch_to_next_volume(&mut self) -> io::Result<()> {
        self.current_file.flush()?;
        self.current_volume += 1;
        self.volume_boundaries.push(self.total_bytes);

        let next_volume_path = self.config.volume_path(self.current_volume);
        self.current_file = File::create(&next_volume_path)?;
        self.current_volume_bytes = 0;

        Ok(())
    }

    /// Returns a mutable reference to the first volume file for header patching.
    ///
    /// This is used during finalization to seek back and update the start header.
    pub fn get_first_volume_mut(&mut self) -> io::Result<File> {
        self.current_file.flush()?;
        let first_volume_path = self.config.volume_path(1);
        File::options().write(true).read(true).open(first_volume_path)
    }

    /// Finishes writing and returns metadata about the created volumes.
    pub fn finish(mut self) -> io::Result<VolumeMetadata> {
        self.current_file.flush()?;
        Ok(VolumeMetadata {
            volume_count: self.current_volume,
            volume_boundaries: self.volume_boundaries,
            total_bytes: self.total_bytes,
            volume_paths: (1..=self.current_volume)
                .map(|n| self.config.volume_path(n))
                .collect(),
        })
    }
}

impl Write for SegmentedWriter {
    /// Writes data to the segmented output.
    ///
    /// This method may perform a partial write when approaching volume boundaries.
    /// Callers should use `write_all()` to ensure all data is written, or handle
    /// partial writes appropriately by calling `write()` again with the remaining data.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let remaining_in_volume = self.config.volume_size.saturating_sub(self.current_volume_bytes);

        // If we've exceeded the volume size, switch to next volume
        if remaining_in_volume == 0 {
            self.switch_to_next_volume()?;
            return self.write(buf);
        }

        // Write as much as we can to the current volume
        let to_write = buf.len().min(remaining_in_volume as usize);
        let written = self.current_file.write(&buf[..to_write])?;

        self.current_volume_bytes += written as u64;
        self.total_bytes += written as u64;

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.current_file.flush()
    }
}

impl Seek for SegmentedWriter {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match pos {
            SeekFrom::Start(abs_pos) => {
                // Find which volume contains this position
                let target_volume = self
                    .volume_boundaries
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|&(_, boundary)| abs_pos >= *boundary)
                    .map(|(idx, _)| idx as u32 + 1)
                    .unwrap_or(1);

                if target_volume != self.current_volume {
                    // Need to switch to a different volume
                    self.current_file.flush()?;
                    let target_path = self.config.volume_path(target_volume);
                    self.current_file = File::options()
                        .read(true)
                        .write(true)
                        .open(target_path)?;
                    self.current_volume = target_volume;
                }

                let volume_start = self.volume_boundaries[(target_volume - 1) as usize];
                let offset_in_volume = abs_pos - volume_start;
                self.current_file.seek(SeekFrom::Start(offset_in_volume))?;
                self.current_volume_bytes = offset_in_volume;
                Ok(abs_pos)
            }
            SeekFrom::Current(offset) => {
                let current_abs = self.volume_boundaries[(self.current_volume - 1) as usize]
                    + self.current_volume_bytes;
                let new_abs = if offset >= 0 {
                    current_abs.saturating_add(offset as u64)
                } else {
                    current_abs.saturating_sub((-offset) as u64)
                };
                self.seek(SeekFrom::Start(new_abs))
            }
            SeekFrom::End(offset) => {
                // Seek from end - calculate position as end_pos + offset
                let end_pos = self.total_bytes;
                let new_pos = if offset >= 0 {
                    end_pos.saturating_add(offset as u64)
                } else {
                    end_pos.saturating_sub((-offset) as u64)
                };
                self.seek(SeekFrom::Start(new_pos))
            }
        }
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        let volume_start = self.volume_boundaries[(self.current_volume - 1) as usize];
        Ok(volume_start + self.current_volume_bytes)
    }
}

/// Metadata about created volumes after finishing the segmented write.
#[derive(Debug, Clone)]
pub struct VolumeMetadata {
    /// Total number of volumes created.
    pub volume_count: u32,
    /// Cumulative byte positions at the start of each volume.
    pub volume_boundaries: Vec<u64>,
    /// Total bytes written across all volumes.
    pub total_bytes: u64,
    /// Paths to all created volume files.
    pub volume_paths: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;

    #[test]
    fn test_single_volume() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().join("test");

        let config = VolumeConfig::new(&base_path, 1024 * 1024); // 1MB volumes
        let mut writer = SegmentedWriter::new(config).unwrap();

        let data = b"Hello, World!";
        writer.write_all(data).unwrap();
        let metadata = writer.finish().unwrap();

        assert_eq!(metadata.volume_count, 1);
        assert_eq!(metadata.total_bytes, data.len() as u64);

        let mut file = File::open(&metadata.volume_paths[0]).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        assert_eq!(content, data);
    }

    #[test]
    fn test_multiple_volumes() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().join("test");

        let config = VolumeConfig::new(&base_path, 10); // 10 bytes per volume
        let mut writer = SegmentedWriter::new(config).unwrap();

        let data = b"0123456789ABCDEFGHIJ"; // 20 bytes = 2 volumes
        writer.write_all(data).unwrap();
        let metadata = writer.finish().unwrap();

        assert_eq!(metadata.volume_count, 2);
        assert_eq!(metadata.total_bytes, 20);

        // Check first volume
        let mut file1 = File::open(&metadata.volume_paths[0]).unwrap();
        let mut content1 = Vec::new();
        file1.read_to_end(&mut content1).unwrap();
        assert_eq!(content1, b"0123456789");

        // Check second volume
        let mut file2 = File::open(&metadata.volume_paths[1]).unwrap();
        let mut content2 = Vec::new();
        file2.read_to_end(&mut content2).unwrap();
        assert_eq!(content2, b"ABCDEFGHIJ");
    }

    #[test]
    fn test_volume_naming() {
        let config = VolumeConfig::new("/path/to/archive", 1024);
        assert_eq!(
            config.volume_path(1),
            PathBuf::from("/path/to/archive.7z.001")
        );
        assert_eq!(
            config.volume_path(10),
            PathBuf::from("/path/to/archive.7z.010")
        );
        assert_eq!(
            config.volume_path(100),
            PathBuf::from("/path/to/archive.7z.100")
        );
    }

    #[test]
    fn test_seek_within_volume() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().join("test");

        let config = VolumeConfig::new(&base_path, 1024);
        let mut writer = SegmentedWriter::new(config).unwrap();

        writer.write_all(b"Hello, World!").unwrap();

        // Seek back to position 7
        writer.seek(SeekFrom::Start(7)).unwrap();
        assert_eq!(writer.stream_position().unwrap(), 7);
    }

    #[test]
    fn test_seek_across_volumes() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().join("test");

        let config = VolumeConfig::new(&base_path, 10); // 10 bytes per volume
        let mut writer = SegmentedWriter::new(config).unwrap();

        // Write 30 bytes across 3 volumes
        writer.write_all(b"0123456789ABCDEFGHIJ0123456789").unwrap();
        assert_eq!(writer.current_volume(), 3);
        assert_eq!(writer.total_bytes(), 30);

        // Seek back to position 0 (first volume)
        writer.seek(SeekFrom::Start(0)).unwrap();
        assert_eq!(writer.current_volume(), 1);
        assert_eq!(writer.stream_position().unwrap(), 0);

        // Write new data at position 0
        writer.write_all(b"XX").unwrap();

        // Seek to position 15 (second volume)
        writer.seek(SeekFrom::Start(15)).unwrap();
        assert_eq!(writer.current_volume(), 2);
        assert_eq!(writer.stream_position().unwrap(), 15);

        let metadata = writer.finish().unwrap();

        // Verify first volume was modified
        let mut file1 = File::open(&metadata.volume_paths[0]).unwrap();
        let mut content1 = Vec::new();
        file1.read_to_end(&mut content1).unwrap();
        assert_eq!(content1, b"XX23456789"); // First two bytes replaced
    }

    #[test]
    fn test_seek_from_end() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().join("test");

        let config = VolumeConfig::new(&base_path, 1024);
        let mut writer = SegmentedWriter::new(config).unwrap();

        writer.write_all(b"Hello, World!").unwrap();

        // Seek to end - 5 bytes
        writer.seek(SeekFrom::End(-5)).unwrap();
        assert_eq!(writer.stream_position().unwrap(), 8);

        // Seek to end
        writer.seek(SeekFrom::End(0)).unwrap();
        assert_eq!(writer.stream_position().unwrap(), 13);
    }
}
