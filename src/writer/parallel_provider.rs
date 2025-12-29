//! Parallel stream provider for solid compression with high-latency data sources.
//!
//! This module provides the [`ParallelStreamProvider`] trait which enables parallel fetching
//! of input streams during solid compression. This is particularly useful when the data source
//! has significant latency (e.g., Azure Blob Storage with 5-20ms per request), allowing
//! multiple files to be fetched concurrently while compression proceeds.
//!
//! # Performance Impact
//!
//! For high-latency sources with many small files, parallel fetching can provide dramatic
//! speedups:
//!
//! | Scenario | Sequential | Parallel (64x) | Speedup |
//! |----------|-----------|----------------|---------|
//! | 50K files, 170ms latency | 2.4 hours | 2.3 min | 63x |
//! | 50K files, 20ms latency | 17 min | 16 sec | 63x |
//!
//! # Example
//!
//! ```no_run
//! use std::io::{Cursor, Read};
//! use std::collections::HashMap;
//! use std::sync::{Arc, Mutex};
//! use sevenz_rust2::ParallelStreamProvider;
//!
//! /// Example provider that serves data from an in-memory cache.
//! struct CacheProvider {
//!     data: Vec<Vec<u8>>,
//!     ready: HashMap<u32, Vec<u8>>,
//! }
//!
//! impl CacheProvider {
//!     fn new(data: Vec<Vec<u8>>) -> Self {
//!         Self { data, ready: HashMap::new() }
//!     }
//! }
//!
//! impl ParallelStreamProvider for CacheProvider {
//!     fn prepare_streams(&mut self, indices: &[u32]) {
//!         // In a real implementation, this would start async downloads
//!         for &idx in indices {
//!             if let Some(data) = self.data.get(idx as usize) {
//!                 self.ready.insert(idx, data.clone());
//!             }
//!         }
//!     }
//!
//!     fn try_get_stream(&mut self, index: u32) -> Option<Box<dyn Read + Send>> {
//!         self.ready.remove(&index).map(|data| {
//!             Box::new(Cursor::new(data)) as Box<dyn Read + Send>
//!         })
//!     }
//!
//!     fn get_stream_blocking(&mut self, index: u32) -> Option<Box<dyn Read + Send>> {
//!         // For a cache, try_get_stream is sufficient
//!         self.try_get_stream(index)
//!     }
//!
//!     fn parallelism(&self) -> usize {
//!         64
//!     }
//! }
//! ```

use std::io::Read;

/// Default batch size for parallel stream fetching.
pub const DEFAULT_PARALLEL_BATCH_SIZE: usize = 64;

/// Minimum batch size for parallel stream fetching.
pub const MIN_PARALLEL_BATCH_SIZE: usize = 1;

/// Maximum batch size for parallel stream fetching.
/// This is limited to prevent excessive memory usage.
pub const MAX_PARALLEL_BATCH_SIZE: usize = 1024;

/// Provider that can prepare multiple streams in parallel.
///
/// Unlike sequential callbacks that request one file at a time, this trait allows
/// the compressor to:
/// 1. Announce which files it needs next (lookahead via `prepare_streams`)
/// 2. Let the provider fetch them in parallel
/// 3. Consume streams as they become ready
///
/// # Thread Safety
///
/// The provider itself does not need to be `Send` or `Sync` - it is used from
/// a single thread. However, the streams it produces must be `Send` to allow
/// for internal buffering.
///
/// # Order Preservation
///
/// Solid compression requires files to be compressed in exact order. While the
/// provider can fetch files out of order (and should, for optimal parallelism),
/// the compressor will always request and consume streams in sequential order.
///
/// # Memory Management
///
/// The provider should implement backpressure if the caller fetches faster than
/// the compressor can consume. The `parallelism()` method indicates the maximum
/// number of streams that should be buffered simultaneously.
pub trait ParallelStreamProvider {
    /// Called when compression needs new streams.
    ///
    /// The compressor calls this method to notify the provider about upcoming
    /// file indices. The provider should start fetching these files in parallel.
    ///
    /// # Arguments
    /// * `indices` - File indices that will be needed, in the order they will be requested.
    ///   The provider should start fetching all of them in parallel.
    ///
    /// # Notes
    /// - This method may be called multiple times as compression progresses.
    /// - The indices are always sequential starting from the current position.
    /// - The provider should not block on fetching; actual fetching should happen asynchronously.
    fn prepare_streams(&mut self, indices: &[u32]);

    /// Check if a specific stream is ready without blocking.
    ///
    /// # Arguments
    /// * `index` - The file index to check.
    ///
    /// # Returns
    /// - `Some(stream)` if the stream is ready and available.
    /// - `None` if the stream is not yet available or the index is invalid.
    ///
    /// # Notes
    /// Once a stream is returned, it is consumed and cannot be retrieved again.
    fn try_get_stream(&mut self, index: u32) -> Option<Box<dyn Read + Send>>;

    /// Block until a specific stream is ready.
    ///
    /// # Arguments
    /// * `index` - The file index to retrieve.
    ///
    /// # Returns
    /// - `Some(stream)` if the stream becomes available.
    /// - `None` if the index is invalid or fetching failed.
    ///
    /// # Notes
    /// - This method may block for extended periods if the stream is being fetched.
    /// - Once a stream is returned, it is consumed and cannot be retrieved again.
    /// - Implementations should provide timeout or cancellation mechanisms if needed.
    fn get_stream_blocking(&mut self, index: u32) -> Option<Box<dyn Read + Send>>;

    /// Returns how many streams can be prepared in parallel.
    ///
    /// This value indicates the maximum number of streams the provider can
    /// buffer simultaneously. The compressor uses this to determine batch size.
    ///
    /// # Returns
    /// The maximum parallelism level (typically 1-1024).
    fn parallelism(&self) -> usize;

    /// Returns the total number of streams available.
    ///
    /// # Returns
    /// The total count of available streams, or `None` if unknown.
    ///
    /// # Default Implementation
    /// Returns `None`, indicating the count is unknown.
    fn stream_count(&self) -> Option<usize> {
        None
    }

    /// Called when the compressor encounters an error and needs to cancel.
    ///
    /// Providers should use this to stop any pending async operations.
    ///
    /// # Default Implementation
    /// Does nothing. Override if cleanup is needed.
    fn cancel(&mut self) {
        // Default: no-op
    }
}

/// A simple parallel stream provider backed by an in-memory vector of data.
///
/// This implementation is useful for testing and for cases where all data
/// is already available in memory.
///
/// # Example
///
/// ```no_run
/// use sevenz_rust2::{VecParallelStreamProvider, ParallelStreamProvider};
/// use std::io::Read;
///
/// let data = vec![
///     vec![1, 2, 3],
///     vec![4, 5, 6],
///     vec![7, 8, 9],
/// ];
/// let mut provider = VecParallelStreamProvider::new(data);
///
/// // Prepare streams 0 and 1
/// provider.prepare_streams(&[0, 1]);
///
/// // Get stream 0
/// let mut stream = provider.get_stream_blocking(0).unwrap();
/// let mut buf = Vec::new();
/// stream.read_to_end(&mut buf).unwrap();
/// assert_eq!(buf, vec![1, 2, 3]);
/// ```
pub struct VecParallelStreamProvider {
    /// The data for each stream, indexed by file index.
    data: Vec<Option<Vec<u8>>>,
    /// Maximum parallelism level.
    max_parallelism: usize,
}

impl VecParallelStreamProvider {
    /// Creates a new provider with the given data.
    ///
    /// # Arguments
    /// * `data` - Vector of byte vectors, one per file. Index in vector corresponds to file index.
    pub fn new(data: Vec<Vec<u8>>) -> Self {
        let count = data.len();
        Self {
            data: data.into_iter().map(Some).collect(),
            max_parallelism: count.min(DEFAULT_PARALLEL_BATCH_SIZE),
        }
    }

    /// Creates a new provider with custom parallelism.
    ///
    /// # Arguments
    /// * `data` - Vector of byte vectors, one per file.
    /// * `parallelism` - Maximum number of streams to buffer (clamped to valid range).
    pub fn with_parallelism(data: Vec<Vec<u8>>, parallelism: usize) -> Self {
        let count = data.len();
        Self {
            data: data.into_iter().map(Some).collect(),
            max_parallelism: parallelism.clamp(MIN_PARALLEL_BATCH_SIZE, MAX_PARALLEL_BATCH_SIZE).min(count),
        }
    }
}

impl ParallelStreamProvider for VecParallelStreamProvider {
    fn prepare_streams(&mut self, _indices: &[u32]) {
        // No-op for in-memory provider - data is already available
    }

    fn try_get_stream(&mut self, index: u32) -> Option<Box<dyn Read + Send>> {
        let idx = index as usize;
        if idx < self.data.len() {
            self.data[idx].take().map(|data| {
                Box::new(std::io::Cursor::new(data)) as Box<dyn Read + Send>
            })
        } else {
            None
        }
    }

    fn get_stream_blocking(&mut self, index: u32) -> Option<Box<dyn Read + Send>> {
        self.try_get_stream(index)
    }

    fn parallelism(&self) -> usize {
        self.max_parallelism
    }

    fn stream_count(&self) -> Option<usize> {
        Some(self.data.len())
    }
}

/// Configuration for parallel solid compression.
#[derive(Debug, Clone, Copy)]
pub struct ParallelSolidConfig {
    /// Batch size for prefetching streams.
    /// This determines how many files are requested from the provider at once.
    pub batch_size: usize,
    /// Buffer size for reading from streams.
    pub buffer_size: usize,
}

impl Default for ParallelSolidConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_PARALLEL_BATCH_SIZE,
            buffer_size: crate::perf::DEFAULT_BUFFER_SIZE,
        }
    }
}

impl ParallelSolidConfig {
    /// Creates a new configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the batch size for prefetching.
    ///
    /// The value is clamped between 1 and 1024.
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.clamp(MIN_PARALLEL_BATCH_SIZE, MAX_PARALLEL_BATCH_SIZE);
        self
    }

    /// Sets the buffer size for reading streams.
    ///
    /// The value is clamped between 4KB and 16MB.
    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size.clamp(
            crate::perf::SMALL_BUFFER_SIZE,
            crate::perf::MAX_BUFFER_SIZE,
        );
        self
    }

    /// Creates a configuration optimized for high-latency sources.
    ///
    /// Uses larger batch sizes to maximize parallel fetching.
    pub fn high_latency() -> Self {
        Self {
            batch_size: 128,
            buffer_size: crate::perf::LARGE_BUFFER_SIZE,
        }
    }

    /// Creates a configuration optimized for low-latency sources.
    ///
    /// Uses smaller batch sizes to reduce memory usage.
    pub fn low_latency() -> Self {
        Self {
            batch_size: 16,
            buffer_size: crate::perf::DEFAULT_BUFFER_SIZE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_vec_provider_basic() {
        let data = vec![
            vec![1, 2, 3],
            vec![4, 5, 6, 7],
            vec![8, 9],
        ];
        let mut provider = VecParallelStreamProvider::new(data);

        assert_eq!(provider.stream_count(), Some(3));
        assert!(provider.parallelism() >= 1);

        // Get streams in order
        let mut buf = Vec::new();
        
        let mut stream = provider.get_stream_blocking(0).unwrap();
        stream.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, vec![1, 2, 3]);

        buf.clear();
        let mut stream = provider.get_stream_blocking(1).unwrap();
        stream.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, vec![4, 5, 6, 7]);

        buf.clear();
        let mut stream = provider.get_stream_blocking(2).unwrap();
        stream.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, vec![8, 9]);
    }

    #[test]
    fn test_vec_provider_try_get_stream() {
        let data = vec![vec![1, 2, 3]];
        let mut provider = VecParallelStreamProvider::new(data);

        // First call should succeed
        assert!(provider.try_get_stream(0).is_some());
        
        // Second call should fail (stream consumed)
        assert!(provider.try_get_stream(0).is_none());
        
        // Invalid index should fail
        assert!(provider.try_get_stream(999).is_none());
    }

    #[test]
    fn test_vec_provider_prepare_streams() {
        let data = vec![vec![1], vec![2], vec![3]];
        let mut provider = VecParallelStreamProvider::new(data);

        // prepare_streams is a no-op for in-memory provider
        provider.prepare_streams(&[0, 1, 2]);

        // All streams should still be available
        assert!(provider.try_get_stream(0).is_some());
        assert!(provider.try_get_stream(1).is_some());
        assert!(provider.try_get_stream(2).is_some());
    }

    #[test]
    fn test_vec_provider_custom_parallelism() {
        let data = vec![vec![1]; 100];
        let provider = VecParallelStreamProvider::with_parallelism(data, 32);

        assert_eq!(provider.parallelism(), 32);
        assert_eq!(provider.stream_count(), Some(100));
    }

    #[test]
    fn test_parallel_solid_config_defaults() {
        let config = ParallelSolidConfig::default();
        assert_eq!(config.batch_size, DEFAULT_PARALLEL_BATCH_SIZE);
        assert_eq!(config.buffer_size, crate::perf::DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn test_parallel_solid_config_builder() {
        let config = ParallelSolidConfig::new()
            .with_batch_size(128)
            .with_buffer_size(256 * 1024);

        assert_eq!(config.batch_size, 128);
        assert_eq!(config.buffer_size, 256 * 1024);
    }

    #[test]
    fn test_parallel_solid_config_clamping() {
        // Test batch size clamping
        let config = ParallelSolidConfig::new().with_batch_size(0);
        assert_eq!(config.batch_size, MIN_PARALLEL_BATCH_SIZE);

        let config = ParallelSolidConfig::new().with_batch_size(10000);
        assert_eq!(config.batch_size, MAX_PARALLEL_BATCH_SIZE);

        // Test buffer size clamping
        let config = ParallelSolidConfig::new().with_buffer_size(100);
        assert_eq!(config.buffer_size, crate::perf::SMALL_BUFFER_SIZE);
    }

    #[test]
    fn test_parallel_solid_config_presets() {
        let high_latency = ParallelSolidConfig::high_latency();
        assert!(high_latency.batch_size >= DEFAULT_PARALLEL_BATCH_SIZE);

        let low_latency = ParallelSolidConfig::low_latency();
        assert!(low_latency.batch_size <= DEFAULT_PARALLEL_BATCH_SIZE);
    }
}
