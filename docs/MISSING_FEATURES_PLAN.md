# Missing Features Implementation Plan

This document outlines features present in the official 7-zip.org implementation that are missing or incomplete in the sevenz-rust2 Rust implementation.

---

## Priority 1: Streaming Compression API (Highest Priority)

### Current Streaming Capabilities

The sevenz-rust2 library **already supports streaming input** for compression:

```rust
// Stream from any Read implementation (memory, network, etc.)
pub fn push_archive_entry<R: Read>(
    &mut self,
    entry: ArchiveEntry,
    reader: Option<R>,  // ← Accepts any std::io::Read!
) -> Result<&ArchiveEntry>
```

**What works today:**
- ✅ **Memory streams**: Use `std::io::Cursor<Vec<u8>>` or `&[u8]` as input
- ✅ **Network streams**: Any `impl Read` can be used, including TCP streams
- ✅ **Buffered readers**: Wrap any source with `BufReader` for performance
- ✅ **Chained readers**: Use `std::io::Chain` to concatenate streams
- ✅ **Multiple files**: `push_archive_entries()` accepts `Vec<SourceReader<R>>`

**Example - Streaming from memory cache:**
```rust
use sevenz_rust2::*;
use std::io::Cursor;

// Your cached content
let cached_data: Vec<u8> = download_or_get_from_cache();

// Create archive from stream
let mut archive = ArchiveWriter::create("output.7z")?;
let entry = ArchiveEntry::new_file("cached_file.dat");
archive.push_archive_entry(entry, Some(Cursor::new(cached_data)))?;
archive.finish()?;
```

**Example - Feeding from active download stream:**
```rust
use sevenz_rust2::*;
use std::sync::mpsc::{Receiver, channel};

// Example: A stream fed by a background download thread
struct DownloadStream {
    receiver: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    position: usize,
}

impl DownloadStream {
    fn new(receiver: Receiver<Vec<u8>>) -> Self {
        Self { receiver, buffer: Vec::new(), position: 0 }
    }
}

impl std::io::Read for DownloadStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Refill buffer if empty
        if self.position >= self.buffer.len() {
            match self.receiver.recv() {
                Ok(data) => {
                    self.buffer = data;
                    self.position = 0;
                }
                Err(_) => return Ok(0), // Channel closed = EOF
            }
        }
        // Copy available data to output buffer
        let available = &self.buffer[self.position..];
        let to_copy = available.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&available[..to_copy]);
        self.position += to_copy;
        Ok(to_copy)
    }
}

// Usage:
let (tx, rx) = channel();
let download = DownloadStream::new(rx);

// Background thread sends chunks: tx.send(chunk).unwrap();
let mut archive = ArchiveWriter::create("output.7z")?;
archive.push_archive_entry(ArchiveEntry::new_file("downloaded.dat"), Some(download))?;
archive.finish()?;
```

### Output Requires Write + Seek

The 7z format requires writing a header at file start that references the end-of-file position. This means:

```rust
// ArchiveWriter requires Write + Seek
impl<W: Write + Seek> ArchiveWriter<W>
```

**This works directly with cloud storage that supports seeking:**

If your Azure Storage wrapper implements `Write + Seek` (reading/seeking to arbitrary positions), 
it will work directly with `ArchiveWriter::new()` - no local buffering required:

```rust
use sevenz_rust2::*;

// Your Azure blob wrapper that implements Write + Seek
struct AzureBlobWriter { /* ... */ }
impl std::io::Write for AzureBlobWriter { /* ... */ }
impl std::io::Seek for AzureBlobWriter { /* ... */ }

// Direct streaming to Azure - works for TB-scale data!
let azure_writer = AzureBlobWriter::new("container/archive.7z");
let mut archive = ArchiveWriter::new(azure_writer)?;

// Stream input data (from cache, downloads, etc.)
for file_info in cached_files {
    let entry = ArchiveEntry::new_file(&file_info.name);
    archive.push_archive_entry(entry, Some(file_info.reader))?;
}

archive.finish()?; // Seeks to start, writes header, done
```

The seek operations are minimal:
1. Initial seek to byte 32 (skip signature header placeholder)
2. Final seek to byte 0 to write the 32-byte start header

**For pure streaming outputs (no seek support):**
1. Write to a temporary seekable buffer (file or `Vec<u8>`)
2. Then stream that buffer to final destination

### Planned Enhancements

| Enhancement | Description | Priority |
|-------------|-------------|----------|
| **Streaming Output** | Buffer-then-stream pattern for non-seekable outputs | High |
| **Progressive API** | Add entries incrementally with size hints | Medium |
| **Async Support** | `AsyncRead`/`AsyncWrite` traits (via tokio feature) | Medium |
| **Backpressure** | Flow control for slow consumers | Low |

### Implementation Plan for Enhanced Streaming

1. **Add `StreamingArchiveWriter`** - A wrapper that buffers to memory/temp file
2. **Add size estimation** - Allow pre-declaring file sizes for better memory planning
3. **Add progress callbacks** - Report bytes processed for UI/monitoring
4. **Async feature flag** - Optional async variants using tokio/async-std

### Performance Tips for Streaming

1. **Buffer size**: The internal compression buffer is 4KB. For network streams with higher latency,
   wrap with a larger buffer to reduce syscall overhead:
   ```rust
   // 64KB is a good balance: large enough to amortize syscall overhead,
   // small enough to keep memory usage reasonable for multiple streams
   let buffered = BufReader::with_capacity(64 * 1024, network_stream);
   // For high-bandwidth local I/O, consider 256KB buffers
   let buffered = BufReader::with_capacity(256 * 1024, fast_ssd_file);
   ```
2. **Parallel feeding**: Use `push_archive_entries()` with solid=false for independent compression
3. **Memory limits**: For very large streams, consider chunking into separate archive entries

---

## Feature Gap Analysis

### Current Implementation Status

| Category | Feature | Decompression | Compression | Notes |
|----------|---------|---------------|-------------|-------|
| **Codecs** | COPY | ✅ | ✅ | |
| | LZMA | ✅ | ✅ | |
| | LZMA2 | ✅ | ✅ | Multi-threading supported |
| | PPMD | ✅ | ✅ | Optional feature |
| | BZIP2 | ✅ | ✅ | |
| | DEFLATE | ✅ | ✅ | Optional feature |
| | DEFLATE64 | ❌ | ❌ | ID defined, no implementation |
| | BROTLI | ✅ | ✅ | Optional feature |
| | LZ4 | ✅ | ✅ | Optional feature |
| | LZS | ❌ | ❌ | ID defined, no implementation |
| | LIZARD | ❌ | ❌ | ID defined, no implementation |
| | ZSTD | ✅ | ✅ | Optional feature |
| **Filters** | BCJ X86 | ✅ | ✅ | |
| | BCJ ARM | ✅ | ✅ | |
| | BCJ ARM64 | ✅ | ✅ | |
| | BCJ ARM_THUMB | ✅ | ✅ | |
| | BCJ RISC_V | ✅ | ✅ | |
| | BCJ PPC | ✅ | ✅ | |
| | BCJ SPARC | ✅ | ✅ | |
| | BCJ IA64 | ✅ | ✅ | |
| | BCJ2 | ✅ | ❌ | Decompression only |
| | DELTA | ✅ | ✅ | |
| | SWAP2 | ✅ | ✅ | Implemented |
| | SWAP4 | ✅ | ✅ | Implemented |
| **Encryption** | AES256-SHA256 | ✅ | ✅ | |
| **Other** | Archive Comments | ✅ | ✅ | Implemented |
| | External References | ❌ | N/A | Returns unsupported |
| | Additional Streams | ❌ | N/A | Returns unsupported |
| | kStartPos | ✅ | N/A | Read support implemented |

---

## Priority 1: High Impact Features

### 1.1 BCJ2 Compression Support

**Current State:** BCJ2 filter is supported for decompression only.

**Impact:** BCJ2 significantly improves compression ratios for x86 executables by separating branch call/jump addresses into multiple streams.

**Implementation Plan:**
1. Analyze the BCJ2 algorithm in `lzma-rust2` crate
2. Implement `Bcj2Writer` following the existing `BcjWriter` pattern
3. Add BCJ2 encoder support in `src/encoder.rs`
4. Handle multi-stream output (BCJ2 produces 4 output streams)
5. Update `UnpackInfo` to write multiple streams for a single coder
6. Add tests with real executable files

**Estimated Effort:** Medium (requires understanding multi-stream architecture)

**Files to Modify:**
- `src/encoder.rs` - Add BCJ2 encoder case
- `src/writer/unpack_info.rs` - Handle multi-stream packing
- `lzma-rust2` crate - Add `Bcj2Writer` if not present

---

### 1.2 Swap2/Swap4 Filter Support ✅ COMPLETED

**Status:** Implemented in `src/codec/swap.rs`

**What was implemented:**
- Method IDs for Swap2 (`0x03, 0x02`) and Swap4 (`0x03, 0x04`) in `archive.rs`
- `Swap2Reader`/`Swap2Writer` and `Swap4Reader`/`Swap4Writer` in `src/codec/swap.rs`
- Decoder support in `src/decoder.rs`
- Encoder support in `src/encoder.rs`
- Unit tests for roundtrip encoding/decoding

**Algorithm:**
- **Swap2:** For each pair of bytes, swap their positions
  ```
  [A, B, C, D] → [B, A, D, C]
  ```
- **Swap4:** For each group of 4 bytes, reverse their order
  ```
  [A, B, C, D, E, F, G, H] → [D, C, B, A, H, G, F, E]
  ```

**Usage:**
```rust
use sevenz_rust2::*;

// Use Swap2 filter with LZMA2 compression for 16-bit audio
let mut archive = ArchiveWriter::create("output.7z")?;
archive.set_content_methods(vec![
    EncoderConfiguration::new(EncoderMethod::SWAP2_FILTER),
    EncoderConfiguration::new(EncoderMethod::LZMA2),
]);
```

---

## Priority 2: Medium Impact Features

### 2.1 Deflate64 Codec

**Current State:** Method ID defined but no implementation.

**Impact:** Better compatibility with some legacy archives.

**Implementation Plan:**
1. Research Deflate64 format (64KB sliding window vs 32KB)
2. Find or create a Rust Deflate64 implementation
3. Add decoder in `src/decoder.rs`
4. Add encoder in `src/encoder.rs` (optional)
5. Add feature flag

**Estimated Effort:** Medium (depends on availability of Rust library)

**Dependencies:** May need to wrap a C library or implement from scratch.

---

### 2.2 Archive Comments Support

**Current State:** `K_COMMENT` constant defined, marked as TODO.

**Impact:** Low usage but required for full format compatibility.

**Implementation Plan:**
1. Add `comment: Option<String>` field to `Archive` struct
2. Implement reading comments in `reader.rs`
3. Implement writing comments in `writer.rs`
4. Expose API for getting/setting comments

**Estimated Effort:** Low

**Files to Modify:**
- `src/archive.rs` - Add comment field
- `src/reader.rs` - Parse K_COMMENT section
- `src/writer.rs` - Write K_COMMENT section

---

## Priority 3: Low Impact Features

### 3.1 External References Support

**Current State:** Returns `ExternalUnsupported` error.

**Impact:** Very rare in practice, used for streaming large archives.

**Implementation Plan:**
1. Study external reference format in 7z specification
2. Implement external stream handling
3. This allows metadata to reference data outside the current archive

**Estimated Effort:** High (complex architecture change)

---

### 3.2 Additional Streams Info

**Current State:** Returns error "Additional streams unsupported".

**Impact:** Rare feature for complex archive structures.

**Implementation Plan:**
1. Study K_ADDITIONAL_STREAMS_INFO format
2. Implement parsing and usage

**Estimated Effort:** Medium-High

---

### 3.3 LZS Codec

**Current State:** Method ID defined, no implementation.

**Impact:** Very rare usage.

**Implementation Plan:**
1. Find/create LZS implementation
2. Integrate with codec system

**Estimated Effort:** Medium

---

### 3.4 LIZARD Codec

**Current State:** Method ID defined, no implementation.

**Impact:** Very rare usage.

**Implementation Plan:**
1. Find/create LIZARD implementation
2. Integrate with codec system

**Estimated Effort:** Medium

---

### 3.5 kStartPos Support

**Current State:** Returns error "kStartPos is unsupported".

**Impact:** Rare feature.

**Implementation Plan:**
1. Study kStartPos in 7z specification
2. Implement reading/handling

**Estimated Effort:** Low

---

## Optimization Opportunities

### O1: Parallel Decompression for Non-Solid Archives

**Current State:** Single-threaded decompression except LZMA2.

**Potential:** For non-solid archives, each block is independent and can be decompressed in parallel.

**Implementation:**
1. Add parallel iteration API for blocks
2. Use rayon or thread pool for parallel decompression

---

### O2: Memory-Mapped File Support

**Current State:** Standard Read/Seek traits used.

**Potential:** Memory mapping can improve performance for large files.

**Implementation:**
1. Add optional `memmap2` dependency
2. Provide memory-mapped reader variant

---

### O3: Larger Internal Buffers

**Current State:** 4KB internal buffer for stream processing.

**Potential:** Larger buffers (64KB-256KB) can improve throughput for network/disk I/O.

**Implementation:**
1. Make buffer size configurable
2. Use adaptive buffering based on source type

---

## Implementation Roadmap

### Phase 1 (Streaming & Core - Highest Priority)
1. [x] Current state analysis
2. [x] Document existing streaming capabilities
3. [x] Add `StreamingArchiveWriter` for non-seekable outputs (2-3 days) - **COMPLETED**
4. [x] Add progress callbacks for stream monitoring (1 day) - **COMPLETED**
5. Add async feature flag with tokio support (3-5 days)

### Phase 2 (Filters)
6. [x] Swap2/Swap4 filters (1-2 days) - **COMPLETED**
7. BCJ2 compression (3-5 days)

### Phase 3 (Extended Features)
8. [x] Archive comments (1 day) - **COMPLETED**
9. [x] kStartPos support (1 day) - **COMPLETED**

### Phase 4 (Extended Codecs)
10. Deflate64 (2-3 days if library available)
11. LZS codec (2-3 days)
12. LIZARD codec (2-3 days)

### Phase 5 (Complex Features)
13. External references (5+ days)
14. Additional streams (3-5 days)

### Phase 6 (Optimizations)
15. Parallel decompression API
16. Memory-mapped file support

---

## Testing Strategy

### Unit Tests
- Add test files for each new codec/filter
- Use official 7-Zip to create reference archives
- Verify roundtrip compression/decompression

### Integration Tests
- Test interoperability with official 7-Zip
- Test with various file types (executables, audio, text)

### Fuzzing
- Add fuzzing targets for new codecs
- Use cargo-fuzz to find edge cases

---

## Contributing Guidelines

When implementing a new feature:

1. **Create feature flag** if the feature adds dependencies
2. **Follow existing patterns** in `decoder.rs` and `encoder.rs`
3. **Add method ID** to `archive.rs` if needed
4. **Write tests** for both encode and decode paths
5. **Update documentation** in README.md and lib.rs
6. **Update CHANGELOG.md** with the new feature

---

## References

- [7z Format Specification](https://www.7-zip.org/7z.html)
- [7-Zip Source Code](https://sourceforge.net/projects/sevenzip/) - Official SourceForge repository
- [7-Zip Documentation](https://documentation.help/7-Zip/) - Format documentation
- [BCJ Algorithm](https://en.wikipedia.org/wiki/BCJ_(algorithm))
