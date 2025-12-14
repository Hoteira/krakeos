cd swiftboot

cargo compile

cd ..

copy "swiftboot\build\disk.img" "build\"

cargo build --package=kernel --target="swiftboot/bits64.json"

wsl objcopy -I elf64-x86-64 -O binary target/bits64/debug/kernel build/kernel.bin

cargo build --package=userland --target=bits64pie.json --release

copy "target\bits64pie\release\userland" "tree\user"

wsl dd if=build/kernel.bin of=build/disk.img seek=6144 bs=512 conv=notrunc

wsl genext2fs -d tree -b 262144 -B 1024 build/disk2.img
wsl dd if=build/disk2.img of=build/disk.img seek=16384 bs=512 conv=notrunc

qemu-system-x86_64 -drive file=build/disk.img,format=raw -serial stdio --no-reboot -device virtio-gpu-pci,xres=1280,yres=720 -display gtk,zoom-to-fit=off -vga none

pause