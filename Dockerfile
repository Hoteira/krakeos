# Builds KrakeOS on any platform. The output is the kernel and a ready disk image.
#
#   docker build --output out .
#
# Then boot it with QEMU, see the README.
FROM rust:1.88-slim-bookworm AS build

RUN rustup target add riscv64gc-unknown-none-elf wasm32-wasip1

WORKDIR /krakeos
COPY . .

# Apps and the user-mode runner
RUN for app in shell calc terminal explorer viewer editor; do \
        cargo build -p $app --target wasm32-wasip1 --release || exit 1; \
    done \
 && cargo build -p wasm_runner --target riscv64gc-unknown-none-elf --release

# Disk image, with fatsquid_fmt built for the machine running the build
RUN host=$(rustc -vV | sed -n 's/^host: //p') \
 && cargo build -p fatsquid_fmt --target $host --release \
 && fmt=target/$host/release/fatsquid_fmt \
 && mkdir -p disk/apps disk/bin disk/home disk/apps/lib/python3.13/lib-dynload \
 && for app in shell calc terminal explorer viewer editor; do \
        cp target/wasm32-wasip1/release/$app.wasm disk/apps/; \
    done \
 && cp target/riscv64gc-unknown-none-elf/release/wasm_runner disk/bin/wasm_runner.elf \
 && truncate -s 1G fs.img \
 && $fmt fs.img format \
 && $fmt fs.img pack disk

RUN cargo build -p kernel --target riscv64gc-unknown-none-elf --release

FROM scratch
COPY --from=build /krakeos/fs.img /
COPY --from=build /krakeos/target/riscv64gc-unknown-none-elf/release/kernel /
