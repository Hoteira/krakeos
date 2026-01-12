use std::fs;
use std::time::{SystemTime, Duration};
use std::io::{Read, Write, Seek, SeekFrom};
use std::thread;

fn main() {
    println!("--- KrakeOS WASI Feature Test ---");

    // 1. Monotonic Time Test
    println!("\n[1] Testing Time Forward Movement:");
    let t1 = SystemTime::now();
    thread::sleep(Duration::from_millis(100));
    let t2 = SystemTime::now();
    match t2.duration_since(t1) {
        Ok(d) => println!("  Time moved forward by {:?} ms", d.as_millis()),
        Err(e) => println!("  CLOCK ERROR: {:?}", e),
    }

    // 2. Set Size Test (fd_filestat_set_size)
    println!("\n[2] Testing Set Size (truncate):");
    let test_file = "/truncate_test.txt";
    {
        let mut f = fs::File::create(test_file).unwrap();
        f.write_all(b"Hello World").unwrap();
    }
    
    let meta_before = fs::metadata(test_file).unwrap();
    println!("  Size before: {} bytes", meta_before.len());
    
    {
        let f = fs::OpenOptions::new().write(true).open(test_file).unwrap();
        f.set_len(5).unwrap();
        f.sync_all().unwrap(); // Test fd_sync
    }
    
    let meta_after = fs::metadata(test_file).unwrap();
    println!("  Size after set_len(5): {} bytes", meta_after.len());
    if meta_after.len() == 5 {
        println!("  TRUNCATE SUCCESS!");
    } else {
        println!("  TRUNCATE FAILED!");
    }

    println!("\n--- WASI Feature Test Finished ---");
}
