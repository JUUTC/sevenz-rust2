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
//! # Example
//!
//! ```no_run
//! use sevenz_rust2::perf::{BufferConfig, DEFAULT_BUFFER_SIZE, LARGE_BUFFER_SIZE};
//!
//! // Use default buffer size (64KB)
//! let default_config = BufferConfig::default();
//!
//! // Use large buffers for high-bandwidth I/O
//! let fast_io_config = BufferConfig::new(LARGE_BUFFER_SIZE);
//!
//! // Use custom buffer size
//! let custom_config = BufferConfig::new(128 * 1024); // 128KB
//! ```

use std::io::{Read, Write};
use crc32fast::Hasher;

/// Small buffer size (4KB) - for memory-constrained environments
pub const SMALL_BUFFER_SIZE: usize = 4 * 1024;

/// Default buffer size (64KB) - good balance for most use cases
pub const DEFAULT_BUFFER_SIZE: usize = 64 * 1024;

/// Large buffer size (256KB) - for high-bandwidth I/O (SSDs, fast networks)
pub const LARGE_BUFFER_SIZE: usize = 256 * 1024;

/// Extra large buffer size (1MB) - for maximum throughput on very fast storage
pub const XLARGE_BUFFER_SIZE: usize = 1024 * 1024;

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
}
