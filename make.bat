@echo off
set CC=clang
set AR=llvm-ar

cd swiftboot
cargo compile

cd ..

copy "swiftboot\build\disk.img" "build\"

cargo build --package=kernel --target="swiftboot/bits64.json"

wsl objcopy -I elf64-x86-64 -O binary target/bits64/debug/kernel build/kernel.bin

cargo build --package wasm_test --target wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\wasm_test.wasm" "tree\wasm_test.wasm"

cargo build --package=userland --target=bits64pie.json --release
copy "target\bits64pie\release\userland" "tree\user.elf"

cargo build --package=term --target=bits64pie.json --release
copy "target\bits64pie\release\term" "tree\sys\bin\term.elf"

cargo build --package=shell --target=bits64pie.json --release
copy "target\bits64pie\release\shell" "tree\sys\bin\shell.elf"

cargo build --package=sysmon --target=bits64pie.json --release
copy "target\bits64pie\release\sysmon" "tree\sys\bin\sysmon.elf"

cargo build --package=fps_test --target=bits64pie.json --release
copy "target\bits64pie\release\fps_test" "tree\sys\bin\fps_test.elf"

cargo build --package=tmap --target=bits64pie.json --release
copy "target\bits64pie\release\tmap" "tree\sys\bin\tmap.elf"

cargo build --package=cat --target=bits64pie.json --release
copy "target\bits64pie\release\cat" "tree\sys\bin\cat.elf"

cargo build --package=taskbar --target=bits64pie.json --release
copy "target\bits64pie\release\taskbar" "tree\sys\bin\taskbar.elf"

cargo build --package=libc --target=bits64pie.json --release

wsl dd if=build/kernel.bin of=build/disk.img seek=6144 bs=512 conv=notrunc

wsl genext2fs -d tree -b 262144 -B 1024 build/disk2.img
wsl dd if=build/disk2.img of=build/disk.img seek=16384 bs=512 conv=notrunc

qemu-system-x86_64 -drive file=build/disk.img,format=raw,if=virtio -serial stdio --no-reboot -device virtio-gpu-pci,xres=1024,yres=576 -display sdl -vga none -m 4G -accel whpx -machine kernel_irqchip=off

REM pause