# Installing, and shipping, Sindri

Sindri ships as two static musl binaries — `x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl` — plus `SHA256SUMS` and a changelog, on a GitHub
Release tagged `vX.Y.Z`. No Rust toolchain, no libc to match, nothing to
compile on the machine that runs it.

## Install, in one command

```console
$ curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
```

The installer:

1. refuses a kernel that is not Linux and a machine it has no binary for;
2. resolves the latest release, or the one you name;
3. downloads the binary **and** `SHA256SUMS`, and installs nothing unless the
   checksum matches;
4. installs to `~/.local/bin/sindri`, replacing any earlier copy atomically;
5. prints the version the installed binary reports, and ends by sending you to
   `sindri doctor`.

```console
$ curl -fsSL …/install.sh | sh -s -- --version 0.1.0 --dir /usr/local/bin
```

| Flag | Environment | Default |
| --- | --- | --- |
| `--version VERSION` | `SINDRI_VERSION` | the latest release |
| `--dir DIR` | `SINDRI_INSTALL_DIR` | `~/.local/bin` |

`SINDRI_RELEASE_BASE_URL` points the installer at a mirror (or a `file://`
directory); the tests use it so no test touches the network. Exit `2` is a
usage error — including a kernel or a machine no release serves — and `1`
anything else: a failed download, a checksum mismatch, an unwritable directory.
`SINDRI_UNAME_S` and `SINDRI_UNAME_M` override `uname`, which is how the tests
exercise the aarch64 and unsupported-machine paths on one host.

The last line is not decoration: **run `sindri doctor` on a fresh install.** It
proves bubblewrap, the binary set and the mount plan on *your* machine, and
tells you one actionable line if your distribution blocks unprivileged user
namespaces. An agent failure caused by a sandbox that could never start is the
one bad first impression a static binary must not deliver.

## What a release carries

| File | What it is |
| --- | --- |
| `sindri-X.Y.Z-x86_64-unknown-linux-musl` | static binary for x86_64 |
| `sindri-X.Y.Z-aarch64-unknown-linux-musl` | static binary for aarch64 |
| `SHA256SUMS` | the two lines above, sorted, verified by the installer |
| `CHANGELOG.md` | the changelog: the tickets closed since the previous tag |

The version reported by `--version` is `git describe` and the commit date,
injected at build time by `crates/types/build.rs` — never the wall clock, so an
unchanged tree does not relink (ADR-0009).

## Cutting a release (maintainer)

The whole of it is one tag; the workflow does the rest.

The tag must point at a commit CI has already checked — a tag does not run the
`make check` gate, only the release job.

```console
$ git checkout main && git pull --ff-only
$ git tag -s v0.1.0 -m "sindri v0.1.0"
$ git push origin v0.1.0
```

`.github/workflows/release.yml` then:

1. generates a release image from the committed `devbox.json`
   (`devbox generate dockerfile`), so dev, CI and release share one toolchain
   definition (ADR-0009);
2. builds both targets inside that image with `cargo-zigbuild`
   (`scripts/release-build.sh`);
3. writes `SHA256SUMS` (`scripts/release-checksums.sh`) and the changelog
   derived from the merge commits since the previous tag
   (`scripts/release-changelog.sh`);
4. re-verifies every checksum and refuses to publish anything that is not
   statically linked;
5. creates the GitHub Release with both binaries, the checksums and the
   changelog.

Locally the same steps, without Docker:

```console
$ make release TAG=v0.1.0   # dist/: both binaries, SHA256SUMS, CHANGELOG.md
$ make scripts-check        # the installer and release scripts, offline
```

A ticket is in the changelog if it landed as a merge commit whose subject is
`TN: <title> (#issue)`, which is this repository's style. A ticket closed by a
squash or a direct commit does not appear, so amend the release notes by hand
when one does.

`make scripts-check` is part of `make check`. It runs the installer through
`sh` against a `file://` release, so the artefact names the build produces are
proven to be the names the installer looks for.

### Where musl std comes from

devbox's `rustc` includes only the host's standard library, and it refuses the
upstream `rust-std` tarball of the same version (the metadata hashes differ:
`E0514`). There is also no rustup, deliberately — its shims shadow `cargo` and
break the pinned toolchain (ADR-0009).

So the release image installs a rustup toolchain **of devbox's own rustc
version**, with the two musl targets, and builds with `CARGO="cargo +<that
version>"`. It exists only inside the container; a developer's `PATH` is
untouched, and `make check` never sees it.

## Proving it end to end (acceptance)

The acceptance criterion for the release is a **pristine container with no Rust
installed, going from the release page to a successful batch task**. It needs a
real release and a model credential, so it is deliberately not part of
`make check` — but it is the one check that proves the whole chain, and a
maintainer runs it once per release:

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

`make scripts-check` proves the installer, the checksums, the changelog and the
artefact naming offline: a `file://` release, a throwaway git repository and a
build dry run. It does **not** prove the two static binaries exist, because a
machine with devbox has no musl `std` to build them with (see below); the first
tag is what proves that, and the release job refuses to publish anything that
fails the checksum or static-link check.
