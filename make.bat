@echo off
set CC=clang
set AR=llvm-ar

cd swiftboot
cargo compile

cd ..

copy "swiftboot\build\disk.img" "build\"

cargo build --package=kernel --target="swiftboot/bits64.json"
wsl objcopy -I elf64-x86-64 -O binary target/bits64/debug/kernel build/kernel.bin

cargo build --package=wasm_loader --target=bits64pie.json --release
copy "target\bits64pie\release\wasm_loader" "tree\sys\bin\wasm_loader.elf"

cargo build --package=init --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\init.wasm" "tree\sys\bin\init.wasm"

cargo build --package=shell --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\shell.wasm" "tree\sys\bin\shell.wasm"
copy "target\wasm32-wasip2\release\shell.wasm" "tree\apps\shell.wasm"

cargo build --package=term --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\term.wasm" "tree\sys\bin\term.wasm"
copy "target\wasm32-wasip2\release\term.wasm" "tree\apps\term.wasm"

cargo build --package=taskbar --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\taskbar.wasm" "tree\apps\taskbar.wasm"

cargo build --package=sysmon --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\sysmon.wasm" "tree\sys\bin\sysmon.wasm"

cargo build --package=fps_test --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\fps_test.wasm" "tree\sys\bin\fps_test.wasm"

cargo build --package=tmap --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\tmap.wasm" "tree\sys\bin\tmap.wasm"

cargo build --package=cat --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\cat.wasm" "tree\sys\bin\cat.wasm"

cargo build --package=aot_test --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\aot_test.wasm" "tree\apps\aot_test.wasm"

cargo build --package=net_test --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\net_test.wasm" "tree\apps\net_test.wasm"

cargo build --package=container_test --target=wasm32-wasip2 --release
copy "target\wasm32-wasip2\release\container_test.wasm" "tree\apps\container_test.wasm"

cargo build --package=libc --target=bits64pie.json --release

wsl dd if=build/kernel.bin of=build/disk.img seek=6144 bs=512 conv=notrunc

wsl genext2fs -d tree -b 262144 -B 1024 build/disk2.img
wsl dd if=build/disk2.img of=build/disk.img seek=16384 bs=512 conv=notrunc

qemu-system-x86_64 -drive file=build/disk.img,format=raw,if=virtio -serial stdio --no-reboot -device virtio-gpu-pci,xres=1024,yres=576 -display sdl -vga none -m 4G -accel whpx -machine kernel_irqchip=off

REM pause
