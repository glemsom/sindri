# `make check` is the single definition of done (spec §16): the stages below, in
# this order, each printing its own summary line, the first failure stopping the
# run with a non-zero exit and naming the stage that failed.
#
# Run it inside `devbox shell`: cargo, rustfmt, clippy and the musl target all
# come from devbox's init hook (ADR 0001), so nothing here names a toolchain
# path. Outside that shell the stages fail because cargo is not on PATH, which
# is the honest answer — a check that cannot run must not report success.
#
#     make check      every stage, in order (the definition of done)
#     make audit      only the four fast audits (spec §16; the commit hook's entry)
#     make sandbox    only the sandbox tier, and the test of its own verdicts
#     make check STAGES='fmt clippy'
#                     a subset, in order — the switch scripts/check-probe.sh uses
#                     to prove the loop stops at a stage that failed without ever
#                     running the stage that proves it: a gate that rang itself
#                     would never end
#     make <stage>    one stage alone — fmt, clippy, audit, test, frames, bench,
#                     gate, toolchain, sandbox, release
#     make frame F=boot-120x40
#                     print one golden frame as the code draws it now, to eyeball it
#     make frame-update F=boot-120x40
#                     bless a deliberate change to that frame (ADR 0012)
#
# A stage line reads `check: <stage> ... ok` or `... FAIL`, printed after the
# stage's own output so the verdict is the last thing the eye lands on. Nothing
# is printed for a stage that did not run. Two kinds of stage end without an `ok`,
# and neither ever prints one:
#
#   ... MISSING (note)  work that has not landed yet — today ticket #58's evals.
#                       `make check` still exits 0, with the gap printed on its
#                       last lines where a reader cannot miss it.
#   ... SKIPPED         a stage the machine would not let run — the sandbox tier
#                       when the kernel forbids unprivileged user namespaces,
#                       which is the case ticket #7 exists for; the release path
#                       when there is no bwrap or no musl C compiler; the gate
#                       when there is no cargo on PATH. The
#                       reason is the line above the verdict, the exit stays 0,
#                       and the verdict is deliberately not `ok`: a green line
#                       over code nothing covered is the one lie this target
#                       must not tell. Only a skip is ever exit 0 this way; a
#                       stage that ran and failed still stops the run.
#
# `make -j` changes nothing: the stages are a loop, not prerequisites, so they
# stay in order and stop at the first failure.

SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := check

# The definition of done, in order: cheap and fast first, so the slow stages run
# only on a tree that already passes the quick ones. `release` is last because it
# is the only stage that compiles anything for a target nobody else builds.
STAGES := fmt clippy audit test frames bench gate toolchain sandbox evals no-relink release

.PHONY: check $(STAGES) frame frame-update sandbox-tier release-tier gate-tier

check:
	@for stage in $(STAGES); do \
		$(MAKE) --no-print-directory $$stage || exit 1; \
	done

# stage: one stage that really runs. (1) the name it is reported under, (2) the
# command that proves it. The verdict is printed after the command, so it lands
# after whatever pages of output the command produced.
define stage
@if $(2); then printf 'check: $(1) ... ok\n'; else printf 'check: $(1) ... FAIL\n'; exit 1; fi
endef

# missing: a stage whose machinery does not exist yet. It says so plainly and
# prints no `ok`, so the gap is visible rather than counted as green. `make
# check` still exits 0 on a tree whose real stages pass: the gap is ticket #58's
# and tracking it is what that ticket is for. Landing one of these stages is a
# one-line change — swap `$(call missing,<name>,<note>)` for
# `$(call stage,<name>,<command>)` — and nothing else in this file moves.
define missing
@printf 'check: $(1) ... MISSING ($(2))\n'
endef

# skippable: a stage that can honestly say the machine would not let it run.
# Unlike `missing` it is no placeholder — the machinery exists and runs wherever
# the machine allows it — and unlike `stage` it has three verdicts, relayed from
# the command's exit code so that two of them can never blur into one:
#
#   0   ... ok        it ran and passed
#   77  ... SKIPPED   it did not run; the reason is the line above, and the run
#                     goes on. Only a skip keeps exit 0 this way.
#   *   ... FAIL      it ran and failed; `make check` stops here.
#
# 77 is automake's "skipped" status; scripts/sandbox-probe.sh documents it as
# part of its own contract.
define skippable
@status=0; $(2) || status=$$?; \
  case $$status in \
    0)  printf 'check: $(1) ... ok\n' ;; \
    77) printf 'check: $(1) ... SKIPPED\n' ;; \
    *)  printf 'check: $(1) ... FAIL\n'; exit 1 ;; \
  esac
endef

fmt:
	$(call stage,fmt,cargo fmt --all --check)

clippy:
	$(call stage,clippy,cargo clippy --all-targets -- -D warnings)

# The four audits of spec §16 print all four of their lines, always, and this
# stage adds the verdict: audit.sh exits non-zero if any rule found anything.
audit:
	$(call stage,audit,bash scripts/audit.sh .)

test:
	$(call stage,test,cargo test --workspace)

# The golden frames of spec §12 and ADR 0012, landed by ticket #63: every look this
# terminal can wear is a fixture under crates/sindri/tests/fixtures, rendered at a fixed
# size and compared cell by cell, ink names and all. The harness prints one
# `check: frame <frame>-<cols>x<rows> ... ok` line per fixture, which is why this stage
# runs with `--nocapture` — those lines are the stage's summary beneath its own verdict
# — and on one thread, so two runs read in the same order.
#
# It sits after `test` because `cargo test --workspace` has already built this target, so
# all the stage pays is running it. A frame that no longer matches stops the run here: a
# picture that changed without its fixture changing in the same commit is the change
# ADR 0012 exists to catch.
frames:
	$(call stage,frames,cargo test -p sindri --test frames -- --nocapture --test-threads=1)

# A frame nobody can see is a frame nobody approves (ADR 0012). `frame` prints one
# fixture's frame as the code draws it now, for eyeballing; `frame-update` rewrites the
# fixture with that frame, which is the only way a golden ever moves: deliberately, in
# the commit that changed the look. Both take the frame from F, `<frame>-<cols>x<rows>`;
# a frame no test holds yet prints nothing, because there is no picture to print.
#
# `SINDRI_FRAME` names the frame to print and never changes what is compared, so a stray
# variable in a shell cannot make a test report a picture it did not check.
# `INSTA_UPDATE=force` is insta's own blessing switch, because `cargo insta` is not
# installed here — and every frame test is named after its fixture with the dashes
# underscored, so the test filter names exactly one frame.
frame:
	$(if $(F),,$(error make frame needs F=<frame>-<cols>x<rows>, e.g. F=boot-120x40))
	@SINDRI_FRAME=$(F) cargo test -q -p sindri --test frames -- --nocapture $(subst -,_,$(F))

frame-update:
	$(if $(F),,$(error make frame-update needs F=<frame>-<cols>x<rows>, e.g. F=boot-120x40))
	INSTA_UPDATE=force cargo test -q -p sindri --test frames -- $(subst -,_,$(F))

# The wording bench of spec §9 and §16, landed by ticket #60: the preamble is
# re-sent every turn, so every word is a tax, and the bench prices each section
# and each tool description by token cost against the budget it states — a
# wording over that budget stops the run here. It sits after `test` on purpose:
# `cargo test --workspace` has already built this target, so all this stage pays
# is running it, not compiling it, and `--nocapture` is what lets the table out.
# The estimator is printed with the table, because it is an estimate of a token
# count and not one.
bench:
	$(call stage,bench,cargo test -p sindri-core --test wording_bench -- --nocapture)

# The stage that proves this gate has teeth (spec §16, ticket #5): `make check` is
# the definition of done, so a run of it must be able to say no. scripts/check-
# probe.sh writes one defect at a time into the tree — an unformatted file, a
# clippy warning, a panicking `#[test]` — drives this file's own loop over a
# stage subset, and requires the owning stage to print `... FAIL` and the run to
# stop behind it; then it takes the defect back out and compares the tree against
# the `git status --porcelain` it took first. Split in two the way the sandbox
# tier is split, and for the same reason: `gate` runs the probe's contract test
# first, and `gate-tier` is the probe alone, which is what that test drives. A
# test that rang the gate would ring itself.
#
# It sits after `test` and `bench` on purpose: the defects are test targets, so
# the workspace is already built and each case costs one fragment and one suite
# run, not a build. And it is `skippable` because a machine with no cargo on PATH
# cannot drive the gate at all: outside the devbox shell it says so at 77 and
# prints SKIPPED, never the `ok` a probe that ran has earned.
gate:
	$(call stage,gate-test,bash scripts/check-probe-test.sh)
	$(call skippable,gate,bash scripts/check-probe.sh)

gate-tier:
	$(call skippable,gate,bash scripts/check-probe.sh)

# Proves the devbox shell supplies the pinned Rust 1.99 and the musl target
# (ADR 0001, ticket #4 wired here by #5).
toolchain:
	$(call stage,toolchain,bash scripts/toolchain-probe.sh)

# The sandbox tier (spec §16, ADR 0001): proves this machine can confine a
# command, which needs unprivileged user namespaces. Split in two so the tier's
# verdicts can be tested without the test running itself — `sandbox` is the stage
# `make check` calls, and runs the tier's contract test first, while
# `sandbox-tier` is the relay alone, which is what scripts/sandbox-probe-test.sh
# drives. The tier still runs
# by default here: only a machine that refuses turns it into a printed SKIPPED.
sandbox:
	$(call stage,sandbox-test,bash scripts/sandbox-probe-test.sh)
	$(call skippable,sandbox,bash scripts/sandbox-probe.sh)

sandbox-tier:
	$(call skippable,sandbox,bash scripts/sandbox-probe.sh)

# Ticket #58 owns this one. A placeholder until it lands.
evals:
	$(call missing,evals,evals are not yet present - ticket #58)

# The provenance proof of spec §16, and the stage ticket #61 landed: the binary
# embeds git describe and the commit date and never the wall clock, so a tree
# nobody changed builds to the same bytes twice and relinks nothing. The script
# is the proof — build twice, touch a doc, re-run the provenance script, read the
# version back out of the binary — and prints `no-relink: ok (N checks)` of its
# own before this stage's verdict.
no-relink:
	$(call stage,no-relink,bash scripts/no-relink-proof.sh)

# The release path of spec §16 and ADR 0001, landed by ticket #62: the artefact the
# product ships, built and held to its promises rather than built by hand on
# release day — static, `x86_64-unknown-linux-musl`, no OpenSSL anywhere in it, and
# able to run on a machine with no devbox, no Nix and no fonts. Split in two the
# way the sandbox tier is split, and for the same reason: `release` runs the
# probe's contract test first, and `release-tier` is the probe alone, which is what
# that test drives. A test that ran itself would never end.
#
# The probe is `skippable` because of what it can honestly lack: bwrap, when the
# kernel forbids unprivileged user namespaces, and a musl C compiler, which
# devbox does not supply today — the C half of rustls's crypto provider is built
# by the host's compiler against glibc headers, and the artefact cannot be linked.
# Both say so in a printed line and a SKIPPED verdict; neither is ever an `ok`.
release:
	$(call stage,release-test,bash scripts/release-probe-test.sh)
	$(call skippable,release,bash scripts/release-probe.sh)

release-tier:
	$(call skippable,release,bash scripts/release-probe.sh)
