#!/bin/sh
# Build the release tarball: the ginrose binary and, when given, a Rosemary
# binary to ship beside it.
#
#   tools/package.sh [path/to/rosemary]
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
cargo build --release
out="dist/ginrose-$version-x86_64"
rm -rf "$out" && mkdir -p "$out"
install -m755 target/release/ginrose "$out/ginrose"
if [ -n "${1:-}" ]; then
    install -m755 "$1" "$out/rosemary"
fi
cp install.sh README.md "$out/"
tar -czf "$out.tar.gz" -C "$out" .
sha256sum "$out.tar.gz" > "$out.tar.gz.sha256"
echo "$out.tar.gz"
