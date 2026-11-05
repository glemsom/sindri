# Installing, and shipping, Sindri

**Nothing is published yet.** `git tag` is empty and the releases API answers `[]`, so
`install.sh` finds no release. The installer, the workflow and the release scripts are in
the tree and exercised by `make check`; what is missing is the first tag. Until it is cut,
the source build is the path that installs, and everything after it is the plan.

## Install from source

A checkout and a toolchain are the whole of it. `devbox` is the toolchain the tree is
tested with; any recent stable Rust builds the same binary.

```console
$ git clone https://github.com/glemsom/sindri
$ cd sindri
$ devbox run -- cargo install --path crates/cli
$ sindri doctor
```

Without devbox, `cargo install --path crates/cli` installs to `~/.cargo/bin`, which has to
be on your `PATH`.

The binary reports `git describe --tags --always --dirty` and the commit date, injected at
build time by `crates/types/build.rs` — never the wall clock, so an unchanged tree does not
relink (`make relink` proves it). A build from a commit rather than a tag says the short
hash: `sindri 9be47ec (2026-11-03)`. A `-dirty` suffix after it says the tree had
uncommitted changes when the binary was built — the line to quote in a bug report, because
it names the tree you were actually running.

**Run `doctor` on a fresh install.** It proves bubblewrap, the binary set and the mount plan
on *your* machine, and prints one actionable line if your distribution blocks unprivileged
user namespaces. Exit `0` when the sandbox can start, `3` when it cannot, `2` for a
configuration a run would refuse.

## Install, in one command (the plan: needs v0.1.0)

A release carries two static musl binaries — `x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl` — plus `SHA256SUMS` and a changelog, on a GitHub Release
tagged `vX.Y.Z`. No toolchain, no libc to match, nothing to compile.

```console
$ curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
$ curl -fsSL …/install.sh | sh -s -- --version 0.1.0 --dir "$HOME/bin"
```

It refuses a kernel that is not Linux and a machine it has no binary for; resolves the
release you name, or asks `SINDRI_LATEST_URL` for the newest `tag_name`; downloads the
binary **and** `SHA256SUMS` and installs nothing unless the checksum matches; installs to
`~/.local/bin/sindri`, replacing an earlier copy atomically; then prints the installed
version and sends you to `sindri doctor`.

| Flag | Environment | Default |
| --- | --- | --- |
| `--version VERSION` | `SINDRI_VERSION` | the latest release |
| `--dir DIR` | `SINDRI_INSTALL_DIR` | `~/.local/bin` |
| `-h`, `--help` | — | prints the usage and exits `0` |

The target directory must already be writable: `mkdir -p` and a plain copy, no elevation.
`SINDRI_RELEASE_BASE_URL` points it at a mirror (or a `file://` directory), `SINDRI_REPO`
and `SINDRI_LATEST_URL` move the lookup, and `SINDRI_UNAME_S`/`SINDRI_UNAME_M` override
`uname` — the tests use all of them, so no test touches the network or needs a second host.
Exit `2` is a usage error, including a kernel or machine no release serves; `1` anything
else.

## What a release will carry (plan)

| File | What it is |
| --- | --- |
| `sindri-X.Y.Z-x86_64-unknown-linux-musl` | static binary for x86_64 |
| `sindri-X.Y.Z-aarch64-unknown-linux-musl` | static binary for aarch64 |
| `SHA256SUMS` | the two lines above, sorted, verified by the installer |
| `CHANGELOG.md` | the tickets closed since the previous tag |

The binaries are built from the tagged checkout, so they report the tag. A tree without
`git` falls back to the `Cargo.toml` version with a commit date of `unknown`.

## Cutting a release (maintainer, plan)

The whole of it is one tag; the workflow does the rest. The tag must point at a commit
`ci.yml` has already checked — a tag runs only the release job, never the gate.

```console
$ git checkout main && git pull --ff-only
$ git tag -s v0.1.0 -m "sindri v0.1.0"
$ git push origin v0.1.0
```

`.github/workflows/release.yml` then generates a release image from the committed
`devbox.json` (so dev, CI and release share one toolchain definition), builds both targets
inside it with `cargo-zigbuild`, writes `SHA256SUMS` and the changelog, re-verifies every
checksum, refuses to publish anything not statically linked, and creates the Release.

A ticket is in the changelog if it landed as a merge commit whose subject is
`TN: <title> (#issue)`. A squash or a direct commit does not appear, so amend by hand.

### Building the artefacts locally

There is no local path to the two binaries on a devbox machine: nixpkgs' `rustc` carries
only the host's standard library, so `cargo zigbuild --target …-musl` stops with no `core`
and `make release` fails in the same place. The build that produces `dist/` runs in the
release image — the `docker run …` step in `release.yml` is the command to copy.

```console
$ devbox generate dockerfile --force --root-user
$ docker build --tag sindri-release .
```

Two of the three scripts do run locally, on artefacts you already have:

```console
$ make scripts-check                          # the installer and the scripts, offline
$ scripts/release-checksums.sh dist           # SHA256SUMS for whatever is in dist/
$ scripts/release-changelog.sh v0.1.0 > dist/CHANGELOG.md
```

`make scripts-check` runs the installer through `sh` against a `file://` release laid out
under the names a dry run of `scripts/release-build.sh` prints, so the build's naming and
the installer's lookup cannot drift apart unnoticed.

### Where musl std comes from

devbox's `rustc` refuses the upstream `rust-std` tarball of the same version (different
metadata hashes, `E0514`), and there is no rustup on purpose — its shims shadow `cargo` and
break the pinned toolchain. So the release image installs a rustup toolchain **of devbox's
own rustc version** with the two musl targets, and builds with `CARGO="cargo +<that
version>"`. It exists only inside the container.

## Proving it end to end (acceptance)

The acceptance criterion is a **pristine container with no Rust installed, going from the
release page to a successful batch task**. It needs a real release and a credential, so it
is not part of `make check`, and a maintainer runs it once per release. No release exists
yet, so nothing has run it:

```console
$ docker run --rm -it ubuntu:24.04 bash
root@container# apt-get update && apt-get install -y curl bubblewrap ca-certificates
root@container# curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
root@container# export PATH="$HOME/.local/bin:$PATH"
root@container# sindri doctor            # exit 0, and the mount plan it prints
root@container# ANTHROPIC_API_KEY=… sindri -b "say hi and exit"
```

If `doctor` reports `setting up uid map: Permission denied`, the container blocks
unprivileged user namespaces: it needs `--privileged` (or a seccomp and AppArmor profile
that permits them) before the sandbox — and therefore the agent — can run at all.

### What is proven here, and what is not

`make scripts-check` proves the installer, the checksums, the changelog and the artefact
naming offline. It does **not** prove the two static binaries exist — no musl `std` on a
devbox machine; the first tag proves that, and the release job refuses to publish anything
that fails the checksum or static-link check.
