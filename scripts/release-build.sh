#!/usr/bin/env bash
# The two static binaries a release carries, named so the installer can find
# them: sindri-<version>-<target>. Cross-compilation goes through cargo-zigbuild
# because nixpkgs' rustc has no rustup and therefore no `rustup target add`
# (ADR-0009). `--dry-run` prints the plan and writes nothing, so the naming
# contract is testable without a cross toolchain.
set -euo pipefail

dry_run=0
if [ "${1:-}" = "--dry-run" ]; then
    dry_run=1
    shift
fi

tag=${1:?usage: release-build.sh [--dry-run] TAG OUTDIR}
outdir=${2:?usage: release-build.sh [--dry-run] TAG OUTDIR}
version=${tag#v}
targets=(x86_64-unknown-linux-musl aarch64-unknown-linux-musl)

# The release image builds with its own `cargo` (a rustup toolchain of
# devbox's rustc version, the only place musl std exists); dev keeps devbox's.
# Unquoted on purpose: CARGO may carry a `+toolchain` argument.
cargo_bin=${CARGO:-cargo}

if [ "$dry_run" -eq 0 ] && ! $cargo_bin zigbuild --version >/dev/null 2>&1; then
    echo "release-build: cargo-zigbuild is missing; build inside the release image (docs/install.md)" >&2
    exit 1
fi

mkdir -p "$outdir"
for target in "${targets[@]}"; do
    artefact="$outdir/sindri-$version-$target"
    if [ "$dry_run" -eq 1 ]; then
        echo "release-build: $cargo_bin zigbuild --release --locked --target $target --bin sindri"
        echo "release-build: would write $artefact"
        continue
    fi
    echo "release-build: $target"
    $cargo_bin zigbuild --release --locked --target "$target" --bin sindri
    install -m 0755 "target/$target/release/sindri" "$artefact"
    # A release that is not static is a release that needs a libc on the
    # user's machine; catch it here rather than in their shell.
    if ! file "$artefact" | grep -q 'statically linked'; then
        echo "release-build: $artefact is not statically linked: $(file "$artefact")" >&2
        exit 1
    fi
done
