@echo off
set "PATH=%PATH%;C:\Program Files\Docker\Docker\resources\bin;C:\Program Files\qemu"
echo Building KrakeOS...
docker build -t krakeos .
if %errorlevel% neq 0 (
    echo Build failed!
    exit /b %errorlevel%
)
echo Extracting disk image...
if not exist build mkdir build
docker run --rm -v "%cd%\build:/output" krakeos
if %errorlevel% neq 0 (
    echo Extraction failed!
    exit /b %errorlevel%
)
echo Done! disk.img is ready.

echo Launching KrakeOS in QEMU...
qemu-system-x86_64 -drive file=build/disk.img,format=raw,if=virtio -serial mon:stdio --no-reboot -device virtio-gpu-gl-pci,xres=1024,yres=576 -display sdl,gl=on -vga none -m 4G -smp 4
