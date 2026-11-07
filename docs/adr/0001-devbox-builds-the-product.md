# devbox builds the product; it never runs it

`devbox.json` supplies build-time tools only — Rust, cargo, rustfmt, clippy, the musl target and
`bwrap` for tests. Nothing it provides may be required at runtime: a `sindri` binary that only
works inside a devbox shell is a binary nobody can install.

Consequences worth remembering:

- `sindri` resolves `bwrap` from the **host** `PATH` at startup, never from a Nix store path
  baked in at build time.
- The release artefact stays `x86_64-unknown-linux-musl`, and because a musl binary cannot use
  the GNU NSS resolver, TLS goes through **rustls + webpki-roots**, never OpenSSL. This
  constrains the `reqwest` feature set from day one rather than at release time.
- `make check` runs inside `devbox shell`; the release build is a separate target. One owner
  per job.
