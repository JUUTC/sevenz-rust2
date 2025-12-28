# Missing Features Implementation Plan

This document outlines features present in the official 7-zip.org implementation that are missing or incomplete in the sevenz-rust2 Rust implementation.

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
| | SWAP2 | ❌ | ❌ | Not implemented |
| | SWAP4 | ❌ | ❌ | Not implemented |
| **Encryption** | AES256-SHA256 | ✅ | ✅ | |
| **Other** | Archive Comments | ❌ | ❌ | TODO in codebase |
| | External References | ❌ | N/A | Returns unsupported |
| | Additional Streams | ❌ | N/A | Returns unsupported |
| | kStartPos | ❌ | N/A | Returns unsupported |

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

### 1.2 Swap2/Swap4 Filter Support

**Current State:** Not implemented.

**Impact:** These filters improve compression for 16-bit and 32-bit aligned binary data (audio, scientific data, etc.).

**Implementation Plan:**
1. Add method IDs for Swap2 (`0x03, 0x02`) and Swap4 (`0x03, 0x04`) in `archive.rs`
2. Implement `Swap2Reader`/`Swap2Writer` and `Swap4Reader`/`Swap4Writer`
3. Add decoder support in `src/decoder.rs`
4. Add encoder support in `src/encoder.rs`
5. Add tests

**Algorithm:**
- **Swap2:** For each pair of bytes, swap their positions
  ```
  [A, B, C, D] → [B, A, D, C]
  ```
- **Swap4:** For each group of 4 bytes, reverse their order
  ```
  [A, B, C, D, E, F, G, H] → [D, C, B, A, H, G, F, E]
  ```

**Estimated Effort:** Low

**Files to Modify:**
- `src/archive.rs` - Add method IDs
- `src/decoder.rs` - Add Swap2/Swap4 decoder cases
- `src/encoder.rs` - Add Swap2/Swap4 encoder cases
- New file: `src/filter/swap.rs` or add to `lzma-rust2` crate

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

### O3: Streaming Compression

**Current State:** All data must be available before compression.

**Potential:** Allow streaming input for very large files.

**Implementation:**
1. Add streaming API that flushes blocks periodically
2. Would require non-solid mode

---

## Implementation Roadmap

### Phase 1 (Core Completeness)
1. [x] Current state analysis
2. Swap2/Swap4 filters (1-2 days)
3. Archive comments (1 day)

### Phase 2 (Advanced Filters)  
4. BCJ2 compression (3-5 days)
5. kStartPos support (1 day)

### Phase 3 (Extended Codecs)
6. Deflate64 (2-3 days if library available)
7. LZS codec (2-3 days)
8. LIZARD codec (2-3 days)

### Phase 4 (Complex Features)
9. External references (5+ days)
10. Additional streams (3-5 days)

### Phase 5 (Optimizations)
11. Parallel decompression API
12. Memory-mapped file support
13. Streaming compression API

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
