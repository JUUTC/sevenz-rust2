use std::{
    cell::Cell,
    io::{Seek, SeekFrom, Write},
    rc::Rc,
};

/// A writer wrapper that tracks the number of bytes written.
/// 
/// This is optimized for the hot path in compression:
/// - Uses `#[inline(always)]` for zero-overhead writes
/// - Tracks bytes locally with a shared counter for external access
pub(crate) struct CountingWriter<W> {
    inner: W,
    counting: Rc<Cell<usize>>,
    written_bytes: usize,
}

impl<W> CountingWriter<W> {
    #[inline]
    pub(crate) fn new(inner: W) -> Self {
        Self {
            inner,
            counting: Rc::new(Cell::new(0)),
            written_bytes: 0,
        }
    }

    #[inline]
    pub(crate) fn counting(&self) -> Rc<Cell<usize>> {
        Rc::clone(&self.counting)
    }

    /// Returns the total bytes written (faster than going through Rc<Cell>)
    #[inline(always)]
    pub(crate) fn bytes_written(&self) -> usize {
        self.written_bytes
    }

    /// Consumes self and returns the inner writer
    #[inline]
    pub(crate) fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for CountingWriter<W> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = self.inner.write(buf)?;
        self.written_bytes += len;
        // Only update shared counter - this is read infrequently
        self.counting.set(self.written_bytes);
        Ok(len)
    }

    #[inline(always)]
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        // Note: We update written_bytes after successful write to maintain
        // consistency. If write_all fails, we don't know how many bytes were
        // actually written, but this is the same behavior as the base impl.
        // For exact counting in error cases, use write() in a loop.
        self.inner.write_all(buf)?;
        let len = buf.len();
        self.written_bytes += len;
        self.counting.set(self.written_bytes);
        Ok(())
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write + Seek> Seek for CountingWriter<W> {
    #[inline]
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}
