use std::fs;
use std::fs::{create_dir, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = root.parent().unwrap();

    let build_path = root.join("build");
    if build_path.exists() {
        fs::remove_dir_all(&build_path).unwrap();
    }

    let _ = create_dir(root.join("build"));

    let build_dir = root.join("build");
    fs::create_dir_all(&build_dir).unwrap();

    let bits16_path = root.join("bits16.json");
    let bits32_path = root.join("bits32.json");
    let bits64_path = root.join("bits64.json");

    obj_copy("bootloader", &bits16_path, &build_dir.join("bootloader.bin"), &root, "bits16");
    obj_copy("stage2", &bits16_path, &build_dir.join("stage2.bin"), &root, "bits16");
    obj_copy("stage3", &bits32_path, &build_dir.join("stage3.bin"), &root, "bits32");
    obj_copy("stage4", &bits64_path, &build_dir.join("stage4.bin"), &root, "bits64");

    let mut disk = File::create(build_dir.join("disk.img")).unwrap();
    copy(&mut disk, "bootloader", 0, &build_dir);
    copy(&mut disk, "stage2", 2048, &build_dir);
    copy(&mut disk, "stage3", 3072, &build_dir);
    copy(&mut disk, "stage4", 5120, &build_dir);
}

fn copy(disk: &mut File, package: &str, lba: u64, build_dir: &Path) {
    let bin_path = build_dir.join(format!("{}.bin", package));
    let bin_data = fs::read(bin_path).unwrap();
    disk.seek(SeekFrom::Start(lba * 512)).unwrap();
    disk.write_all(&bin_data).unwrap();
}

fn obj_copy(package: &str, target_file: &Path, output: &Path, root: &Path, target_name: &str) {
    // 1. Build the package
    let status = Command::new("cargo")
        .current_dir(root)
        .args([
            "build",
            "-Z", "build-std=core,alloc,compiler_builtins",
            "-Z", "build-std-features=compiler-builtins-mem",
            "-Z", "json-target-spec",
            &format!("--package={}", package),
            &format!("--target={}", target_file.display()),
        ])
        .status()
        .expect("Failed to run cargo build");
    
    if !status.success() {
        panic!("Cargo build failed for package {}", package);
    }

    // 2. Locate the elf
    let elf_path = root.join("target").join(target_name).join("debug").join(package);

    // 3. Run system objcopy
    let status = Command::new("objcopy")
        .args([
            "-O",
            "binary",
            elf_path.to_str().expect("Invalid ELF path"),
            output.to_str().expect("Invalid output path"),
        ])
        .status()
        .expect("Failed to run system objcopy");

    if !status.success() {
        panic!("objcopy failed for package {}", package);
    }
}
