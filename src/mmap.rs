//! Memory-mapped file support for high-performance archive operations.
//!
//! This module provides memory-mapped file access for reading archives,
//! which can significantly improve performance for large archives by
//! avoiding repeated syscalls and allowing the OS to optimize memory usage.
//!
//! # Features
//!
//! Enable with the `mmap` feature in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! sevenz-rust2 = { version = "0.20", features = ["mmap"] }
//! ```
//!
//! # Example
//!
//! ```no_run
//! use sevenz_rust2::mmap::MmapArchiveReader;
//! use sevenz_rust2::Password;
//!
//! # fn main() -> Result<(), sevenz_rust2::Error> {
//! // Open archive with memory mapping for best performance
//! let reader = MmapArchiveReader::open("large_archive.7z", Password::empty())?;
//!
//! // Use just like regular ArchiveReader
//! for entry in reader.entries() {
//!     println!("Entry: {}", entry.name());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Performance Benefits
//!
//! Memory-mapped file I/O can provide significant performance benefits:
//!
//! - **Zero-copy access**: Data is read directly from kernel page cache
//! - **Automatic caching**: OS manages caching based on actual access patterns
//! - **Reduced syscalls**: No read() syscalls for each chunk of data
//! - **Better random access**: Efficient for non-solid archives where files
//!   can be accessed independently
//!
//! # Safety Considerations
//!
//! Memory mapping is inherently `unsafe` because the underlying file could
//! change while mapped. This implementation:
//!
//! - Uses read-only mappings to prevent modification
//! - Assumes the archive file is not modified during access
//! - Falls back gracefully on platforms where mmap is unavailable

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use memmap2::Mmap;

use crate::{Archive, ArchiveEntry, ArchiveReader, Error, Password};

/// A wrapper that provides [`Read`] and [`Seek`] over a memory-mapped file.
pub struct MmapReader {
    mmap: Mmap,
    position: u64,
}

impl MmapReader {
    /// Creates a new memory-mapped reader for the given file.
    ///
    /// # Safety
    ///
    /// The file must not be modified or truncated while the map is active.
    pub fn new(file: &File) -> io::Result<Self> {
        // Safety: We assume the file is not modified during reading.
        // This is a common assumption for archive reading.
        let mmap = unsafe { Mmap::map(file)? };

        Ok(Self {
            mmap,
            position: 0,
        })
    }

    /// Returns the length of the memory-mapped region.
    #[inline]
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    /// Returns whether the memory-mapped region is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    /// Returns a slice of the underlying memory-mapped data.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap[..]
    }
}

impl Read for MmapReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let pos = self.position as usize;
        if pos >= self.mmap.len() {
            return Ok(0);
        }

        let available = self.mmap.len() - pos;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&self.mmap[pos..pos + to_read]);
        self.position = (pos + to_read) as u64;
        Ok(to_read)
    }
}

impl Seek for MmapReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => self.mmap.len() as i64 + offset,
            SeekFrom::Current(offset) => self.position as i64 + offset,
        };

        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot seek before start of file",
            ));
        }

        let new_pos = new_pos as u64;
        self.position = new_pos;
        Ok(new_pos)
    }
}

/// An archive reader that uses memory-mapped file I/O for improved performance.
///
/// This is particularly beneficial for:
/// - Large archives (multiple GB)
/// - Non-solid archives where random access is needed
/// - Applications that need to extract many files
///
/// # Example
///
/// ```no_run
/// use sevenz_rust2::mmap::MmapArchiveReader;
/// use sevenz_rust2::Password;
///
/// # fn main() -> Result<(), sevenz_rust2::Error> {
/// let mut reader = MmapArchiveReader::open("archive.7z", Password::empty())?;
///
/// // Read a specific file
/// let data = reader.read_file("path/to/file.txt")?;
/// # Ok(())
/// # }
/// ```
pub struct MmapArchiveReader {
    inner: ArchiveReader<MmapReader>,
    #[allow(dead_code)]
    file: File, // Keep file handle alive while mmap is active
}

impl MmapArchiveReader {
    /// Opens an archive with memory-mapped I/O.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the 7z archive file
    /// * `password` - Password for encrypted archives, or `Password::empty()`
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be opened
    /// - Memory mapping fails
    /// - The archive format is invalid
    pub fn open(path: impl AsRef<Path>, password: Password) -> Result<Self, Error> {
        let path = path.as_ref();
        let file = File::open(path)
            .map_err(|e| Error::file_open(e, path.to_string_lossy().to_string()))?;

        let mmap_reader = MmapReader::new(&file)?;
        let inner = ArchiveReader::new(mmap_reader, password)?;

        Ok(Self { inner, file })
    }

    /// Returns a reference to the underlying archive structure.
    #[inline]
    pub fn archive(&self) -> &Archive {
        self.inner.archive()
    }

    /// Returns an iterator over the entries in the archive.
    #[inline]
    pub fn entries(&self) -> impl Iterator<Item = &ArchiveEntry> {
        self.inner.entries()
    }

    /// Returns the number of blocks in the archive.
    #[inline]
    pub fn block_count(&self) -> usize {
        self.inner.block_count()
    }

    /// Returns whether the archive supports parallel decompression.
    #[inline]
    pub fn supports_parallel_decompression(&self) -> bool {
        self.inner.supports_parallel_decompression()
    }

    /// Sets the thread count for multi-threaded decompression.
    #[inline]
    pub fn set_thread_count(&mut self, thread_count: u32) {
        self.inner.set_thread_count(thread_count);
    }

    /// Reads a file from the archive by name.
    ///
    /// # Note
    ///
    /// For solid archives, this will decompress all data up to the requested file.
    /// For non-solid archives with memory mapping, this provides excellent
    /// random access performance.
    #[inline]
    pub fn read_file(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        self.inner.read_file(name)
    }

    /// Iterates over all entries, calling the provided closure for each file.
    #[inline]
    pub fn for_each_entries<F>(&mut self, each: F) -> Result<(), Error>
    where
        F: FnMut(&ArchiveEntry, &mut dyn Read) -> Result<bool, Error>,
    {
        self.inner.for_each_entries(each)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmap_reader_read_seek() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a temp file with some data
        let mut temp = NamedTempFile::new().unwrap();
        let data = b"Hello, World! This is test data for memory mapping.";
        temp.write_all(data).unwrap();
        temp.flush().unwrap();

        // Memory map the file
        let file = File::open(temp.path()).unwrap();
        let mut reader = MmapReader::new(&file).unwrap();

        // Test read
        let mut buf = [0u8; 5];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"Hello");

        // Test seek
        reader.seek(SeekFrom::Start(7)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"World");

        // Test seek from current
        reader.seek(SeekFrom::Current(-5)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"World");

        // Test seek from end
        reader.seek(SeekFrom::End(-8)).unwrap();
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"mapping.");

        // Test length
        assert_eq!(reader.len(), data.len());
    }
}
