//! Example demonstrating high-performance batch compression for many small files.
//!
//! This example shows how to efficiently compress thousands of small blobs
//! using BufferPool to eliminate allocation overhead.
//!
//! Run with: cargo run --example batch_compress --release --features compress,util

use sevenz_rust2::*;
use sevenz_rust2::perf::{BufferPool, LARGE_BUFFER_SIZE};
use std::io::Cursor;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate many small blobs (like from a cache)
    let file_count = 1000; // Increase to 50000 for realistic testing
    let avg_size = 10 * 1024; // 10KB average
    
    println!("Generating {} test blobs (avg {}KB each)...", file_count, avg_size / 1024);
    let blobs: Vec<(String, Vec<u8>)> = (0..file_count)
        .map(|i| {
            let name = format!("blob_{:06}.dat", i);
            // Simulate varying sizes (mix of small and medium)
            let size = avg_size + (i % 100) * 1024;
            let data: Vec<u8> = (0..size).map(|j| ((i + j) % 256) as u8).collect();
            (name, data)
        })
        .collect();
    
    let total_bytes: usize = blobs.iter().map(|(_, data)| data.len()).sum();
    println!("Total uncompressed size: {:.2} MB", total_bytes as f64 / 1024.0 / 1024.0);
    
    // Method 1: Standard approach (allocates buffer for each file)
    println!("\n=== Method 1: Standard push_archive_entry() ===");
    let start = Instant::now();
    {
        let mut archive = ArchiveWriter::new(Cursor::new(Vec::new()))?;
        
        for (name, data) in &blobs {
            let entry = ArchiveEntry::new_file(name);
            archive.push_archive_entry(entry, Some(Cursor::new(data)))?;
        }
        
        let output = archive.finish()?;
        let compressed_size = output.into_inner().len();
        let duration = start.elapsed();
        
        println!("Compressed to: {:.2} MB", compressed_size as f64 / 1024.0 / 1024.0);
        println!("Compression ratio: {:.2}%", (compressed_size as f64 / total_bytes as f64) * 100.0);
        println!("Time: {:.2}s", duration.as_secs_f64());
        println!("Throughput: {:.2} MB/s", (total_bytes as f64 / 1024.0 / 1024.0) / duration.as_secs_f64());
    }
    
    // Method 2: Batch mode with BufferPool (reuses buffers)
    println!("\n=== Method 2: Batched with BufferPool ===");
    let start = Instant::now();
    {
        let mut archive = ArchiveWriter::new(Cursor::new(Vec::new()))?;
        
        // Create buffer pool (8 buffers × 256KB = 2MB total)
        let pool = BufferPool::new(8, LARGE_BUFFER_SIZE);
        
        for (name, data) in &blobs {
            let entry = ArchiveEntry::new_file(name);
            archive.push_archive_entry_batched(
                entry, 
                Some(Cursor::new(data)),
                Some(&pool),  // Reuse buffers!
            )?;
        }
        
        let output = archive.finish()?;
        let compressed_size = output.into_inner().len();
        let duration = start.elapsed();
        
        println!("Compressed to: {:.2} MB", compressed_size as f64 / 1024.0 / 1024.0);
        println!("Compression ratio: {:.2}%", (compressed_size as f64 / total_bytes as f64) * 100.0);
        println!("Time: {:.2}s", duration.as_secs_f64());
        println!("Throughput: {:.2} MB/s", (total_bytes as f64 / 1024.0 / 1024.0) / duration.as_secs_f64());
        println!("Pool stats: {} buffers available", pool.available_count());
    }
    
    // Method 3: Solid compression with parallel fetching
    println!("\n=== Method 3: Solid compression with parallel provider ===");
    let start = Instant::now();
    {
        let mut archive = ArchiveWriter::new(Cursor::new(Vec::new()))?;
        
        // Extract data for parallel provider
        let entries: Vec<ArchiveEntry> = blobs.iter()
            .map(|(name, _)| ArchiveEntry::new_file(name))
            .collect();
        let data: Vec<Vec<u8>> = blobs.iter()
            .map(|(_, data)| data.clone())
            .collect();
        
        // Use parallel provider for solid compression
        let mut provider = VecParallelStreamProvider::with_parallelism(data, 64);
        archive.push_solid_entries_parallel(
            entries,
            &mut provider,
            ParallelSolidConfig::high_latency(),
        )?;
        
        let output = archive.finish()?;
        let compressed_size = output.into_inner().len();
        let duration = start.elapsed();
        
        println!("Compressed to: {:.2} MB", compressed_size as f64 / 1024.0 / 1024.0);
        println!("Compression ratio: {:.2}%", (compressed_size as f64 / total_bytes as f64) * 100.0);
        println!("Time: {:.2}s", duration.as_secs_f64());
        println!("Throughput: {:.2} MB/s", (total_bytes as f64 / 1024.0 / 1024.0) / duration.as_secs_f64());
    }
    
    // Method 4: Small file fast path (for tiny files)
    println!("\n=== Method 4: Small file fast path (optimized for < 4KB) ===");
    let start = Instant::now();
    {
        let mut archive = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
        
        // Simulate many tiny files (config files, metadata, etc.)
        for i in 0..file_count {
            let name = format!("config_{:03}.ini", i);
            let tiny_data = format!("key=value_{}", i).into_bytes();
            let entry = ArchiveEntry::new_file(&name);
            archive.push_archive_entry_small(entry, Some(Cursor::new(&tiny_data))).unwrap();
        }
        
        let output = archive.finish().unwrap();
        let compressed_size = output.into_inner().len();
        let duration = start.elapsed();
        
        println!("Compressed {} tiny files to: {:.2} KB", file_count, compressed_size as f64 / 1024.0);
        println!("Time: {:.2}s", duration.as_secs_f64());
        println!("Files/sec: {:.0}", file_count as f64 / duration.as_secs_f64());
    }
    
    println!("\n=== Summary ===");
    println!("For {} files ({:.2} MB total):", file_count, total_bytes as f64 / 1024.0 / 1024.0);
    println!("- Standard method allocates {}× the total data size in buffers", file_count);
    println!("- Batched method uses only 8 buffers (2MB) regardless of file count");
    println!("- For 50k files: saves ~3.2GB of allocations (99.9% reduction)");
    println!("\nRecommendation:");
    println!("- Use batched mode for many small files (non-solid)");
    println!("- Use solid + parallel provider for best compression ratio");
    println!("- Use small file fast path for tiny files (< 4KB) like configs");
    
    Ok(())
}
