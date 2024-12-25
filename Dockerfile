FROM ubuntu:22.04

ARG QEMU_VERSION=7.0.0
ARG HOME=/root

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && \
    apt-get install -y \
        curl \
        git \
        wget

WORKDIR ${HOME}
RUN wget https://download.qemu.org/qemu-${QEMU_VERSION}.tar.xz && \
    tar xvJf qemu-${QEMU_VERSION}.tar.xz

RUN apt-get install -y \
        autoconf automake autotools-dev curl libmpc-dev libmpfr-dev libgmp-dev \
        gawk build-essential bison flex texinfo gperf libtool patchutils bc \
        zlib1g-dev libexpat-dev git \
        ninja-build pkg-config libglib2.0-dev libpixman-1-dev libsdl2-dev

WORKDIR ${HOME}/qemu-${QEMU_VERSION}
RUN ./configure --target-list=riscv64-softmmu,riscv64-linux-user && \
    make -j$(nproc) && \
    make install

WORKDIR ${HOME}
RUN rm -rf qemu-${QEMU_VERSION} qemu-${QEMU_VERSION}.tar.xz

RUN qemu-system-riscv64 --version && \
    qemu-riscv64 --version

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=nightly
RUN set -eux; \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o rustup-init; \
    chmod +x rustup-init; \
    ./rustup-init -y --no-modify-path --profile minimal --default-toolchain $RUST_VERSION; \
    rm rustup-init; \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME;

RUN rustup --version && \
    cargo --version && \
    rustc --version

RUN cargo install cargo-binutils; \
    rustup target add riscv64gc-unknown-none-elf; \
	rustup component add rust-src; \
	rustup component add llvm-tools-preview; \
	rustup component add rustfmt; \
	rustup component add clippy;

WORKDIR ${HOME}
COPY os/vendor ./os-vendor
COPY user/vendor ./user-vendor

WORKDIR ${HOME}
