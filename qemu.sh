#!/bin/bash
set -e

cargo build
qemu-system-x86_64 \
    -machine q35 \
    -smp 6  \
    -enable-kvm \
    -m 128 \
    -nographic \
    -bios /usr/share/OVMF/OVMF_CODE.fd \
    -device driver=e1000,netdev=n0 \
    -netdev user,id=n0,tftp=/home/geert/projects/fuzzoz/target/x86_64-unknown-uefi/debug,bootfile=FuzzOS.efi