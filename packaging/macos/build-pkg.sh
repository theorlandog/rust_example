#!/usr/bin/env bash
#
# Builds a macOS installer package (.pkg) containing a universal binary.
#
#   ./packaging/macos/build-pkg.sh
#
# The result installs `rust-example` into /usr/local/bin for every user, and
# runs natively on both Apple silicon and Intel.
#
# Requires: macOS with the Xcode command line tools (`xcode-select --install`),
# which provide lipo, pkgbuild, and productbuild.
#
# Signing and notarisation (optional; set these to ship outside your own
# machine, otherwise Gatekeeper will refuse the package on download):
#
#   MACOS_CODESIGN_IDENTITY   "Developer ID Application: ..."  signs the binary
#   MACOS_INSTALLER_IDENTITY  "Developer ID Installer: ..."    signs the .pkg
#   MACOS_NOTARY_PROFILE      a `xcrun notarytool store-credentials` profile
#
# Notarisation is a round trip to Apple's servers, so it is opt-in rather than
# part of every local build.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

version="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)"
identifier="com.theorlandog.rust-example"
build="$repo_root/target/macos-pkg"
dist="$repo_root/dist"
pkg="$dist/rust-example-$version-universal.pkg"

echo "==> Building rust-example $version for both architectures"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target add "$target"
    cargo build --release --locked --target "$target"
done

# One binary that contains both slices. Shipping a universal binary is simpler
# for users than publishing two downloads and explaining which is which.
echo "==> Merging into a universal binary"
rm -rf "$build"
mkdir -p "$build/root/usr/local/bin" "$build/resources" "$build/flat"
lipo -create -output "$build/root/usr/local/bin/rust-example" \
    target/aarch64-apple-darwin/release/rust-example \
    target/x86_64-apple-darwin/release/rust-example
lipo -info "$build/root/usr/local/bin/rust-example"

if [[ -n "${MACOS_CODESIGN_IDENTITY:-}" ]]; then
    echo "==> Signing the binary"
    # --options runtime opts into the hardened runtime, which notarisation
    # requires; --timestamp records a trusted timestamp so the signature
    # outlives the certificate.
    codesign --force --options runtime --timestamp \
        --sign "$MACOS_CODESIGN_IDENTITY" \
        "$build/root/usr/local/bin/rust-example"
else
    echo "==> MACOS_CODESIGN_IDENTITY not set; leaving the binary unsigned"
fi

# pkgbuild produces a "component package": the payload and where it goes.
echo "==> Building the component package"
pkgbuild \
    --root "$build/root" \
    --identifier "$identifier" \
    --version "$version" \
    --install-location / \
    --ownership recommended \
    "$build/flat/component.pkg"

# productbuild wraps it in a "product archive": the installer UI, the licence
# pane, and the architecture requirements described by distribution.xml.
echo "==> Building the installer"
cp LICENSE "$build/resources/LICENSE"
mkdir -p "$dist"

productbuild_args=(
    --distribution "$repo_root/packaging/macos/distribution.xml"
    --package-path "$build/flat"
    --resources "$build/resources"
)
if [[ -n "${MACOS_INSTALLER_IDENTITY:-}" ]]; then
    productbuild_args+=(--sign "$MACOS_INSTALLER_IDENTITY" --timestamp)
else
    echo "==> MACOS_INSTALLER_IDENTITY not set; leaving the installer unsigned"
fi

productbuild "${productbuild_args[@]}" "$pkg"

if [[ -n "${MACOS_NOTARY_PROFILE:-}" ]]; then
    echo "==> Notarising (this waits on Apple)"
    xcrun notarytool submit "$pkg" --keychain-profile "$MACOS_NOTARY_PROFILE" --wait
    # Stapling attaches the notarisation ticket so the package verifies even on
    # a machine that is offline when it is opened.
    xcrun stapler staple "$pkg"
fi

echo "==> Built $pkg"
pkgutil --payload-files "$pkg" | head -n 20
