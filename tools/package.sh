#!/bin/sh
# Build the release files into dist/:
#   ginrose-<version>-x86_64.tar.gz   the launcher, its installer and README
#   rosemary-ginrose-x86_64           Rosemary from the fork, when given its checkout
#
# On FreeBSD both names end in -freebsd. Build there on the oldest FreeBSD
# release still supported: binaries run on that release and later ones only.
#
#   tools/package.sh [path/to/rosemary/checkout]
#
# Rosemary is built for any x86-64-v2 CPU (2009 onwards) rather than this one,
# with the fork's link-time optimised profile.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
cargo build --release
case "$(uname -s)" in
    FreeBSD) os=-freebsd ;;
    *) os= ;;
esac
out="dist/ginrose-$version-x86_64$os"
rm -rf "$out" && mkdir -p "$out"
install -m755 target/release/ginrose "$out/ginrose"
cp install.sh README.md LICENSE "$out/"
tar -czf "$out.tar.gz" -C "$out" .
files="$out.tar.gz"
if [ -n "${1:-}" ]; then
    (cd "$1" && RUSTFLAGS="-C target-cpu=x86-64-v2" cargo build --bin rosemary --profile fullnative)
    install -m755 "$1/target/fullnative/rosemary" "dist/rosemary-ginrose-x86_64$os"
    files="$files dist/rosemary-ginrose-x86_64$os"
fi
# The other system's files stay listed, so the Linux and FreeBSD builds can
# share one SHA256SUMS: copy the one already made into dist/ first.
names=$(for f in $files; do basename "$f"; done)
(
    cd dist
    touch SHA256SUMS
    for n in $names; do grep -v "  $n\$" SHA256SUMS > SHA256SUMS.new || true; mv SHA256SUMS.new SHA256SUMS; done
    sha256sum $names >> SHA256SUMS
)
echo "$files dist/SHA256SUMS"
