mod common;

// #[cfg(unix)]
mod tests {
    use super::common::*;
    use crate::setup_e2e;

    use anyhow::{Result, anyhow};
    use std::fs::{self, File};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::Path;
    use std::process::Command;
    use std::thread;

    /// Helper to calculate MD5 checksum
    fn compare_md5<P: AsRef<Path>>(src: P, dest: P) -> Result<(String, String)> {
        let (out_src, out_dest) = compare_command_outputs(
            "md5sum",
            ["-b"],
            src.as_ref().as_os_str(),
            dest.as_ref().as_os_str(),
        )?;

        // function processes the output of md5sum which is in the format "hash  filename"
        let extract_hash = |s: String| s.split_whitespace().next().unwrap_or("").to_string();

        Ok((extract_hash(out_src), extract_hash(out_dest)))
    }

    /// Helper to generate a large random file efficiently using `dd`
    fn generate_large_file(path: &Path, size_mb: usize) -> Result<()> {
        let status = Command::new("dd")
            .args([
                "if=/dev/urandom",
                &format!("of={}", path.to_str().ok_or(anyhow!("Invalid path"))?),
                "bs=1M",
                &format!("count={}", size_mb),
                "status=none",
            ])
            .status()
            .map_err(|e| anyhow!("Failed to generate large file with dd: {}", e))?;

        if !status.success() {
            return Err(anyhow!("dd command failed"));
        }
        Ok(())
    }

    fn assert_copy_cmd(src: &Path, dest: &Path) {
        let status = Command::new("cp").arg(src).arg(dest).status().unwrap();
        assert!(status.success(), "Copy command failed");
    }

    /// LARGE FILE INTEGRITY
    /// Creates a large file (50MB), copies it to the mount
    /// and verifies checksums to ensure no corruption during transfer.
    /// This tests the Client's read/write streaming logic.
    #[test]
    fn test_large_file_integrity() -> Result<()> {
        // Setup system
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let file_name = "large_test.bin";
        let src_file = std::env::temp_dir().join(file_name);
        let dest_file = mount_point.join(file_name);

        // Generate 50MB random file
        let timer = std::time::Instant::now();
        generate_large_file(Path::new(&src_file), 50)?;
        println!("Generated large file in {:.3?}", timer.elapsed());

        // Copy to Fuse Mount
        let timer = std::time::Instant::now();
        assert_copy_cmd(&src_file, &dest_file);
        println!("Copied large file in {:.3?}", timer.elapsed());

        thread::sleep(std::time::Duration::from_millis(50));

        // Verify Checksums
        // We calculate the checksum of the file ON THE MOUNT
        let (md5_src, md5_dest) = compare_md5(&src_file, &dest_file)?;

        assert_eq!(
            md5_src, md5_dest,
            "File corruption detected during large transfer"
        );

        fs::remove_file(&src_file)?;

        Ok(())
    }

    /// LARGE UPLOAD (Write)
    /// Writes a 100MB file to the mount and verifies integrity on the server.
    /// This tests the Client's `write` loop and chunking logic.
    #[test]
    fn test_large_file_upload_integrity() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let src_file = std::env::temp_dir().join("source_100mb.bin");
        let file_name = "dest_100mb.bin";
        let dest_file = mount_point.join(file_name);

        // Generate 100MB source
        generate_large_file(&src_file, 100)?;
        // Copy to Mount
        assert_copy_cmd(&src_file, &dest_file);

        thread::sleep(std::time::Duration::from_millis(50));

        let (src_hash, dest_hash) = compare_md5(&src_file, &dest_file)?;
        assert_eq!(src_hash, dest_hash, "Mount file corrupted after upload");

        // Verify Integrity on Server Backing Store
        let server_file = server_root.join(file_name);
        let (src_hash, server_hash) = compare_md5(&src_file, &server_file)?;
        assert_eq!(src_hash, server_hash, "Upload corrupted the file content");

        assert_eq!(
            dest_hash, server_hash,
            "Mount and Server files differ after upload"
        );

        fs::remove_file(&src_file)?;

        Ok(())
    }

    /// LARGE DOWNLOAD (Read)
    /// Reads a 100MB file from the server.
    /// This tests the Client's `read` loop and cache handling.
    #[test]
    fn test_large_file_download_integrity() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        // Setup: Create file directly on Server
        let file_name = "server_data.bin";
        let server_file = server_root.join(file_name);
        generate_large_file(&server_file, 50)?; // 50MB

        // Action: Read from Mount
        let client_file = mount_point.join(file_name);

        let (server_hash, client_hash) = compare_md5(&server_file, &client_file)?;
        assert_eq!(
            server_hash, client_hash,
            "Download corrupted the file content"
        );

        Ok(())
    }

    /// RANDOM ACCESS (Seeking)
    /// Reads small chunks from random offsets.
    /// This validates `lseek` support and that the client doesn't download the whole file for one byte.
    #[test]
    fn test_random_access_read() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let file_name = "seek_test.txt";
        let src_file = std::env::temp_dir().join(file_name);
        let dest_file = mount_point.join(file_name);
        generate_large_file(&src_file, 50)?;

        // Copy to Mount
        assert_copy_cmd(&src_file, &dest_file);

        thread::sleep(std::time::Duration::from_millis(50));

        let mut original_file = File::open(&src_file)?;
        let mut file = File::open(&dest_file)?;

        for offset in [0, 10, 500] {
            // Read 5 bytes from offset
            original_file.seek(SeekFrom::Start(offset))?;
            let mut buffer = [0u8; 5];
            original_file.read_exact(&mut buffer)?;

            file.seek(SeekFrom::Start(offset))?;
            let mut client_buffer = [0u8; 5];
            file.read_exact(&mut client_buffer)?;

            assert_eq!(
                &buffer, &client_buffer,
                "Data mismatch at offset {}",
                offset
            );
        }

        // Read from offset 500
        for offset in [500, 10, 1] {
            original_file.seek(SeekFrom::End(-offset))?;
            let mut buffer = [0u8; 5];
            original_file.read_exact(&mut buffer)?;

            file.seek(SeekFrom::End(-offset))?;
            let mut client_buffer = [0u8; 5];
            file.read_exact(&mut client_buffer)?;

            assert_eq!(
                &buffer, &client_buffer,
                "Data mismatch at offset -{}",
                offset
            );
        }

        fs::remove_file(&src_file)?;

        Ok(())
    }

    /// SPARSE FILES / HOLES
    /// Writes to the beginning and very end of a file, leaving a "hole" in the middle.
    /// Linux filesystems should treat the middle as zeros.
    #[test]
    fn test_sparse_file_writing() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();
        let file_path = mount_point.join("sparse.bin");

        let mut file = File::create(&file_path)?;

        // Write Start
        file.write_all(b"HEAD")?;

        // Create a 10MB hole (Seek past end)
        let max_file_size = 10 * 1024 * 1024;
        file.seek(SeekFrom::Start(max_file_size))?;

        // Write End
        file.write_all(b"TAIL")?;

        thread::sleep(std::time::Duration::from_millis(50));

        // Verify Size
        let meta = fs::metadata(&file_path)?;

        // Size should be 10MB + 4 bytes (TAIL)
        assert!(
            meta.len() >= max_file_size + 4,
            "File size too small for sparse test"
        );

        // Read the hole (should be zeros)
        let mut file_read = File::open(&file_path)?;
        file_read.seek(SeekFrom::Start(100))?; // Inside the hole
        let mut buffer = [0u8; 4];
        file_read.read_exact(&mut buffer)?;
        assert_eq!(buffer, [0, 0, 0, 0], "Sparse hole contained non-zero data!");

        Ok(())
    }

    /// CONCURRENT STREAMING
    /// Simulates two processes streaming data simultaneously.
    /// Ensures the client doesn't cross-wire data between handles.
    #[test]
    fn test_concurrent_streaming() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let file1 = mount_point.join("stream1.bin");
        let file2 = mount_point.join("stream2.bin");

        // Create sources
        let src1 = std::env::temp_dir().join("src1.bin");
        let src2 = std::env::temp_dir().join("src2.bin");
        generate_large_file(&src1, 20)?;
        generate_large_file(&src2, 20)?;

        let src1_clone = src1.clone();
        let file1_clone = file1.clone();

        // Spawn Thread 1: Copy File 1
        let t1 = thread::spawn(move || {
            assert_copy_cmd(&src1_clone, &file1_clone);
        });

        // Main Thread: Copy File 2
        assert_copy_cmd(&src2, &file2);

        t1.join().unwrap();

        // Verify both files
        let (s1_hash, d1_hash) = compare_md5(&src1, &file1)?;
        assert_eq!(s1_hash, d1_hash, "Concurrent stream 1 corrupted");

        let (s2_hash, d2_hash) = compare_md5(&src2, &file2)?;
        assert_eq!(s2_hash, d2_hash, "Concurrent stream 2 corrupted");

        fs::remove_file(&src1)?;
        fs::remove_file(&src2)?;

        Ok(())
    }
}
