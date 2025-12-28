//! Swap2 and Swap4 byte-swapping filters for 7z archives.
//!
//! These filters improve compression for 16-bit and 32-bit aligned binary data
//! (such as audio samples, scientific data, etc.) by rearranging bytes to
//! increase redundancy patterns that compression algorithms can exploit.
//!
//! # 7-Zip Specification Compliance
//!
//! - **Method ID**: Swap2 = `0x03 0x02`, Swap4 = `0x03 0x04`
//! - **Behavior**: Matches 7-Zip.org's Swap2/Swap4 filters exactly
//! - **Odd lengths**: Trailing bytes are output in reverse order (same as 7-Zip)

use std::io::{Read, Write};

/// Reader that applies Swap2 filter (swaps every pair of bytes).
///
/// Transforms `[A, B, C, D]` to `[B, A, D, C]`
pub struct Swap2Reader<R> {
    inner: R,
    pending: Option<u8>,
}

impl<R: Read> Swap2Reader<R> {
    /// Creates a new Swap2Reader wrapping the given reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            pending: None,
        }
    }
}

impl<R: Read> Read for Swap2Reader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut pos = 0;

        // First, emit any pending byte from previous read
        if let Some(b) = self.pending.take() {
            buf[pos] = b;
            pos += 1;
        }

        // Read pairs and swap them - use larger reads for better performance
        while pos < buf.len() {
            let mut pair = [0u8; 2];
            let n = self.inner.read(&mut pair)?;
            match n {
                0 => break, // EOF
                1 => {
                    // Got first byte, try to get second
                    let n2 = self.inner.read(&mut pair[1..2])?;
                    if n2 == 0 {
                        // Odd byte at EOF - output as-is
                        buf[pos] = pair[0];
                        pos += 1;
                        break;
                    }
                    // Got a pair - swap and output
                    buf[pos] = pair[1];
                    pos += 1;
                    if pos < buf.len() {
                        buf[pos] = pair[0];
                        pos += 1;
                    } else {
                        self.pending = Some(pair[0]);
                    }
                }
                2 => {
                    // Got a pair - swap and output
                    buf[pos] = pair[1];
                    pos += 1;
                    if pos < buf.len() {
                        buf[pos] = pair[0];
                        pos += 1;
                    } else {
                        self.pending = Some(pair[0]);
                    }
                }
                _ => unreachable!(),
            }
        }

        Ok(pos)
    }
}

/// Writer that applies Swap2 filter (swaps every pair of bytes).
///
/// Transforms `[A, B, C, D]` to `[B, A, D, C]`
pub struct Swap2Writer<W> {
    inner: W,
    pending: Option<u8>,
}

impl<W: Write> Swap2Writer<W> {
    /// Creates a new Swap2Writer wrapping the given writer.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            pending: None,
        }
    }

    /// Finishes writing, flushing any pending byte.
    pub fn finish(mut self) -> std::io::Result<W> {
        if let Some(b) = self.pending.take() {
            self.inner.write_all(&[b])?;
        }
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for Swap2Writer<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut pos = 0;

        // Handle pending byte from previous write
        if let Some(prev) = self.pending.take() {
            // We have a pending byte - combine with first byte of new buffer
            self.inner.write_all(&[buf[0], prev])?;
            pos += 1;
        }

        // Process pairs from buffer
        while pos + 1 < buf.len() {
            // Swap and write pair
            self.inner.write_all(&[buf[pos + 1], buf[pos]])?;
            pos += 2;
        }

        // Save odd byte for next write
        if pos < buf.len() {
            self.pending = Some(buf[pos]);
            pos += 1;
        }

        Ok(pos)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Reader that applies Swap4 filter (reverses every group of 4 bytes).
///
/// Transforms `[A, B, C, D, E, F, G, H]` to `[D, C, B, A, H, G, F, E]`
pub struct Swap4Reader<R> {
    inner: R,
    pending: [u8; 3],
    pending_len: usize,
}

impl<R: Read> Swap4Reader<R> {
    /// Creates a new Swap4Reader wrapping the given reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            pending: [0; 3],
            pending_len: 0,
        }
    }
}

impl<R: Read> Read for Swap4Reader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut pos = 0;

        // First, emit any pending bytes from previous read
        while self.pending_len > 0 && pos < buf.len() {
            let pending_start_idx = 3 - self.pending_len + 1;
            buf[pos] = self.pending[pending_start_idx - 1];
            self.pending_len -= 1;
            pos += 1;
        }

        // Read groups of 4 and reverse them
        while pos < buf.len() {
            let mut quad = [0u8; 4];
            let mut read_pos = 0;

            // Read up to 4 bytes
            while read_pos < 4 {
                let n = self.inner.read(&mut quad[read_pos..])?;
                if n == 0 {
                    break; // EOF
                }
                read_pos += n;
            }

            if read_pos == 0 {
                break; // EOF
            }

            if read_pos == 4 {
                // Full quad - reverse and output
                let reversed = [quad[3], quad[2], quad[1], quad[0]];
                let to_copy = (buf.len() - pos).min(4);
                buf[pos..pos + to_copy].copy_from_slice(&reversed[..to_copy]);
                pos += to_copy;

                // Save remaining bytes for next read
                let remaining = 4 - to_copy;
                if remaining > 0 {
                    self.pending[..remaining].copy_from_slice(&reversed[to_copy..]);
                    self.pending_len = remaining;
                }
            } else {
                // Partial read at EOF - output reversed up to what we have
                for i in 0..read_pos {
                    if pos < buf.len() {
                        buf[pos] = quad[read_pos - 1 - i];
                        pos += 1;
                    } else {
                        // Save for next read
                        self.pending[self.pending_len] = quad[read_pos - 1 - i];
                        self.pending_len += 1;
                    }
                }
                break;
            }
        }

        Ok(pos)
    }
}

/// Writer that applies Swap4 filter (reverses every group of 4 bytes).
///
/// Transforms `[A, B, C, D, E, F, G, H]` to `[D, C, B, A, H, G, F, E]`
pub struct Swap4Writer<W> {
    inner: W,
    pending: [u8; 4],
    pending_len: usize,
}

impl<W: Write> Swap4Writer<W> {
    /// Creates a new Swap4Writer wrapping the given writer.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            pending: [0; 4],
            pending_len: 0,
        }
    }

    /// Finishes writing, flushing any pending bytes.
    pub fn finish(mut self) -> std::io::Result<W> {
        // Write remaining bytes in reverse order
        for i in (0..self.pending_len).rev() {
            self.inner.write_all(&[self.pending[i]])?;
        }
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for Swap4Writer<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut pos = 0;

        // Try to complete pending quad
        while self.pending_len > 0 && self.pending_len < 4 && pos < buf.len() {
            self.pending[self.pending_len] = buf[pos];
            self.pending_len += 1;
            pos += 1;

            if self.pending_len == 4 {
                // Complete quad - write reversed
                self.inner.write_all(&[
                    self.pending[3],
                    self.pending[2],
                    self.pending[1],
                    self.pending[0],
                ])?;
                self.pending_len = 0;
            }
        }

        // Process full quads from buffer
        while pos + 4 <= buf.len() {
            // Reverse and write quad
            self.inner.write_all(&[
                buf[pos + 3],
                buf[pos + 2],
                buf[pos + 1],
                buf[pos],
            ])?;
            pos += 4;
        }

        // Save remaining bytes for next write
        while pos < buf.len() {
            self.pending[self.pending_len] = buf[pos];
            self.pending_len += 1;
            pos += 1;
        }

        Ok(pos)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_swap2_roundtrip() {
        let original = b"ABCDEFGH";
        let expected_swapped = b"BADCFEHG";

        // Encode
        let mut writer = Swap2Writer::new(Vec::new());
        writer.write_all(original).unwrap();
        let encoded = writer.finish().unwrap();
        assert_eq!(&encoded, expected_swapped);

        // Decode
        let mut reader = Swap2Reader::new(Cursor::new(encoded));
        let mut decoded = Vec::new();
        reader.read_to_end(&mut decoded).unwrap();
        assert_eq!(&decoded, original);
    }

    #[test]
    fn test_swap2_odd_length() {
        let original = b"ABC";
        
        // Encode
        let mut writer = Swap2Writer::new(Vec::new());
        writer.write_all(original).unwrap();
        let encoded = writer.finish().unwrap();
        assert_eq!(&encoded, b"BAC");

        // Decode
        let mut reader = Swap2Reader::new(Cursor::new(encoded));
        let mut decoded = Vec::new();
        reader.read_to_end(&mut decoded).unwrap();
        assert_eq!(&decoded, original);
    }

    #[test]
    fn test_swap4_roundtrip() {
        let original = b"ABCDEFGH";
        let expected_swapped = b"DCBAHGFE";

        // Encode
        let mut writer = Swap4Writer::new(Vec::new());
        writer.write_all(original).unwrap();
        let encoded = writer.finish().unwrap();
        assert_eq!(&encoded, expected_swapped);

        // Decode
        let mut reader = Swap4Reader::new(Cursor::new(encoded));
        let mut decoded = Vec::new();
        reader.read_to_end(&mut decoded).unwrap();
        assert_eq!(&decoded, original);
    }

    #[test]
    fn test_swap4_partial() {
        let original = b"ABCDE"; // 5 bytes - one full quad plus one byte

        // Encode
        let mut writer = Swap4Writer::new(Vec::new());
        writer.write_all(original).unwrap();
        let encoded = writer.finish().unwrap();
        assert_eq!(&encoded, b"DCBAE");

        // Decode
        let mut reader = Swap4Reader::new(Cursor::new(encoded));
        let mut decoded = Vec::new();
        reader.read_to_end(&mut decoded).unwrap();
        assert_eq!(&decoded, original);
    }
}
