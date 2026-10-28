#!/usr/bin/env bash
# Builds the release files from the programs in target/release:
#
#   target/dist/quietlinux-VERSION-x86_64-linux.tar.gz   prebuilt programs and install.sh
#   target/dist/quietlinux_VERSION-1_amd64.deb           Debian, Ubuntu, …
#   target/dist/quietlinux-VERSION-1.x86_64.rpm          Fedora, openSUSE, …
#   target/dist/SHA256SUMS
#
# Needs cargo-about (third-party licenses) and nfpm (packages). Build first:
#   cargo build --release --locked -p quietlinux-cli -p quietlinux-gui -p quietlinux-daemon
# Used by .github/workflows/release.yml; see docs/releasing.md.

set -euo pipefail

cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
programs=(quietlinux quietlinux-gui quietlinux-daemon)
stage=target/package
dist=target/dist

for program in "${programs[@]}"; do
    [ -x "target/release/$program" ] || {
        echo "target/release/$program is missing: build it first (see the top of this script)" >&2
        exit 1
    }
done
for tool in cargo-about nfpm; do
    command -v "$tool" >/dev/null || { echo "$tool not found" >&2; exit 1; }
done

rm -rf "$stage" "$dist"
mkdir -p "$stage" "$dist"

# licenses of the Rust crates compiled into the programs
cargo about generate --locked --config packaging/about.toml \
    --output-file "$stage/THIRD-PARTY-LICENSES.html" packaging/about.hbs

# the packages put the programs in /usr/bin
sed 's|^ExecStart=.*|ExecStart=/usr/bin/quietlinux-daemon|' \
    packaging/systemd/quietlinux-daemon.service >"$stage/quietlinux-daemon.service"

# --- archive: install.sh finds the programs in bin/ and compiles nothing ----------

name="quietlinux-$version-x86_64-linux"
root="$stage/$name"
mkdir -p "$root/bin" "$root/packaging/udev" "$root/packaging/systemd" "$root/packaging/desktop"
for program in "${programs[@]}"; do
    install -m 755 "target/release/$program" "$root/bin/"
done
install -m 644 packaging/udev/70-quietlinux.rules "$root/packaging/udev/"
install -m 644 packaging/systemd/quietlinux-daemon.service "$root/packaging/systemd/"
install -m 644 packaging/desktop/quietlinux.desktop "$root/packaging/desktop/"
install -m 644 crates/quietlinux-gui/ui/icon.svg "$root/packaging/desktop/quietlinux.svg"
install -m 755 install.sh uninstall.sh "$root/"
install -m 644 README.md CHANGELOG.md LICENSE NOTICE "$stage/THIRD-PARTY-LICENSES.html" "$root/"
cp -r docs "$root/docs"
tar -C "$stage" --owner=0 --group=0 --numeric-owner -czf "$dist/$name.tar.gz" "$name"

# --- packages ---------------------------------------------------------------------

for packager in deb rpm; do
    VERSION="$version" nfpm package --config packaging/nfpm.yaml \
        --packager "$packager" --target "$dist/"
done

(cd "$dist" && sha256sum -- * >SHA256SUMS)
ls -l "$dist"
