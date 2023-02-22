#!/bin/sh

date=$(date +%s)

cleanup()
{
    rustup override set stable
}

rustup override set nightly # > /dev/null 2>&1
trap "cleanup" EXIT

echo "\033[90m[INFO]\033[0m Using nightly rustc\n"

mkdir -p pkg/bz2

# Build wasm package
wasm-pack build -t web --release \
    --manifest-path ./Cargo.toml \
    -Z build-std=panic_abort,std -Z build-std-features=panic_immediate_abort

# Optimize wasm package
bzip2 pkg/gifski_bg.wasm -kc > "pkg/bz2/${date}_gifski_bg_build.wasm.bz2"
wasm-opt -Os --dce -o pkg/gifski_bg.wasm pkg/gifski_bg.wasm # Optional
bzip2 pkg/gifski_bg.wasm -kc > "pkg/bz2/${date}_gifski_bg_opt1.wasm.bz2"
wasm-opt -Os --dce -o pkg/gifski_bg.wasm pkg/gifski_bg.wasm # Optional
bzip2 pkg/gifski_bg.wasm -kc > "pkg/bz2/${date}_gifski_bg_opt2.wasm.bz2"

echo
echo "\033[92m[OUT ]\033[0m $(du -h pkg/gifski_bg.wasm)"
echo "\033[92m[OUT ]\033[0m $(du -h pkg/bz2/${date}_gifski_bg_build.wasm.bz2)"
echo "\033[92m[OUT ]\033[0m $(du -h pkg/bz2/${date}_gifski_bg_opt1.wasm.bz2)"
echo "\033[92m[OUT ]\033[0m $(du -h pkg/bz2/${date}_gifski_bg_opt2.wasm.bz2)"

cleanup > /dev/null 2>&1
echo "\n\033[90m[INFO]\033[0m Using stable rustc"