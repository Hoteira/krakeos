use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};

fn main() {
    println!("--- KrakeOS WASI Full Feature Test ---");

    // 1. Randomness (random_get)
    println!("\n[1] Randomness:");
    let mut random_buf = [0u8; 8];
    // In wasm32-wasip1, standard library uses random_get internally for some things,
    // but we can't easily call it directly without unsafe or a crate.
    // However, the host logs will show if it's called.
    println!("  (Check host logs for WASI: random_get)");

    // 2. Directory Management (path_create_directory, path_remove_directory)
    println!("\n[2] Directory Management:");
    let test_dir = "/wasi_manual_dir";
    match fs::create_dir(test_dir) {
        Ok(_) => {
            println!("  Created: {}", test_dir);
            match fs::remove_dir(test_dir) {
                Ok(_) => println!("  Removed: {}", test_dir),
                Err(e) => println!("  Remove failed: {:?}", e),
            }
        }
        Err(e) => println!("  Create failed: {:?}", e),
    }

    // 3. File Seek (fd_seek) - Double check it moves
    println!("\n[3] File Seek:");
    let test_file = "/seek_test.txt";
    let content = "0123456789ABCDEF";
    {
        let mut f = fs::File::create(test_file).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
    {
        let mut f = fs::File::open(test_file).unwrap();
        f.seek(SeekFrom::Start(10)).unwrap();
        let mut buf = [0u8; 1];
        f.read_exact(&mut buf).unwrap();
        println!("  At offset 10: '{}' (Expected 'A')", buf[0] as char);

        f.seek(SeekFrom::Current(-5)).unwrap();
        f.read_exact(&mut buf).unwrap();
        println!("  After relative -5: '{}' (Expected '6')", buf[0] as char);
    }

    // 4. File Unlink (path_unlink_file)
    println!("\n[4] File Unlink:");
    match fs::remove_file(test_file) {
        Ok(_) => println!("  Unlinked: {}", test_file),
        Err(e) => println!("  Unlink failed: {:?}", e),
    }

    println!("\n--- WASI Full Feature Test Finished ---");
}