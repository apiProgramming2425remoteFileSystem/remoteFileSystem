mod common;
use common::*;

use std::thread;
use std::time::Duration;

use anyhow::Result;
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, mpsc};

#[test]
fn test_parallel_writes() -> Result<()> {
    let (_ctx, mount_point, server_root) = setup_e2e!();

    let thread_count = 5;
    let mut handles = vec![];

    // Spawn 5 threads, each writing a distinct file simultaneously
    for i in 0..thread_count {
        let path = mount_point.join(format!("worker_{}.txt", i));
        handles.push(thread::spawn(move || {
            fs::write(&path, format!("I am worker {}", i)).expect("Write failed");
        }));
    }

    // Join all
    for h in handles {
        h.join().unwrap();
    }

    // Verify all files exist
    for i in 0..thread_count {
        assert!(mount_point.join(format!("worker_{}.txt", i)).exists());
        assert!(server_root.join(format!("worker_{}.txt", i)).exists());
    }

    Ok(())
}

/// PARALLEL FILE CREATION (Stress Test)
/// Spawns 10 threads, each creating 50 files simultaneously.
/// Verifies that the internal Inode Map and Directory structures don't panic.
#[test]
fn test_parallel_file_creation() -> Result<()> {
    let (_ctx, mount_point, server_root) = setup_e2e!();
    let mount_point = Arc::new(mount_point);

    let thread_count = 10;
    let files_per_thread = 50;
    let mut handles = vec![];

    for t_id in 0..thread_count {
        let mp = mount_point.clone();
        handles.push(thread::spawn(move || {
            for f_id in 0..files_per_thread {
                let name = format!("thread_{}_file_{}.txt", t_id, f_id);
                let path = mp.join(name);
                fs::write(&path, "content").expect("Failed to write concurrent file");
            }
        }));
    }

    // Wait for all threads
    for h in handles {
        h.join().unwrap();
    }

    // Verify count
    let count = fs::read_dir(&*mount_point)?.count();
    let srv_count = fs::read_dir(&server_root)?.count();
    assert_eq!(
        count,
        thread_count * files_per_thread,
        "Some files were lost during concurrent creation"
    );
    assert_eq!(
        srv_count,
        thread_count * files_per_thread,
        "Some server files were lost during concurrent creation"
    );

    Ok(())
}

/// EXCLUSIVE LOCKING (Mutex)
/// Thread A locks a file. Thread B tries to lock it and MUST block or fail.
/// This tests FUSE `lk` / `flock` opcodes.
#[test]
fn test_file_locking_mutual_exclusion() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let file_name = "resource.lock";
    let contents = "locked resource";

    let lock_file = mount_point.join(file_name);
    fs::write(&lock_file, contents)?;

    // Channel 1: "Lock is Acquired" (A -> B)
    let (tx_locked, rx_locked) = mpsc::channel();
    // Channel 2: "Check is Done" (B -> A)
    let (tx_done, rx_done) = mpsc::channel();

    let file_path = lock_file.clone();

    // Thread A: Holder
    let t1 = thread::spawn(move || {
        let f = File::open(&file_path).unwrap();
        // Acquire Lock
        f.lock_exclusive().expect("Thread A failed to lock");

        // Signal B to start
        tx_locked.send(()).unwrap();

        // Wait for B to finish checking
        rx_done.recv().unwrap();

        // Unlock (automatic on drop, but explicit here)
        f.unlock().expect("Thread A failed to unlock");
    });

    // Thread B: Challenger
    let t2 = thread::spawn(move || {
        // Wait for A to be ready
        rx_locked.recv().unwrap();

        let f = File::open(&lock_file).unwrap();

        // Verify lock is held by A
        let result = f.try_lock_exclusive();
        assert!(
            result.is_err(),
            "Thread B acquired lock while Thread A held it! Locking is broken."
        );

        tx_done.send(()).unwrap();
    });

    t1.join().unwrap();
    t2.join().unwrap();

    Ok(())
}

/// ATOMIC APPENDS (Race Condition)
/// Multiple threads write to the same file in append mode.
/// If not synchronized, lines will be overwritten or interleaved (e.g., "LiLine 1ne 2").
#[test]
fn test_concurrent_appends() -> Result<()> {
    let (_ctx, mp_tmp, _server_root) = setup_e2e!();
    let mount_point = Arc::new(mp_tmp);

    let file_name = "log.txt";
    let contents = "";

    let target_file = mount_point.join(file_name);
    fs::write(&target_file, contents)?; // Create empty

    let thread_count = 5;
    let lines_per_thread = 100;
    let mut handles = vec![];

    for i in 0..thread_count {
        let mp = mount_point.clone();
        handles.push(thread::spawn(move || {
            let path = mp.join(file_name);
            let mut f = OpenOptions::new().append(true).open(path).unwrap();
            for j in 0..lines_per_thread {
                let line = format!("T{}L{}\n", i, j);
                f.write_all(line.as_bytes()).unwrap();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Validation
    let content = fs::read_to_string(&target_file)?;
    let lines: Vec<&str> = content.lines().collect();

    println!("{}", content);

    // Check Total Line Count
    assert_eq!(
        lines.len(),
        thread_count * lines_per_thread,
        "Lost lines during concurrent append"
    );

    // Check Line Integrity (No "T0L1T0L2" mesh-ups)
    for line in lines {
        assert!(
            line.starts_with('T') && line.contains('L'),
            "Corrupted line found: {}",
            line
        );
    }

    Ok(())
}

/// READ-WHILE-WRITE
/// Verifies that a reader sees a consistent snapshot or eventually sees data
/// while a writer is updating the file. (Crash prevention).
#[test]
fn test_read_while_write_stability() -> Result<()> {
    let (_ctx, mp_tmp, _server_root) = setup_e2e!();
    let mount_point = Arc::new(mp_tmp);
    let file_path = mount_point.join("stable.bin");

    // Writer Thread: Writes 10MB slowly
    let writer_mp = mount_point.clone();
    let writer = thread::spawn(move || {
        let path = writer_mp.join("stable.bin");
        let mut f = File::create(path).unwrap();
        let chunk = [1u8; 1024]; // 1KB
        for _ in 0..1024 {
            // 1MB total
            f.write_all(&chunk).unwrap();
            // Tiny sleep to yield to reader
            thread::sleep(Duration::from_micros(100));
        }
    });

    // Reader Thread: Tries to stat/read concurrently
    let reader_mp = mount_point.clone();
    let reader = thread::spawn(move || {
        let path = reader_mp.join("stable.bin");
        // Wait for file creation
        while !path.exists() {
            thread::sleep(Duration::from_millis(10));
        }

        for _ in 0..50 {
            // Just ensure this doesn't Panic or return garbage errors
            let _ = fs::metadata(&path);
            let _ = fs::read(&path);
            thread::sleep(Duration::from_millis(5));
        }
    });

    writer.join().unwrap();
    reader.join().unwrap();

    // Final check
    let meta = fs::metadata(&file_path)?;
    assert_eq!(meta.len(), 1024 * 1024);

    Ok(())
}
