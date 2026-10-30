---
status: accepted
date: 2026-10-29
---

# 0009 — devbox for the dev loop, containers for release

## Decision

1. `devbox.json` + `devbox.lock` are committed and pin nixpkgs; devbox supplies `rustc`, `cargo`, `rustfmt`, `clippy`, `cargo-nextest`, `zig`, `cargo-zigbuild`.
2. Never install rustup in the devbox environment or on a developer's `PATH`: shims shadow `cargo`.
3. The release image is generated from the same `devbox.json`; one toolchain definition, two consumers.
4. The image installs a rustup toolchain of devbox's rustc version plus `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, and builds with `CARGO="cargo +<version>"`.
5. `make check` wraps devbox; `make build`, `make run`, `make install` follow.

## Consequences

- nixpkgs' `rustc` ships host `rust-std` only, and an upstream `rust-std` of the same version string fails `error[E0514]` (different metadata hash) — hence the rustup toolchain in the image, which is the only place rustup exists.
- **Not hermetic**: `devbox run` re-fetches nixpkgs; only the container build is hermetic. The release build needs the network for rustup and the target `std`.
- Drop rule 4 if nixpkgs ever ships musl `rust-std` for the pinned rustc.
