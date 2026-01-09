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

cd apps\nano-master\src
clang -target x86_64-unknown-elf -ffreestanding -fno-stack-protector -fPIC -I ..\..\..\libs\libc\include -c *.c -DNANO_TINY
cd ..\..\..
ld.lld -pie --entry _start -o apps\nano-master\src\nano.elf apps\nano-master\src\*.o target\bits64pie\release\liblibc.a
copy "apps\nano-master\src\nano.elf" "tree\apps\nano\nano.elf"

cd apps\doomgeneric-master\doomgeneric
clang -target x86_64-unknown-elf -ffreestanding -fno-stack-protector -fPIC -I ..\..\..\libs\libc\include -c *.c
cd ..\..\..
ld.lld -pie --entry _start -o apps\doomgeneric-master\doomgeneric\doom.elf apps\doomgeneric-master\doomgeneric\*.o target\bits64pie\release\liblibc.a
copy "apps\doomgeneric-master\doomgeneric\doom.elf" "tree\apps\doom\doom.elf"

cd apps\lua-master
clang -target x86_64-unknown-elf -ffreestanding -fno-stack-protector -fPIC -I ..\..\libs\libc\include -DLUA_COMPAT_5_3 -c *.c
cd ..\..
ld.lld -pie --entry _start -o apps\lua-master\lua.elf apps\lua-master\lapi.o apps\lua-master\lcode.o apps\lua-master\lctype.o apps\lua-master\ldebug.o apps\lua-master\ldo.o apps\lua-master\ldump.o apps\lua-master\lfunc.o apps\lua-master\lgc.o apps\lua-master\llex.o apps\lua-master\lmem.o apps\lua-master\lobject.o apps\lua-master\lopcodes.o apps\lua-master\lparser.o apps\lua-master\lstate.o apps\lua-master\lstring.o apps\lua-master\ltable.o apps\lua-master\ltm.o apps\lua-master\lundump.o apps\lua-master\lvm.o apps\lua-master\lzio.o apps\lua-master\lauxlib.o apps\lua-master\lbaselib.o apps\lua-master\lcorolib.o apps\lua-master\ldblib.o apps\lua-master\liolib.o apps\lua-master\lmathlib.o apps\lua-master\loslib.o apps\lua-master\lstrlib.o apps\lua-master\ltablib.o apps\lua-master\lutf8lib.o apps\lua-master\loadlib.o apps\lua-master\linit.o apps\lua-master\lua.o target\bits64pie\release\liblibc.a
mkdir "tree\apps\lua"
copy "apps\lua-master\lua.elf" "tree\apps\lua\lua.elf"


wsl dd if=build/kernel.bin of=build/disk.img seek=6144 bs=512 conv=notrunc

wsl genext2fs -d tree -b 262144 -B 1024 build/disk2.img
wsl dd if=build/disk2.img of=build/disk.img seek=16384 bs=512 conv=notrunc

qemu-system-x86_64 -drive file=build/disk.img,format=raw,if=virtio -serial stdio --no-reboot -device virtio-gpu-pci,xres=1024,yres=576 -display sdl -vga none -m 4G -accel whpx -machine kernel_irqchip=off

pause