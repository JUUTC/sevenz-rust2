//! Parallel input stream reader for high-performance compression.
//!
//! This module provides utilities for reading multiple input streams in parallel,
//! which is particularly useful when compressing data from fast I/O sources like
//! in-memory caches, NVMe drives, or high-speed networks.

use std::io::{self, Read};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use crc32fast::Hasher;

use crate::perf::{ParallelConfig, LARGE_BUFFER_SIZE, SMALL_BUFFER_SIZE, MAX_BUFFER_SIZE};

/// Minimum prefetch count
const MIN_PREFETCH_COUNT: usize = 1;

/// Maximum prefetch count
const MAX_PREFETCH_COUNT: usize = 16;

/// Default prefetch count
const DEFAULT_PREFETCH_COUNT: usize = 2;

/// A reader that prefetches data from multiple sources in parallel.
///
/// This is useful for maximizing I/O throughput when reading from multiple
/// files or streams simultaneously.
pub struct ParallelPrefetchReader<R> {
    sources: Vec<R>,
    buffer_size: usize,
    prefetch_count: usize,
}

impl<R: Read + Send + 'static> ParallelPrefetchReader<R> {
    /// Creates a new parallel prefetch reader with the given sources.
    pub fn new(sources: Vec<R>) -> Self {
        Self {
            sources,
            buffer_size: LARGE_BUFFER_SIZE,
            prefetch_count: DEFAULT_PREFETCH_COUNT,
        }
    }

    /// Sets the buffer size for reading.
    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size.clamp(SMALL_BUFFER_SIZE, MAX_BUFFER_SIZE);
        self
    }

    /// Sets the number of buffers to prefetch ahead.
    pub fn with_prefetch_count(mut self, count: usize) -> Self {
        self.prefetch_count = count.clamp(MIN_PREFETCH_COUNT, MAX_PREFETCH_COUNT);
        self
    }

    /// Creates a reader from a parallel configuration.
    pub fn from_config(sources: Vec<R>, config: &ParallelConfig) -> Self {
        Self {
            sources,
            buffer_size: config.buffer_size,
            prefetch_count: DEFAULT_PREFETCH_COUNT,
        }
    }

    /// Returns the number of sources.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Consumes the reader and returns the underlying sources.
    pub fn into_sources(self) -> Vec<R> {
        self.sources
    }
}

/// A buffered reader that reads data in large chunks for better performance.
///
/// This reader maintains an internal buffer and reads data in configurable
/// chunk sizes to reduce syscall overhead.
pub struct BufferedChunkReader<R: Read> {
    inner: R,
    buffer: Vec<u8>,
    buffer_size: usize,
    hasher: Hasher,
    total_read: u64,
}

impl<R: Read> BufferedChunkReader<R> {
    /// Creates a new buffered chunk reader.
    pub fn new(inner: R, buffer_size: usize) -> Self {
        Self {
            inner,
            buffer: vec![0u8; buffer_size],
            buffer_size,
            hasher: Hasher::new(),
            total_read: 0,
        }
    }

    /// Reads a full chunk of data, returning the number of bytes read.
    ///
    /// This method will read up to `buffer_size` bytes, making multiple
    /// underlying read calls if necessary to fill the buffer.
    pub fn read_chunk(&mut self) -> io::Result<&[u8]> {
        let mut total = 0;
        while total < self.buffer_size {
            let n = self.inner.read(&mut self.buffer[total..])?;
            if n == 0 {
                break;
            }
            total += n;
        }
        self.hasher.update(&self.buffer[..total]);
        self.total_read += total as u64;
        Ok(&self.buffer[..total])
    }

    /// Returns the total number of bytes read so far.
    pub fn total_read(&self) -> u64 {
        self.total_read
    }

    /// Finalizes the reader and returns the CRC32 checksum of all data read.
    pub fn finalize_crc(self) -> u32 {
        self.hasher.finalize()
    }
}

impl<R: Read> Read for BufferedChunkReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.hasher.update(&buf[..n]);
            self.total_read += n as u64;
        }
        Ok(n)
    }
}

/// A wrapper that reads from a source in the background using a separate thread.
///
/// This can help hide I/O latency by reading ahead while compression is happening.
pub struct BackgroundReader {
    receiver: Receiver<io::Result<Vec<u8>>>,
    _handle: JoinHandle<()>,
    current_buffer: Option<Vec<u8>>,
    position: usize,
}

impl BackgroundReader {
    /// Creates a new background reader that reads from the given source.
    pub fn new<R: Read + Send + 'static>(mut source: R, buffer_size: usize) -> Self {
        let (sender, receiver) = mpsc::channel();

        let handle = thread::spawn(move || {
            loop {
                let mut buffer = vec![0u8; buffer_size];
                let mut total = 0;
                
                // Fill the buffer
                while total < buffer_size {
                    match source.read(&mut buffer[total..]) {
                        Ok(0) => break,
                        Ok(n) => total += n,
                        Err(e) => {
                            let _ = sender.send(Err(e));
                            return;
                        }
                    }
                }

                if total == 0 {
                    // EOF - send empty buffer to signal completion
                    let _ = sender.send(Ok(Vec::new()));
                    return;
                }

                buffer.truncate(total);
                if sender.send(Ok(buffer)).is_err() {
                    return; // Receiver dropped
                }
            }
        });

        Self {
            receiver,
            _handle: handle,
            current_buffer: None,
            position: 0,
        }
    }
}

impl Read for BackgroundReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If we have data in the current buffer, use it
        if let Some(ref current) = self.current_buffer {
            if self.position < current.len() {
                let available = current.len() - self.position;
                let to_copy = available.min(buf.len());
                buf[..to_copy].copy_from_slice(&current[self.position..self.position + to_copy]);
                self.position += to_copy;
                return Ok(to_copy);
            }
        }

        // Need to get a new buffer
        match self.receiver.recv() {
            Ok(Ok(buffer)) => {
                if buffer.is_empty() {
                    // EOF
                    return Ok(0);
                }
                let to_copy = buffer.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&buffer[..to_copy]);
                if to_copy < buffer.len() {
                    self.current_buffer = Some(buffer);
                    self.position = to_copy;
                } else {
                    self.current_buffer = None;
                    self.position = 0;
                }
                Ok(to_copy)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // Channel closed unexpectedly
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Background reader thread terminated",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_buffered_chunk_reader() {
        let data = vec![42u8; 100000];
        let mut reader = BufferedChunkReader::new(Cursor::new(&data), 16384);

        let mut total = 0;
        loop {
            let chunk = reader.read_chunk().unwrap();
            if chunk.is_empty() {
                break;
            }
            total += chunk.len();
        }

        assert_eq!(total, data.len());
        assert_eq!(reader.total_read(), data.len() as u64);
    }

    #[test]
    fn test_buffered_chunk_reader_crc() {
        let data = vec![42u8; 100000];
        let mut reader = BufferedChunkReader::new(Cursor::new(&data), 16384);

        loop {
            let chunk = reader.read_chunk().unwrap();
            if chunk.is_empty() {
                break;
            }
        }

        let crc = reader.finalize_crc();
        assert_eq!(crc, crc32fast::hash(&data));
    }

    #[test]
    fn test_background_reader() {
        let data = vec![42u8; 100000];
        let mut reader = BackgroundReader::new(Cursor::new(data.clone()), 8192);

        let mut result = Vec::new();
        reader.read_to_end(&mut result).unwrap();

        assert_eq!(result, data);
    }

    #[test]
    fn test_parallel_prefetch_reader_creation() {
        let sources: Vec<Cursor<Vec<u8>>> = vec![
            Cursor::new(vec![1u8; 1000]),
            Cursor::new(vec![2u8; 1000]),
            Cursor::new(vec![3u8; 1000]),
        ];

        let reader = ParallelPrefetchReader::new(sources);
        assert_eq!(reader.source_count(), 3);
    }
}
