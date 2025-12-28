//! Streaming archive writer for non-seekable outputs.
//!
//! This module provides `StreamingArchiveWriter` which allows writing 7z archives
//! to non-seekable outputs like network streams, pipes, or cloud storage APIs
//! that don't support seeking.
//!
//! # How it works
//!
//! The 7z format requires patching the start header at byte 0 after writing
//! all data. This normally requires a seekable output. `StreamingArchiveWriter`
//! works around this by:
//!
//! 1. Buffering all archive data to memory during compression
//! 2. Writing the complete archive (with correct header) to the output on finish
//!
//! # Memory Usage
//!
//! The entire compressed archive is buffered in memory. For large archives,
//! consider using `ArchiveWriter` directly with a seekable output instead.
//!
//! # Example
//!
//! ```no_run
//! use std::io::{Cursor, Read};
//! use sevenz_rust2::*;
//!
//! // Create a streaming writer to any Write output
//! let mut output = Vec::new();
//! let mut archive = StreamingArchiveWriter::new(&mut output)?;
//!
//! // Add files normally
//! let data = b"Hello, world!";
//! let entry = ArchiveEntry::new_file("hello.txt");
//! archive.push_archive_entry(entry, Some(&data[..]))?;
//!
//! // Finish writes the complete archive to the output
//! archive.finish()?;
//! # Ok::<(), sevenz_rust2::Error>(())
//! ```

use std::io::{Cursor, Read, Write};

use crate::{
    ArchiveEntry, Error,
    archive::EncoderConfiguration,
};

use super::{ArchiveWriter, SourceReader};
use super::progress::ProgressCallback;

/// A streaming archive writer that buffers to memory for non-seekable outputs.
///
/// This writer allows creating 7z archives when the output doesn't support seeking.
/// The archive is buffered in memory and written to the output when `finish()` is called.
///
/// # Memory Considerations
///
/// The entire compressed archive is held in memory. For large archives, consider:
/// - Using `ArchiveWriter` directly with a seekable output (file, Azure blob, etc.)
/// - Splitting into multiple smaller archives
/// - Using solid compression to reduce memory usage
pub struct StreamingArchiveWriter<W: Write> {
    /// The underlying non-seekable output
    output: W,
    /// The actual archive writer that writes to the buffer
    inner: Option<ArchiveWriter<Cursor<Vec<u8>>>>,
}

impl<W: Write> StreamingArchiveWriter<W> {
    /// Creates a new streaming archive writer.
    ///
    /// The archive will be buffered in memory and written to `output` when
    /// `finish()` is called.
    ///
    /// # Example
    /// ```no_run
    /// use sevenz_rust2::*;
    ///
    /// let mut output = Vec::new();
    /// let mut archive = StreamingArchiveWriter::new(&mut output)?;
    /// // Add entries...
    /// archive.finish()?;
    /// # Ok::<(), sevenz_rust2::Error>(())
    /// ```
    pub fn new(output: W) -> Result<Self, Error> {
        let buffer = Cursor::new(Vec::with_capacity(64 * 1024));
        let inner = ArchiveWriter::new(buffer)?;
        
        Ok(Self {
            output,
            inner: Some(inner),
        })
    }

    /// Sets the compression methods to use. Default is LZMA2.
    ///
    /// # Example
    /// ```no_run
    /// use sevenz_rust2::*;
    ///
    /// let mut output = Vec::new();
    /// let mut archive = StreamingArchiveWriter::new(&mut output)?;
    /// archive.set_content_methods(vec![
    ///     EncoderConfiguration::new(EncoderMethod::LZMA2),
    /// ]);
    /// # Ok::<(), sevenz_rust2::Error>(())
    /// ```
    pub fn set_content_methods(&mut self, methods: Vec<EncoderConfiguration>) -> &mut Self {
        if let Some(inner) = &mut self.inner {
            inner.set_content_methods(methods);
        }
        self
    }

    /// Sets whether to encrypt the header. Default is `true`.
    pub fn set_encrypt_header(&mut self, enabled: bool) -> &mut Self {
        if let Some(inner) = &mut self.inner {
            inner.set_encrypt_header(enabled);
        }
        self
    }

    /// Sets an optional archive comment.
    pub fn set_comment(&mut self, comment: impl Into<String>) -> &mut Self {
        if let Some(inner) = &mut self.inner {
            inner.set_comment(comment);
        }
        self
    }

    /// Clears the archive comment.
    pub fn clear_comment(&mut self) -> &mut Self {
        if let Some(inner) = &mut self.inner {
            inner.clear_comment();
        }
        self
    }

    /// Adds an archive entry with data from reader.
    ///
    /// # Example
    /// ```no_run
    /// use sevenz_rust2::*;
    ///
    /// let mut output = Vec::new();
    /// let mut archive = StreamingArchiveWriter::new(&mut output)?;
    ///
    /// let data = b"Hello, world!";
    /// let entry = ArchiveEntry::new_file("hello.txt");
    /// archive.push_archive_entry(entry, Some(&data[..]))?;
    ///
    /// archive.finish()?;
    /// # Ok::<(), sevenz_rust2::Error>(())
    /// ```
    pub fn push_archive_entry<R: Read>(
        &mut self,
        entry: ArchiveEntry,
        reader: Option<R>,
    ) -> Result<&ArchiveEntry, Error> {
        let inner = self.inner.as_mut().ok_or_else(|| {
            Error::other("StreamingArchiveWriter already finished")
        })?;
        inner.push_archive_entry(entry, reader)
    }

    /// Adds an archive entry with progress callback.
    ///
    /// See `ArchiveWriter::push_archive_entry_with_progress` for details.
    pub fn push_archive_entry_with_progress<R: Read, P: ProgressCallback>(
        &mut self,
        entry: ArchiveEntry,
        reader: Option<R>,
        progress: P,
        progress_interval: u64,
    ) -> Result<&ArchiveEntry, Error> {
        let inner = self.inner.as_mut().ok_or_else(|| {
            Error::other("StreamingArchiveWriter already finished")
        })?;
        inner.push_archive_entry_with_progress(entry, reader, progress, progress_interval)
    }

    /// Adds multiple entries with solid compression.
    ///
    /// See `ArchiveWriter::push_archive_entries` for details.
    pub fn push_archive_entries<R: Read>(
        &mut self,
        entries: Vec<ArchiveEntry>,
        readers: Vec<SourceReader<R>>,
    ) -> Result<&mut Self, Error> {
        let inner = self.inner.as_mut().ok_or_else(|| {
            Error::other("StreamingArchiveWriter already finished")
        })?;
        inner.push_archive_entries(entries, readers)?;
        Ok(self)
    }

    /// Finishes the compression and writes the complete archive to the output.
    ///
    /// This method:
    /// 1. Finalizes the archive (writes header, patches start header)
    /// 2. Writes the complete buffered archive to the output
    /// 3. Flushes the output
    ///
    /// # Returns
    /// The output writer is returned for further use.
    pub fn finish(mut self) -> Result<W, Error> {
        let inner = self.inner.take().ok_or_else(|| {
            Error::other("StreamingArchiveWriter already finished")
        })?;
        
        // Finish the inner writer, which returns the buffer
        let buffer = inner.finish()
            .map_err(|e| Error::io_msg(e, "Failed to finish archive"))?;
        
        // Write the complete archive to the output
        self.output.write_all(buffer.get_ref())
            .map_err(|e| Error::io_msg(e, "Failed to write archive to output"))?;
        
        self.output.flush()
            .map_err(|e| Error::io_msg(e, "Failed to flush output"))?;
        
        Ok(self.output)
    }

    /// Finishes the compression and returns the buffered archive data.
    ///
    /// This is useful when you want to handle the output yourself or
    /// need access to the raw archive bytes.
    pub fn finish_to_vec(mut self) -> Result<Vec<u8>, Error> {
        let inner = self.inner.take().ok_or_else(|| {
            Error::other("StreamingArchiveWriter already finished")
        })?;
        
        // Finish the inner writer, which returns the buffer
        let buffer = inner.finish()
            .map_err(|e| Error::io_msg(e, "Failed to finish archive"))?;
        
        Ok(buffer.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_writer_basic() {
        let mut output = Vec::new();
        let mut archive = StreamingArchiveWriter::new(&mut output).unwrap();
        
        let data = b"Hello, streaming world!";
        let entry = ArchiveEntry::new_file("test.txt");
        archive.push_archive_entry(entry, Some(&data[..])).unwrap();
        
        archive.finish().unwrap();
        
        // Verify the output is a valid 7z archive (starts with signature)
        assert!(output.len() > 32);
        assert_eq!(&output[0..6], &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]);
    }

    #[test]
    fn test_streaming_writer_with_comment() {
        let mut output = Vec::new();
        let mut archive = StreamingArchiveWriter::new(&mut output).unwrap();
        
        archive.set_comment("Test comment");
        
        let data = b"test data";
        let entry = ArchiveEntry::new_file("file.txt");
        archive.push_archive_entry(entry, Some(&data[..])).unwrap();
        
        archive.finish().unwrap();
        
        // Verify output is valid
        assert!(output.len() > 32);
    }

    #[test]
    fn test_streaming_writer_finish_to_vec() {
        let mut output = Vec::new();
        let mut archive = StreamingArchiveWriter::new(&mut output).unwrap();
        
        let data = b"test";
        let entry = ArchiveEntry::new_file("test.txt");
        archive.push_archive_entry(entry, Some(&data[..])).unwrap();
        
        let bytes = archive.finish_to_vec().unwrap();
        
        // Verify it's a valid 7z archive
        assert!(bytes.len() > 32);
        assert_eq!(&bytes[0..6], &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]);
    }
}
