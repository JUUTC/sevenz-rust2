//! Performance tuning utilities for sevenz-rust2.
//!
//! This module provides configuration options and utilities for optimizing
//! compression and decompression performance.
//!
//! # Buffer Sizes
//!
//! The library uses internal buffers for compression and decompression operations.
//! The default buffer size is 64KB, which provides a good balance between
//! performance and memory usage for most use cases.
//!
//! For high-bandwidth I/O (SSDs, NVMe drives, 10GbE+ networks), larger buffers
//! (256KB-1MB) can reduce syscall overhead and improve throughput.
//!
//! For memory-constrained environments or many concurrent operations,
//! smaller buffers (4KB-16KB) may be more appropriate.
//!
//! # Parallel Compression
//!
//! For maximum compression throughput with fast I/O (in-memory cache, fast SSDs,
//! high-speed networks), use [`ParallelConfig`] to configure parallel input streams
//! and compression threads.
//!
//! # Example
//!
//! ```no_run
//! use sevenz_rust2::perf::{BufferConfig, ParallelConfig, DEFAULT_BUFFER_SIZE, LARGE_BUFFER_SIZE};
//!
//! // Use default buffer size (64KB)
//! let default_config = BufferConfig::default();
//!
//! // Use large buffers for high-bandwidth I/O
//! let fast_io_config = BufferConfig::new(LARGE_BUFFER_SIZE);
//!
//! // Use custom buffer size
//! let custom_config = BufferConfig::new(128 * 1024); // 128KB
//!
//! // Configure parallel compression for maximum throughput
//! let parallel_config = ParallelConfig::max_throughput();
//! ```

use std::io::{Read, Write};
use std::num::NonZeroUsize;
use crc32fast::Hasher;

/// Small buffer size (4KB) - for memory-constrained environments
pub const SMALL_BUFFER_SIZE: usize = 4 * 1024;

/// Default buffer size (64KB) - good balance for most use cases
pub const DEFAULT_BUFFER_SIZE: usize = 64 * 1024;

/// Large buffer size (256KB) - for high-bandwidth I/O (SSDs, fast networks)
pub const LARGE_BUFFER_SIZE: usize = 256 * 1024;

/// Extra large buffer size (1MB) - for maximum throughput on very fast storage
pub const XLARGE_BUFFER_SIZE: usize = 1024 * 1024;

/// Hyper buffer size (4MB) - for extremely fast I/O (in-memory, fast networks)
pub const HYPER_BUFFER_SIZE: usize = 4 * 1024 * 1024;

/// Default chunk size for parallel compression (4MB)
pub const DEFAULT_PARALLEL_CHUNK_SIZE: u64 = 4 * 1024 * 1024;

/// Large chunk size for parallel compression (16MB) - better compression ratio
pub const LARGE_PARALLEL_CHUNK_SIZE: u64 = 16 * 1024 * 1024;

/// Small chunk size for parallel compression (1MB) - lower latency
pub const SMALL_PARALLEL_CHUNK_SIZE: u64 = 1024 * 1024;

/// Configuration for parallel compression and I/O operations.
///
/// This configuration allows fine-tuning of parallel operations for maximum
/// throughput when working with fast I/O subsystems (in-memory cache, NVMe,
/// high-speed networks).
///
/// # Example
///
/// ```no_run
/// use sevenz_rust2::perf::ParallelConfig;
///
/// // Maximum throughput for fast I/O
/// let config = ParallelConfig::max_throughput();
///
/// // Custom configuration
/// let config = ParallelConfig::new()
///     .with_threads(8)
///     .with_chunk_size(8 * 1024 * 1024)
///     .with_buffer_size(1024 * 1024);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ParallelConfig {
    /// Number of threads to use for parallel compression.
    /// Defaults to the number of available CPU cores.
    pub threads: u32,
    /// Size of each compression chunk in bytes.
    /// Larger chunks provide better compression but use more memory.
    pub chunk_size: u64,
    /// Size of I/O buffers in bytes.
    pub buffer_size: usize,
    /// Number of input streams to read in parallel (for solid compression).
    pub parallel_input_streams: u32,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        let threads = std::thread::available_parallelism()
            .unwrap_or(NonZeroUsize::new(1).unwrap())
            .get() as u32;
        Self {
            threads,
            chunk_size: DEFAULT_PARALLEL_CHUNK_SIZE,
            buffer_size: LARGE_BUFFER_SIZE,
            parallel_input_streams: threads.min(8),
        }
    }
}

impl ParallelConfig {
    /// Creates a new parallel configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a configuration optimized for maximum throughput.
    ///
    /// This is ideal for:
    /// - In-memory data sources (caches, buffers)
    /// - NVMe/SSD storage
    /// - High-speed networks (10GbE+)
    /// - Systems with many CPU cores
    pub fn max_throughput() -> Self {
        let threads = std::thread::available_parallelism()
            .unwrap_or(NonZeroUsize::new(1).unwrap())
            .get() as u32;
        Self {
            threads,
            chunk_size: LARGE_PARALLEL_CHUNK_SIZE,
            buffer_size: HYPER_BUFFER_SIZE,
            parallel_input_streams: threads,
        }
    }

    /// Creates a configuration optimized for low latency.
    ///
    /// Uses smaller chunks for faster initial output and lower memory usage.
    pub fn low_latency() -> Self {
        let threads = std::thread::available_parallelism()
            .unwrap_or(NonZeroUsize::new(1).unwrap())
            .get() as u32;
        Self {
            threads,
            chunk_size: SMALL_PARALLEL_CHUNK_SIZE,
            buffer_size: DEFAULT_BUFFER_SIZE,
            parallel_input_streams: threads.min(4),
        }
    }

    /// Creates a configuration optimized for balanced performance.
    pub fn balanced() -> Self {
        Self::default()
    }

    /// Sets the number of compression threads.
    ///
    /// The value is clamped between 1 and 256.
    pub fn with_threads(mut self, threads: u32) -> Self {
        self.threads = threads.clamp(1, 256);
        self
    }

    /// Sets the compression chunk size in bytes.
    ///
    /// Larger chunks provide better compression ratio but use more memory.
    /// The value is clamped between 64KB and 1GB.
    pub fn with_chunk_size(mut self, chunk_size: u64) -> Self {
        self.chunk_size = chunk_size.clamp(64 * 1024, 1024 * 1024 * 1024);
        self
    }

    /// Sets the I/O buffer size in bytes.
    ///
    /// Larger buffers reduce syscall overhead but use more memory.
    /// The value is clamped between 4KB and 16MB.
    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size.clamp(4096, 16 * 1024 * 1024);
        self
    }

    /// Sets the number of parallel input streams.
    ///
    /// This controls how many input files are read in parallel during
    /// solid compression. The value is clamped between 1 and 64.
    pub fn with_parallel_input_streams(mut self, streams: u32) -> Self {
        self.parallel_input_streams = streams.clamp(1, 64);
        self
    }

    /// Returns an LZMA2 options instance configured for this parallel configuration.
    #[cfg(feature = "compress")]
    pub fn to_lzma2_options(&self, level: u32) -> crate::encoder_options::Lzma2Options {
        crate::encoder_options::Lzma2Options::from_level_mt(level, self.threads, self.chunk_size)
    }
}

/// Configuration for buffer sizes used in compression/decompression.
#[derive(Debug, Clone, Copy)]
pub struct BufferConfig {
    /// The size of internal buffers used for I/O operations.
    pub buffer_size: usize,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_BUFFER_SIZE,
        }
    }
}

impl BufferConfig {
    /// Creates a new buffer configuration with the specified buffer size.
    ///
    /// The buffer size is clamped to a minimum of 4KB and maximum of 16MB.
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer_size: buffer_size.clamp(4096, 16 * 1024 * 1024),
        }
    }

    /// Creates a configuration optimized for high-bandwidth I/O.
    ///
    /// Uses 256KB buffers suitable for SSDs, NVMe drives, and fast networks.
    pub fn high_bandwidth() -> Self {
        Self::new(LARGE_BUFFER_SIZE)
    }

    /// Creates a configuration optimized for memory-constrained environments.
    ///
    /// Uses 4KB buffers to minimize memory usage.
    pub fn low_memory() -> Self {
        Self::new(SMALL_BUFFER_SIZE)
    }
}

/// A buffered copy utility that uses a specified buffer size.
///
/// This is more efficient than using small default buffers when copying
/// large amounts of data.
///
/// # Example
///
/// ```no_run
/// use std::io::Cursor;
/// use sevenz_rust2::perf::{buffered_copy, DEFAULT_BUFFER_SIZE};
///
/// let data = vec![0u8; 1000000];
/// let mut reader = Cursor::new(&data);
/// let mut writer = Vec::new();
///
/// buffered_copy(&mut reader, &mut writer, DEFAULT_BUFFER_SIZE).unwrap();
/// ```
#[inline]
pub fn buffered_copy<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    buffer_size: usize,
) -> std::io::Result<u64> {
    let mut buffer = vec![0u8; buffer_size];
    let mut total = 0u64;
    
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        writer.write_all(&buffer[..bytes_read])?;
        total += bytes_read as u64;
    }
    
    Ok(total)
}

/// A buffered copy utility with CRC32 calculation.
///
/// Returns the total bytes copied and the CRC32 checksum.
#[inline]
pub fn buffered_copy_with_crc<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    buffer_size: usize,
) -> std::io::Result<(u64, u32)> {
    let mut buffer = vec![0u8; buffer_size];
    let mut total = 0u64;
    let mut hasher = Hasher::new();
    
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        writer.write_all(&buffer[..bytes_read])?;
        total += bytes_read as u64;
    }
    
    Ok((total, hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_buffer_config_default() {
        let config = BufferConfig::default();
        assert_eq!(config.buffer_size, DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn test_buffer_config_clamp() {
        // Too small - should be clamped to 4KB
        let config = BufferConfig::new(100);
        assert_eq!(config.buffer_size, 4096);

        // Too large - should be clamped to 16MB
        let config = BufferConfig::new(100 * 1024 * 1024);
        assert_eq!(config.buffer_size, 16 * 1024 * 1024);
    }

    #[test]
    fn test_buffered_copy() {
        let data = vec![42u8; 100000];
        let mut reader = Cursor::new(&data);
        let mut writer = Vec::new();

        let bytes = buffered_copy(&mut reader, &mut writer, DEFAULT_BUFFER_SIZE).unwrap();
        
        assert_eq!(bytes, 100000);
        assert_eq!(writer, data);
    }

    #[test]
    fn test_buffered_copy_with_crc() {
        let data = vec![42u8; 100000];
        let mut reader = Cursor::new(&data);
        let mut writer = Vec::new();

        let (bytes, crc) = buffered_copy_with_crc(&mut reader, &mut writer, DEFAULT_BUFFER_SIZE).unwrap();
        
        assert_eq!(bytes, 100000);
        assert_eq!(writer, data);
        assert_eq!(crc, crc32fast::hash(&data));
    }

    #[test]
    fn test_parallel_config_default() {
        let config = ParallelConfig::default();
        assert!(config.threads >= 1);
        assert_eq!(config.chunk_size, DEFAULT_PARALLEL_CHUNK_SIZE);
        assert_eq!(config.buffer_size, LARGE_BUFFER_SIZE);
    }

    #[test]
    fn test_parallel_config_max_throughput() {
        let config = ParallelConfig::max_throughput();
        assert!(config.threads >= 1);
        assert_eq!(config.chunk_size, LARGE_PARALLEL_CHUNK_SIZE);
        assert_eq!(config.buffer_size, HYPER_BUFFER_SIZE);
    }

    #[test]
    fn test_parallel_config_low_latency() {
        let config = ParallelConfig::low_latency();
        assert!(config.threads >= 1);
        assert_eq!(config.chunk_size, SMALL_PARALLEL_CHUNK_SIZE);
        assert_eq!(config.buffer_size, DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn test_parallel_config_builder() {
        let config = ParallelConfig::new()
            .with_threads(4)
            .with_chunk_size(8 * 1024 * 1024)
            .with_buffer_size(512 * 1024)
            .with_parallel_input_streams(2);
        
        assert_eq!(config.threads, 4);
        assert_eq!(config.chunk_size, 8 * 1024 * 1024);
        assert_eq!(config.buffer_size, 512 * 1024);
        assert_eq!(config.parallel_input_streams, 2);
    }

    #[test]
    fn test_parallel_config_clamping() {
        // Test thread clamping
        let config = ParallelConfig::new().with_threads(0);
        assert_eq!(config.threads, 1);
        
        let config = ParallelConfig::new().with_threads(1000);
        assert_eq!(config.threads, 256);
        
        // Test chunk size clamping
        let config = ParallelConfig::new().with_chunk_size(100);
        assert_eq!(config.chunk_size, 64 * 1024);
        
        // Test parallel input streams clamping
        let config = ParallelConfig::new().with_parallel_input_streams(0);
        assert_eq!(config.parallel_input_streams, 1);
        
        let config = ParallelConfig::new().with_parallel_input_streams(100);
        assert_eq!(config.parallel_input_streams, 64);
    }
}
