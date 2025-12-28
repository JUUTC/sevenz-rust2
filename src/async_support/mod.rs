//! Async support for 7z archive operations using tokio.
//!
//! This module provides async-compatible wrappers for reading and writing 7z archives.
//! Enable with the `tokio` feature flag.
//!
//! # Architecture
//!
//! The 7z compression/decompression is inherently synchronous (using sync Read/Write traits).
//! This module provides async wrappers that:
//!
//! - Use `tokio::task::spawn_blocking` for heavy I/O operations
//! - Provide `AsyncReadBridge`/`AsyncWriteBridge` for bridging async streams (with caveats)
//!
//! # Performance Considerations
//!
//! - For large files, `spawn_blocking` is used to avoid blocking the async runtime
//! - The bridge types (`AsyncReadBridge`, `AsyncWriteBridge`) call `block_on` internally,
//!   which should only be used carefully within a tokio context
//!
//! # Example
//!
//! ```rust,ignore
//! use sevenz_rust2::async_support::AsyncArchiveReader;
//! use sevenz_rust2::Password;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let reader = AsyncArchiveReader::open("archive.7z", Password::empty()).await?;
//!     let entries = reader.entries();
//!     for entry in entries {
//!         println!("Entry: {}", entry.name());
//!     }
//!     Ok(())
//! }
//! ```

use std::io::{Read, Write};
use std::path::Path;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{ArchiveEntry, ArchiveReader, Error, Password};
#[cfg(feature = "compress")]
use crate::ArchiveWriter;

/// An async wrapper for reading 7z archives.
///
/// This provides an async-compatible interface to the underlying sync `ArchiveReader`.
/// File operations are performed using tokio's async file operations.
///
/// # Example
///
/// ```rust,ignore
/// let reader = AsyncArchiveReader::open("archive.7z", Password::empty()).await?;
/// for entry in reader.entries() {
///     println!("File: {}", entry.name());
/// }
/// ```
pub struct AsyncArchiveReader {
    inner: ArchiveReader<std::fs::File>,
}

impl AsyncArchiveReader {
    /// Opens a 7z archive file asynchronously.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the archive file
    /// * `password` - Password for encrypted archives (use `Password::empty()` for unencrypted)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let reader = AsyncArchiveReader::open("archive.7z", Password::empty()).await?;
    /// ```
    pub async fn open<P: AsRef<Path>>(path: P, password: Password) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        // Use spawn_blocking to perform the sync file open in a thread pool
        let inner: Result<ArchiveReader<std::fs::File>, Error> = tokio::task::spawn_blocking(move || ArchiveReader::open(&path, password))
            .await
            .map_err(|e| Error::other(format!("async task join error: {}", e)))?;
        Ok(Self { inner: inner? })
    }

    /// Returns a slice of all archive entries.
    pub fn entries(&self) -> &[ArchiveEntry] {
        &self.inner.archive().files
    }

    /// Returns the archive comment, if any.
    pub fn comment(&self) -> Option<&str> {
        self.inner.archive().comment()
    }
}

/// An async wrapper for writing 7z archives.
///
/// This provides an async-compatible interface for creating 7z archives.
///
/// # Example
///
/// ```rust,ignore
/// let mut writer = AsyncArchiveWriter::create("output.7z").await?;
/// writer.add_file("input.txt").await?;
/// writer.finish().await?;
/// ```
#[cfg(feature = "compress")]
pub struct AsyncArchiveWriter {
    inner: ArchiveWriter<std::fs::File>,
}

#[cfg(feature = "compress")]
impl AsyncArchiveWriter {
    /// Creates a new archive file asynchronously.
    ///
    /// # Arguments
    ///
    /// * `path` - Path for the new archive file
    pub async fn create<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let inner: Result<ArchiveWriter<std::fs::File>, Error> = tokio::task::spawn_blocking(move || ArchiveWriter::create(&path))
            .await
            .map_err(|e| Error::other(format!("async task join error: {}", e)))?;
        Ok(Self { inner: inner? })
    }

    /// Adds a file to the archive asynchronously.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to add
    pub async fn add_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        let path = path.as_ref().to_path_buf();
        
        // Read file content asynchronously
        let content = tokio::fs::read(&path)
            .await
            .map_err(|e| Error::io_msg(e, "failed to read file"))?;
        
        // Get file name
        let name = path
            .file_name()
            .ok_or_else(|| Error::other("invalid file path"))?
            .to_string_lossy()
            .to_string();
        
        // Create archive entry
        let entry = crate::ArchiveEntry::new_file(&name);
        
        // Add entry synchronously (would ideally be spawn_blocking but requires &mut self)
        self.inner.push_archive_entry(entry, Some(&content[..]))?;
        
        Ok(())
    }

    /// Adds data from an async reader as an archive entry.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the entry in the archive
    /// * `reader` - Async reader providing the data
    ///
    /// # Note
    ///
    /// This method buffers all data from the async reader before compression.
    /// For very large files, consider writing to a temp file and using `add_file`.
    pub async fn add_entry_from_async_reader<R: AsyncRead + Unpin>(
        &mut self,
        name: &str,
        mut reader: R,
    ) -> Result<(), Error> {
        // Read all data from async reader
        let mut data = Vec::new();
        reader
            .read_to_end(&mut data)
            .await
            .map_err(|e| Error::io_msg(e, "failed to read from async reader"))?;
        
        // Create and add entry
        let entry = crate::ArchiveEntry::new_file(name);
        self.inner.push_archive_entry(entry, Some(&data[..]))?;
        
        Ok(())
    }

    /// Sets the archive comment.
    pub fn set_comment(&mut self, comment: impl Into<String>) {
        self.inner.set_comment(comment);
    }

    /// Finishes writing the archive asynchronously.
    pub async fn finish(self) -> Result<(), Error> {
        // Finish is sync, returns the underlying file
        self.inner.finish().map_err(|e| Error::io_msg(e, "failed to finish archive"))?;
        Ok(())
    }
}

/// A bridge that wraps an async reader to provide a sync Read interface.
///
/// This allows using tokio async readers with the sync compression/decompression APIs.
/// The bridge blocks on async operations when read() is called.
///
/// # Warning
///
/// This should only be used within a tokio runtime context. Using it outside
/// a runtime will panic.
///
/// # Performance Warning
///
/// This bridge calls `block_on` for each read operation, which can cause
/// performance issues if the async reader has high latency or if used in
/// a context where blocking is problematic. For production use, consider:
///
/// - Pre-reading all data with `read_to_end().await` before compression
/// - Using `spawn_blocking` to run the entire compression on a blocking thread
pub struct AsyncReadBridge<R> {
    inner: R,
    runtime_handle: tokio::runtime::Handle,
}

impl<R: AsyncRead + Unpin> AsyncReadBridge<R> {
    /// Creates a new bridge wrapping an async reader.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a tokio runtime context.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            runtime_handle: tokio::runtime::Handle::current(),
        }
    }

    /// Creates a new bridge with an explicit runtime handle.
    pub fn with_handle(inner: R, handle: tokio::runtime::Handle) -> Self {
        Self {
            inner,
            runtime_handle: handle,
        }
    }
}

impl<R: AsyncRead + Unpin> Read for AsyncReadBridge<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.runtime_handle.block_on(async {
            let mut read_buf = tokio::io::ReadBuf::new(buf);
            tokio::io::AsyncReadExt::read_buf(&mut self.inner, &mut read_buf).await?;
            Ok(read_buf.filled().len())
        })
    }
}

/// A bridge that wraps an async writer to provide a sync Write interface.
pub struct AsyncWriteBridge<W> {
    inner: W,
    runtime_handle: tokio::runtime::Handle,
}

impl<W: AsyncWrite + Unpin> AsyncWriteBridge<W> {
    /// Creates a new bridge wrapping an async writer.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a tokio runtime context.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            runtime_handle: tokio::runtime::Handle::current(),
        }
    }

    /// Creates a new bridge with an explicit runtime handle.
    pub fn with_handle(inner: W, handle: tokio::runtime::Handle) -> Self {
        Self {
            inner,
            runtime_handle: handle,
        }
    }
}

impl<W: AsyncWrite + Unpin> Write for AsyncWriteBridge<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.runtime_handle
            .block_on(async { self.inner.write(buf).await })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.runtime_handle
            .block_on(async { self.inner.flush().await })
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_async_module_compiles() {
        // Just verify the module compiles
        assert!(true);
    }
}
