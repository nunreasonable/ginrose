#!/bin/sh
# Check that an official release was built from this source: rebuild the
# tagged commit and compare its binary with the one in the release, and Ginrose
# Studio's too when the release has it (0.3.0 onwards).
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
curl -fsSL -o "$tmp/ginrose-$version-x86_64.tar.gz" "$release/ginrose-$version-x86_64.tar.gz"
curl -fsSL -o "$tmp/SHA256SUMS" "$release/SHA256SUMS"
(cd "$tmp" && grep " ginrose-$version-x86_64.tar.gz\$" SHA256SUMS | sha256sum -c --quiet -)
tar -xzf "$tmp/ginrose-$version-x86_64.tar.gz" -C "$tmp"

echo ":: Building v$version"
(
    cd "$tmp/src"
    export SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)"
    export RUSTFLAGS="--remap-path-prefix=$PWD=/ginrose --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
    cargo build --release --locked --quiet
)

compare() {
    official=$(sha256sum < "$1" | cut -d' ' -f1)
    rebuilt=$(sha256sum < "$2" | cut -d' ' -f1)
    echo "$3 official: $official"
    echo "$3 rebuilt:  $rebuilt"
    [ "$official" = "$rebuilt" ]
}
same=1
compare "$tmp/ginrose-$version/ginrose" "$tmp/src/target/release/ginrose" ginrose || same=0
# Ginrose Studio's builds are released from their own repository.
studio="ginrose-studio-$version-x86_64.tar.gz"
studio_release="https://github.com/nunreasonable/ginrose-studio/releases/download/v$version"
mkdir "$tmp/studio"
if curl -fsSL -o "$tmp/studio/SHA256SUMS" "$studio_release/SHA256SUMS" 2>/dev/null \
    && grep -q " $studio\$" "$tmp/studio/SHA256SUMS"; then
    echo ":: Downloading the Ginrose Studio release"
    curl -fsSL -o "$tmp/studio/$studio" "$studio_release/$studio"
    (cd "$tmp/studio" && grep " $studio\$" SHA256SUMS | sha256sum -c --quiet -)
    mv "$tmp/studio/$studio" "$tmp/$studio"
    tar -xzf "$tmp/$studio" -C "$tmp"
    compare "$tmp/ginrose-studio-$version/ginrose-studio" \
        "$tmp/src/target/release/ginrose-studio" ginrose-studio || same=0
fi
if [ "$same" = 1 ]; then
    echo ":: The release matches its source."
else
    echo "!! They differ: the release was not built from this tag, or the build is not reproducible here." >&2
    exit 1
fi
