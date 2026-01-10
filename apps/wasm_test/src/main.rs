use std::fs;
use std::time::SystemTime;

fn main() {
    println!("Hello from KrakeOS WASM!");

    // Test 1: Time
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => println!("Current WASI Time (nanos): {}", n.as_nanos()),
        Err(_) => println!("Time error!"),
    }

    // Test 2: Filesystem (Read)
    println!("Reading / directory...");
    let entries = fs::read_dir("/").unwrap();
    println!("read_dir Ok. Iterating...");
    
    let mut count = 0;
    for entry in entries {
        match entry {
            Ok(e) => {
                println!("Entry: {:?}", e.path());
                count += 1;
            },
            Err(err) => println!("Error: {:?}", err),
        }
    }
    println!("Total entries: {}", count);

    // Test 3: Filesystem (Write)
    let test_file = "/wasm_hello.txt";
    println!("Writing to {}...", test_file);
    if fs::write(test_file, "WASI works on KrakeOS!").is_ok() {
        println!("Write successful.");
        
        if let Ok(content) = fs::read_to_string(test_file) {
            println!("Read back content: '{}'", content);
        }
    } else {
        println!("Write failed.");
    }

    println!("WASM Test Complete.");
}
