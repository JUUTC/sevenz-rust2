mod counting_writer;
#[cfg(all(feature = "util", not(target_arch = "wasm32")))]
mod lazy_file_reader;
mod pack_info;
#[cfg(not(target_arch = "wasm32"))]
mod parallel_reader;
mod progress;
#[cfg(not(target_arch = "wasm32"))]
mod segmented_writer;
mod seq_reader;
mod source_reader;
mod streaming;
mod unpack_info;

use std::{
    cell::Cell,
    io::{Read, Seek, Write},
    rc::Rc,
    sync::Arc,
};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs::File, path::Path};

pub(crate) use counting_writer::CountingWriter;
use crc32fast::Hasher;

#[cfg(all(feature = "util", not(target_arch = "wasm32")))]
pub(crate) use self::lazy_file_reader::LazyFileReader;
#[cfg(not(target_arch = "wasm32"))]
pub use self::parallel_reader::{BackgroundReader, BufferedChunkReader, ParallelPrefetchReader};
pub use self::progress::{
    FnProgressCallback, NoopProgressCallback, ProgressCallback, ProgressInfo, ProgressResult,
};
#[cfg(not(target_arch = "wasm32"))]
pub use self::segmented_writer::{SegmentedWriter, VolumeConfig, VolumeMetadata};
pub(crate) use self::seq_reader::SeqReader;
pub use self::source_reader::SourceReader;
pub use self::streaming::StreamingArchiveWriter;
use self::{pack_info::PackInfo, unpack_info::UnpackInfo};
use crate::{
    ArchiveEntry, AutoFinish, AutoFinisher, ByteWriter, Error,
    archive::*,
    bitset::{BitSet, write_bit_set},
    encoder,
};

macro_rules! write_times {
    //write_i64
    ($fn_name:tt, $nid:expr, $has_time:tt, $time:tt) => {
        write_times!($fn_name, $nid, $has_time, $time, write_u64);
    };
    ($fn_name:tt, $nid:expr, $has_time:tt, $time:tt, $write_fn:tt) => {
        fn $fn_name<H: Write>(&self, header: &mut H) -> std::io::Result<()> {
            let mut num = 0;
            for entry in self.files.iter() {
                if entry.$has_time {
                    num += 1;
                }
            }
            if num > 0 {
                header.write_u8($nid)?;
                let mut temp: Vec<u8> = Vec::with_capacity(128);
                let mut out = &mut temp;
                if num != self.files.len() {
                    out.write_u8(0)?;
                    let mut times = BitSet::with_capacity(self.files.len());
                    for i in 0..self.files.len() {
                        if self.files[i].$has_time {
                            times.insert(i);
                        }
                    }
                    write_bit_set(&mut out, &times)?;
                } else {
                    out.write_u8(1)?;
                }
                out.write_u8(0)?;
                for file in self.files.iter() {
                    if file.$has_time {
                        out.$write_fn((file.$time).into())?;
                    }
                }
                out.flush()?;
                write_u64(header, temp.len() as u64)?;
                header.write_all(&temp)?;
            }
            Ok(())
        }
    };
}

type Result<T> = std::result::Result<T, Error>;

/// Writes a 7z archive file.
pub struct ArchiveWriter<W: Write> {
    output: W,
    files: Vec<ArchiveEntry>,
    content_methods: Arc<Vec<EncoderConfiguration>>,
    pack_info: PackInfo,
    unpack_info: UnpackInfo,
    encrypt_header: bool,
    comment: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ArchiveWriter<File> {
    /// Creates a file to write a 7z archive to.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::create(path.as_ref())
            .map_err(|e| Error::file_open(e, path.as_ref().to_string_lossy().to_string()))?;
        Self::new(file)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ArchiveWriter<SegmentedWriter> {
    /// Creates a multi-volume archive writer with the specified volume configuration.
    ///
    /// Multi-volume archives are split across multiple files, each with a maximum size
    /// defined by the volume configuration. Files are named with extensions `.7z.001`,
    /// `.7z.002`, etc.
    ///
    /// # Arguments
    /// * `config` - The volume configuration specifying size limits and base path.
    ///
    /// # Example
    /// ```no_run
    /// use sevenz_rust2::*;
    ///
    /// // Create a multi-volume archive with 10MB volumes
    /// let config = VolumeConfig::new("path/to/archive", 10 * 1024 * 1024);
    /// let mut writer = ArchiveWriter::create_multi_volume(config).unwrap();
    ///
    /// // Add files as normal
    /// // writer.push_archive_entry(...);
    ///
    /// // Finish returns metadata about created volumes
    /// let metadata = writer.finish_multi_volume().unwrap();
    /// println!("Created {} volumes", metadata.volume_count);
    /// ```
    pub fn create_multi_volume(config: VolumeConfig) -> Result<Self> {
        let writer = SegmentedWriter::new(config)
            .map_err(|e| Error::io_msg(e, "Failed to create segmented writer"))?;
        Self::new(writer)
    }

    /// Finishes the multi-volume compression and returns metadata about created volumes.
    ///
    /// This method writes the final header, patches the start header in volume 1,
    /// and returns information about all created volume files.
    pub fn finish_multi_volume(mut self) -> std::io::Result<VolumeMetadata> {
        let mut header: Vec<u8> = Vec::with_capacity(64 * 1024);
        self.write_encoded_header(&mut header)?;
        let header_pos = self.output.stream_position()?;
        self.output.write_all(&header)?;
        let crc32 = crc32fast::hash(&header);
        let mut hh = [0u8; SIGNATURE_HEADER_SIZE as usize];
        {
            let mut hhw = hh.as_mut_slice();
            // sig
            hhw.write_all(SEVEN_Z_SIGNATURE)?;
            // version
            hhw.write_u8(0)?;
            hhw.write_u8(4)?;
            // placeholder for crc: index = 8
            hhw.write_u32(0)?;

            // start header
            hhw.write_u64(header_pos - SIGNATURE_HEADER_SIZE)?;
            hhw.write_u64(0xFFFFFFFF & header.len() as u64)?;
            hhw.write_u32(crc32)?;
        }
        let crc32 = crc32fast::hash(&hh[12..]);
        hh[8..12].copy_from_slice(&crc32.to_le_bytes());

        // Seek back to the start and write the header
        self.output.seek(std::io::SeekFrom::Start(0))?;
        self.output.write_all(&hh)?;
        self.output.flush()?;

        // Finish the segmented writer and return metadata
        self.output.finish()
    }
}

impl<W: Write + Seek> ArchiveWriter<W> {
    /// Prepares writer to write a 7z archive to.
    pub fn new(mut writer: W) -> Result<Self> {
        writer.seek(std::io::SeekFrom::Start(SIGNATURE_HEADER_SIZE))?;

        Ok(Self {
            output: writer,
            files: Default::default(),
            content_methods: Arc::new(vec![EncoderConfiguration::new(EncoderMethod::LZMA2)]),
            pack_info: Default::default(),
            unpack_info: Default::default(),
            encrypt_header: true,
            comment: None,
        })
    }

    /// Returns a wrapper around `self` that will finish the stream on drop.
    pub fn auto_finish(self) -> AutoFinisher<Self> {
        AutoFinisher(Some(self))
    }

    /// Sets the default compression methods to use for entry data. Default is LZMA2.
    pub fn set_content_methods(&mut self, content_methods: Vec<EncoderConfiguration>) -> &mut Self {
        if content_methods.is_empty() {
            return self;
        }
        self.content_methods = Arc::new(content_methods);
        self
    }

    /// Whether to enable the encryption of the -header. Default is `true`.
    pub fn set_encrypt_header(&mut self, enabled: bool) {
        self.encrypt_header = enabled;
    }

    /// Sets an optional archive comment.
    ///
    /// # Example
    /// ```no_run
    /// use sevenz_rust2::*;
    ///
    /// let mut sz = ArchiveWriter::create("path/to/archive.7z").unwrap();
    /// sz.set_comment("This archive was created with sevenz-rust2");
    /// // Add files...
    /// sz.finish().unwrap();
    /// ```
    pub fn set_comment(&mut self, comment: impl Into<String>) -> &mut Self {
        self.comment = Some(comment.into());
        self
    }

    /// Clears the archive comment.
    pub fn clear_comment(&mut self) -> &mut Self {
        self.comment = None;
        self
    }

    /// Configures compression for maximum parallel throughput.
    ///
    /// This method sets up LZMA2 multi-threaded compression with settings optimized
    /// for fast I/O sources like in-memory caches, NVMe drives, or high-speed networks.
    ///
    /// # Arguments
    /// * `config` - Parallel configuration specifying threads, chunk size, etc.
    /// * `level` - Compression level (0-9)
    ///
    /// # Example
    /// ```no_run
    /// use sevenz_rust2::*;
    /// use sevenz_rust2::perf::ParallelConfig;
    ///
    /// let mut sz = ArchiveWriter::create("path/to/dest.7z").expect("create writer ok");
    /// sz.configure_parallel(ParallelConfig::max_throughput(), 6);
    /// // Add files...
    /// sz.finish().expect("done");
    /// ```
    pub fn configure_parallel(
        &mut self,
        config: crate::perf::ParallelConfig,
        level: u32,
    ) -> &mut Self {
        let lzma2_options = config.to_lzma2_options(level);
        self.set_content_methods(vec![lzma2_options.into()])
    }

    /// Non-solid compression - Adds an archive `entry` with data from `reader`.
    ///
    /// # Example
    /// ```no_run
    /// use std::{fs::File, path::Path};
    ///
    /// use sevenz_rust2::*;
    /// let mut sz = ArchiveWriter::create("path/to/dest.7z").expect("create writer ok");
    /// let src = Path::new("path/to/source.txt");
    /// let name = "source.txt".to_string();
    /// let entry = sz
    ///     .push_archive_entry(
    ///         ArchiveEntry::from_path(&src, name),
    ///         Some(File::open(src).unwrap()),
    ///     )
    ///     .expect("ok");
    /// let compressed_size = entry.compressed_size;
    /// sz.finish().expect("done");
    /// ```
    pub fn push_archive_entry<R: Read>(
        &mut self,
        mut entry: ArchiveEntry,
        reader: Option<R>,
    ) -> Result<&ArchiveEntry> {
        if !entry.is_directory {
            if let Some(mut r) = reader {
                let mut compressed_len = 0;
                let mut compressed = CompressWrapWriter::new(&mut self.output, &mut compressed_len);

                let mut more_sizes: Vec<Rc<Cell<usize>>> =
                    Vec::with_capacity(self.content_methods.len() - 1);

                let (crc, size) = {
                    let mut w = Self::create_writer(
                        &self.content_methods,
                        &mut compressed,
                        &mut more_sizes,
                    )?;
                    let mut write_len = 0;
                    let mut w = CompressWrapWriter::new(&mut w, &mut write_len);
                    // Use 64KB buffer for better I/O performance
                    let mut buf = vec![0u8; crate::perf::DEFAULT_BUFFER_SIZE];
                    loop {
                        match r.read(&mut buf) {
                            Ok(n) => {
                                if n == 0 {
                                    break;
                                }
                                w.write_all(&buf[..n]).map_err(|e| {
                                    Error::io_msg(e, format!("Encode entry:{}", entry.name()))
                                })?;
                            }
                            Err(e) => {
                                return Err(Error::io_msg(
                                    e,
                                    format!("Encode entry:{}", entry.name()),
                                ));
                            }
                        }
                    }
                    w.flush()
                        .map_err(|e| Error::io_msg(e, format!("Encode entry:{}", entry.name())))?;
                    w.write(&[])
                        .map_err(|e| Error::io_msg(e, format!("Encode entry:{}", entry.name())))?;

                    (w.crc_value(), write_len)
                };
                let compressed_crc = compressed.crc_value();
                entry.has_stream = true;
                entry.size = size as u64;
                entry.crc = crc as u64;
                entry.has_crc = true;
                entry.compressed_crc = compressed_crc as u64;
                entry.compressed_size = compressed_len as u64;
                self.pack_info
                    .add_stream(compressed_len as u64, compressed_crc);

                let mut sizes = Vec::with_capacity(more_sizes.len() + 1);
                sizes.extend(more_sizes.iter().map(|s| s.get() as u64));
                sizes.push(size as u64);

                self.unpack_info
                    .add(self.content_methods.clone(), sizes, crc);

                self.files.push(entry);
                return Ok(self.files.last().unwrap());
            }
        }
        entry.has_stream = false;
        entry.size = 0;
        entry.compressed_size = 0;
        entry.has_crc = false;
        self.files.push(entry);
        Ok(self.files.last().unwrap())
    }

    /// High-performance non-solid compression for fast I/O sources.
    ///
    /// This method is optimized for maximum throughput when reading from fast sources
    /// like in-memory caches, NVMe drives, or high-speed networks. It uses larger buffers
    /// and can optionally use background reading to overlap I/O with compression.
    ///
    /// # Arguments
    /// * `entry` - The archive entry metadata
    /// * `reader` - Optional reader providing the entry's data
    /// * `buffer_size` - Size of the I/O buffer (use `perf::HYPER_BUFFER_SIZE` for max throughput)
    ///
    /// # Example
    /// ```no_run
    /// use std::{fs::File, path::Path};
    /// use sevenz_rust2::*;
    /// use sevenz_rust2::perf::HYPER_BUFFER_SIZE;
    ///
    /// let mut sz = ArchiveWriter::create("path/to/dest.7z").expect("create writer ok");
    /// let src = Path::new("path/to/source.txt");
    /// let entry = sz
    ///     .push_archive_entry_fast(
    ///         ArchiveEntry::from_path(&src, "source.txt".to_string()),
    ///         Some(File::open(src).unwrap()),
    ///         HYPER_BUFFER_SIZE,
    ///     )
    ///     .expect("ok");
    /// sz.finish().expect("done");
    /// ```
    pub fn push_archive_entry_fast<R: Read>(
        &mut self,
        mut entry: ArchiveEntry,
        reader: Option<R>,
        buffer_size: usize,
    ) -> Result<&ArchiveEntry> {
        if !entry.is_directory {
            if let Some(mut r) = reader {
                let mut compressed_len = 0;
                let mut compressed = CompressWrapWriter::new(&mut self.output, &mut compressed_len);

                let mut more_sizes: Vec<Rc<Cell<usize>>> =
                    Vec::with_capacity(self.content_methods.len() - 1);

                let (crc, size) = {
                    let mut w = Self::create_writer(
                        &self.content_methods,
                        &mut compressed,
                        &mut more_sizes,
                    )?;
                    let mut write_len = 0;
                    let mut w = CompressWrapWriter::new(&mut w, &mut write_len);
                    // Use configurable buffer size for high-performance I/O
                    let buffer_size = buffer_size.clamp(crate::perf::SMALL_BUFFER_SIZE, crate::perf::MAX_BUFFER_SIZE);
                    let mut buf = vec![0u8; buffer_size];
                    loop {
                        match r.read(&mut buf) {
                            Ok(n) => {
                                if n == 0 {
                                    break;
                                }
                                w.write_all(&buf[..n]).map_err(|e| {
                                    Error::io_msg(e, format!("Encode entry:{}", entry.name()))
                                })?;
                            }
                            Err(e) => {
                                return Err(Error::io_msg(
                                    e,
                                    format!("Encode entry:{}", entry.name()),
                                ));
                            }
                        }
                    }
                    w.flush()
                        .map_err(|e| Error::io_msg(e, format!("Encode entry:{}", entry.name())))?;
                    w.write(&[])
                        .map_err(|e| Error::io_msg(e, format!("Encode entry:{}", entry.name())))?;

                    (w.crc_value(), write_len)
                };
                let compressed_crc = compressed.crc_value();
                entry.has_stream = true;
                entry.size = size as u64;
                entry.crc = crc as u64;
                entry.has_crc = true;
                entry.compressed_crc = compressed_crc as u64;
                entry.compressed_size = compressed_len as u64;
                self.pack_info
                    .add_stream(compressed_len as u64, compressed_crc);

                let mut sizes = Vec::with_capacity(more_sizes.len() + 1);
                sizes.extend(more_sizes.iter().map(|s| s.get() as u64));
                sizes.push(size as u64);

                self.unpack_info
                    .add(self.content_methods.clone(), sizes, crc);

                self.files.push(entry);
                return Ok(self.files.last().unwrap());
            }
        }
        entry.has_stream = false;
        entry.size = 0;
        entry.compressed_size = 0;
        entry.has_crc = false;
        self.files.push(entry);
        Ok(self.files.last().unwrap())
    }

    /// Non-solid compression with progress callback - Adds an archive `entry` with data from `reader`.
    ///
    /// This method is similar to `push_archive_entry`, but allows monitoring compression progress
    /// through a callback function.
    ///
    /// # Arguments
    /// * `entry` - The archive entry metadata
    /// * `reader` - Optional reader providing the entry's data
    /// * `progress` - Progress callback that receives updates during compression
    /// * `progress_interval` - How often to call the progress callback (in bytes read)
    ///
    /// # Example
    /// ```no_run
    /// use std::{fs::File, path::Path};
    /// use sevenz_rust2::*;
    ///
    /// let mut sz = ArchiveWriter::create("path/to/dest.7z").expect("create writer ok");
    /// let src = Path::new("path/to/source.txt");
    /// let name = "source.txt".to_string();
    ///
    /// let progress_callback = FnProgressCallback::new(|info| {
    ///     println!("Read {} bytes, wrote {} bytes", info.bytes_read, info.bytes_written);
    ///     ProgressResult::Continue
    /// });
    ///
    /// let entry = sz
    ///     .push_archive_entry_with_progress(
    ///         ArchiveEntry::from_path(&src, name),
    ///         Some(File::open(src).unwrap()),
    ///         progress_callback,
    ///         64 * 1024, // Report every 64KB
    ///     )
    ///     .expect("ok");
    /// sz.finish().expect("done");
    /// ```
    pub fn push_archive_entry_with_progress<R: Read, P: progress::ProgressCallback>(
        &mut self,
        mut entry: ArchiveEntry,
        reader: Option<R>,
        mut progress: P,
        progress_interval: u64,
    ) -> Result<&ArchiveEntry> {
        if !entry.is_directory {
            if let Some(mut r) = reader {
                let mut compressed_len = 0;
                let mut compressed = CompressWrapWriter::new(&mut self.output, &mut compressed_len);

                let mut more_sizes: Vec<Rc<Cell<usize>>> =
                    Vec::with_capacity(self.content_methods.len() - 1);

                let entry_name = entry.name().to_string();
                let entry_index = self.files.len();

                let (crc, size) = {
                    let mut w = Self::create_writer(
                        &self.content_methods,
                        &mut compressed,
                        &mut more_sizes,
                    )?;
                    let mut write_len = 0;
                    let mut w = CompressWrapWriter::new(&mut w, &mut write_len);
                    // Use 64KB buffer for better I/O performance
                    let mut buf = vec![0u8; crate::perf::DEFAULT_BUFFER_SIZE];
                    let mut total_read: u64 = 0;
                    let mut last_progress_read: u64 = 0;

                    loop {
                        match r.read(&mut buf) {
                            Ok(n) => {
                                if n == 0 {
                                    break;
                                }
                                w.write_all(&buf[..n]).map_err(|e| {
                                    Error::io_msg(e, format!("Encode entry:{}", entry.name()))
                                })?;
                                total_read += n as u64;

                                // Call progress callback at specified intervals
                                // Note: bytes_written is 0 during compression because final
                                // compressed size is unknown until encoding completes
                                if total_read - last_progress_read >= progress_interval {
                                    let info = progress::ProgressInfo::new(total_read, 0)
                                        .with_entry(&entry_name)
                                        .with_index(entry_index);
                                    
                                    if progress.on_progress(&info) == progress::ProgressResult::Cancel {
                                        return Err(Error::other("Operation cancelled by progress callback"));
                                    }
                                    last_progress_read = total_read;
                                }
                            }
                            Err(e) => {
                                return Err(Error::io_msg(
                                    e,
                                    format!("Encode entry:{}", entry.name()),
                                ));
                            }
                        }
                    }

                    w.flush()
                        .map_err(|e| Error::io_msg(e, format!("Encode entry:{}", entry.name())))?;
                    w.write(&[])
                        .map_err(|e| Error::io_msg(e, format!("Encode entry:{}", entry.name())))?;

                    (w.crc_value(), write_len)
                };

                let compressed_crc = compressed.crc_value();

                // Final progress update with actual compressed size
                // (Cancellation ignored at this point since compression is complete)
                let info = progress::ProgressInfo::new(size as u64, compressed_len as u64)
                    .with_entry(&entry_name)
                    .with_index(entry_index);
                let _ = progress.on_progress(&info);

                entry.has_stream = true;
                entry.size = size as u64;
                entry.crc = crc as u64;
                entry.has_crc = true;
                entry.compressed_crc = compressed_crc as u64;
                entry.compressed_size = compressed_len as u64;
                self.pack_info
                    .add_stream(compressed_len as u64, compressed_crc);

                let mut sizes = Vec::with_capacity(more_sizes.len() + 1);
                sizes.extend(more_sizes.iter().map(|s| s.get() as u64));
                sizes.push(size as u64);

                self.unpack_info
                    .add(self.content_methods.clone(), sizes, crc);

                self.files.push(entry);
                return Ok(self.files.last().unwrap());
            }
        }
        entry.has_stream = false;
        entry.size = 0;
        entry.compressed_size = 0;
        entry.has_crc = false;
        self.files.push(entry);
        Ok(self.files.last().unwrap())
    }

    /// Solid compression - packs `entries` into one pack.
    ///
    /// # Panics
    /// * If `entries`'s length not equals to `reader.reader_len()`
    pub fn push_archive_entries<R: Read>(
        &mut self,
        entries: Vec<ArchiveEntry>,
        reader: Vec<SourceReader<R>>,
    ) -> Result<&mut Self> {
        let mut entries = entries;
        let mut r = SeqReader::new(reader);
        assert_eq!(r.reader_len(), entries.len());
        let mut compressed_len = 0;
        let mut compressed = CompressWrapWriter::new(&mut self.output, &mut compressed_len);
        let content_methods = &self.content_methods;
        let mut more_sizes: Vec<Rc<Cell<usize>>> = Vec::with_capacity(content_methods.len() - 1);

        let (crc, size) = {
            let mut w = Self::create_writer(content_methods, &mut compressed, &mut more_sizes)?;
            let mut write_len = 0;
            let mut w = CompressWrapWriter::new(&mut w, &mut write_len);
            // Use 64KB buffer for better I/O performance
            let mut buf = vec![0u8; crate::perf::DEFAULT_BUFFER_SIZE];

            fn entries_names(entries: &[ArchiveEntry]) -> String {
                let mut names = String::with_capacity(512);
                for ele in entries.iter() {
                    names.push_str(&ele.name);
                    names.push(';');
                    if names.len() > 512 {
                        break;
                    }
                }
                names
            }

            loop {
                match r.read(&mut buf) {
                    Ok(n) => {
                        if n == 0 {
                            break;
                        }
                        w.write_all(&buf[..n]).map_err(|e| {
                            Error::io_msg(e, format!("Encode entries:{}", entries_names(&entries)))
                        })?;
                    }
                    Err(e) => {
                        return Err(Error::io_msg(
                            e,
                            format!("Encode entries:{}", entries_names(&entries)),
                        ));
                    }
                }
            }
            w.flush().map_err(|e| {
                let mut names = String::with_capacity(512);
                for ele in entries.iter() {
                    names.push_str(&ele.name);
                    names.push(';');
                    if names.len() > 512 {
                        break;
                    }
                }
                Error::io_msg(e, format!("Encode entry:{names}"))
            })?;
            w.write(&[]).map_err(|e| {
                Error::io_msg(e, format!("Encode entry:{}", entries_names(&entries)))
            })?;

            (w.crc_value(), write_len)
        };
        let compressed_crc = compressed.crc_value();
        let mut sub_stream_crcs = Vec::with_capacity(entries.len());
        let mut sub_stream_sizes = Vec::with_capacity(entries.len());
        for i in 0..entries.len() {
            let entry = &mut entries[i];
            let ri = &r[i];
            entry.crc = ri.crc_value() as u64;
            entry.size = ri.read_count() as u64;
            sub_stream_crcs.push(entry.crc as u32);
            sub_stream_sizes.push(entry.size);
            entry.has_crc = true;
        }

        self.pack_info
            .add_stream(compressed_len as u64, compressed_crc);

        let mut sizes = Vec::with_capacity(more_sizes.len() + 1);
        sizes.extend(more_sizes.iter().map(|s| s.get() as u64));
        sizes.push(size as u64);

        self.unpack_info.add_multiple(
            content_methods.clone(),
            sizes,
            crc,
            entries.len() as u64,
            sub_stream_sizes,
            sub_stream_crcs,
        );

        self.files.extend(entries);
        Ok(self)
    }

    fn create_writer<'a, O: Write + 'a>(
        methods: &[EncoderConfiguration],
        out: O,
        more_sized: &mut Vec<Rc<Cell<usize>>>,
    ) -> Result<Box<dyn Write + 'a>> {
        let mut encoder: Box<dyn Write> = Box::new(out);
        let mut first = true;
        for mc in methods.iter() {
            if !first {
                let counting = CountingWriter::new(encoder);
                more_sized.push(counting.counting());
                encoder = Box::new(encoder::add_encoder(counting, mc)?);
            } else {
                let counting = CountingWriter::new(encoder);
                encoder = Box::new(encoder::add_encoder(counting, mc)?);
            }
            first = false;
        }
        Ok(encoder)
    }

    /// Finishes the compression.
    pub fn finish(mut self) -> std::io::Result<W> {
        let mut header: Vec<u8> = Vec::with_capacity(64 * 1024);
        self.write_encoded_header(&mut header)?;
        let header_pos = self.output.stream_position()?;
        self.output.write_all(&header)?;
        let crc32 = crc32fast::hash(&header);
        let mut hh = [0u8; SIGNATURE_HEADER_SIZE as usize];
        {
            let mut hhw = hh.as_mut_slice();
            //sig
            hhw.write_all(SEVEN_Z_SIGNATURE)?;
            //version
            hhw.write_u8(0)?;
            hhw.write_u8(4)?;
            //placeholder for crc: index = 8
            hhw.write_u32(0)?;

            // start header
            hhw.write_u64(header_pos - SIGNATURE_HEADER_SIZE)?;
            hhw.write_u64(0xFFFFFFFF & header.len() as u64)?;
            hhw.write_u32(crc32)?;
        }
        let crc32 = crc32fast::hash(&hh[12..]);
        hh[8..12].copy_from_slice(&crc32.to_le_bytes());

        self.output.seek(std::io::SeekFrom::Start(0))?;
        self.output.write_all(&hh)?;
        self.output.flush()?;
        Ok(self.output)
    }

    fn write_header<H: Write>(&mut self, header: &mut H) -> std::io::Result<()> {
        header.write_u8(K_HEADER)?;
        // Write archive properties if there's a comment
        if self.comment.is_some() {
            self.write_archive_properties(header)?;
        }
        header.write_u8(K_MAIN_STREAMS_INFO)?;
        self.write_streams_info(header)?;
        self.write_files_info(header)?;
        header.write_u8(K_END)?;
        Ok(())
    }

    fn write_archive_properties<H: Write>(&self, header: &mut H) -> std::io::Result<()> {
        header.write_u8(K_ARCHIVE_PROPERTIES)?;
        if let Some(comment) = &self.comment {
            // Write comment property
            header.write_u8(K_COMMENT)?;

            // Convert comment to UTF-16LE with null terminator
            let utf16: Vec<u16> = comment.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes: Vec<u8> = utf16.iter().flat_map(|&c| c.to_le_bytes()).collect();

            write_u64(header, bytes.len() as u64)?;
            header.write_all(&bytes)?;
        }
        header.write_u8(K_END)?;
        Ok(())
    }

    fn write_encoded_header<H: Write>(&mut self, header: &mut H) -> std::io::Result<()> {
        let mut raw_header = Vec::with_capacity(64 * 1024);
        self.write_header(&mut raw_header)?;
        let mut pack_info = PackInfo::default();

        let position = self.output.stream_position()?;
        let pos = position - SIGNATURE_HEADER_SIZE;
        pack_info.pos = pos;

        let mut more_sizes = vec![];
        let size = raw_header.len() as u64;
        let crc32 = crc32fast::hash(&raw_header);
        let mut methods = vec![];

        if self.encrypt_header {
            for conf in self.content_methods.iter() {
                if conf.method.id() == EncoderMethod::AES256_SHA256.id() {
                    methods.push(conf.clone());
                    break;
                }
            }
        }

        methods.push(EncoderConfiguration::new(EncoderMethod::LZMA));

        let methods = Arc::new(methods);

        let mut encoded_data = Vec::with_capacity(size as usize / 2);

        let mut compress_size = 0;
        let mut compressed = CompressWrapWriter::new(&mut encoded_data, &mut compress_size);
        {
            let mut encoder = Self::create_writer(&methods, &mut compressed, &mut more_sizes)
                .map_err(std::io::Error::other)?;
            encoder.write_all(&raw_header)?;
            encoder.flush()?;
            let _ = encoder.write(&[])?;
        }

        let compress_crc = compressed.crc_value();
        let compress_size = *compressed.bytes_written;
        if compress_size as u64 + 20 >= size {
            // compression made it worse. Write raw data
            header.write_all(&raw_header)?;
            return Ok(());
        }
        self.output.write_all(&encoded_data[..compress_size])?;

        pack_info.add_stream(compress_size as u64, compress_crc);

        let mut unpack_info = UnpackInfo::default();
        let mut sizes = Vec::with_capacity(1 + more_sizes.len());
        sizes.extend(more_sizes.iter().map(|s| s.get() as u64));
        sizes.push(size);
        unpack_info.add(methods, sizes, crc32);

        header.write_u8(K_ENCODED_HEADER)?;

        pack_info.write_to(header)?;
        unpack_info.write_to(header)?;
        unpack_info.write_substreams(header)?;

        header.write_u8(K_END)?;

        Ok(())
    }

    fn write_streams_info<H: Write>(&mut self, header: &mut H) -> std::io::Result<()> {
        if self.pack_info.len() > 0 {
            self.pack_info.write_to(header)?;
            self.unpack_info.write_to(header)?;
        }
        self.unpack_info.write_substreams(header)?;

        header.write_u8(K_END)?;
        Ok(())
    }

    fn write_files_info<H: Write>(&self, header: &mut H) -> std::io::Result<()> {
        header.write_u8(K_FILES_INFO)?;
        write_u64(header, self.files.len() as u64)?;
        self.write_file_empty_streams(header)?;
        self.write_file_empty_files(header)?;
        self.write_file_anti_items(header)?;
        self.write_file_names(header)?;
        self.write_file_ctimes(header)?;
        self.write_file_atimes(header)?;
        self.write_file_mtimes(header)?;
        self.write_file_windows_attrs(header)?;
        header.write_u8(K_END)?;
        Ok(())
    }

    fn write_file_empty_streams<H: Write>(&self, header: &mut H) -> std::io::Result<()> {
        let mut has_empty = false;
        for entry in self.files.iter() {
            if !entry.has_stream {
                has_empty = true;
                break;
            }
        }
        if has_empty {
            header.write_u8(K_EMPTY_STREAM)?;
            let mut bitset = BitSet::with_capacity(self.files.len());
            for (i, entry) in self.files.iter().enumerate() {
                if !entry.has_stream {
                    bitset.insert(i);
                }
            }
            let mut temp: Vec<u8> = Vec::with_capacity(bitset.len() / 8 + 1);
            write_bit_set(&mut temp, &bitset)?;
            write_u64(header, temp.len() as u64)?;
            header.write_all(temp.as_slice())?;
        }
        Ok(())
    }

    fn write_file_empty_files<H: Write>(&self, header: &mut H) -> std::io::Result<()> {
        let mut has_empty = false;
        let mut empty_stream_counter = 0;
        let mut bitset = BitSet::new();
        for entry in self.files.iter() {
            if !entry.has_stream {
                let is_dir = entry.is_directory();
                has_empty |= !is_dir;
                if !is_dir {
                    bitset.insert(empty_stream_counter);
                }
                empty_stream_counter += 1;
            }
        }
        if has_empty {
            header.write_u8(K_EMPTY_FILE)?;

            let mut temp: Vec<u8> = Vec::with_capacity(bitset.len() / 8 + 1);
            write_bit_set(&mut temp, &bitset)?;
            write_u64(header, temp.len() as u64)?;
            header.write_all(&temp)?;
        }
        Ok(())
    }

    fn write_file_anti_items<H: Write>(&self, header: &mut H) -> std::io::Result<()> {
        let mut has_anti = false;
        let mut counter = 0;
        let mut bitset = BitSet::new();
        for entry in self.files.iter() {
            if !entry.has_stream {
                let is_anti = entry.is_anti_item();
                has_anti |= !is_anti;
                if !is_anti {
                    bitset.insert(counter);
                }
                counter += 1;
            }
        }
        if has_anti {
            header.write_u8(K_ANTI)?;

            let mut temp: Vec<u8> = Vec::with_capacity(bitset.len() / 8 + 1);
            write_bit_set(&mut temp, &bitset)?;
            write_u64(header, temp.len() as u64)?;
            header.write_all(temp.as_slice())?;
        }
        Ok(())
    }

    fn write_file_names<H: Write>(&self, header: &mut H) -> std::io::Result<()> {
        header.write_u8(K_NAME)?;
        let mut temp: Vec<u8> = Vec::with_capacity(128);
        let out = &mut temp;
        out.write_u8(0)?;
        for file in self.files.iter() {
            for c in file.name().encode_utf16() {
                let buf = c.to_le_bytes();
                out.write_all(&buf)?;
            }
            out.write_all(&[0u8; 2])?;
        }
        write_u64(header, temp.len() as u64)?;
        header.write_all(temp.as_slice())?;
        Ok(())
    }

    write_times!(
        write_file_ctimes,
        K_C_TIME,
        has_creation_date,
        creation_date
    );
    write_times!(write_file_atimes, K_A_TIME, has_access_date, access_date);
    write_times!(
        write_file_mtimes,
        K_M_TIME,
        has_last_modified_date,
        last_modified_date
    );
    write_times!(
        write_file_windows_attrs,
        K_WIN_ATTRIBUTES,
        has_windows_attributes,
        windows_attributes,
        write_u32
    );
}

impl<W: Write + Seek> AutoFinish for ArchiveWriter<W> {
    fn finish_ignore_error(self) {
        let _ = self.finish();
    }
}

#[inline]
pub(crate) fn write_u64<W: Write>(header: &mut W, mut value: u64) -> std::io::Result<()> {
    let mut first = 0;
    let mut mask = 0x80;
    let mut i = 0;
    while i < 8 {
        if value < (1u64 << (7 * (i + 1))) {
            first |= value >> (8 * i);
            break;
        }
        first |= mask;
        mask >>= 1;
        i += 1;
    }
    header.write_u8((first & 0xFF) as u8)?;
    while i > 0 {
        header.write_u8((value & 0xFF) as u8)?;
        value >>= 8;
        i -= 1;
    }
    Ok(())
}

struct CompressWrapWriter<'a, W> {
    writer: W,
    crc: Hasher,
    bytes_written: &'a mut usize,
}

impl<'a, W: Write> CompressWrapWriter<'a, W> {
    pub fn new(writer: W, bytes_written: &'a mut usize) -> Self {
        Self {
            writer,
            crc: Hasher::new(),
            bytes_written,
        }
    }

    pub fn crc_value(&mut self) -> u32 {
        let crc = std::mem::replace(&mut self.crc, Hasher::new());
        crc.finalize()
    }
}

impl<W: Write> Write for CompressWrapWriter<'_, W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = self.writer.write(buf)?;
        self.crc.update(&buf[..len]);
        *self.bytes_written += len;
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
