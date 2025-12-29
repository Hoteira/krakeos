cls

cargo compile

cargo build --package=kernel --target=bits64.json

wsl objcopy -I elf64-x86-64 -O binary target/bits64/debug/kernel build/kernel.bin

wsl dd if=build/kernel.bin of=build/disk.img seek=6144 bs=512 conv=notrunc

qemu-system-x86_64 -drive file=build/disk.img,format=raw -serial stdio