# The developer entry point. Every Rust step goes through devbox, which pins the
# toolchain (ADR-0009); `make check` is the single definition of done.

DEVBOX ?= devbox
CARGO  ?= $(DEVBOX) run -- cargo

.PHONY: all check fmt clippy audit test relink eval eval-live build run install clean

all: check

## check: the gate. fmt, clippy with warnings denied, the 400-line audit, tests,
## the evals, and proof that an unchanged tree does not relink.
check: fmt clippy audit test relink eval

fmt:
	$(CARGO) fmt --all --check

clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

## audit: source files stay at or under 400 lines (ADR-0008).
audit:
	@scripts/audit-line-length.sh

test:
	$(CARGO) nextest run --workspace --no-fail-fast
	$(CARGO) test --doc --workspace

## relink: the build embeds `git describe` and the commit date, never the wall
## clock, so an unchanged tree must not relink.
relink:
	@$(DEVBOX) run -- scripts/check-relink.sh

## eval: eight scripted runs against recorded fixtures — offline, no key, no
## live provider. Prints one pass/fail line per eval and fails if any did.
## `make eval-live` asks your own provider instead; see the README.
eval:
	$(CARGO) build --quiet --package sindri-cli --bin sindri
	$(CARGO) run --quiet --package sindri-evals

## eval-live: the same eight evals against a real provider, with your own
## credential and at your own cost.
eval-live:
	$(CARGO) build --quiet --package sindri-cli --bin sindri
	$(CARGO) run --quiet --package sindri-evals -- --live

build:
	$(CARGO) build --release

run:
	$(CARGO) run -- $(ARGS)

## install: install the working tree's binary.
install:
	$(CARGO) install --path crates/cli --force

clean:
	$(DEVBOX) run -- cargo clean
