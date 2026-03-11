# KrakeOS Makefile

# Tools
CC := clang
AR := llvm-ar
CARGO := cargo
OBJCOPY := objcopy
GENEXT2FS := genext2fs
DD := dd
QEMU := qemu-system-x86_64

# Directories
BUILD_DIR := build
TREE_DIR := tree
TARGET_DIR := target
SYS_BIN_DIR := $(TREE_DIR)/sys/bin
APPS_DIR := $(TREE_DIR)/apps

# Flags
KERNEL_TARGET := swiftboot/bits64.json
PIE_TARGET := bits64pie.json
WASM_TARGET := wasm32-wasip2
UNSTABLE_FLAGS := -Z json-target-spec

# QEMU Options
QEMU_OPTS := -drive file=$(BUILD_DIR)/disk.img,format=raw,if=virtio \
             -serial mon:stdio --no-reboot \
             -device virtio-gpu-gl-pci,xres=1024,yres=576 \
             -display sdl,gl=on -vga none -m 4G \
             -accel kvm

.PHONY: all clean run swiftboot kernel wasm_loader userland fs

all: fs

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

swiftboot: $(BUILD_DIR)
	cd swiftboot && $(CARGO) compile
	cp swiftboot/build/disk.img $(BUILD_DIR)/disk.img

kernel: $(BUILD_DIR) swiftboot
	$(CARGO) build $(UNSTABLE_FLAGS) --package=kernel --target=$(KERNEL_TARGET)
	$(OBJCOPY) -O binary $(TARGET_DIR)/bits64/debug/kernel $(BUILD_DIR)/kernel.bin
	$(DD) if=$(BUILD_DIR)/kernel.bin of=$(BUILD_DIR)/disk.img seek=6144 bs=512 conv=notrunc

wasm_loader:
	$(CARGO) build $(UNSTABLE_FLAGS) --package=wasm_loader --target=$(PIE_TARGET) --release
	mkdir -p $(SYS_BIN_DIR)
	cp $(TARGET_DIR)/bits64pie/release/wasm_loader $(SYS_BIN_DIR)/wasm_loader.elf

# Userland applications
SYS_WASM_APPS := init sysmon fps_test tmap cat
APP_WASM_APPS := shell term taskbar aot_test net_test container_test

SYS_WASM_TARGETS := $(addprefix build-, $(SYS_WASM_APPS))
APP_WASM_TARGETS := $(addprefix build-, $(APP_WASM_APPS))

$(SYS_WASM_TARGETS): build-%:
	$(CARGO) build --package=$* --target=$(WASM_TARGET) --release
	mkdir -p $(SYS_BIN_DIR)
	cp $(TARGET_DIR)/$(WASM_TARGET)/release/$*.wasm $(SYS_BIN_DIR)/$*.wasm

$(APP_WASM_TARGETS): build-%:
	$(CARGO) build --package=$* --target=$(WASM_TARGET) --release
	mkdir -p $(APPS_DIR)
	cp $(TARGET_DIR)/$(WASM_TARGET)/release/$*.wasm $(APPS_DIR)/$*.wasm

userland: $(SYS_WASM_TARGETS) $(APP_WASM_TARGETS)

fs: swiftboot kernel wasm_loader userland
	$(GENEXT2FS) -d $(TREE_DIR) -b 262144 -B 1024 $(BUILD_DIR)/disk2.img
	$(DD) if=$(BUILD_DIR)/disk2.img of=$(BUILD_DIR)/disk.img seek=16384 bs=512 conv=notrunc

run: fs
	$(QEMU) $(QEMU_OPTS)

clean:
	rm -rf $(BUILD_DIR) $(TARGET_DIR)
	cd swiftboot && rm -rf build target
