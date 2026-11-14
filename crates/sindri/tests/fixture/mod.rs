//! A fixture line is one row of the screen: its number, its glyphs between two bars, and the look of
//! each run of cells from the left edge, so a row of blanks keeps its width in a file an editor may
//! touch. The files are `.snap` because `insta` compares and blesses them and takes no other extension.

use std::path::{Path, PathBuf};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use sindri::composer::{Composer, Key, Outcome, State};
use sindri::graphics::CellSize;
use sindri::ink::{Faces, Ink, Look};
use sindri::ledger::{Boot, Ctx, Facts};
use sindri::mark;
use sindri::palette::Palette;
use sindri::pen::{self, Clip};
use sindri::pictures::{PLACEHOLDER, Pictures};
use sindri::pulse::Pulse;
use sindri::rows::Scroll;
use sindri::view::View;
use sindri_core::config::Config;
use sindri_core::open::Ground;
use sindri_core::policy::{Boundary, Enforcement, PolicyRecord};
use sindri_core::provider::Route;
use sindri_core::provider::served::Where;
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential, Deltas};
use sindri_core::records::{Dials, Entry, SessionDocument, Usage};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Budget, Exit, State as RunState, Stop};
use sindri_core::wire::Wire;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

pub const GUID: &str = "9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61";

/// A recorded Anthropic `GET /v1/models` listing, so the `/model` screen is painted from names a
/// wire really served rather than from a list written down here.
const SERVED: &str =
    include_str!("../../../sindri-core/tests/fixtures/providers/anthropic_models.json");

pub fn assert_frame(name: &str, cols: u16, rows: u16) {
    let fixture = format!("{name}-{cols}x{rows}");
    let drawn = drawn(name, cols, rows);

    // `make frame F=…` only changes what is printed, never what is compared, so a
    // stray variable in a shell cannot make a test report a picture it did not check.
    if std::env::var("SINDRI_FRAME").as_deref() == Ok(fixture.as_str()) {
        println!("{drawn}");
    }

    // The cell before the line: `insta`'s diff of a frame says which rows moved and never
    // which cell is wrong.
    if let Some(picture) = accepted(&fixture)
        && let Some(differed) = first_difference(&picture, &drawn)
    {
        println!("frame {fixture}: {differed}");
    }

    insta::with_settings!({ snapshot_path => FIXTURES, prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!(fixture.as_str(), drawn);
    });

    // The line `make check` ends on, written on a line of its own: libtest has left the
    // test's name unterminated on the terminal while this test ran.
    println!("\ncheck: frame {fixture} ... ok");
}

fn named(name: &str) -> View {
    match name {
        "boot" => door(false),
        "boot-face" => door(true),
        "boot-face-small" => door(true),
        "transcript" => running(transcript(), Scroll::Tail),
        "gutter" => View::of(gutter(), Scroll::Tail),
        "transcript-scrolled" => running(transcript(), Scroll::Back(4)),
        "thumb-head" => thumbing(Scroll::Back(44)),
        "thumb-middle" => thumbing(Scroll::Back(22)),
        "thumb-tail" => thumbing(Scroll::Tail),
        "rail" => live(opening(), Scroll::Tail),
        "face" => faced(true),
        "face-off" => faced(false),
        "streaming" => streaming(),
        "calls" => running(calls(), Scroll::Tail),
        "tools" => tools(),
        "fold" => fold(false),
        "fold-open" => fold(true),
        "fold-answer" => folded_beside_an_answer(),
        "empty" => View::boot(),
        "typing" => View::typing(draft(&["why does the retry test fail"])),
        "wrapped" => View::typing(draft(&[
            "the draft is set where an answer is set, and a line of it wider than the band wraps \
             under the gutter — 日本語 keeps both its columns — while a line the reader broke is \
             one row of its own.",
        ])),
        "grown" => View::typing(draft(&[
            "read the transcript, then the failing test,",
            "then the patch, and say which line of the",
            "patch the test disagrees with. Keep it to",
            "one sentence and quote the line you mean,",
            "the way the report does.",
        ])),
        "scrolled" => View::typing(draft(&[
            "one: the test that fails",
            "two: the line it fails on",
            "three: the patch that moved it",
            "four: the retry that hid it",
            "five: the budget it spent",
            "six: the stop reason",
            "seven: the session it left",
            "eight: the question to ask",
        ])),
        "commands" => View::typing(draft(&["/"])),
        "commands-narrow" => View::typing(draft(&["/s"])),
        "commands-none" => View::typing(draft(&[
            "/usr/local/bin is not on the path the run is handed",
        ])),
        "markdown-headings" => construct(headings()),
        "markdown-headings-large" => sized(headings()),
        "markdown-lists" => construct(lists()),
        "markdown-table" => construct(table()),
        "markdown-quote" => construct(quoted()),
        "markdown-code" => construct(code()),
        "markdown-inline" => construct(inline()),
        "markdown-emphasis" => construct(emphasised()),
        "markdown-link" => ways(),
        "markdown-table-narrow" => construct(narrow()),
        // The same answer in both, three of its four fragments under the live marker and all of it
        // under the settled one; nine point four seconds is under ten, so that marker keeps its decimal.
        "pulse-live" => with_pulse(Pulse::live(4_200), &PULSE_ANSWER[..3]),
        "pulse-settled" => with_pulse(Pulse::settled(&Stop::Final, 9_400), &PULSE_ANSWER),
        "stops" => running(stops(), Scroll::Tail),
        "stop" => stopped_on(wire_refused()),
        "refused" => refused_run(),
        "queue" => queue(),
        "queue-long" => queue_long(),
        "draft-long" => draft_long(),
        "keys" => {
            let mut view = View::boot();
            view.key(Key::Char('?'));
            view
        }
        "card" => over_a_run(),
        "files" => picking(""),
        "files-narrow" => picking("rs"),
        "files-few" => picking("adr"),
        "files-none" => picking("zzz"),
        "models" => model_screen(),
        "settings" => {
            let mut view = View::boot();
            view.open_settings(&in_force());
            view
        }
        "settings-refused" => refused(),
        "stats" => stats(),
        "footer" => View::boot(),
        "footer-live" => asked(),
        "footer-queue" => waiting(asked(), DURING_A_RUN),
        "picture" => pictured(),
        "mark" => {
            let mut view = View::of(transcript(), Scroll::Tail);
            view.drag(mark::Cell::new(20, 0), mark::Cell::new(23, 70));
            view
        }
        unknown => panic!(
            "no frame is named {unknown}: `named` lists the frames this front end \
             paints, and a fixture under any other name is a picture of nothing"
        ),
    }
}

/// The frame a picture block is drawn on, held at the top of the answer: the block is taller than the
/// band, so it is never placed and its words stand alone. The refused source is a real diagram one
/// directory above the workspace, so the caption is the refusal this product gives.
fn pictured() -> View {
    let workspace = PathBuf::from(FIXTURES).join("pictures");

    let mut view = View::of(
        vec![answer(
            1,
            "![a diagram outside the workspace](../outside-the-workspace.svg)\n\n![a tall field]\
             (tall.svg)",
        )],
        Scroll::Back(70),
    );
    view.set_workspace(workspace);
    view.set_cell_size(CellSize::new(10, 20));
    view.set_pictures(Pictures::kitty(CellSize::new(10, 20)));
    view
}

/// A frame whose terminal places pictures, with the rail's facts where the face is drawn: the
/// picture over the rail's top rows with the switch on, and the facts from the rail's first row
/// with it off.
fn faced(face: bool) -> View {
    let mut view = running(opening(), Scroll::Tail);
    view.set_cell_size(CellSize::new(10, 20));
    view.set_pictures(Pictures::kitty(CellSize::new(10, 20)));
    view.set_face(face);
    view
}

/// The frame a launch opens on: the config's wire and model, the workspace and its branch, and the
/// enforcement the policy holds — a session and a temp are a run's own, so the rail draws an em dash
/// for each. A terminal that places pictures is handed Sindri's face over the wordmark.
fn door(draws: bool) -> View {
    let mut view = View::boot();
    view.door(Boot {
        wire: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-5")),
        workspace: PathBuf::from("/srv/workspace"),
        branch: Some(String::from("main")),
        enforcement: Enforcement::Bubblewrap,
    });
    if draws {
        view.set_cell_size(CellSize::new(10, 20));
        view.set_pictures(Pictures::kitty(CellSize::new(10, 20)));
    }
    view
}

/// A screen opened over a run, so the card's frame behind it is a transcript a reader was reading.
fn over_a_run() -> View {
    let mut view = running(transcript(), Scroll::Tail);
    view.key(Key::Char('?'));
    view
}

fn model_screen() -> View {
    let replay = Replay::new([Canned::reply(200, SERVED)]);
    let at = Where::own(Wire::Anthropic);
    let listing = Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at);
    let mut view = View::boot();
    view.paste("/model");
    view.key(Key::Enter);
    view.set_served(listing);
    view
}

fn construct(text: &str) -> View {
    View::of(vec![answer(1, text)], Scroll::Tail)
}

/// The same answer on a terminal that draws text at more than one cell: the two highest headings are
/// laid out as the cells the sizing leaves and measured by them.
fn sized(text: &str) -> View {
    let mut view = construct(text);
    view.set_sizing(true);
    view
}

fn headings() -> &'static str {
    "# Markdown rendering\n\n## The scale of headings, two ruled and four not\n\n### A third \
     level, muted and bold\n\n#### A fourth, muted and bare\n\n##### A fifth, dim and \
     bold\n\n###### A sixth, dim and both"
}

fn lists() -> &'static str {
    "- a bullet per item\n- two spaces per depth\n  - a nested item\n    - and one \
     deeper\n- an ordered list underneath\n  1. numbered from one\n  2. and hung under \
     their own words when they wrap"
}

fn table() -> &'static str {
    "| construct | rendering |\n| --- | --- |\n| heading | bold, one blank row \
     above |\n| table | columns joined by a bar, a rule under the header |\n| code | a \
     bar down its left and the language's tag |"
}

fn quoted() -> &'static str {
    "> ## A heading that keeps the bar\n>\n> A fence still open while streaming renders as a \
     code block, so a half-written diagram never costs a raster pass and never flickers while \
     it is being typed.\n>\n> - ### an item's heading, two columns further in"
}

fn code() -> &'static str {
    "```rust\nfn main() {\n    let fence = Fence::open();\n    fence.words();\n}\n```\n\n\
     ```frobnicate\nthis language names no syntax definition\n```"
}

fn inline() -> &'static str {
    "Run `make check` before the gate: it is the definition of done, and `make frame \
     F=boot-120x40` is how a look is eyeballed before it is blessed."
}

fn emphasised() -> &'static str {
    "A strong word is **bold**, an emphasis is *italic*, and one word can be ***both*** at \
     once.\n\n## A heading keeps its level: **bold**, *italic* and ***both*** inside it\n\n> a quote \
     keeps its ink and its gutter, *even here*\n\n```\n**bold** and *italic* stay glyphs in a \
     fence\n```"
}

/// An answer's ways and the paths a call names: a link is its words with the destination travelling
/// beside the row, a link written over its own URL says it once, and a path a call names is a way to
/// the file the run judged openable — the one it did not is drawn as the words it was written as.
fn ways() -> View {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the tree the product is built in");
    let mut view = View::of(
        vec![
            answer(
                1,
                "The command is `/export` [in the spec](https://example.test/spec#12), and \
                 [§16](§16) is where the gate is written down.",
            ),
            call(2, "open", r#"{"path": "GLOSSARY.md"}"#),
            call(3, "open", r#"{"path": "site.html"}"#),
        ],
        Scroll::Tail,
    );
    view.set_ground(Some(ground(workspace)));
    view
}

/// One call of each tool this product has: the settled three beside the result each one wrote, and
/// then a turn whose three calls are still in flight — the row says the thing the call names, and a
/// result says the verdict, the duration and the size and never the name the call row holds.
fn tools() -> View {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the tree the product is built in");
    let mut view = View::of(
        vec![
            Entry::Turn { turn: 1 },
            call(
                1,
                "bash",
                r#"{"command": "rg -n 'the gutter' crates/sindri/src"}"#,
            ),
            result(
                1,
                "bash",
                Exit::Code(0),
                "src/rows.rs: the glyph a turn opens with",
                412,
                23,
            ),
            call(1, "open", r#"{"path": "GLOSSARY.md"}"#),
            result(1, "open", Exit::Code(0), "", 0, 4),
            call(1, "skill", r#"{"name": "rust-review"}"#),
            result(
                1,
                "skill",
                Exit::Code(0),
                "the rust-review skill is loaded",
                33,
                2,
            ),
            Entry::Turn { turn: 2 },
            call(2, "bash", r#"{"command": "cargo test --workspace"}"#),
            call(2, "open", r#"{"path": "docs/agents/domain.md"}"#),
            call(2, "skill", r#"{"name": "code-review"}"#),
        ],
        Scroll::Tail,
    );
    view.set_ground(Some(ground(workspace)));
    view.set_state(State::Running);
    view
}

/// The ground a run standing in `workspace` judges a target from.
fn ground(workspace: &std::path::Path) -> Ground {
    let boundary = Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
        .expect("the boundary holds");
    Ground::new(&boundary, workspace, workspace).expect("the ground resolves")
}

fn narrow() -> &'static str {
    "| a heading of the first level is bold and stands under one blank row wherever it \
     stands | a table wider than the band it is drawn in wraps inside its cells and keeps \
     the rule under its header |\n| --- | --- |\n| a code block | is guttered by its \
     language's tag and highlighted by syntect |"
}

/// A workspace of the test's own, opened through the `@` key and narrowed by `query`: every row is
/// a path relative to the temp directory, so nothing of a temp directory's name is frozen into a
/// fixture.
fn picking(query: &str) -> View {
    let root = tempfile::tempdir().expect("a temp workspace");
    // `no_hardcoded_models` reads two numbered paths in one bracket as a hand-written model list,
    // so each record is named here and offered by name.
    let first = "docs/adr/0020-the-artefact-is-one-static-musl-binary.md";
    let second = "docs/adr/0021-a-key-is-answered-by-the-loop-that-owns-it.md";
    for file in [
        ".gitignore",
        "Cargo.toml",
        "GLOSSARY.md",
        "Makefile",
        first,
        second,
        "docs/agents/domain.md",
        "crates/sindri-core/src/policy.rs",
        "crates/sindri-core/src/records.rs",
        "crates/sindri-core/src/run.rs",
        "crates/sindri/src/card.rs",
        "crates/sindri/src/keys.rs",
        "crates/sindri/src/pen.rs",
        "crates/sindri/src/rail.rs",
        "crates/sindri/src/rows.rs",
        "crates/sindri/src/view.rs",
        "crates/sindri/tests/frames.rs",
        "notes.local.md",
        "target/debug/sindri",
    ] {
        let path = root.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the directories a file sits in");
        }
        std::fs::write(&path, "").expect("a file");
    }
    std::fs::write(root.path().join(".gitignore"), "target/\n*.local.md\n").expect("a gitignore");

    let mut view = View::boot();
    view.set_workspace(root.path().to_path_buf());
    view.key(Key::Char('@'));
    for glyph in query.chars() {
        view.key(Key::Char(glyph));
    }
    view
}

fn in_force() -> Config {
    Config {
        provider: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-5")),
        thinking: Some(true),
        extra_writable_paths: Some(vec![PathBuf::from("/srv/data")]),
        context_window: Some(200_000),
        max_turns: Some(250),
        timeout: Some(120),
        api_key: Some(String::from("sk-not-a-real-key")),
        face: Some(true),
        ..Config::default()
    }
}

fn refused() -> View {
    let mut view = View::boot();
    view.open_settings(&in_force());
    for _ in 0..Config::KEYS.len() {
        let row = view
            .settings()
            .rows()
            .nth(view.settings().current())
            .map(|(key, _)| key);
        if row == Some("max_turns") {
            break;
        }
        view.key(Key::Down);
    }
    view.key(Key::Clear);
    for glyph in "two hundred".chars() {
        view.key(Key::Char(glyph));
    }
    let Outcome::Save(touched) = view.key(Key::Enter) else {
        panic!("a value typed into a row is a visit Enter hands to the save");
    };
    let path = tempfile::tempdir()
        .expect("a temp dir")
        .path()
        .join("config.toml");
    let error = Config::save_touched_to(&path, &touched).expect_err("a turn cap is a number");
    view.refuse(&error.to_string());
    view
}

fn stats() -> View {
    let root = tempfile::tempdir().expect("a temp workspace");
    for name in ["bash", "records"] {
        let directory = root.path().join(".agents/skills").join(name);
        std::fs::create_dir_all(&directory).expect("the skill's directory");
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: the {name} skill\n---\n\nBody.\n"),
        )
        .expect("a SKILL.md");
    }

    let mut view = View::boot();
    view.set_workspace(root.path().to_path_buf());
    view.set_home(None);
    for name in ["bash", "records"] {
        view.paste(&format!("/skill:{name}"));
        assert_eq!(
            view.key(Key::Enter),
            Outcome::Edited,
            "a skill the catalog offers is one the frame loads"
        );
    }
    view.set_facts(facts());
    view.set_record(&document());
    view.paste("/stats");
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "a `/stats` line opens the screen"
    );
    view
}

pub fn document() -> SessionDocument {
    SessionDocument {
        session: String::from(GUID),
        wire: Wire::Anthropic,
        model: String::from("claude-sonnet-4-5-20250929"),
        enforcement: Enforcement::Bubblewrap,
        policy: PolicyRecord {
            enforcement: Enforcement::Bubblewrap,
            writable: vec![String::from(
                "/home/glemsom/Documents/git/sindri/.worktrees/46-rail",
            )],
            timeout_seconds: 120,
            command_budget_ms: 3_600_000,
            output_budget_bytes: 8 * 1024 * 1024,
            description: String::from("the boundary the model was told about"),
        },
        workspace: PathBuf::from("/home/glemsom/Documents/git/sindri/.worktrees/46-rail"),
        branch: Some(String::from("spec/1-capability")),
        commit: Some(String::from("16becb5")),
        dials: Dials {
            max_turns: 250,
            command_budget_ms: 3_600_000,
            output_budget_bytes: 8 * 1024 * 1024,
        },
        start: String::from("2026-01-01T00:00:00Z"),
        end: Some(String::from("2026-01-01T00:04:12Z")),
        turns: 6,
        usage: Usage {
            input: Some(48_200),
            output: Some(3_100),
            total: Some(51_300),
            cached_input: Some(12_000),
            reasoning: Some(900),
        },
        stopped: RunState::Stopped(Stop::Final),
        route: Some(Route::Chat),
        thinking: true,
    }
}

/// A transcript of eighty turn dividers, one row each, so the rows of the band the thumb covers are
/// counted off the frame.
fn thumbing(scroll: Scroll) -> View {
    View::of((1..=80).map(|turn| Entry::Turn { turn }), scroll)
}

fn running(entries: Vec<Entry>, scroll: Scroll) -> View {
    let mut view = View::of(entries, scroll);
    view.set_facts(facts());
    view
}

/// A live run's rail: the facts a run in flight holds and the marker its newest answer wears, so
/// the rail's own row steps with the frame's marker.
fn live(entries: Vec<Entry>, scroll: Scroll) -> View {
    let mut view = running(entries, scroll);
    view.set_facts(Facts {
        state: Some(RunState::Running),
        ..facts()
    });
    view.set_pulse(Some(Pulse::live(4_200)));
    view.set_state(State::Running);
    view
}

pub fn facts() -> Facts {
    Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet-4-5-20250929")),
        think: Some(String::from("high")),
        turns: Some((6, 250)),
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        cache: Some(88),
        workspace: Some(PathBuf::from(
            "/home/glemsom/Documents/git/sindri/.worktrees/46-rail",
        )),
        branch: Some(String::from("spec/1-capability")),
        loaded: vec![String::from("bash"), String::from("records")],
        sandbox: Some(Enforcement::Bubblewrap),
        session: Some(String::from(GUID)),
        temp: Some(PathBuf::from(
            "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
        )),
        ..Facts::default()
    }
}

fn opening() -> Vec<Entry> {
    transcript().into_iter().take(4).collect()
}

fn transcript() -> Vec<Entry> {
    vec![
        Entry::Turn { turn: 1 },
        call(1, "bash", r#"{"command": "rg -n 'fence' src"}"#),
        result(
            1,
            "bash",
            Exit::Code(0),
            "src/rows.rs: the glyph a turn opens with\nsrc/rows.rs: a word longer than a row is broken",
            412,
            23,
        ),
        answer(
            1,
            "Rendering lives in the front end and not in the core, because an entry is a \
             fact and never a rendering: the TUI and any reader draw one run from one \
             value, and the core may not name a terminal at all.",
        ),
        Entry::Turn { turn: 2 },
        call(
            2,
            "bash",
            r#"{"command": "cargo test -p sindri --test rows"}"#,
        ),
        result(
            2,
            "bash",
            Exit::Code(0),
            "running 4 tests\ntest result: ok. 4 passed; 0 failed",
            208,
            97,
        ),
        answer(
            2,
            "A width change drops what the cache held and reflows on the next paint, once, \
             because a paint lays an entry out to draw it and then keeps the answer. The \
             cache is one width deep, which is what a resize being a reflow and not a \
             lookup means.",
        ),
        Entry::Turn { turn: 3 },
        call(3, "bash", r#"{"command": "make check STAGES=rows"}"#),
        result(3, "bash", Exit::Code(0), "check: test ... ok", 64, 41),
        answer(
            3,
            "The window is measured from the tail backwards, so following the newest row \
             never asks how tall the history is: it lays out the entries it shows and \
             stops at the first row that fills the band. Only the rows a band reaches are \
             laid out, so a 250-turn history on a 40-row terminal pays for what it shows \
             and no more.",
        ),
        Entry::Turn { turn: 4 },
        call(4, "bash", r#"{"command": "make check"}"#),
        result(
            4,
            "bash",
            Exit::Code(0),
            "check: fmt ... ok\ncheck: clippy ... ok\ncheck: test ... ok\ncheck: frames ... ok",
            96,
            118,
        ),
        answer(
            4,
            "What that buys is the plain case and what it costs is the scrolled one: a \
             window is a row offset from the tail, so resolving one walks the entries \
             between the tail and the window and lays out every one of them on the way. \
             Each is laid out once and kept, so the whole history scrolled through costs \
             one layout per entry and scrolling it again costs none.",
        ),
        Entry::Turn { turn: 5 },
        call(5, "bash", r#"{"command": "git diff --stat"}"#),
        result(
            5,
            "bash",
            Exit::Code(0),
            "crates/sindri/src/rows.rs | 412 +++++++++++++\ncrates/sindri/tests/rows.rs | 198 +++++++++",
            128,
            9,
        ),
        answer(
            5,
            "The transcript's unit is the entry. An entry never carries a formatted line, \
             so the same run is drawn the same way by the TUI, by a reader of the \
             transcript and by whatever front end comes next.\n\n| construct | where it \
             stands |\n| --- | --- |\n| heading | in the answer's own gutter |\n| code | \
             behind a bar, highlighted |\n| table | columns joined by a bar |",
        ),
        Entry::Turn { turn: 6 },
        call(6, "bash", r#"{"command": "make check STAGES=frames"}"#),
        result(
            6,
            "bash",
            Exit::Code(0),
            "check: frame calls ... ok\ncheck: frame transcript ... ok",
            104,
            31,
        ),
        answer(
            6,
            "## A row is one line\n\nA row says the exit code and not the command, because \
             the verdict is a fact the record holds and the command is a fact the row \
             above it holds.\n\n```rust\nfn main() {\n    let row = Row::of(spans);\n    \
             row.paint(area, buffer);\n}\n```",
        ),
    ]
}

/// One of every kind of entry a run writes, one above the other, so a frame says where each of them
/// begins its words.
fn gutter() -> Vec<Entry> {
    vec![
        Entry::Turn { turn: 1 },
        Entry::Prompt {
            text: SecretSet::default().redact("triage the retry test"),
        },
        call(1, "bash", r#"{"command": "rg -n 'gutter' src"}"#),
        result(
            1,
            "bash",
            Exit::Code(0),
            "src/rows.rs: every entry stands in one gutter",
            412,
            23,
        ),
        Entry::Reasoning {
            turn: 2,
            text: SecretSet::default()
                .redact("the marker of a row stands in the gutter ahead of its words"),
        },
        answer(
            2,
            "Every row begins at one column:\n```rust\nfn gutter() {\n    let row = Row::of(spans);\n}\n```\nand the words follow the marker.",
        ),
        call(2, "bash", r#"{"command": "make check STAGES=rows"}"#),
    ]
}

fn streaming() -> View {
    let mut entries = transcript();
    entries.push(Entry::Turn { turn: 6 });
    let mut view = running(entries, Scroll::Tail);
    view.set_state(State::Running);
    for fragment in [
        "The ticket asks whether the tail survives. ",
        "A fence still streaming is a code block, ",
        "so its words are drawn as code and nothing is swallowed.",
    ] {
        view.reasoning(fragment);
    }
    for fragment in [
        "It survives: ",
        "the rule is one boolean over the lines, ",
        "and #52 keeps it.\n",
        "```rust\n",
        "fn main() {\n",
        "    let fence = Fence::open();\n",
        "    let  tail = fence.words();\n",
    ] {
        view.text(fragment);
    }
    view
}

const PULSE_ANSWER: [&str; 4] = [
    "A braille marker at the head of the newest answer carries the run's elapsed ",
    "time, one step every 80 ms, and settles on the verdict it stopped with — ",
    "the duration it stopped at, kept and never recomputed. ",
    "The rows under it keep the answer's own gutter, held open and blank.",
];

fn with_pulse(pulse: Pulse, said: &[&str]) -> View {
    let state = match &pulse {
        Pulse::Live { .. } => RunState::Running,
        Pulse::Settled { stop, .. } => RunState::Stopped(stop.clone()),
    };
    let mut entries = opening();
    entries.push(Entry::Turn { turn: 2 });
    let mut view = running(entries, Scroll::Tail);
    for fragment in [
        "The marker is a clock, so the row it heads is laid out again whenever ",
        "the mark it wears moves, and a step that changes nothing costs nothing. ",
    ] {
        view.reasoning(fragment);
    }
    for fragment in said {
        view.text(fragment);
    }
    if state == RunState::Running {
        view.set_state(State::Running);
    }
    view.set_facts(Facts {
        state: Some(state),
        ..facts()
    });
    view.set_pulse(Some(pulse));
    view
}

/// The failure a run can end on, in the words a wire gives it: the one stop that carries a line.
fn wire_refused() -> Stop {
    Stop::Error(SecretSet::default().redact("the wire refused the request: 401 unauthorized"))
}

/// One entry of every stop a run can end on, one above the other, so the five words and the two
/// marks are read at once.
fn stops() -> Vec<Entry> {
    vec![
        Stop::Final,
        Stop::MaxTurns,
        Stop::Budget(Budget::Clock),
        Stop::Cancelled,
        wire_refused(),
    ]
    .into_iter()
    .map(|stop| Entry::Stop { stop })
    .collect()
}

const STOP_ANSWER: [&str; 4] = [
    "A run that stopped for a reason says the reason, and it says it twice: once in ",
    "the marker beside the answer it ended on, and once in the entry the record keeps ",
    "where the run ended. A failure carries one line of the run's own words about it, ",
    "and a final answer and a cancellation carry no line they do not have.",
];

/// The frame a failed run leaves: the marker settled on the answer it last said, and the stop entry
/// the record keeps under it.
fn stopped_on(stop: Stop) -> View {
    let mut view = with_pulse(Pulse::settled(&stop, 1_240), &STOP_ANSWER);
    view.stopped(&stop);
    view
}

/// The frame a run that could not start leaves: the entry a stop is, in the refusal's own words, and
/// the draft that asked for it back in the composer.
fn refused_run() -> View {
    let mut view = running(transcript(), Scroll::Tail);
    view.paste("why does the retry test fail");
    match view.key(Key::Enter) {
        Outcome::Send(prompt) => view.run_refused(
            "cannot create the sandbox: bubblewrap is on PATH\n  fix:   install bubblewrap, or run \
             with --yolo-unsafe to run unconfined on purpose",
            prompt,
        ),
        other => panic!("a draft should have been sent, and {other:?} came instead"),
    }
    view
}

const DURING_A_RUN: &[&str] = &[
    "run the test that failed and read it",
    "then the fmt check",
    "and say why the retry hid it",
];

fn queue() -> View {
    let mut view = running(transcript(), Scroll::Tail);
    view.set_state(State::Running);
    let mut view = waiting(view, DURING_A_RUN);
    view.paste("and this one while they wait");
    view
}

/// Seven prompts waiting behind a one-line draft, which is a queue taller than the band it is
/// drawn in.
const SEVEN_QUEUED: &[&str] = &[
    "run the test that failed and read it",
    "then the fmt check",
    "and say why the retry hid it",
    "then the frame the queue is drawn in",
    "then the row that counts it",
    "then the caret that stays inside it",
    "and then the report",
];

fn queue_long() -> View {
    let mut view = running(transcript(), Scroll::Tail);
    view.set_state(State::Running);
    let mut view = waiting(view, SEVEN_QUEUED);
    view.paste("and this one while they wait");
    view
}

/// A draft of nine lines, which is more than the composer's band draws.
fn draft_long() -> View {
    View::typing(draft(&[
        "one: the test that failed",
        "two: the line it failed on",
        "three: the patch that moved it",
        "four: the retry that hid it",
        "five: the budget it spent",
        "six: the stop reason",
        "seven: the session it left",
        "eight: the question to ask",
        "nine: the row it keeps back",
    ]))
}

fn waiting(mut view: View, prompts: &[&str]) -> View {
    for prompt in prompts {
        view.paste(prompt);
        match view.key(Key::Enter) {
            Outcome::Send(prompt) => view.enqueue(prompt),
            other => panic!("a draft should have been sent, and {other:?} came instead"),
        }
    }
    view
}

/// The frame a prompt just sent leaves: a run in flight and nothing said yet.
fn asked() -> View {
    let mut view = View::boot();
    view.settle();
    view.set_state(State::Running);
    view.set_pulse(Some(Pulse::live(0)));
    view
}

/// A `cargo test` run of twenty rows, which is a result longer than its fold.
fn output() -> String {
    (1..=20)
        .map(|line| format!("test rows::a_row_of_output_{line} ... ok"))
        .collect::<Vec<String>>()
        .join("\n")
}

/// A transcript whose newest result is longer than its fold, with the answer the run wrote above
/// it: the result is the work and folds, the answer is the reader's and never does.
fn fold(open: bool) -> View {
    let mut view = running(folded_run(), Scroll::Tail);
    if open {
        assert_eq!(
            view.key(Key::Fold),
            Outcome::Edited,
            "Ctrl-O opens the newest result's fold, and a frame that answered nothing would be a \
             picture of the fold it already had"
        );
    }
    view
}

/// The run a fold is drawn on, and the long answer under it that proves a fold is a result's and
/// never an answer's: one frame holds both kinds of entry beside one another.
fn folded_run() -> Vec<Entry> {
    vec![
        Entry::Turn { turn: 1 },
        answer(
            1,
            "Every row of the run passes: the gutter holds one column for every entry, and a row \
             of a command's output is set behind it rather than beside the answer.",
        ),
        call(
            1,
            "bash",
            r#"{"command": "cargo test -p sindri --test rows"}"#,
        ),
        result(1, "bash", Exit::Code(0), &output(), 640, 1_800),
    ]
}

fn folded_beside_an_answer() -> View {
    let mut entries = folded_run();
    entries.push(Entry::Turn { turn: 2 });
    entries.push(answer(
        2,
        "The run proves the fold and nothing else: every row of a command's output is drawn in the \
         quieter ink behind the gutter the call opened, and the rows a reader usually needs are the \
         head of it, the row counting the rows kept back and the last row the command wrote.\n\n\
         An answer is the answer, so it is drawn whole in the ink it wears today, and a reader \
         reads it beside a folded result without losing one to the other.\n\n\
         The fold opens on one key and on a mark grown over a row of it, and a fold a reader \
         opened stays open through a scroll, a resize and the entry that lands beside it.",
    ));
    running(entries, Scroll::Tail)
}

fn calls() -> Vec<Entry> {
    vec![
        Entry::Turn { turn: 1 },
        call(1, "bash", r#"{"command": "rg -n 'verdict' src"}"#),
        result(
            1,
            "bash",
            Exit::Code(0),
            "src/rows.rs: the check or the cross in `ok`/`err`",
            64,
            18,
        ),
        answer(1, "The row is drawn from the verdict the record holds."),
        Entry::Turn { turn: 2 },
        call(2, "bash", r#"{"command": "cargo clippy --all-targets"}"#),
        result(
            2,
            "bash",
            Exit::Code(1),
            "warning: unused import: `std::fmt`\n  --> src/rows.rs:12:5",
            96,
            211,
        ),
        answer(2, "An unused import, and nowhere else to put it."),
        Entry::Turn { turn: 3 },
        call(3, "bash", r#"{"command": "cargo test --workspace"}"#),
        result(3, "bash", Exit::TimedOut, "", 0, 120_000),
        answer(3, "The call ran past its minute and was killed."),
        Entry::Turn { turn: 4 },
        call(4, "bash", r#"{"command": "make check STAGES=frames"}"#),
        result(4, "bash", Exit::Cancelled, "", 0, 3_400),
        answer(
            4,
            "## What a verdict says\n\nThe run stopped on the user's word: `Ctrl-C` \
             cancels and keeps the draft, so the row says `✗ cancelled` rather than \
             leaving a spinner turning for a result that will never come.",
        ),
        Entry::Turn { turn: 5 },
        call(5, "bash", r#"{"command": "make check STAGES=test"}"#),
    ]
}

fn call(turn: u32, name: &str, arguments: &str) -> Entry {
    Entry::Call {
        turn,
        name: SecretSet::default().redact(name),
        arguments: SecretSet::default().redact(arguments),
    }
}

fn result(turn: u32, name: &str, exit: Exit, text: &str, bytes: u64, elapsed_ms: u64) -> Entry {
    Entry::Result {
        turn,
        name: SecretSet::default().redact(name),
        text: SecretSet::default().redact(text),
        bytes,
        elapsed_ms,
        exit: Some(exit),
    }
}

fn answer(turn: u32, text: &str) -> Entry {
    Entry::Assistant {
        turn,
        text: SecretSet::default().redact(text),
    }
}

/// A composer typed a line at a time through the same keys a reader's fingers send, so a picture
/// of the draft cannot be of a state the product cannot reach.
fn draft(lines: &[&str]) -> Composer {
    let mut composer = Composer::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            composer.key(Key::Newline);
        }
        composer.paste(line);
    }
    composer
}

fn drawn(name: &str, cols: u16, rows: u16) -> String {
    let mut view = named(name);
    let palette = view.palette();
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let completed = terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted");
    text(name, &palette, completed.buffer)
}

fn text(name: &str, palette: &Palette, buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut text = format!("{name} at {}x{}", area.width, area.height);
    for y in area.y..area.bottom() {
        let mut glyphs = String::new();
        let mut runs: Vec<(Look, u16)> = Vec::new();
        for x in area.x..area.right() {
            let cell = buffer
                .cell((x, y))
                .unwrap_or_else(|| panic!("row {y}, column {x} is outside the frame it is in"));
            let look = Look::of(cell, palette).unwrap_or_else(|| {
                panic!(
                    "row {y}, column {x} is drawn in a style that is not a look: \
                     foreground {:?}, background {:?}, modifiers {:?} — a fixture names \
                     looks, so a cell no look names cannot be written down",
                    cell.fg, cell.bg, cell.modifier
                )
            });
            glyphs.push(glyph(cell, x, y));
            match runs.last_mut() {
                Some((last, count)) if *last == look => *count += 1,
                _ => runs.push((look, 1)),
            }
        }
        let runs: Vec<String> = runs
            .iter()
            .map(|(look, count)| format!("{}:{count}", look.name()))
            .collect();
        text.push_str(&format!("\n{y:02} |{glyphs}| {}", runs.join(" ")));
    }
    text
}

/// The glyph one cell of the frame is written as. The cells a picture covers hold the protocol's
/// sequence and the placeholders behind it rather than a glyph, so they are written as the blank
/// of the band it is drawn on: a picture is not glyphs, and its rows are what a frame shows.
fn glyph(cell: &Cell, x: u16, y: u16) -> char {
    let symbol = cell.symbol();
    if cell.skip || symbol.contains(PLACEHOLDER) || symbol.contains('\u{1b}') {
        return ' ';
    }
    let mut characters = symbol.chars();
    match (characters.next(), characters.next()) {
        (Some(glyph), None) => glyph,
        _ => panic!(
            "row {y}, column {x} holds {symbol:?}, which is not one glyph: a fixture \
             counts one glyph per column, so a glyph wider than one column — and the \
             cell it ate — has nowhere to be written down"
        ),
    }
}

fn accepted(fixture: &str) -> Option<String> {
    let path = PathBuf::from(FIXTURES).join(format!("{fixture}.snap"));
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "---" {
        return Some(text);
    }
    let frame: Vec<&str> = lines.skip_while(|line| *line != "---").skip(1).collect();
    Some(frame.join("\n"))
}

fn first_difference(picture: &str, drawn: &str) -> Option<String> {
    let picture: Vec<&str> = picture.lines().collect();
    let drawn: Vec<&str> = drawn.lines().collect();

    for (line, (expected, actual)) in picture.iter().zip(&drawn).enumerate() {
        if expected == actual {
            continue;
        }
        return Some(match (row_of(expected), row_of(actual)) {
            (Some(expected), Some(actual)) => differing_cell(&expected, &actual),
            _ => format!(
                "line {} reads {expected:?} where the frame draws {actual:?}",
                line + 1
            ),
        });
    }

    (picture.len() != drawn.len()).then(|| {
        format!(
            "the fixture holds {} lines and the frame draws {}",
            picture.len(),
            drawn.len()
        )
    })
}

fn differing_cell(expected: &Row, actual: &Row) -> String {
    let columns = expected.glyphs.len().max(actual.glyphs.len());
    for column in 0..columns {
        let (held, drawn) = (cell(expected, column), cell(actual, column));
        if held != drawn {
            return format!(
                "row {}, column {column}: the fixture holds {held} and the frame draws {drawn}",
                expected.number
            );
        }
    }
    format!(
        "row {}: every cell of it agrees and its line does not, which this harness wrote wrong",
        expected.number
    )
}

fn cell(row: &Row, column: usize) -> String {
    match (row.glyphs.get(column), row.looks.get(column)) {
        (Some(glyph), Some(look)) => format!("{glyph:?} in {}", look.name()),
        _ => String::from("no cell at all"),
    }
}

struct Row {
    number: u16,
    glyphs: Vec<char>,
    looks: Vec<Look>,
}

fn row_of(line: &str) -> Option<Row> {
    let (number, rest) = line.split_once(' ')?;
    let (glyphs, runs) = rest.strip_prefix('|')?.rsplit_once('|')?;
    let looks = looks_in(runs.trim())?;
    let glyphs: Vec<char> = glyphs.chars().collect();
    (glyphs.len() == looks.len()).then_some(Row {
        number: number.parse().ok()?,
        glyphs,
        looks,
    })
}

fn looks_in(runs: &str) -> Option<Vec<Look>> {
    let mut looks = Vec::new();
    for run in runs.split_whitespace() {
        let (look, count) = run.split_once(':')?;
        let look = Look::from_name(look)?;
        let count: u16 = count.parse().ok()?;
        looks.extend(std::iter::repeat_n(look, usize::from(count)));
    }
    Some(looks)
}

#[test]
fn a_cell_wearing_two_faces_is_written_and_read_back_as_the_one_name_it_has() {
    let palette = Palette::TRUECOLOR;
    let emphasis = Look::new(Ink::Accent, Faces::BOLD.and(Faces::ITALIC));
    let linked = Look::new(Ink::Muted, Faces::ITALIC).underlined(Ink::Dim);
    let area = Rect::new(0, 0, 2, 1);
    let mut buffer = Buffer::empty(area);
    pen::write(
        &mut buffer,
        area,
        &[("a", emphasis), ("b", linked)],
        Clip::Whole,
        &palette,
    );

    let drawn = text("two-faces", &palette, &buffer);
    let written = drawn.lines().nth(1).expect("the one row of the frame");
    assert_eq!(
        written,
        format!(
            "00 |ab| {}:1 {}:1",
            "accent+bold+italic", "muted+italic~dim"
        ),
        "the faces a cell wears and the line under it are written in one order, so one frame has \
         one picture of it"
    );
    let row = row_of(written).expect("the row the fixture wrote is the row it reads back");
    assert_eq!(row.looks, [emphasis, linked]);
}
