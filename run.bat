@echo off
echo ========================================================
echo Building WASM Guest Apps...
echo ========================================================
echo "Building shell..."
rem 32MB per app: the runner heap is 160MB and every app wasm memory is
rem allocated eagerly, so bigger values stop additional apps from spawning.
set RUSTFLAGS=-C link-arg=--initial-memory=33554432 -C link-arg=--max-memory=33554432
cargo build -p shell --target wasm32-wasip1 --release
echo "Building calc..."
cargo build -p calc --target wasm32-wasip1 --release
echo "Building terminal..."
cargo build -p terminal --target wasm32-wasip1 --release
echo "Building explorer..."
cargo build -p explorer --target wasm32-wasip1 --release
echo "Building viewer..."
cargo build -p viewer --target wasm32-wasip1 --release
echo "Building editor..."
cargo build -p editor --target wasm32-wasip1 --release
set RUSTFLAGS=

echo "Building WASM Runner..."
cargo build -p wasm_runner --target riscv64gc-unknown-none-elf --release

echo "Rebuilding File System Image..."
if exist fs.img del fs.img
fsutil file createnew fs.img 1073741824

echo "Formatting File System..."
cargo run -p fatsquid_fmt --target x86_64-pc-windows-msvc -- fs.img format

echo "Adding App and Runner to File System..."
if not exist disk\apps mkdir disk\apps
if not exist disk\bin mkdir disk\bin
copy /Y target\wasm32-wasip1\release\shell.wasm disk\apps\shell.wasm
copy /Y target\wasm32-wasip1\release\calc.wasm disk\apps\calc.wasm
copy /Y target\wasm32-wasip1\release\terminal.wasm disk\apps\terminal.wasm
copy /Y target\wasm32-wasip1\release\explorer.wasm disk\apps\explorer.wasm
copy /Y target\wasm32-wasip1\release\viewer.wasm disk\apps\viewer.wasm
copy /Y target\wasm32-wasip1\release\editor.wasm disk\apps\editor.wasm
copy /Y target\riscv64gc-unknown-none-elf\release\wasm_runner disk\bin\wasm_runner.elf

if not exist disk\img mkdir disk\img
copy /Y ref\tree\sys\img\wallpaper2.png disk\img\wallpaper.png
copy /Y ref\tree\sys\img\cursor1.png disk\img\cursor1.png

echo Generating wallpaper.raw...
cargo run -p png2raw --target x86_64-pc-windows-msvc -- disk/img/wallpaper.png disk/img/wallpaper.raw 1024 576

echo Generating ASCII-subset UI font...
cargo run -p fontsubset --target x86_64-pc-windows-msvc -- disk/fonts/CaskaydiaNerd.ttf disk/fonts/ui.ttf

cargo run -p fatsquid_fmt --target x86_64-pc-windows-msvc -- fs.img pack disk
echo ========================================================
echo Building and running KrakeOS (Release Mode)
echo ========================================================
echo.
cargo build --manifest-path kernel/Cargo.toml --release --target riscv64gc-unknown-none-elf
if %errorlevel% neq 0 exit /b %errorlevel%

qemu-system-riscv64 -display gtk,zoom-to-fit=off -m 512M -machine virt -bios default -drive file=fs.img,format=raw,if=none,id=hd0,cache=writethrough -device virtio-blk-device,drive=hd0 -device virtio-gpu-device -device virtio-keyboard-device -device virtio-tablet-device -serial stdio -kernel target\riscv64gc-unknown-none-elf\release\kernel

echo ========================================================
echo OS Execution Finished.
echo ========================================================
pause
