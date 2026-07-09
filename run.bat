@echo off
echo ========================================================
echo Building WASM Guest App...
echo ========================================================
echo "Building User WASM App..."
cargo build -p wasm_app --target wasm32-wasip1 --release

echo "Building WASM Window Manager..."
cargo build -p window_manager --target wasm32-wasip1 --release

echo "Building WASM Runner..."
cargo build -p wasm_runner --target riscv64gc-unknown-none-elf --release

echo "Rebuilding File System Image..."
if exist fs.img del fs.img
fsutil file createnew fs.img 1073741824

echo "Formatting File System..."
cargo run -p fatsquid_fmt --target x86_64-pc-windows-msvc -- fs.img format

echo "Adding App and Runner to File System..."
cargo run -p fatsquid_fmt --target x86_64-pc-windows-msvc -- fs.img add target\wasm32-wasip1\release\wasm_app.wasm app.wasm
cargo run -p fatsquid_fmt --target x86_64-pc-windows-msvc -- fs.img add target\wasm32-wasip1\release\window_manager.wasm wm.wasm
cargo run -p fatsquid_fmt --target x86_64-pc-windows-msvc -- fs.img add target\riscv64gc-unknown-none-elf\release\wasm_runner wasm_runner.elf
echo ========================================================
echo Building and running KrakeOS (Release Mode)
echo ========================================================
echo.
cargo build --manifest-path kernel/Cargo.toml --release --target riscv64gc-unknown-none-elf
if %errorlevel% neq 0 exit /b %errorlevel%

qemu-system-riscv64 -machine virt -bios default -drive file=fs.img,format=raw,if=none,id=hd0,cache=writethrough -device virtio-blk-device,drive=hd0 -device virtio-gpu-device -serial stdio -kernel target\riscv64gc-unknown-none-elf\release\kernel

echo ========================================================
echo OS Execution Finished.
echo ========================================================
pause
