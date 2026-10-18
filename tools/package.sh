#!/bin/sh
# Build the release files into dist/:
#   ginrose-<version>-x86_64.tar.gz   the launcher, its installer and README
#   rosemary-ginrose-x86_64           Rosemary from the fork, when given its checkout
#
#   tools/package.sh [path/to/rosemary/checkout]
#
# Rosemary is built for any x86-64-v2 CPU (2009 onwards) rather than this one,
# with the fork's link-time optimised profile.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
cargo build --release
out="dist/ginrose-$version-x86_64"
rm -rf "$out" && mkdir -p "$out"
install -m755 target/release/ginrose "$out/ginrose"
cp install.sh README.md LICENSE "$out/"
tar -czf "$out.tar.gz" -C "$out" .
files="$out.tar.gz"
if [ -n "${1:-}" ]; then
    (cd "$1" && RUSTFLAGS="-C target-cpu=x86-64-v2" cargo build --bin rosemary --profile fullnative)
    install -m755 "$1/target/fullnative/rosemary" dist/rosemary-ginrose-x86_64
    files="$files dist/rosemary-ginrose-x86_64"
fi
(cd dist && sha256sum $(for f in $files; do basename "$f"; done) > SHA256SUMS)
echo "$files dist/SHA256SUMS"
