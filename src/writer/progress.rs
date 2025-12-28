//! Progress callback types for monitoring compression/decompression progress.
//!
//! Use progress callbacks to track bytes processed, display progress bars,
//! or implement cancellation logic.

/// Progress information passed to progress callbacks.
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    /// Number of bytes read from input so far.
    pub bytes_read: u64,
    /// Number of bytes written to output so far.
    pub bytes_written: u64,
    /// Name of the current entry being processed, if available.
    pub current_entry: Option<String>,
    /// Index of the current entry (0-based).
    pub entry_index: usize,
    /// Total number of entries, if known.
    pub total_entries: Option<usize>,
}

impl ProgressInfo {
    /// Creates a new ProgressInfo with the given values.
    pub fn new(bytes_read: u64, bytes_written: u64) -> Self {
        Self {
            bytes_read,
            bytes_written,
            current_entry: None,
            entry_index: 0,
            total_entries: None,
        }
    }

    /// Sets the current entry name.
    pub fn with_entry(mut self, name: impl Into<String>) -> Self {
        self.current_entry = Some(name.into());
        self
    }

    /// Sets the entry index.
    pub fn with_index(mut self, index: usize) -> Self {
        self.entry_index = index;
        self
    }

    /// Sets the total number of entries.
    pub fn with_total(mut self, total: usize) -> Self {
        self.total_entries = Some(total);
        self
    }
}

/// Result returned from a progress callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressResult {
    /// Continue processing.
    Continue,
    /// Cancel the operation.
    Cancel,
}

/// A trait for progress callbacks.
pub trait ProgressCallback: Send {
    /// Called periodically during compression/decompression.
    ///
    /// Return `ProgressResult::Continue` to continue processing,
    /// or `ProgressResult::Cancel` to abort the operation.
    fn on_progress(&mut self, info: &ProgressInfo) -> ProgressResult;
}

/// A simple progress callback that wraps a closure.
pub struct FnProgressCallback<F>
where
    F: FnMut(&ProgressInfo) -> ProgressResult + Send,
{
    callback: F,
}

impl<F> FnProgressCallback<F>
where
    F: FnMut(&ProgressInfo) -> ProgressResult + Send,
{
    /// Creates a new progress callback from a closure.
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<F> ProgressCallback for FnProgressCallback<F>
where
    F: FnMut(&ProgressInfo) -> ProgressResult + Send,
{
    fn on_progress(&mut self, info: &ProgressInfo) -> ProgressResult {
        (self.callback)(info)
    }
}

/// A no-op progress callback that does nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopProgressCallback;

impl ProgressCallback for NoopProgressCallback {
    fn on_progress(&mut self, _info: &ProgressInfo) -> ProgressResult {
        ProgressResult::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_info() {
        let info = ProgressInfo::new(1000, 500)
            .with_entry("test.txt")
            .with_index(0)
            .with_total(10);

        assert_eq!(info.bytes_read, 1000);
        assert_eq!(info.bytes_written, 500);
        assert_eq!(info.current_entry, Some("test.txt".to_string()));
        assert_eq!(info.entry_index, 0);
        assert_eq!(info.total_entries, Some(10));
    }

    #[test]
    fn test_fn_progress_callback() {
        let mut count = 0;
        let mut callback = FnProgressCallback::new(|_| {
            count += 1;
            ProgressResult::Continue
        });

        let info = ProgressInfo::new(100, 50);
        assert_eq!(callback.on_progress(&info), ProgressResult::Continue);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_cancel_callback() {
        let mut callback = FnProgressCallback::new(|info| {
            if info.bytes_read > 1000 {
                ProgressResult::Cancel
            } else {
                ProgressResult::Continue
            }
        });

        let info1 = ProgressInfo::new(500, 250);
        assert_eq!(callback.on_progress(&info1), ProgressResult::Continue);

        let info2 = ProgressInfo::new(1500, 750);
        assert_eq!(callback.on_progress(&info2), ProgressResult::Cancel);
    }
}
