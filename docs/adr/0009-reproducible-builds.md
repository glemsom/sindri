# 0009 — devbox for the dev loop, containers for release

- **Status:** accepted
- **Date:** 2025-10-29

## Context

Builds must be reproducible, and an AI agent's inner cycle is
edit → `make check` → edit. A container per iteration punishes exactly the loop
we optimise for; a bare host toolchain makes "works on my machine" permanent.

## Decision

1. **devbox is the dev loop's source of truth.** `devbox.json` and `devbox.lock`
   are committed. It pins nixpkgs and supplies rustc, cargo, rustfmt, clippy and
   nextest. Never install rustup alongside it — shims shadow `cargo`.
2. **Containers build the releases.** The release image is generated from the
   same `devbox.json`, so there is one toolchain definition with two consumers.
3. `make check` wraps devbox; `make build`, `make run` and `make install` follow.
4. Static musl binaries are produced with `cargo-zigbuild` (devbox supplies
   `zig`), because nixpkgs' `rustc` has no rustup and therefore no
   `rustup target add`.

## Consequences

- Reproducible, but **not hermetic**: devbox re-fetches nixpkgs from the network
  on `devbox run`, even after install. Only the container build is hermetic.
  Anyone needing an offline or air-gapped build must use that path.
- One toolchain definition cannot drift between dev, CI and release.
- A container is only paid for when CI or a release needs it.
