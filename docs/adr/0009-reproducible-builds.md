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
   `zig` and `cargo-zigbuild`), because nixpkgs' `rustc` has no rustup and
   therefore no `rustup target add`. See the addendum for where musl `std`
   comes from.

## Consequences

- Reproducible, but **not hermetic**: devbox re-fetches nixpkgs from the network
  on `devbox run`, even after install. Only the container build is hermetic.
  Anyone needing an offline or air-gapped build must use that path.
- One toolchain definition cannot drift between dev, CI and release.
- A container is only paid for when CI or a release needs it.

## Addendum (T11): zig and cargo-zigbuild in devbox, musl std in the image

`devbox.json` now pins `zig` and `cargo-zigbuild`, because cross-compiling for
musl is what a release needs and the ADR above already makes devbox the only
toolchain definition. What devbox cannot supply is the part that made rustup
tempting: **musl `std`**.

- nixpkgs' `rustc` installs only the host `rust-std`; `x86_64-unknown-linux-musl`
  and `aarch64-unknown-linux-musl` are absent from `lib/rustlib`.
- The upstream `rust-std` tarball of the *same* version string is not usable
  with it: `error[E0514]: found crate `std` compiled by an incompatible version
  of rustc`, because a nixpkgs `rustc` built from source and an upstream built
  one do not share metadata hashes.

Decision: the **release image** installs a rustup toolchain of devbox's own
rustc version, with the two musl targets, and builds with
`CARGO="cargo +<version>"`. This does not weaken rule 1 above: rustup is never
installed in the devbox environment and never appears on a developer's `PATH`,
so no shim can shadow `cargo` in the edit → `make check` loop. The release image
is the only place it exists.

Consequence to keep in mind: the release build needs the network (rustup and the
target std) even though the image itself is generated from `devbox.json`. If a
future nixpkgs ships musl `rust-std` for the pinned rustc, this addendum should
be deleted and the `CARGO` override dropped from the workflow.
