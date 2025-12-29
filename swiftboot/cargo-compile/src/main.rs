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

    obj_copy("bootloader", &bits16_path, &build_dir.join("bootloader.bin"), &root);
    obj_copy("stage2", &bits16_path, &build_dir.join("stage2.bin"), &root);
    obj_copy("stage3", &bits32_path, &build_dir.join("stage3.bin"), &root);
    obj_copy("stage4", &bits64_path, &build_dir.join("stage4.bin"), &root);

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

fn obj_copy(package: &str, target: &Path, output: &Path, root: &Path) {
    let arg0 = format!("-p={}", package);
    let arg1 = format!("--bin={}", package);
    let arg2 = format!("--target={}", target.display());
    Command::new("cargo")
        .current_dir(root)
        .args(["objcopy", &arg0, &arg1, &arg2, "--", "-O", "binary", output.to_str().unwrap()])
        .status()
        .unwrap();
}

fn cargo_build(package: &str, target: &Path, root: &Path) {
    let arg1 = format!("--target={}", target.display());
    let arg2 = format!("--package={}", package);
    Command::new("cargo")
        .current_dir(root)
        .args(["build", &arg1, &arg2])
        .status()
        .unwrap();
}