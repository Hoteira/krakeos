@echo off
set CC=clang
set AR=llvm-ar

cd swiftboot
cargo compile
cd ..

copy "swiftboot\build\disk.img" "build\"

cargo build --package=kernel --target="swiftboot/bits64.json"
wsl objcopy -I elf64-x86-64 -O binary target/bits64/debug/kernel build/kernel.bin

cargo build --package=userland --target=bits64pie.json --release
copy "target\bits64pie\release\userland" "tree\user.elf"

REM Skip full taskbar/term/etc build to save time
cargo build --package=net_test --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\net_test.wasm" "tree\apps\net_test.wasm"

wsl dd if=build/kernel.bin of=build/disk.img seek=6144 bs=512 conv=notrunc

wsl genext2fs -d tree -b 262144 -B 1024 build/disk2.img
wsl dd if=build/disk2.img of=build/disk.img seek=16384 bs=512 conv=notrunc

echo "Starting QEMU..."
qemu-system-x86_64 -drive file=build/disk.img,format=raw,if=virtio -serial file:qemu_net_log.txt -display none -m 1G -accel whpx -machine kernel_irqchip=off -no-reboot
