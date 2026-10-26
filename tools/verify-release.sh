#!/bin/sh
# Check that an official release was built from this source: rebuild the
# tagged commit and compare its binary with the one in the release.
#
#   tools/verify-release.sh 0.2.6
#
# Needs git, curl, sha256sum and Rust (rust-toolchain.toml picks the version).
set -eu
version=${1:?usage: tools/verify-release.sh VERSION}
cd "$(dirname "$0")/.."
release="https://github.com/nunreasonable/ginrose-flatpak/releases/download/v$version"
tmp=$(mktemp -d)
trap 'git worktree remove --force "$tmp/src" 2>/dev/null; rm -rf "$tmp"' EXIT INT TERM

git rev-parse -q --verify "refs/tags/v$version" >/dev/null \
    || git fetch -q origin "refs/tags/v$version:refs/tags/v$version"
git worktree add -q --detach "$tmp/src" "v$version"

echo ":: Downloading the release"
curl -fsSL -o "$tmp/ginrose.tar.gz" "$release/ginrose-$version-x86_64.tar.gz"
curl -fsSL -o "$tmp/SHA256SUMS" "$release/SHA256SUMS"
(cd "$tmp" && grep " ginrose-$version-x86_64.tar.gz\$" SHA256SUMS | sha256sum -c --quiet -)
tar -xzf "$tmp/ginrose.tar.gz" -C "$tmp"

echo ":: Building v$version"
(
    cd "$tmp/src"
    export SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)"
    export RUSTFLAGS="--remap-path-prefix=$PWD=/ginrose --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
    cargo build --release --locked --quiet
)

official=$(sha256sum < "$tmp/ginrose-$version/ginrose" | cut -d' ' -f1)
rebuilt=$(sha256sum < "$tmp/src/target/release/ginrose" | cut -d' ' -f1)
echo "official: $official"
echo "rebuilt:  $rebuilt"
if [ "$official" = "$rebuilt" ]; then
    echo ":: The release matches its source."
else
    echo "!! They differ: the release was not built from this tag, or the build is not reproducible here." >&2
    exit 1
fi
