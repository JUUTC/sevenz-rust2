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
//! # Buffer Pool
//!
//! For workloads processing many files (e.g., 50k+ small blobs), use [`BufferPool`]
//! to reuse buffers and eliminate allocation overhead.
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
use std::cell::RefCell;
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

/// Maximum buffer size (16MB) - upper limit for buffer sizes
pub const MAX_BUFFER_SIZE: usize = 16 * 1024 * 1024;

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

/// A buffer pool for reusing allocations across multiple compression operations.
///
/// When processing many files (e.g., 50k+ small blobs), allocating a new buffer
/// for each file creates significant overhead. A buffer pool eliminates this by
/// reusing buffers.
///
/// # Performance Impact
///
/// For 50k files with 64KB buffers:
/// - Without pool: 50,000 allocations × 64KB = 3.2GB allocated
/// - With pool (8 buffers): 8 allocations × 64KB = 512KB allocated
///
/// This reduces allocation overhead by ~99% and improves cache locality.
///
/// # Example
///
/// ```no_run
/// use sevenz_rust2::perf::{BufferPool, DEFAULT_BUFFER_SIZE};
///
/// // Create a pool with 8 buffers of 64KB each
/// let pool = BufferPool::new(8, DEFAULT_BUFFER_SIZE);
///
/// // Get a buffer from the pool
/// let mut buffer = pool.get();
/// assert_eq!(buffer.len(), DEFAULT_BUFFER_SIZE);
///
/// // Use the buffer...
/// buffer[0] = 42;
///
/// // When `buffer` is dropped, it automatically returns to the pool
/// drop(buffer);
///
/// // Get it again (reuses the same allocation)
/// let buffer2 = pool.get();
/// assert_eq!(buffer2.len(), DEFAULT_BUFFER_SIZE);
/// ```
#[derive(Clone)]
pub struct BufferPool {
    buffers: std::rc::Rc<RefCell<Vec<Vec<u8>>>>,
    buffer_size: usize,
    max_buffers: usize,
}

impl BufferPool {
    /// Creates a new buffer pool.
    ///
    /// # Arguments
    /// * `max_buffers` - Maximum number of buffers to keep in the pool (1-256)
    /// * `buffer_size` - Size of each buffer in bytes (clamped to 4KB-16MB)
    pub fn new(max_buffers: usize, buffer_size: usize) -> Self {
        let max_buffers = max_buffers.clamp(1, 256);
        let buffer_size = buffer_size.clamp(SMALL_BUFFER_SIZE, MAX_BUFFER_SIZE);
        
        Self {
            buffers: std::rc::Rc::new(RefCell::new(Vec::with_capacity(max_buffers))),
            buffer_size,
            max_buffers,
        }
    }

    /// Gets a buffer from the pool, or allocates a new one if the pool is empty.
    ///
    /// The returned buffer is automatically returned to the pool when dropped.
    pub fn get(&self) -> PooledBuffer {
        let mut buffers = self.buffers.borrow_mut();
        let buffer = buffers.pop().unwrap_or_else(|| vec![0u8; self.buffer_size]);
        
        PooledBuffer {
            buffer: Some(buffer),
            pool: self.buffers.clone(),
            max_buffers: self.max_buffers,
        }
    }

    /// Returns the buffer size used by this pool.
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Returns the current number of buffers in the pool.
    pub fn available_count(&self) -> usize {
        self.buffers.borrow().len()
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new(8, DEFAULT_BUFFER_SIZE)
    }
}

/// A buffer borrowed from a pool that automatically returns itself when dropped.
///
/// This type implements `Deref` and `DerefMut` to `Vec<u8>`, so it can be used
/// like a regular Vec<u8>.
pub struct PooledBuffer {
    buffer: Option<Vec<u8>>,
    pool: std::rc::Rc<RefCell<Vec<Vec<u8>>>>,
    max_buffers: usize,
}

impl PooledBuffer {
    /// Converts this pooled buffer into an owned Vec<u8>, preventing it from
    /// being returned to the pool.
    pub fn into_vec(mut self) -> Vec<u8> {
        self.buffer.take().unwrap()
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        self.buffer.as_ref().unwrap()
    }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer.as_mut().unwrap()
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            let mut buffers = self.pool.borrow_mut();
            // Only return to pool if not at capacity
            if buffers.len() < self.max_buffers {
                buffers.push(buffer);
            }
            // Otherwise, let it drop (deallocate)
        }
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

/// Default lookahead count for prefetch hints
pub const DEFAULT_LOOKAHEAD_COUNT: usize = 4;

/// Maximum lookahead count for prefetch hints.
/// Set high to support large-scale workloads with millions of files.
pub const MAX_LOOKAHEAD_COUNT: usize = 10000;

/// A hint about an upcoming blob/entry that will be requested.
///
/// This allows callers with smart caches to pre-populate data just-in-time,
/// minimizing memory usage while maximizing I/O throughput.
#[derive(Debug, Clone)]
pub struct PrefetchHint<T> {
    /// The identifier for the upcoming blob (e.g., file path, key, index).
    pub id: T,
    /// The index in the processing queue (0 = next, 1 = after next, etc.).
    pub lookahead_index: usize,
    /// Optional estimated size of the blob in bytes.
    pub estimated_size: Option<u64>,
}

impl<T> PrefetchHint<T> {
    /// Creates a new prefetch hint.
    pub fn new(id: T, lookahead_index: usize) -> Self {
        Self {
            id,
            lookahead_index,
            estimated_size: None,
        }
    }

    /// Creates a prefetch hint with an estimated size.
    pub fn with_size(id: T, lookahead_index: usize, estimated_size: u64) -> Self {
        Self {
            id,
            lookahead_index,
            estimated_size: Some(estimated_size),
        }
    }
}

/// A callback trait for receiving prefetch hints.
///
/// Implement this trait to receive notifications about which blobs will be
/// requested next, allowing you to pre-populate a smart cache just-in-time.
///
/// # Example
///
/// ```no_run
/// use sevenz_rust2::perf::{PrefetchCallback, PrefetchHint};
///
/// struct MyCache {
///     // Your cache implementation
/// }
///
/// impl PrefetchCallback<String> for MyCache {
///     fn on_prefetch_hint(&mut self, hints: &[PrefetchHint<String>]) {
///         for hint in hints {
///             println!("Pre-loading blob: {} (lookahead: {})", hint.id, hint.lookahead_index);
///             // Pre-populate your cache here
///         }
///     }
/// }
/// ```
pub trait PrefetchCallback<T> {
    /// Called with hints about upcoming blobs that will be requested.
    ///
    /// The hints are ordered by `lookahead_index`, with 0 being the next blob
    /// to be requested. This allows smart caches to pre-populate data
    /// just-in-time while minimizing memory usage.
    fn on_prefetch_hint(&mut self, hints: &[PrefetchHint<T>]);
}

/// A no-op prefetch callback that ignores all hints.
///
/// Use this when you don't need prefetch notifications.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopPrefetchCallback;

impl<T> PrefetchCallback<T> for NoopPrefetchCallback {
    fn on_prefetch_hint(&mut self, _hints: &[PrefetchHint<T>]) {
        // No-op
    }
}

/// A prefetch callback that uses a closure.
///
/// # Example
///
/// ```no_run
/// use sevenz_rust2::perf::{FnPrefetchCallback, PrefetchHint};
///
/// let callback = FnPrefetchCallback::new(|hints: &[PrefetchHint<String>]| {
///     for hint in hints {
///         println!("Upcoming blob: {}", hint.id);
///     }
/// });
/// ```
pub struct FnPrefetchCallback<T, F: FnMut(&[PrefetchHint<T>])> {
    callback: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F: FnMut(&[PrefetchHint<T>])> FnPrefetchCallback<T, F> {
    /// Creates a new callback from a closure.
    pub fn new(callback: F) -> Self {
        Self {
            callback,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, F: FnMut(&[PrefetchHint<T>])> PrefetchCallback<T> for FnPrefetchCallback<T, F> {
    fn on_prefetch_hint(&mut self, hints: &[PrefetchHint<T>]) {
        (self.callback)(hints);
    }
}

/// A queue that tracks items and provides lookahead hints for prefetching.
///
/// This is useful for compression workflows where you want to notify
/// callers about upcoming blobs so they can pre-populate their cache.
///
/// # Example
///
/// ```no_run
/// use sevenz_rust2::perf::{PrefetchQueue, FnPrefetchCallback, PrefetchHint};
///
/// let items = vec!["file1.txt", "file2.txt", "file3.txt", "file4.txt"];
/// let callback = FnPrefetchCallback::new(|hints: &[PrefetchHint<&str>]| {
///     for hint in hints {
///         println!("Upcoming: {} (index: {})", hint.id, hint.lookahead_index);
///     }
/// });
///
/// let mut queue = PrefetchQueue::new(items, 3, callback);
///
/// while let Some(item) = queue.next() {
///     println!("Processing: {}", item);
/// }
/// ```
pub struct PrefetchQueue<T: Clone, C: PrefetchCallback<T>> {
    items: Vec<T>,
    current_index: usize,
    lookahead_count: usize,
    callback: C,
    /// Reusable buffer for prefetch hints to avoid allocations on each call
    hints_buffer: Vec<PrefetchHint<T>>,
}

impl<T: Clone, C: PrefetchCallback<T>> PrefetchQueue<T, C> {
    /// Creates a new prefetch queue with the given items and lookahead count.
    ///
    /// # Arguments
    /// * `items` - The items to process in order.
    /// * `lookahead_count` - How many items ahead to hint (1 to MAX_LOOKAHEAD_COUNT).
    /// * `callback` - The callback to receive prefetch hints.
    ///
    /// # Order Knowledge
    /// The system knows the order as soon as you provide the `items` vector.
    /// For millions of files, you can provide them all upfront, or use
    /// `extend_items()` to add more items dynamically as you discover them.
    pub fn new(items: Vec<T>, lookahead_count: usize, callback: C) -> Self {
        let lookahead_count = lookahead_count.clamp(1, MAX_LOOKAHEAD_COUNT);
        let mut queue = Self {
            items,
            current_index: 0,
            lookahead_count,
            callback,
            hints_buffer: Vec::with_capacity(lookahead_count),
        };
        // Send initial prefetch hints
        queue.send_hints();
        queue
    }

    /// Returns the number of remaining items.
    pub fn remaining(&self) -> usize {
        self.items.len().saturating_sub(self.current_index)
    }

    /// Returns the current index in the queue.
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Returns the total number of items.
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Returns the current lookahead count.
    pub fn lookahead_count(&self) -> usize {
        self.lookahead_count
    }

    /// Sets a new lookahead count.
    ///
    /// The value is clamped between 1 and MAX_LOOKAHEAD_COUNT.
    /// This can be useful to dynamically adjust prefetch depth based on
    /// cache capacity or network conditions.
    pub fn set_lookahead_count(&mut self, count: usize) {
        self.lookahead_count = count.clamp(1, MAX_LOOKAHEAD_COUNT);
        // Ensure buffer has enough capacity
        if self.hints_buffer.capacity() < self.lookahead_count {
            self.hints_buffer.reserve(self.lookahead_count - self.hints_buffer.capacity());
        }
        // Re-send hints with the new lookahead count
        self.send_hints();
    }

    /// Extends the queue with additional items.
    ///
    /// This is useful when working with millions of files where you may
    /// discover items incrementally (e.g., from directory traversal) rather
    /// than having all items upfront.
    pub fn extend_items(&mut self, items: impl IntoIterator<Item = T>) {
        self.items.extend(items);
        // Re-send hints to include newly added items if within lookahead range
        self.send_hints();
    }

    /// Gets the next item and sends prefetch hints for upcoming items.
    ///
    /// Note: This method is intentionally not implementing `Iterator` because
    /// `PrefetchQueue` has additional state (callback) and behavior (sending hints)
    /// that don't fit the standard iterator pattern. Use `advance()` for clearer semantics.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<T> {
        self.advance()
    }

    /// Advances to the next item and sends prefetch hints for upcoming items.
    ///
    /// This is the preferred method for iterating through the queue.
    pub fn advance(&mut self) -> Option<T> {
        if self.current_index >= self.items.len() {
            return None;
        }

        let item = self.items[self.current_index].clone();
        self.current_index += 1;
        
        // Send hints for the next items
        self.send_hints();
        
        Some(item)
    }

    /// Advances to the next item by index, avoiding cloning.
    ///
    /// Returns the index of the current item (before advancing) and sends prefetch hints.
    /// Use `get(index)` to access the item by reference if cloning is expensive.
    ///
    /// # Example
    /// ```no_run
    /// use sevenz_rust2::perf::{PrefetchQueue, NoopPrefetchCallback};
    ///
    /// let items = vec!["large_item1".to_string(), "large_item2".to_string()];
    /// let mut queue = PrefetchQueue::new(items, 2, NoopPrefetchCallback);
    ///
    /// while let Some(index) = queue.advance_index() {
    ///     let item = queue.get(index).unwrap();
    ///     println!("Processing item at index {}: {}", index, item);
    /// }
    /// ```
    pub fn advance_index(&mut self) -> Option<usize> {
        if self.current_index >= self.items.len() {
            return None;
        }

        let index = self.current_index;
        self.current_index += 1;
        
        // Send hints for the next items
        self.send_hints();
        
        Some(index)
    }

    /// Gets a reference to the item at the given index.
    ///
    /// Use this with `advance_index()` to avoid cloning large items.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    /// Peeks at upcoming items without advancing the queue.
    ///
    /// Returns up to `count` upcoming items starting from the current position.
    pub fn peek_upcoming(&self, count: usize) -> &[T] {
        let start = self.current_index;
        let end = (start + count).min(self.items.len());
        &self.items[start..end]
    }

    fn send_hints(&mut self) {
        let start = self.current_index;
        let end = (start + self.lookahead_count).min(self.items.len());
        
        if start >= end {
            return;
        }

        // Reuse buffer to avoid allocation
        self.hints_buffer.clear();
        // Use with_capacity and reserve to minimize reallocations
        self.hints_buffer.reserve(end - start);
        for i in 0..(end - start) {
            // Clone only when creating hint - unavoidable for callback API
            // but we minimize by only cloning what's needed
            // Note: `i` is the relative position in the lookahead window (0 = next item)
            self.hints_buffer.push(PrefetchHint::new(self.items[start + i].clone(), i));
        }

        self.callback.on_prefetch_hint(&self.hints_buffer);
    }
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

    #[test]
    fn test_prefetch_hint() {
        let hint = PrefetchHint::new("test.txt", 0);
        assert_eq!(hint.id, "test.txt");
        assert_eq!(hint.lookahead_index, 0);
        assert!(hint.estimated_size.is_none());

        let hint_with_size = PrefetchHint::with_size("big.bin", 1, 1024 * 1024);
        assert_eq!(hint_with_size.id, "big.bin");
        assert_eq!(hint_with_size.lookahead_index, 1);
        assert_eq!(hint_with_size.estimated_size, Some(1024 * 1024));
    }

    #[test]
    fn test_prefetch_queue_basic() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let received_hints: Rc<RefCell<Vec<Vec<String>>>> = Rc::new(RefCell::new(Vec::new()));
        let hints_clone = received_hints.clone();

        let callback = FnPrefetchCallback::new(move |hints: &[PrefetchHint<String>]| {
            let ids: Vec<String> = hints.iter().map(|h| h.id.clone()).collect();
            hints_clone.borrow_mut().push(ids);
        });

        let items = vec![
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "file3.txt".to_string(),
            "file4.txt".to_string(),
        ];

        let mut queue = PrefetchQueue::new(items, 2, callback);
        
        // Initial hints should include first 2 items
        assert_eq!(received_hints.borrow().len(), 1);
        assert_eq!(received_hints.borrow()[0], vec!["file1.txt", "file2.txt"]);

        // Get first item
        let item = queue.next().unwrap();
        assert_eq!(item, "file1.txt");
        // Should now hint file2 and file3
        assert_eq!(received_hints.borrow()[1], vec!["file2.txt", "file3.txt"]);

        // Get second item
        let item = queue.next().unwrap();
        assert_eq!(item, "file2.txt");
        // Should now hint file3 and file4
        assert_eq!(received_hints.borrow()[2], vec!["file3.txt", "file4.txt"]);

        // Get third item
        let item = queue.next().unwrap();
        assert_eq!(item, "file3.txt");
        // Should now hint only file4
        assert_eq!(received_hints.borrow()[3], vec!["file4.txt"]);

        // Get fourth item
        let item = queue.next().unwrap();
        assert_eq!(item, "file4.txt");

        // No more items
        assert!(queue.next().is_none());
    }

    #[test]
    fn test_prefetch_queue_peek() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let queue = PrefetchQueue::new(items, 2, NoopPrefetchCallback);
        
        let upcoming = queue.peek_upcoming(3);
        assert_eq!(upcoming, &["a", "b", "c"]);
        
        assert_eq!(queue.remaining(), 3);
        assert_eq!(queue.current_index(), 0);
        assert_eq!(queue.total_count(), 3);
    }

    #[test]
    fn test_noop_prefetch_callback() {
        let mut callback = NoopPrefetchCallback;
        let hints = vec![PrefetchHint::new("test", 0)];
        // Should not panic
        callback.on_prefetch_hint(&hints);
    }

    #[test]
    fn test_prefetch_queue_large_lookahead() {
        // Test with a large lookahead count for millions-of-files scenarios
        let items: Vec<String> = (0..100).map(|i| format!("file{}.txt", i)).collect();
        
        use std::cell::RefCell;
        use std::rc::Rc;

        let received_count: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
        let count_clone = received_count.clone();

        let callback = FnPrefetchCallback::new(move |hints: &[PrefetchHint<String>]| {
            *count_clone.borrow_mut() = hints.len();
        });

        // Request 50 items lookahead
        let mut queue = PrefetchQueue::new(items, 50, callback);
        
        // Initial hints should include first 50 items
        assert_eq!(*received_count.borrow(), 50);
        assert_eq!(queue.lookahead_count(), 50);

        // Advance through some items
        for _ in 0..10 {
            queue.next();
        }
        
        // Still 50 items lookahead (items 10-59)
        assert_eq!(*received_count.borrow(), 50);
    }

    #[test]
    fn test_prefetch_queue_extend_items() {
        let items = vec!["a".to_string(), "b".to_string()];
        let mut queue = PrefetchQueue::new(items, 10, NoopPrefetchCallback);
        
        assert_eq!(queue.total_count(), 2);
        
        // Extend with more items
        queue.extend_items(vec!["c".to_string(), "d".to_string()]);
        assert_eq!(queue.total_count(), 4);
        
        // Verify all items are accessible
        assert_eq!(queue.next(), Some("a".to_string()));
        assert_eq!(queue.next(), Some("b".to_string()));
        assert_eq!(queue.next(), Some("c".to_string()));
        assert_eq!(queue.next(), Some("d".to_string()));
        assert_eq!(queue.next(), None);
    }

    #[test]
    fn test_prefetch_queue_set_lookahead() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let received_counts: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
        let counts_clone = received_counts.clone();

        let callback = FnPrefetchCallback::new(move |hints: &[PrefetchHint<String>]| {
            counts_clone.borrow_mut().push(hints.len());
        });

        let items: Vec<String> = (0..20).map(|i| format!("file{}.txt", i)).collect();
        let mut queue = PrefetchQueue::new(items, 5, callback);
        
        // Initial lookahead is 5
        assert_eq!(queue.lookahead_count(), 5);
        assert_eq!(*received_counts.borrow().last().unwrap(), 5);
        
        // Increase lookahead to 10
        queue.set_lookahead_count(10);
        assert_eq!(queue.lookahead_count(), 10);
        assert_eq!(*received_counts.borrow().last().unwrap(), 10);
    }

    #[test]
    fn test_prefetch_queue_advance_index() {
        let items = vec!["large_item_a".to_string(), "large_item_b".to_string(), "large_item_c".to_string()];
        let mut queue = PrefetchQueue::new(items, 2, NoopPrefetchCallback);
        
        // Use advance_index to get index without cloning
        let index = queue.advance_index().unwrap();
        assert_eq!(index, 0);
        assert_eq!(queue.get(index).unwrap(), "large_item_a");
        
        let index = queue.advance_index().unwrap();
        assert_eq!(index, 1);
        assert_eq!(queue.get(index).unwrap(), "large_item_b");
        
        let index = queue.advance_index().unwrap();
        assert_eq!(index, 2);
        assert_eq!(queue.get(index).unwrap(), "large_item_c");
        
        // No more items
        assert!(queue.advance_index().is_none());
        
        // Can still access items by index after iteration
        assert_eq!(queue.get(0).unwrap(), "large_item_a");
        assert_eq!(queue.get(1).unwrap(), "large_item_b");
        assert_eq!(queue.get(2).unwrap(), "large_item_c");
        assert!(queue.get(3).is_none());
    }

    #[test]
    fn test_buffer_pool_basic() {
        let pool = BufferPool::new(4, DEFAULT_BUFFER_SIZE);
        
        // Get a buffer
        let mut buffer = pool.get();
        assert_eq!(buffer.len(), DEFAULT_BUFFER_SIZE);
        buffer[0] = 42;
        
        // Drop it
        drop(buffer);
        
        // Pool should have 1 buffer now
        assert_eq!(pool.available_count(), 1);
        
        // Get it again - should reuse
        let buffer2 = pool.get();
        assert_eq!(buffer2.len(), DEFAULT_BUFFER_SIZE);
        assert_eq!(buffer2[0], 42); // Previous data still there
    }

    #[test]
    fn test_buffer_pool_multiple() {
        let pool = BufferPool::new(3, LARGE_BUFFER_SIZE);
        
        // Get 3 buffers
        let b1 = pool.get();
        let b2 = pool.get();
        let b3 = pool.get();
        
        assert_eq!(pool.available_count(), 0);
        
        // Drop them
        drop(b1);
        assert_eq!(pool.available_count(), 1);
        drop(b2);
        assert_eq!(pool.available_count(), 2);
        drop(b3);
        assert_eq!(pool.available_count(), 3);
    }

    #[test]
    fn test_buffer_pool_max_capacity() {
        let pool = BufferPool::new(2, DEFAULT_BUFFER_SIZE);
        
        // Get and drop 3 buffers
        let b1 = pool.get();
        let b2 = pool.get();
        let b3 = pool.get();
        
        drop(b1);
        drop(b2);
        drop(b3);
        
        // Pool should only keep 2 (max capacity)
        assert_eq!(pool.available_count(), 2);
    }

    #[test]
    fn test_buffer_pool_default() {
        let pool = BufferPool::default();
        assert_eq!(pool.buffer_size(), DEFAULT_BUFFER_SIZE);
        
        let buffer = pool.get();
        assert_eq!(buffer.len(), DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn test_pooled_buffer_into_vec() {
        let pool = BufferPool::new(2, DEFAULT_BUFFER_SIZE);
        let mut buffer = pool.get();
        buffer[0] = 99;
        
        // Convert to owned Vec
        let vec = buffer.into_vec();
        assert_eq!(vec.len(), DEFAULT_BUFFER_SIZE);
        assert_eq!(vec[0], 99);
        
        // Pool should not have received it back
        assert_eq!(pool.available_count(), 0);
    }

    #[test]
    fn test_buffer_pool_clamping() {
        // Test buffer count clamping
        let pool = BufferPool::new(0, DEFAULT_BUFFER_SIZE);
        let buffer = pool.get();
        assert_eq!(buffer.len(), DEFAULT_BUFFER_SIZE);
        drop(buffer);
        assert_eq!(pool.available_count(), 1); // Min 1 buffer
        
        let pool = BufferPool::new(1000, DEFAULT_BUFFER_SIZE);
        let buffer = pool.get();
        assert_eq!(buffer.len(), DEFAULT_BUFFER_SIZE);
        drop(buffer);
        assert!(pool.available_count() <= 256); // Max 256 buffers
        
        // Test buffer size clamping
        let pool = BufferPool::new(2, 100);
        let buffer = pool.get();
        assert_eq!(buffer.len(), SMALL_BUFFER_SIZE); // Clamped to min 4KB
        
        let pool = BufferPool::new(2, 100 * 1024 * 1024);
        let buffer = pool.get();
        assert_eq!(buffer.len(), MAX_BUFFER_SIZE); // Clamped to max 16MB
    }
}
