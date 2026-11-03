"""The fifteen system-prompt variants, and the baseline they are measured against.

The baseline is not written here: `BASE` is the shipped `STANCE`, read out of
`crates/core/src/preamble.rs` at import, so `v00` builds the prompt the tree
actually carries. A run sends that stance with the Environment section composed
after it, and the bench measures the stance — the part these variants vary.

`v15` is the exception to that rule and the reason it is worth stating: it is the
rewritten preamble as #416 landed it, pinned here as text rather than read out of
the file. A variant that followed the file could not be the record of a wording —
the next rewrite would silently re-label every sweep already taken with it — so the
one the tree ships today is written down, measured under its own name, and left
behind when the file moves. `scripts/tests/bench-prompt.sh` holds the two together:
the prompt the tree ships is always one of the bench's own texts, so it can always be
named — `v15` today, and a new variant beside it on the day a ticket rewrites the
stance. An entry is never overwritten: `v15` stays the record of #416's wording.

`patch` writes one variant's text into the file the tree ships the stance in;
`build_variant.sh` keeps a copy of that file before it calls `patch` and puts the
copy back on every way out, a failed build included, so the source it leaves is
the source it found.
"""

import pathlib

SHIPPED = pathlib.Path(__file__).resolve().parents[2] / "crates/core/src/preamble.rs"

MARK = 'const STANCE: &str = "'


def shipped(path=SHIPPED) -> str:
    """`STANCE` as the tree spells it: its literal with the line continuations
    and escapes resolved. A copy kept here would drift from the prompt the tree
    ships, and a bench measuring a prompt no run carries measures nothing."""
    source = pathlib.Path(path).read_text()
    start = source.index(MARK) + len(MARK)
    text, at = [], start
    while source[at] != '"':
        if source[at] != "\\":
            text.append(source[at])
            at += 1
            continue
        escape = source[at + 1]
        if escape == "\n":
            at += 2
            while source[at] in " \t":
                at += 1
            continue
        text.append({"n": "\n", "t": "\t"}.get(escape, escape))
        at += 2
    return "".join(text)


def patch(variant: str, path=SHIPPED) -> str:
    """Write `variant`'s text into the `STANCE` literal of `path`, and answer
    the text that went in. `v00` is the tree's own prompt: it writes nothing and
    answers what the tree ships. A variant the bench does not hold is refused
    before a byte is written, because a typo is not a prompt."""
    source = pathlib.Path(path).read_text()
    if variant == "v00":
        return shipped(path)
    try:
        text = VARIANTS[variant][1]
    except KeyError:
        raise SystemExit(
            f"variants: the bench holds no variant {variant!r}: {sorted(VARIANTS)}"
        ) from None
    start = source.index(MARK) + len(MARK)
    end = source.index('";', start)
    escaped = text.replace("\\", "\\\\").replace('"', '\\"')
    pathlib.Path(path).write_text(source[:start] + escaped + source[end:])
    return text


BASE = shipped()

# The rewritten preamble, as #416 landed it: the stance the tree ships at the
# commit that pinned this entry, written down so the sweep that measured it stays
# recomputable after the next rewrite. A `v15` result file is a measurement of
# these words and no other.
REWRITTEN = (
    "You are Sindri, answering a developer at a Unix terminal. Reply with the answer "
    "itself: no lead-in, no closing offer of further help. Run commands rather than "
    "guessing. Read the fewest bytes that answer: search first (`rg -n`), then open "
    "only the lines you need (`sed -n 'A,Bp'`); never `cat` a whole file, and never "
    "run a search or a listing twice. Many inputs are one script: ten files are one "
    "loop in one call. A fence marked `mermaid` is drawn as a picture: draw what "
    "prose cannot."
)

TRIM = ("You are Sindri, an assistant answering a developer at a Unix terminal. Answer the "
        "question you were given, using the context after it when there is some. You have one "
        "tool, `bash`: run commands rather than guessing, and check the exit code before "
        "believing the output. Reply with the answer itself: no preamble, no closing offer of "
        "further help.")

VARIANTS_TRIM = TRIM

VARIANTS = {
    # The rewritten preamble of #416, pinned: the one variant that is a wording the
    # tree has shipped rather than a text proposed for it.
    "v15": ("shipped-416", REWRITTEN),

    "v01": ("trim", TRIM),

    "v02": ("search-then-range", TRIM + " Read the fewest bytes that answer: search first "
            "(`rg -n`), then open only the lines you need (`sed -n 'A,Bp'`); never `cat` a whole "
            "file, and never run a search or a listing twice."),

    "v03": ("orient-once", TRIM + " Orient once: one command that lists and searches is enough. "
            "Never repeat a lookup you have already run."),

    "v04": ("minimal-patch", TRIM + " Change as few bytes as the change needs: read only the "
            "lines you will touch, patch them in place (a here-doc, `sed -i`, a short python "
            "script), and never rewrite a file to alter a few lines."),

    "v05": ("narrow-proof", TRIM + " Prove a change with the narrowest command that can fail — "
            "the one test, the one invocation — not the whole suite twice."),

    "v06": ("short-answer", TRIM + " Answer in the shortest form that carries it: no echoed file "
            "contents, and no code the user did not ask for."),

    "v07": ("one-script", TRIM + " Do several steps in one turn: a search, the read it implies "
            "and the edit it justifies belong in one script joined with `&&` and `;`, not in "
            "three requests."),

    "v08": ("trust-results", TRIM + " A result you have read is evidence: do not re-read a file, "
            "re-run a test or re-search a pattern to confirm what it already said."),

    "v09": ("compact-rewrite", "You are Sindri, answering a developer at a Unix terminal with one "
            "tool, `bash`. Search before you open (`rg -n`), open only the lines you need, change "
            "only the lines you must, and prove it with the narrowest command that can fail. Join "
            "the steps of one task into one script. Reply with the answer itself: no preamble."),

    "v14": ("final-batched", TRIM + " Read the fewest bytes that answer: search first "
            "(`rg -n`), then open only the lines you need (`sed -n 'A,Bp'`); never `cat` a whole "
            "file, and never run a search or a listing twice. Put the steps of one task in one "
            "script: filter there, print only what you concluded, and keep intermediates in "
            "`$TMPDIR`, which outlives the call. A fence marked `mermaid` is drawn as a picture: "
            "draw what prose cannot."),

    "v13": ("final", TRIM + " Read the fewest bytes that answer: search first (`rg -n`), then "
            "open only the lines you need (`sed -n 'A,Bp'`); never `cat` a whole file, and never "
            "run a search or a listing twice. A fence marked `mermaid` is drawn as a picture: draw "
            "what prose cannot."),

    "v11": ("combined", VARIANTS_TRIM + " Read the fewest bytes that answer: search first "
            "(`rg -n`), then open only the lines you need (`sed -n 'A,Bp'`); never `cat` a whole "
            "file, and never run a search or a listing twice. Change as few bytes as the change "
            "needs: patch the lines in place rather than rewriting a file, and prove it with the "
            "narrowest command that can fail."),

    "v12": ("combined-whole-names", VARIANTS_TRIM + " Read the fewest bytes that answer: search "
            "first (`rg -n`), then open only the lines you need (`sed -n 'A,Bp'`); never `cat` a "
            "whole file, and never run a search or a listing twice. Change as few bytes as the "
            "change needs: patch the lines in place rather than rewriting a file, match a name "
            "whole so a longer name starting with it is left alone, and prove the change with the "
            "narrowest command that can fail."),

    "v10": ("worked-examples", TRIM + " A read: not `cat src/ledger.py` but "
            "`rg -n 'def normalise_config' src/ledger.py`, then `sed -n '370,390p' src/ledger.py`. "
            "An edit: not a rewritten file, but the two lines that changed, patched in place, "
            "then the one test that covers them."),
}
