use std::env;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

fn main() {
    println!("=== KrakeOS WASI Compliance Test Suite ===");

    test_stdio();
    test_args_and_env();
    test_clocks_and_sleep();
    test_filesystem_basics();
    test_filesystem_directory_ops();
    test_filesystem_metadata_and_truncate();
    
    println!("\n=== All Tests Completed ===");
}

fn test_stdio() {
    println!("\n[TEST] Standard I/O");
    println!("  Writing to stdout...");
    eprintln!("  Writing to stderr...");
    // We can't easily test stdin automation here without an input feeder.
    println!("  (Skipping stdin test)");
}

fn test_args_and_env() {
    println!("\n[TEST] Args & Environment");
    println!("  Args:");
    for (i, arg) in env::args().enumerate() {
        println!("    [{}] {}", i, arg);
    }
    
    println!("  Env Vars:");
    let vars: Vec<(String, String)> = env::vars().collect();
    if vars.is_empty() {
        println!("    (No environment variables found)");
    } else {
        for (k, v) in vars.iter().take(5) { // Print first 5 to avoid spam
            println!("    {}={}", k, v);
        }
        if vars.len() > 5 {
            println!("    ... and {} more", vars.len() - 5);
        }
    }
}

fn test_clocks_and_sleep() {
    println!("\n[TEST] Clocks & Sleep");
    
    // System Time
    let sys_time = SystemTime::now();
    println!("  SystemTime::now(): {:?}", sys_time);
    
    // Monotonic Time
    let start = Instant::now();
    println!("  Sleeping for 100ms...");
    thread::sleep(Duration::from_millis(100));
    let elapsed = start.elapsed();
    println!("  Elapsed: {:?} (Expected ~100ms)", elapsed);
    
    if elapsed.as_millis() < 50 || elapsed.as_millis() > 500 {
        println!("  [WARN] Sleep duration seems off!");
    } else {
        println!("  [PASS] Sleep duration acceptable.");
    }
}

fn test_filesystem_basics() {
    println!("\n[TEST] Filesystem: Basics (Write/Read)");
    let filename = "test_file.txt";
    
    // Cleanup
    let _ = fs::remove_file(filename);

    // Create & Write
    {
        let mut file = match fs::File::create(filename) {
            Ok(f) => f,
            Err(e) => {
                println!("  [FAIL] Failed to create file: {}", e);
                return;
            }
        };
        if let Err(e) = file.write_all(b"Hello KrakeOS") {
            println!("  [FAIL] Failed to write to file: {}", e);
            return;
        }
        println!("  Created and wrote to '{}'", filename);
    }

    // Read
    {
        let mut file = match fs::File::open(filename) {
            Ok(f) => f,
            Err(e) => {
                println!("  [FAIL] Failed to open file: {}", e);
                return;
            }
        };
        let mut content = String::new();
        if let Err(e) = file.read_to_string(&mut content) {
            println!("  [FAIL] Failed to read file: {}", e);
            return;
        }
        if content == "Hello KrakeOS" {
            println!("  [PASS] Read content matches written content.");
        } else {
            println!("  [FAIL] Content mismatch. Got: '{}'", content);
        }
    }
    
    // Cleanup
    if let Err(e) = fs::remove_file(filename) {
        println!("  [WARN] Failed to remove test file: {}", e);
    } else {
        println!("  Removed test file.");
    }
}

fn test_filesystem_directory_ops() {
    println!("\n[TEST] Filesystem: Directory Operations");
    let dir_name = "test_dir";
    let file_in_dir = "test_dir/nested.txt";

    // Create Directory
    if let Err(e) = fs::create_dir(dir_name) {
        println!("  [FAIL] Failed to create directory '{}': {}", dir_name, e);
        // Try to continue, maybe it exists
    } else {
        println!("  Created directory '{}'", dir_name);
    }

    // Create File in Directory
    {
        let mut f = match fs::File::create(file_in_dir) {
            Ok(f) => f,
            Err(e) => {
                println!("  [FAIL] Failed to create nested file: {}", e);
                return;
            }
        };
        let _ = f.write_all(b"Nested Data");
    }
    println!("  Created nested file '{}'", file_in_dir);

    // Read Dir
    println!("  Reading directory '{}':", dir_name);
    match fs::read_dir(dir_name) {
        Ok(entries) => {
            let mut found = false;
            for entry in entries {
                match entry {
                    Ok(e) => {
                        println!("    Found entry: {:?}", e.file_name());
                        if e.file_name() == "nested.txt" {
                            found = true;
                        }
                    }
                    Err(e) => println!("    [ERR] Error reading entry: {}", e),
                }
            }
            if found {
                println!("  [PASS] Found nested file in directory listing.");
            } else {
                println!("  [FAIL] Nested file not found in listing.");
            }
        }
        Err(e) => println!("  [FAIL] Failed to read directory: {}", e),
    }

    // Rename File
    let renamed_file = "test_dir/renamed.txt";
    if let Err(e) = fs::rename(file_in_dir, renamed_file) {
        println!("  [FAIL] Failed to rename file: {}", e);
    } else {
        println!("  [PASS] Renamed file to '{}'", renamed_file);
        if fs::metadata(renamed_file).is_ok() {
             println!("  [PASS] Renamed file exists.");
        } else {
             println!("  [FAIL] Renamed file does not exist metadata check.");
        }
    }

    // Cleanup
    let _ = fs::remove_file(renamed_file);
    let _ = fs::remove_file(file_in_dir); // In case rename failed
    if let Err(e) = fs::remove_dir(dir_name) {
        println!("  [FAIL] Failed to remove directory: {}", e);
    } else {
        println!("  [PASS] Removed directory.");
    }
}

fn test_filesystem_metadata_and_truncate() {
    println!("\n[TEST] Filesystem: Metadata, Seek & Truncate");
    let filename = "meta_test.bin";
    
    {
        let mut f = fs::File::create(filename).unwrap();
        f.write_all(b"1234567890").unwrap();
    }

    // Metadata
    match fs::metadata(filename) {
        Ok(meta) => {
            println!("  Metadata size: {} (Expected 10)", meta.len());
            if meta.len() == 10 {
                println!("  [PASS] Size correct.");
            } else {
                println!("  [FAIL] Size incorrect.");
            }
            println!("  Is file: {}", meta.is_file());
            println!("  Is dir:  {}", meta.is_dir());
        }
        Err(e) => println!("  [FAIL] Failed to get metadata: {}", e),
    }

    // Truncate (Set Length)
    {
        let f = fs::OpenOptions::new().write(true).open(filename).unwrap();
        if let Err(e) = f.set_len(5) {
            println!("  [FAIL] Failed to set_len: {}", e);
        } else {
            println!("  Called set_len(5).");
        }
    }

    match fs::metadata(filename) {
        Ok(meta) => {
            println!("  Metadata size after truncate: {} (Expected 5)", meta.len());
            if meta.len() == 5 {
                println!("  [PASS] Truncate successful.");
            } else {
                println!("  [FAIL] Truncate failed.");
            }
        }
        Err(_) => {} // Ignore error here, already handled above
    }

    // Seek
    {
        let mut f = fs::File::open(filename).unwrap();
        if let Err(e) = f.seek(SeekFrom::Start(2)) {
             println!("  [FAIL] Seek failed: {}", e);
        } else {
             let mut buf = [0u8; 1];
             f.read_exact(&mut buf).unwrap();
             println!("  Seek(2) -> Read: {} (Expected '3' or byte 51)", buf[0] as char);
             if buf[0] == b'3' {
                 println!("  [PASS] Seek and Read correct.");
             } else {
                 println!("  [FAIL] Seek content mismatch.");
             }
        }
    }

    let _ = fs::remove_file(filename);
}
