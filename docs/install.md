# Installing, and shipping, Sindri

**Nothing is published yet.** `git tag` is empty and the releases API answers `[]`, so
`install.sh` — which asks `releases/latest` for a `tag_name` — finds none and stops asking
you to pass `--version`. The installer, the release workflow and the release scripts are
all in the tree and exercised by `make check`; what is missing is the first tag. Until it
is cut, the source build below is the path that installs, and every section after it
describes the release as the plan rather than the state.

## Install from source

Sindri is a Rust workspace, so a checkout and a toolchain are the whole of it. `devbox` is
the toolchain the tree is tested with; any recent stable Rust builds the same binary.

```console
$ git clone https://github.com/glemsom/sindri
$ cd sindri
$ devbox run -- cargo install --path crates/cli
$ sindri doctor
```

Without devbox the same line without the wrapper works — `cargo install --path
crates/cli` — and installs to `~/.cargo/bin`, which has to be on your `PATH`. Either way it
is a release build from a cold target directory, so it takes a couple of minutes.

The binary reports `git describe --tags --always` and the commit date, injected at build
time by `crates/types/build.rs` — never the wall clock, so an unchanged tree does not
relink (`make relink` proves it). A build from a commit rather than a tag says the short
commit hash instead: `sindri 9be47ec (2026-11-03)`. That is the tree's own version, not a
fault.

`sindri doctor` is not decoration: **run it on a fresh install.** It proves bubblewrap, the
binary set and the mount plan on *your* machine, and tells you one actionable line if your
distribution blocks unprivileged user namespaces. It exits `0` when the sandbox can start,
`3` when it cannot, and `2` for a configuration a run would refuse. An agent failure
caused by a sandbox that could never start is the one bad first impression a fresh install
must not deliver.

## Install, in one command (the plan: needs v0.1.0)

Once a release exists, Sindri ships as two static musl binaries —
`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` — plus `SHA256SUMS` and a
changelog, on a GitHub Release tagged `vX.Y.Z`. No Rust toolchain, no libc to match,
nothing to compile on the machine that runs it.

```console
$ curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
```

The installer:

1. refuses a kernel that is not Linux and a machine it has no binary for;
2. resolves the release you name, or asks `SINDRI_LATEST_URL` (GitHub's
   `releases/latest`) for the newest one and takes the first `tag_name` it finds —
   if that document names no tag, it stops and asks you to pass `--version`;
3. downloads the binary **and** `SHA256SUMS`, and installs nothing unless the
   checksum matches;
4. installs to `~/.local/bin/sindri`, replacing any earlier copy atomically;
5. prints the version the installed binary reports, and ends by sending you to
   `sindri doctor`.

```console
$ curl -fsSL …/install.sh | sh -s -- --version 0.1.0 --dir "$HOME/bin"
```

| Flag | Environment | Default |
| --- | --- | --- |
| `--version VERSION` | `SINDRI_VERSION` | the latest release |
| `--dir DIR` | `SINDRI_INSTALL_DIR` | `~/.local/bin` |
| `-h`, `--help` | — | prints the usage and exits `0` |

The target directory must already be writable by you: the installer runs
`mkdir -p` and then a plain copy, with no elevation, so `--dir /usr/local/bin`
only works where you can write there. `SINDRI_RELEASE_BASE_URL` points it at a
mirror (or a `file://` directory) and `SINDRI_REPO` (`owner/name`) and
`SINDRI_LATEST_URL` move the release lookup and the "latest" document
elsewhere; the tests use them so no test touches the network. `SINDRI_UNAME_S`
and `SINDRI_UNAME_M` override `uname`, which is how the tests exercise the
aarch64 and unsupported-machine paths on one host. Exit `2` is a usage error —
including a kernel or a machine no release serves — and `1` anything else: a
failed download, a checksum mismatch, an unwritable directory.

## What a release will carry (plan)

| File | What it is |
| --- | --- |
| `sindri-X.Y.Z-x86_64-unknown-linux-musl` | static binary for x86_64 |
| `sindri-X.Y.Z-aarch64-unknown-linux-musl` | static binary for aarch64 |
| `SHA256SUMS` | the two lines above, sorted, verified by the installer |
| `CHANGELOG.md` | the changelog: the tickets closed since the previous tag |

The release binaries are built from the tagged checkout, so they report the tag. A tree
without `git` at all falls back to the `Cargo.toml` version with a commit date of
`unknown`.

## Cutting a release (maintainer, plan)

The whole of it is one tag; the workflow does the rest.

The tag must point at a commit `.github/workflows/ci.yml` has already checked —
a tag runs only the release job, never the `make check` gate.

```console
$ git checkout main && git pull --ff-only
$ git tag -s v0.1.0 -m "sindri v0.1.0"
$ git push origin v0.1.0
```

`.github/workflows/release.yml` then:

1. generates a release image from the committed `devbox.json`
   (`devbox generate dockerfile`), so dev, CI and release share one toolchain
   definition;
2. builds both targets inside that image with `cargo-zigbuild`
   (`scripts/release-build.sh`);
3. writes `SHA256SUMS` (`scripts/release-checksums.sh`) and the changelog
   derived from the merge commits since the previous tag
   (`scripts/release-changelog.sh`);
4. re-verifies every checksum and refuses to publish anything that is not
   statically linked;
5. creates the GitHub Release with both binaries, the checksums and the
   changelog.

A ticket is in the changelog if it landed as a merge commit whose subject is
`TN: <title> (#issue)`, which is this repository's style. A ticket closed by a
squash or a direct commit does not appear, so amend the release notes by hand
when one does.

### Building the artefacts locally

There is no local path to the two binaries on a machine that has only devbox: nixpkgs'
`rustc` carries the host's standard library alone, so `cargo zigbuild --target …-musl`
stops with no `core` for that target, and `make release` — the same three scripts in one
target — fails in the same place.

The one build that produces `dist/` runs in the release image, whose `docker run` step in
`.github/workflows/release.yml` is the command to copy: it installs a rustup toolchain of
devbox's own rustc version inside the container (*Where musl std comes from* below) before
`scripts/release-build.sh`, and nothing outside that image has that toolchain.

```console
$ devbox generate dockerfile --force --root-user
$ docker build --tag sindri-release .
$ # then the `docker run … sindri-release` step from .github/workflows/release.yml
```

Two of the three scripts do run locally, on artefacts you already have:

```console
$ make scripts-check                          # the installer and the scripts, offline
$ scripts/release-checksums.sh dist           # SHA256SUMS for whatever is in dist/
$ scripts/release-changelog.sh v0.1.0 > dist/CHANGELOG.md
```

`make scripts-check` is part of `make check`. It runs the installer through `sh` against a
`file://` release laid out under the names a dry run of `scripts/release-build.sh` says it
would write, so the build's naming and the installer's lookup cannot drift apart
unnoticed; it proves that contract, and nothing about whether a binary exists.

### Where musl std comes from

devbox's `rustc` includes only the host's standard library, and it refuses the
upstream `rust-std` tarball of the same version (the metadata hashes differ:
`E0514`). There is also no rustup, deliberately — its shims shadow `cargo` and
break the pinned toolchain.

So the release image installs a rustup toolchain **of devbox's own rustc
version**, with the two musl targets, and builds with `CARGO="cargo +<that
version>"`. It exists only inside the container; a developer's `PATH` is
untouched, and `make check` never sees it.

## Proving it end to end (acceptance)

The acceptance criterion for a release is a **pristine container with no Rust
installed, going from the release page to a successful batch task**. It needs a
real release and a model credential, so it is deliberately not part of
`make check` — but it is the one check that proves the whole chain, and a
maintainer runs it once per release. No release exists yet, so nothing has run it:

```console
$ docker run --rm -it ubuntu:24.04 bash
# no rustc, no cargo, nothing but a base image
root@container# apt-get update && apt-get install -y curl bubblewrap ca-certificates
root@container# curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
…
Next: run `sindri doctor` to prove the sandbox works on this machine.
root@container# export PATH="$HOME/.local/bin:$PATH"
root@container# sindri doctor            # exit 0, and the mount plan it prints
root@container# ANTHROPIC_API_KEY=… sindri -b "say hi and exit"
```

If `doctor` reports `setting up uid map: Permission denied`, the container
blocks unprivileged user namespaces: it needs `--privileged` (or a seccomp and
AppArmor profile that permits them) before the sandbox — and therefore the
agent — can run at all.

### What is proven here, and what is not

`make scripts-check` proves the installer, the checksums, the changelog and the artefact
naming offline: a `file://` release, a throwaway git repository and a dry run of the build
whose own printed names the fixture release is laid out under. It does **not** prove the
two static binaries exist, because a machine with devbox has no musl `std` to build them
with (see above); the first tag is what proves that, and the release job refuses to
publish anything that fails the checksum or static-link check.
