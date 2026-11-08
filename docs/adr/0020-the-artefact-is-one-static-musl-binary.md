# The artefact is one static musl binary, and the gate builds it

The product ships as one `x86_64-unknown-linux-musl` binary with no loader, and the release stage of
`make check` builds it and holds its bytes to its claims: it is a static ELF, it is musl and not
glibc, it names no OpenSSL symbol or string, it carries no Nix store path, and it prints its version
on a machine with no devbox, no Nix and no fonts. The store claim is the newest of the five and came
with the build rather than with the decision: `zig` links its own libc with debug info that names the
directory it came from, so a binary meant for every machine arrived carrying a path into the one it
was built on. This record keeps the decision to stay on musl, and says why the alternatives are
worse than the libc it costs.

**Dynamic glibc is not the smaller ask — it is a second toolchain.** devbox's glibc is 2.44 at the
time of this decision, newer than every distribution a reader would install onto: Ubuntu 24.04 is
2.39, Debian 12 is 2.36, RHEL 9 is 2.34. A naive dynamic artefact therefore runs on almost nothing,
and shipping one means building in an old-glibc root, outside the shell ADR-0001 (retired) says
supplies everything. That is a second definition of done for the one artefact that has none.

**Static glibc looks like this artefact and is worse than not shipping.** It links, then fails the
first `getaddrinfo` or `/etc/nsswitch.conf` lookup on the reader's machine a month after release,
with nothing in the build to blame. The `__isoc99_sscanf` family of link errors is the same bug seen
from the other side: glibc's headers against a libc that is not glibc.

**musl static is one file with no loader**, which is also what runs in a scratch container and on a
distribution older than this decade, and it keeps the no-relink proof meaningful: a published
checksum then says something about every machine that runs the binary.

**The gate builds the artefact, and a build that cannot link is a FAIL.** A skip over the compiler's
output is a green line no day could contradict, because the output that excuses a broken toolchain
and the output a broken tree prints are the same output. So three things had to be true before the
stage could stop skipping: the tree links, because the shell supplies a C compiler that targets musl;
the link is a verdict, because a failing one is now a defect in the tree or the shell rather than a
shortcoming of the machine; and the shell pins what the release needs, every package with a version
and the compiler named for its target, so no build reaches the host's `cc` by accident. The one skip
left is a machine whose kernel will not let a sandbox be made, which is a fact about the machine and
never about the tree.

**What the artefact promises is one binary file, and not one dependency.** `bwrap` for the sandbox
tier and `git` for records and the opener stay the host's, and TLS still reads the system trust
store, because rustls goes through the platform verifier: a machine with no CA bundle fails on
either libc. That boundary is part of the decision, not an apology for it.

**A workflow that publishes release assets, and a `curl | bash` installer, are deliberately not in
scope.** Both wait until there is an artefact this stage has held; an installer only deserves to
exist once there is a binary it can verify by checksum and by `sindri --version`.

## What a reader is held to

- The artefact stays `x86_64-unknown-linux-musl` and the release stage is the one path to it:
  `cargo zigbuild` puts the C half of rustls's provider, and the link after it, through `zig cc`, and
  the stage's contract test fails a build that reaches plain `cargo build` instead.
- No OpenSSL symbol or string reaches the binary. `aws-lc-rs` keeping OpenSSL's API names behind its
  own prefix is BoringSSL speaking the same protocol, and `openssl-probe` reading an environment
  variable is a path rather than the library.
- No `/nix/store` path reaches the binary either, which is a property of the artefact and not of the
  machine: devbox supplies build-time tools only.
- The binary runs with no devbox, no Nix and no fonts — an empty root, an empty environment and the
  artefact as the only file in it — and says there the bytes it says here.
- Nothing is added to the shell for the product's runtime; `bubblewrap` stays because the sandbox
  tier proves it, not because the artefact needs it.
- TLS goes through the platform verifier and the system trust store. This supersedes the retired
  record's `webpki-roots` half, which nothing in the tree depends on now.

The reference guard in `crates/sindri-core/tests/doc_references.rs` holds this file as it holds every other: a numbered name, a slug, no number
reused, and no retired marker on a live record.
