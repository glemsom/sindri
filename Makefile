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
#     make <stage>    one stage alone — fmt, clippy, audit, test, toolchain,
#                     sandbox
#
# A stage line reads `check: <stage> ... ok` or `... FAIL`, printed after the
# stage's own output so the verdict is the last thing the eye lands on. Nothing
# is printed for a stage that did not run. Two kinds of stage end without an `ok`,
# and neither ever prints one:
#
#   ... MISSING (note)  work that has not landed yet — today ticket #58's evals.
#                       `make check` still exits 0, with the gap printed on its
#                       last lines where a reader cannot miss it.
#   ... SKIPPED         a stage the machine would not let run — today only the
#                       sandbox tier, when the kernel forbids unprivileged user
#                       namespaces, which is the case ticket #7 exists for. The
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
# only on a tree that already passes the quick ones.
STAGES := fmt clippy audit test toolchain sandbox evals no-relink

.PHONY: check $(STAGES) sandbox-tier

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
