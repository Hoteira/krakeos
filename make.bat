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

cargo build --package=term --target=bits64pie.json --release
copy "target\bits64pie\release\term" "tree\sys\bin\term.elf"

cargo build --package=shell --target=bits64pie.json --release
copy "target\bits64pie\release\shell" "tree\sys\bin\shell.elf"

cargo build --package=tmap --target=bits64pie.json --release
copy "target\bits64pie\release\tmap" "tree\sys\bin\tmap.elf"

cargo build --package=cat --target=bits64pie.json --release
copy "target\bits64pie\release\cat" "tree\sys\bin\cat.elf"

cargo build --package=taskbar --target=bits64pie.json --release
copy "target\bits64pie\release\taskbar" "tree\sys\bin\taskbar.elf"

cargo build --package=krake_libc --target=bits64pie.json --release

cd apps\doomgeneric-master\doomgeneric
clang -target x86_64-unknown-elf -ffreestanding -fno-stack-protector -fPIC -I ..\..\..\libs\krake_libc\include -c *.c
cd ..\..\..
ld.lld -pie --entry _start -o apps\doomgeneric-master\doomgeneric\doom.elf apps\doomgeneric-master\doomgeneric\*.o target\bits64pie\release\libkrake_libc.a
copy "apps\doomgeneric-master\doomgeneric\doom.elf" "tree\apps\doom\doom.elf"

wsl dd if=build/kernel.bin of=build/disk.img seek=6144 bs=512 conv=notrunc

wsl genext2fs -d tree -b 262144 -B 1024 build/disk2.img
wsl dd if=build/disk2.img of=build/disk.img seek=16384 bs=512 conv=notrunc

qemu-system-x86_64 -drive file=build/disk.img,format=raw,if=virtio -serial stdio --no-reboot -device virtio-gpu-gl-pci,xres=1280,yres=720 -display sdl,gl=on -vga none -m 2G

pause