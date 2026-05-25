# Use a Debian-based Rust image
FROM rust:bullseye

# Install system dependencies
RUN apt-get update && apt-get install -y \
    clang \
    llvm \
    lld \
    make \
    genext2fs \
    python3 \
    gcc-multilib \
    binutils \
    && rm -rf /var/lib/apt/lists/*

# Install a specific Rust nightly version to match the codebase requirements
# nightly-2024-10-01 is chosen to avoid recent breaking changes in intrinsics and attributes
ARG TOOLCHAIN=nightly
RUN rustup toolchain install $TOOLCHAIN && \
    rustup default $TOOLCHAIN && \
    rustup component add rust-src --toolchain $TOOLCHAIN && \
    rustup target add wasm32-wasip1 wasm32-wasip2 --toolchain $TOOLCHAIN

# Set the working directory
WORKDIR /app

# Copy the project files
COPY . .

# 1. Patch Cargo.toml files to include the unstable edition2024 feature
RUN find . -name "Cargo.toml" -exec sed -i '1i cargo-features = ["edition2024"]' {} +

# 2. Remove [unstable] from all .cargo/config.toml files to avoid host tool conflicts
RUN find . -name "config.toml" -path "*/.cargo/*" -exec sed -i '/\[unstable\]/,/\[/d' {} +

# 3. Patch code to handle recent Rust Nightly changes (in-container only)
# Wrap no_mangle in unsafe()
RUN find std/src -name "*.rs" -exec sed -i 's/#[no_mangle]/#[unsafe(no_mangle)]/g' {} +

# Replace fabsf64/fabsf32 intrinsics with bitwise operations
RUN sed -i 's/unsafe { core::intrinsics::fabsf64(self) }/Self::from_bits(self.to_bits() \& 0x7FFFFFFFFFFFFFFF)/g' std/src/math.rs && \
    sed -i 's/unsafe { core::intrinsics::fabsf32(self) }/Self::from_bits(self.to_bits() \& 0x7FFFFFFF)/g' std/src/math.rs

# Append WASI stubs to std/src/rt/mod.rs to satisfy linker
RUN printf '\n#[cfg(target_arch = "wasm32")]\n#[unsafe(no_mangle)]\npub extern "C" fn __wasi_init_tp() {}\n\n#[cfg(target_arch = "wasm32")]\n#[unsafe(no_mangle)]\npub extern "C" fn __wasm_call_dtors() {}\n\n#[cfg(target_arch = "wasm32")]\n#[unsafe(no_mangle)]\npub extern "C" fn __wasi_proc_exit(code: i32) -> ! { crate::os::exit(code as u64); loop {} }\n' >> std/src/rt/mod.rs

# 4. Ensure Makefile and cargo-compile use -Z json-target-spec as a cargo flag
RUN sed -i 's/RUSTFLAGS="-Awarnings"/RUSTFLAGS="-Awarnings"/g' Makefile && \
    sed -i 's/cargo --quiet/cargo --quiet -Z json-target-spec/g' Makefile && \
    sed -i 's/Command::new("cargo")/Command::new("cargo").arg("-Z").arg("json-target-spec")/g' swiftboot/cargo-compile/src/main.rs

# Run the build
RUN make fs

# Default command: copy the built disk image to the /output volume
CMD ["cp", "build/disk.img", "/output/disk.img"]
