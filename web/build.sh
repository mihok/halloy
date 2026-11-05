#!/usr/bin/env bash
# Build the browser bundle (web port) into web/dist/ and a release tarball.
#
#   web/build.sh            -> web/dist/{index.html,pkg/}, web/halloy-web.tar.gz(.sha256)
#
# Needs: rustup target wasm32-unknown-unknown, and wasm-bindgen-cli matching
# the wasm-bindgen crate version (Cargo.lock):
#   cargo install wasm-bindgen-cli --version <version> --locked
set -euo pipefail

cd "$(dirname "$0")/.."

want=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/, ""); print; exit}' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [[ "$want" != "$have" ]]; then
	echo "wasm-bindgen CLI $have doesn't match crate $want:" >&2
	echo "  cargo install wasm-bindgen-cli --version $want --locked" >&2
	exit 1
fi

cargo build --profile web --target wasm32-unknown-unknown -p halloy

rm -rf web/dist
mkdir -p web/dist
wasm-bindgen --target web --no-typescript --out-dir web/dist/pkg \
	target/wasm32-unknown-unknown/web/halloy.wasm
cp web/app/index.html web/dist/index.html

size=$(wc -c <web/dist/pkg/halloy_bg.wasm)
limit=$((25 * 1024 * 1024))
if ((size > limit)); then
	echo "halloy_bg.wasm is $size bytes; Cloudflare static assets allow $limit per file" >&2
	exit 1
fi

tar -C web/dist -czf web/halloy-web.tar.gz .
(cd web && sha256sum halloy-web.tar.gz >halloy-web.tar.gz.sha256)
echo "built web/dist ($size-byte wasm) and web/halloy-web.tar.gz:"
cat web/halloy-web.tar.gz.sha256
