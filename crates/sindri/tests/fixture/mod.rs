use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::ledger::{self, Boot};
use sindri::mark;
use sindri::palette::Palette;
use sindri::pulse::Pulse;
use sindri::rows::Scroll;
use sindri::view::{Transcript, View};
use sindri_core::config::Config;
use sindri_core::open::Ground;
use sindri_core::policy::{
    Boundary, DEFAULT_COMMAND_BUDGET_MS, DEFAULT_OUTPUT_BUDGET_BYTES, DEFAULT_TIMEOUT_SECONDS,
    Enforcement,
};
use sindri_core::provider::Route;
use sindri_core::provider::endpoint::Endpoint;
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential, Deltas};
use sindri_core::records::{Dials, Entry, SessionDocument, Usage};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Budget, DEFAULT_MAX_TURNS, Exit, Limits, Loaded, State as RunState, Stop};
use sindri_core::session::Session;
use sindri_core::wire::Wire;

pub const GUID: &str = "9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61";

/// The recorded files a fixture's view is built from: the picture corpus under `pictures`.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

const LOADED_SKILLS: [&str; 2] = ["bash", "records"];

/// The workspace the record names: a name and no directory of this machine, because the frame
/// draws the whole of it and a golden may not be a picture of the box that blessed it.
const WORKSPACE: &str = "/home/glemsom/Documents/git/sindri/.worktrees/46-rail";

const CONTEXT_WINDOW: u64 = 200_000;

const MEASURED: Usage = Usage {
    input: Some(48_200),
    cached_input: Some(42_416),
    output: None,
    total: None,
    reasoning: None,
};

const SERVED: &str =
    include_str!("../../../sindri-core/tests/fixtures/providers/anthropic_models.json");

pub fn assert_frame(name: &str, cols: u16, rows: u16) {
    let fixture = format!("{name}-{cols}x{rows}");
    let mut view = named(name, Capabilities::default());
    let frame = crate::frame::painted(&mut view, cols, rows);
    let drawn = frame.text(name);

    // `make frame F=…` only changes what is printed, never what is compared, so a
    // stray variable in a shell cannot make a test report a picture it did not check.
    if std::env::var("SINDRI_FRAME").as_deref() == Ok(fixture.as_str()) {
        println!("{drawn}");
    }

    // The cell before the line: `insta`'s diff of a frame says which rows moved and never
    // which cell is wrong.
    if let Some(picture) = crate::frame::accepted(&fixture)
        && let Some(differed) = crate::frame::first_difference(&picture, &drawn)
    {
        println!("frame {fixture}: {differed}");
    }

    crate::frame::assert_margin(&frame, &fixture);
    frame.snapshot(name, &fixture);
    frame.assert_caret(&fixture);

    // The line `make check` ends on, written on a line of its own: libtest has left the
    // test's name unterminated on the terminal while this test ran.
    println!("\ncheck: frame {fixture} ... ok");
}

fn run_facts() -> Facts {
    Facts::default().asked().railed(facts())
}

fn typing(capabilities: Capabilities, lines: &[&str]) -> View {
    drafting(capabilities, Facts::default().asked(), lines)
}

fn drafting(capabilities: Capabilities, facts: Facts, lines: &[&str]) -> View {
    let mut view = View::of(capabilities, facts, Transcript::none());
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            view.key(Key::Newline);
        }
        view.paste(line);
    }
    view
}

fn named(name: &str, capabilities: Capabilities) -> View {
    match name {
        "boot" => splash(false),
        "boot-face" => splash(true),
        "boot-face-small" => splash(true),
        "transcript" => running(capabilities, transcript(), Scroll::Tail),
        "gutter" => View::of(
            capabilities,
            Facts::default().asked(),
            Transcript::of(gutter()),
        ),
        "transcript-scrolled" => running(capabilities, transcript(), Scroll::Back(4)),
        "thumb-head" => thumbing(capabilities, Scroll::Back(44)),
        "thumb-middle" => thumbing(capabilities, Scroll::Back(22)),
        "thumb-tail" => thumbing(capabilities, Scroll::Tail),
        "rail" => live(capabilities, opening(), Scroll::Tail),
        "face" => faced(true),
        "face-off" => faced(false),
        "streaming" => streaming(capabilities),
        "calls" => pressed(
            running(capabilities, calls(), Scroll::Tail),
            Key::Fold,
            120,
            40,
            "the four calls the newest settled exchange made are opened, so a frame draws each \
             verdict the work holds",
        ),
        "tools" => tools(capabilities),
        "fold" => fold(capabilities, false),
        "fold-open" => fold(capabilities, true),
        "prompt-fold" => prompt_fold(capabilities, false),
        "prompt-fold-open" => prompt_fold(capabilities, true),
        "fold-answer" => folded_beside_an_answer(capabilities),
        "empty" => View::of(capabilities, Facts::default().asked(), Transcript::none()),
        "edge" => typing(capabilities, &EDGE_DRAFT),
        "edge-live" => drafting(capabilities, Facts::default().running(), &EDGE_DRAFT),
        "typing" => typing(capabilities, &["why does the retry test fail"]),
        "wrapped" => typing(
            capabilities,
            &[
                "the draft is set inside the band's box, and a line of it wider than the band wraps \
             under the gutter — 日本語 keeps both its columns — while a line the reader broke is \
             one row of its own.",
            ],
        ),
        "grown" => typing(
            capabilities,
            &[
                "read the transcript, then the failing test,",
                "then the patch, and say which line of the",
                "patch the test disagrees with. Keep it to",
                "one sentence and quote the line you mean,",
                "the way the report does.",
            ],
        ),
        "prompt-flat" => flat_prompt(),
        "scrolled" => typing(
            capabilities,
            &[
                "one: the test that fails",
                "two: the line it fails on",
                "three: the patch that moved it",
                "four: the retry that hid it",
                "five: the budget it spent",
                "six: the stop reason",
                "seven: the session it left",
                "eight: the question to ask",
            ],
        ),
        "commands" => typing(capabilities, &["/"]),
        "commands-narrow" => typing(capabilities, &["/s"]),
        "commands-none" => typing(
            capabilities,
            &["/usr/local/bin is not on the path the run is handed"],
        ),
        "markdown-headings" => construct(capabilities, headings()),
        "markdown-headings-large" => sized(headings()),
        "markdown-lists" => construct(capabilities, lists()),
        "markdown-tasks" => construct(capabilities, tasks()),
        "markdown-table" => construct(capabilities, table()),
        "markdown-table-aligned" => construct(capabilities, aligned()),
        "markdown-table-unaligned" => construct(capabilities, &unaligned()),
        "markdown-table-aligned-narrow" => construct(capabilities, aligned()),
        "markdown-quote" => construct(capabilities, quoted()),
        "markdown-alert" => construct(capabilities, alerts()),
        "markdown-alert-marker" => construct(capabilities, marker_alerts()),
        "markdown-code" => construct(capabilities, code()),
        "markdown-fence-nested" => construct(capabilities, nested()),
        "markdown-fence-unknown" => construct(capabilities, unknown()),
        "markdown-diff" => construct(capabilities, diff()),
        "markdown-fence-cut" => construct(capabilities, cut()),
        "markdown-fence-fits" => construct(capabilities, fits()),
        "markdown-fence-named" => construct(capabilities, named_file()),
        "markdown-fence-named-cut" => construct(capabilities, named_cut()),
        "markdown-fence-walked" => {
            let mut view = construct(capabilities, cut());
            view.drag(mark::Cell::new(1, 0), mark::Cell::new(3, 70));
            let mut view = drawn(view, 80, 24);
            view.key(Key::ShiftRight);
            view
        }
        "markdown-diff-quoted" => construct(capabilities, &diff_quoted()),
        "markdown-inline" => construct(capabilities, inline()),
        "markdown-emphasis" => construct(capabilities, emphasised()),
        "markdown-struck" => construct(capabilities, struck()),
        "markdown-link" => ways(capabilities),
        "markdown-table-narrow" => construct(capabilities, narrow()),
        "markdown-rule" => construct(capabilities, ruled()),
        "markdown-footnote" => construct(capabilities, footnotes()),
        "markdown-footnote-undefined" => construct(capabilities, unfooted()),
        "markdown-break" => construct(capabilities, breaks()),
        "pulse-live" => with_pulse(capabilities, Pulse::live(4_200), &PULSE_ANSWER[..3]),
        "pulse-settled" => with_pulse(
            capabilities,
            Pulse::settled(&Stop::Final, 9_400),
            &PULSE_ANSWER,
        ),
        "stops" => running(capabilities, stops(), Scroll::Tail),
        "stop" => stopped_on(capabilities, wire_refused()),
        "refused" => refused_run(capabilities),
        "queue" => queue(capabilities),
        "queue-long" => queue_long(capabilities),
        "draft-long" => draft_long(capabilities),
        "keys" => {
            let mut view = View::of(capabilities, Facts::default().asked(), Transcript::none());
            view.key(Key::Char('?'));
            view
        }
        "card" => over_a_run(capabilities),
        "files" => picking(capabilities, ""),
        "files-narrow" => picking(capabilities, "rs"),
        "files-few" => picking(capabilities, "adr"),
        "files-none" => picking(capabilities, "zzz"),
        "models" => model_screen(capabilities),
        "settings" => {
            let mut view = View::of(capabilities, Facts::default().asked(), Transcript::none());
            view.open_settings(&in_force());
            view
        }
        "settings-refused" => refused(capabilities),
        "stats" => stats(capabilities),
        "footer" => View::of(capabilities, Facts::default().asked(), Transcript::none()),
        "footer-live" => asked(capabilities),
        "footer-queue" => waiting(asked(capabilities), DURING_A_RUN),
        "picture" => pictured(),
        "mark" => {
            let mut view = View::of(
                capabilities,
                Facts::default().asked(),
                Transcript::of(transcript()),
            );
            view.drag(mark::Cell::new(20, 0), mark::Cell::new(23, 70));
            view
        }
        unknown => panic!(
            "no frame is named {unknown}: `named` lists the frames this front end \
             paints, and a fixture under any other name is a picture of nothing"
        ),
    }
}

fn pictured() -> View {
    let workspace = PathBuf::from(FIXTURES).join("pictures");

    View::of(
        crate::caps::pictured(),
        Facts::default().asked().working(workspace, None),
        Transcript::of([answer(
            1,
            "![a diagram outside the workspace](../outside-the-workspace.svg)\n\n![a tall field]\
                 (tall.svg)",
        )])
        .scrolled(Scroll::Back(70)),
    )
}

fn faced(face: bool) -> View {
    View::of(
        crate::caps::pictured().faced(face),
        run_facts(),
        Transcript::of(opening()),
    )
}

fn splash(draws: bool) -> View {
    View::of(
        match draws {
            true => crate::caps::pictured(),
            false => Capabilities::default(),
        },
        Facts::boot(
            Boot {
                wire: Some(Wire::Anthropic),
                model: Some(String::from("claude-sonnet-4-5")),
                workspace: PathBuf::from("/srv/workspace"),
                branch: Some(String::from("main")),
                enforcement: Enforcement::Bubblewrap,
            },
            None,
        ),
        Transcript::none(),
    )
}

fn over_a_run(capabilities: Capabilities) -> View {
    let mut view = running(capabilities, transcript(), Scroll::Tail);
    view.key(Key::Char('?'));
    view
}

fn model_screen(capabilities: Capabilities) -> View {
    let replay = Replay::new([Canned::reply(200, SERVED)]);
    let at = Endpoint::own(Wire::Anthropic);
    let listing = Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at);
    let mut view = View::of(capabilities, Facts::default().asked(), Transcript::none());
    view.paste("/model");
    view.key(Key::Enter);
    view.handed(view.facts().clone().served_by(listing));
    view
}

fn construct(capabilities: Capabilities, text: &str) -> View {
    View::of(
        capabilities,
        Facts::default().asked(),
        Transcript::of([answer(1, text)]),
    )
}

fn sized(text: &str) -> View {
    View::of(
        crate::caps::kitty(),
        Facts::default().asked(),
        Transcript::of([answer(1, text)]),
    )
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

fn tasks() -> &'static str {
    "- [ ] a task waiting on a decision\n- [x] a task that is done\n- [X] one written with a \
     capital\n- an ordinary bullet among them\n- [ ] a task with one under it\n  - [x] the \
     nested task\n- [ ] a task whose words are long enough that this row cannot hold them all \
     and the rest hangs under them\n1. a numbered item among them\n2. [x] a numbered task"
}

fn table() -> &'static str {
    "| construct | rendering |\n| --- | --- |\n| heading | bold, one blank row \
     above |\n| table | columns joined by a bar, a rule under the header |\n| code | a \
     bar down its left and the language's tag |"
}

fn aligned() -> &'static str {
    "| construct | rendering | count |\n| :--- | :---: | ---: |\n| heading | bold, one blank row \
     above | 3 |\n| table | columns joined by a bar, a rule | 12 |\n| code | a bar down its \
     left | 128 |"
}

fn unaligned() -> String {
    aligned().replace("| :--- | :---: | ---: |", "| --- | --- | --- |")
}

fn ruled() -> &'static str {
    "A rule divides one paragraph from the next.\n\n---\n\nAnd the answer carries on under \
     it.\n\n> a quote keeps its bar over the rule it holds\n>\n> ---\n>\n> and goes on behind \
     the bar\n\n- an item hangs its rule under its own gutter\n\n  ---\n\n  and goes on under \
     the bullet"
}

fn footnotes() -> &'static str {
    "A reference[^note] draws a dim marker where the brackets were, numbered by the order the \
     answer read it in[^second].\n\n> a quote keeps its bar[^note] and the marker of its \
     reference\n\n| written | where |\n| --- | --- |\n| a cell[^cell] | gathers too |\n\n\
     [^note]: The one definition the first and the third reference share, drawn once at the foot \
     however many markers stand in its place.\n\n[^second]: A definition gathered in the order \
     this reference was read.\n\n[^cell]: A definition a cell gathered, drawn at the foot of the \
     answer and never inside the table that named it."
}

fn unfooted() -> &'static str {
    "A reference[^gone] whose definition never arrived is drawn as the words it was written with, \
     and one[^here] whose definition came draws the marker of its number.\n\n[^here]: The one \
     definition this answer arrived with, drawn under its marker at the answer's foot.\n\n\
     [^unused]: A definition no reference names, which no foot of this answer holds."
}

fn breaks() -> &'static str {
    "- an item's words break here<br>and carry on under its gutter\n\nThe words before a \
     break<br>and the words after it are two rows of one paragraph.\n\nThe tag <span>around these \
     words</span> draws no cell of its own, and neither does <kbd>this</kbd> one.\n\n> a quote \
     keeps its bar<br>over the words a break puts under it\n\n| row | words |\n| --- | --- |\n| \
     one | above<br>below |"
}

fn quoted() -> &'static str {
    "> ## A heading that keeps the bar\n>\n> A fence still open while streaming renders as a \
     code block, so a half-written diagram never costs a raster pass and never flickers while \
     it is being typed.\n>\n> - ### an item's heading, two columns further in"
}

fn alerts() -> &'static str {
    "> [!NOTE]\n> a note is the quietest thing a quote can say\n\n\
     > [!TIP]\n> a tip is advice the reader can take\n\n\
     > [!IMPORTANT]\n> what is important is said in the accent\n\n\
     > [!WARNING]\n> a warning is said in the ink the prose is\n\n\
     > [!CAUTION]\n> and a caution is the frame's own alarm"
}

fn marker_alerts() -> &'static str {
    "> [!tip]\n> a lowercase marker names the kind a capitalised one does\n\n\
     > [!WaRnInG]\n> and a mixed one names no third kind\n\n\
     > [!GREETING]\n> a marker no kind answers to is the quote's own words\n\n\
     - an item holding an alert\n  > [!CAUTION]\n  > its words hang under the item's gutter"
}

fn code() -> &'static str {
    "```rust\n#[derive(Debug)]\nstruct Fence {\n    tag: &'static str,\n    words: usize,\n}\
     \n\nimpl Fence {\n    fn open(tag: &'static str) -> Fence {\n        \
     // the language names the parse\n        let words = 2;\n        \
     Fence { tag, words }\n    }\n}\n\nfn rust() -> Fence {\n    Fence::open(\"rust\")\n}\n```"
}

fn nested() -> &'static str {
    concat!(
        "```rust\nfn open() {\n let one = 1;\n    let four = 4;\n",
        "        let eight = 8;\n            let twelve = 12;\n}\n```"
    )
}

fn unknown() -> &'static str {
    "```frobnicate\n// a comment stays the glyphs it was written as\nlet x = 1; # not rust\n\
     \"a string\" is a word too\n```"
}

fn cut() -> &'static str {
    "```rust\nlet a_fence_line_that_is_wider_than_the_band_it_is_drawn_in_is_kept_in_the_shape_\
     the_model_wrote_it_in = 1;\n```"
}

fn fits() -> &'static str {
    "```rust\nlet a_fence_line_that_fits = 1;\n```"
}

fn named_file() -> &'static str {
    concat!(
        "```rust crates/sindri/src/rows.rs\n",
        "let rows = markdown::rows(text, cols);\n",
        "```"
    )
}

fn named_cut() -> &'static str {
    concat!(
        "```rust crates/sindri/src/provider/transport/hauling/the/whole/of/it.rs\n",
        "let rows = markdown::rows(text, cols);\n",
        "```"
    )
}

fn diff() -> &'static str {
    concat!(
        "```diff\n",
        "--- a/crates/sindri/src/rows.rs\n",
        "+++ b/crates/sindri/src/rows.rs\n",
        "@@ -12,7 +12,9 @@ impl Row {\n",
        "     pub fn spans(&self) -> &[(String, Look)] {\n",
        "-        &self.spans\n",
        "+        self.spans.as_slice()\n",
        "     }\n",
        "```"
    )
}

fn diff_quoted() -> String {
    diff()
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
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

fn struck() -> &'static str {
    "A ~~struck~~ word is crossed out in the ink the prose around it gave it; a **~~bold~~** \
     one and an *~~italic~~* one are crossed in the faces their marks gave them.\n\n## A heading \
     keeps its level: ~~struck~~ and ***~~both~~*** inside it\n\n> a quote keeps its bar and its \
     ink, ~~struck~~ word and all"
}

fn ways(capabilities: Capabilities) -> View {
    View::of(
        capabilities,
        Facts::default().asked().grounded(Some(ground())),
        Transcript::of([
            answer(
                1,
                "The command is `/export` [in the spec](https://example.test/spec#12), and \
                     [§16](§16) is where the gate is written down.",
            ),
            call(2, "open", r#"{"path": "GLOSSARY.md"}"#),
            call(3, "open", r#"{"path": "site.html"}"#),
        ]),
    )
}

fn tools(capabilities: Capabilities) -> View {
    View::of(
        capabilities,
        Facts::default().asked().grounded(Some(ground())).running(),
        Transcript::of(vec![
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
        ]),
    )
}

fn tree() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the tree the product is built in")
}

fn boundary(config: &Config) -> Boundary {
    Boundary::from_config(config, tree(), Enforcement::Bubblewrap).expect("the boundary holds")
}

fn ground() -> Ground {
    Ground::new(&boundary(&Config::default()), session().temp(), tree())
        .expect("the ground resolves")
}

fn narrow() -> &'static str {
    "| a heading of the first level is bold and stands under one blank row wherever it \
     stands | a table wider than the band it is drawn in wraps inside its cells and keeps \
     the rule under its header |\n| --- | --- |\n| a code block | is guttered by its \
     language's tag and highlighted by syntect |"
}

fn picking(capabilities: Capabilities, query: &str) -> View {
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

    let mut view = View::of(
        capabilities,
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
    );
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

fn refused(capabilities: Capabilities) -> View {
    let mut view = View::of(capabilities, Facts::default(), Transcript::none());
    view.open_settings(&in_force());
    for _ in 0..Config::SCHEMA.len() {
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

fn stats(capabilities: Capabilities) -> View {
    let root = tempfile::tempdir().expect("a temp workspace");
    for name in LOADED_SKILLS {
        let directory = root.path().join(".agents/skills").join(name);
        std::fs::create_dir_all(&directory).expect("the skill's directory");
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: the {name} skill\n---\n\nBody.\n"),
        )
        .expect("a SKILL.md");
    }

    let mut view = View::of(
        capabilities,
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None)
            .railed(ledger::Facts {
                loaded: Loaded::default(),
                ..facts()
            })
            .recorded(&document()),
        Transcript::none(),
    );
    for name in LOADED_SKILLS {
        view.paste(&format!("/skill:{name}"));
        assert_eq!(
            view.key(Key::Enter),
            Outcome::Edited,
            "a skill the catalog offers is one the frame loads"
        );
    }
    view.paste("/stats");
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "a `/stats` line opens the screen"
    );
    view
}

pub fn document() -> SessionDocument {
    document_of(&Config::default())
}

fn document_of(config: &Config) -> SessionDocument {
    let boundary = boundary(config);
    SessionDocument {
        session: String::from(GUID),
        wire: Wire::Anthropic,
        model: String::from("claude-sonnet-4-5-20250929"),
        enforcement: boundary.enforcement(),
        policy: boundary.record_policy(),
        workspace: PathBuf::from(WORKSPACE),
        branch: Some(String::from("spec/1-capability")),
        commit: Some(String::from("16becb5")),
        dials: Dials::of(&Limits::from_config(config, &boundary)),
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

fn thought(turn: u32) -> Entry {
    Entry::Reasoning {
        turn,
        text: SecretSet::default().redact(format!("turn {turn}")),
    }
}

fn thumbing(capabilities: Capabilities, scroll: Scroll) -> View {
    View::of(
        capabilities,
        Facts::default().asked(),
        Transcript::of((1..=80).map(thought)).scrolled(scroll),
    )
}

fn running(capabilities: Capabilities, entries: Vec<Entry>, scroll: Scroll) -> View {
    View::of(
        capabilities,
        run_facts(),
        Transcript::of(entries).scrolled(scroll),
    )
}

fn live(capabilities: Capabilities, entries: Vec<Entry>, scroll: Scroll) -> View {
    View::of(
        capabilities,
        run_facts()
            .railed(ledger::Facts {
                state: Some(RunState::Running),
                ..facts()
            })
            .pulsed(Some(Pulse::live(4_200)))
            .running(),
        Transcript::of(entries).scrolled(scroll),
    )
}

pub fn session() -> &'static Session {
    static SESSION: OnceLock<Session> = OnceLock::new();
    SESSION.get_or_init(|| {
        let home = std::env::temp_dir().join("sindri-frames");
        Session::named(&home, GUID).expect("a session the frames can name")
    })
}

pub fn railed(document: &SessionDocument) -> ledger::Facts {
    ledger::Facts::of(session(), document)
        .recorded(document)
        .loading(LOADED_SKILLS.map(String::from).into_iter().collect())
        .measured(&MEASURED, Some(CONTEXT_WINDOW))
}

pub fn facts() -> ledger::Facts {
    ledger::Facts {
        state: None,
        ..railed(&document())
    }
}

fn opening() -> Vec<Entry> {
    transcript().into_iter().take(5).collect()
}

fn transcript() -> Vec<Entry> {
    vec![
        prompt("where does rendering live?"),
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
        end(),
        prompt("how does a width change reflow the cache?"),
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
        end(),
        prompt("how is a window measured?"),
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
        end(),
        prompt("what does the check cost?"),
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
        end(),
        prompt("what is the transcript's unit?"),
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
        end(),
        prompt("what does a row of the band say?"),
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
        end(),
    ]
}

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

fn streaming(capabilities: Capabilities) -> View {
    let mut entries = transcript();
    entries.push(Entry::Turn { turn: 6 });
    let mut view = View::of(capabilities, run_facts().running(), Transcript::of(entries));
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

const EDGE_DRAFT: [&str; 1] = ["the band's edge says what the band is for"];

const PULSE_ANSWER: [&str; 4] = [
    "A braille marker at the head of the newest answer carries the run's elapsed ",
    "time, one step every 80 ms, and settles on the verdict it stopped with — ",
    "the duration it stopped at, kept and never recomputed. ",
    "The rows under it keep the answer's own gutter, held open and blank.",
];

fn with_pulse(capabilities: Capabilities, pulse: Pulse, said: &[&str]) -> View {
    let (state, ended) = match &pulse {
        Pulse::Live { .. } => (RunState::Running, None),
        Pulse::Settled { stop, .. } => (RunState::Stopped(stop.clone()), Some(stop.clone())),
    };
    let mut entries = opening();
    entries.push(Entry::Turn { turn: 2 });
    let running = state == RunState::Running;
    let rail = ledger::Facts {
        state: Some(state),
        ..facts()
    };
    let facts = Facts::default().asked().railed(rail).pulsed(Some(pulse));
    let mut view = View::of(
        capabilities,
        match running {
            true => facts.running(),
            false => facts,
        },
        Transcript::of(entries),
    );
    for fragment in [
        "The marker is a clock, so the row it heads is laid out again whenever ",
        "the mark it wears moves, and a step that changes nothing costs nothing. ",
    ] {
        view.reasoning(fragment);
    }
    for fragment in said {
        view.text(fragment);
    }
    if let Some(stop) = ended {
        view.stopped(&stop);
    }
    view
}

fn wire_refused() -> Stop {
    Stop::Error(SecretSet::default().redact("the wire refused the request: 401 unauthorized"))
}

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

fn stopped_on(capabilities: Capabilities, stop: Stop) -> View {
    let mut view = with_pulse(capabilities, Pulse::settled(&stop, 1_240), &STOP_ANSWER);
    view.stopped(&stop);
    view
}

fn flat_prompt() -> View {
    let mut view = View::of(
        crate::caps::inked(Palette::SLOTS),
        Facts::default().asked(),
        Transcript::of(vec![
            prompt(
                "triage the retry test, and say which line of the patch the test disagrees with: \
                 the answer is one sentence, and it quotes the line the test fails on rather than \
                 the line the patch moved, because a reader reads the failure and never the diff.",
            ),
            Entry::Turn { turn: 1 },
            call(1, "bash", r#"{"command": "rg -n 'retry' src"}"#),
            result(
                1,
                "bash",
                Exit::Code(0),
                "src/retry.rs: the test disagrees on the third line",
                412,
                23,
            ),
            answer(
                1,
                "The third line of the patch: the retry the test waits on never lands, because \
                 the budget it counts is the turn's rather than the call's.",
            ),
            end(),
        ]),
    );
    view.paste("why does the retry test fail");
    view
}

fn refused_run(capabilities: Capabilities) -> View {
    let mut view = running(capabilities, transcript(), Scroll::Tail);
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

fn queue(capabilities: Capabilities) -> View {
    let view = View::of(
        capabilities,
        run_facts().running(),
        Transcript::of(transcript()),
    );
    let mut view = waiting(view, DURING_A_RUN);
    view.paste("and this one while they wait");
    view
}

const SEVEN_QUEUED: &[&str] = &[
    "run the test that failed and read it",
    "then the fmt check",
    "and say why the retry hid it",
    "then the frame the queue is drawn in",
    "then the row that counts it",
    "then the caret that stays inside it",
    "and then the report",
];

fn queue_long(capabilities: Capabilities) -> View {
    let view = View::of(
        capabilities,
        run_facts().running(),
        Transcript::of(transcript()),
    );
    let mut view = waiting(view, SEVEN_QUEUED);
    view.paste("and this one while they wait");
    view
}

fn draft_long(capabilities: Capabilities) -> View {
    typing(
        capabilities,
        &[
            "one: the test that failed",
            "two: the line it failed on",
            "three: the patch that moved it",
            "four: the retry that hid it",
            "five: the budget it spent",
            "six: the stop reason",
            "seven: the session it left",
            "eight: the question to ask",
            "nine: the row it keeps back",
        ],
    )
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

fn asked(capabilities: Capabilities) -> View {
    View::of(
        capabilities,
        Facts::default().running().pulsed(Some(Pulse::live(0))),
        Transcript::none(),
    )
}

fn output() -> String {
    (1..=20)
        .map(|line| format!("test rows::a_row_of_output_{line} ... ok"))
        .collect::<Vec<String>>()
        .join("\n")
}

fn fold(capabilities: Capabilities, open: bool) -> View {
    let mut view = running(capabilities, folded_run(), Scroll::Tail);
    if open {
        assert_eq!(
            view.key(Key::Fold),
            Outcome::Edited,
            "Ctrl-O opens the newest settled work, and a frame that answered nothing would be a \
             picture of the row it already had"
        );
    }
    view
}

fn folded_run() -> Vec<Entry> {
    vec![
        prompt("why does the retry test fail?"),
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
        answer(
            2,
            "Three rows of the fold are the whole of it: the head, the count and the last.",
        ),
        end(),
    ]
}

fn prompt_fold(capabilities: Capabilities, open: bool) -> View {
    let view = running(capabilities, prompt_fold_run(), Scroll::Tail);
    match open {
        true => pressed(
            view,
            Key::Fold,
            120,
            40,
            "Ctrl-O opens the newest fold a reader may move, which is this prompt's where no work \
             stands after it",
        ),
        false => view,
    }
}

fn pressed(mut view: View, key: Key, cols: u16, rows: u16, why: &str) -> View {
    let area = Rect::new(0, 0, cols, rows);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);
    assert_eq!(view.key(key), Outcome::Edited, "{why}");
    view
}

const FOLDED_PROMPT: &str = "the retry test fails on the line the patch moved, and only there\
\nsay which line the patch moved, and which row of the output disagrees\
\nthe report names the file it read and the line it stopped on\
\ngive me the test name and the assertion it failed on\
\nthen say whether the retry that hid it was the one the run recorded\
\nkeep every line one sentence long, so the transcript stays readable\
\nno need to paste the whole output, the head of it is enough\
\nand end with the one thing you would change first";

fn prompt_fold_run() -> Vec<Entry> {
    vec![
        prompt("why does the retry test fail?"),
        Entry::Turn { turn: 1 },
        call(
            1,
            "bash",
            r#"{"command": "cargo test -p sindri --test rows"}"#,
        ),
        result(
            1,
            "bash",
            Exit::Code(0),
            "test rows::a_row_of_output_1 ... ok\ntest result: ok. 4 passed; 0 failed",
            208,
            97,
        ),
        answer(
            1,
            "The retry test fails on the line the patch moved, and the output under it says which.",
        ),
        end(),
        prompt(FOLDED_PROMPT),
    ]
}

fn folded_beside_an_answer(capabilities: Capabilities) -> View {
    let mut entries = folded_run();
    entries.push(Entry::Turn { turn: 3 });
    entries.push(answer(
        3,
        "The run proves the fold and nothing else: every row of a command's output is drawn in the \
         quieter ink behind the gutter the call opened, and the rows a reader usually needs are the \
         head of it, the row counting the rows kept back and the last row the command wrote.\n\n\
         An answer is the answer, so it is drawn whole in the ink it wears today, and a reader \
         reads it beside a folded result without losing one to the other.\n\n\
         The fold opens on one key and on a mark grown over a row of it, and a fold a reader \
         opened stays open through a scroll, a resize and the entry that lands beside it.",
    ));
    let mut view = running(capabilities, entries, Scroll::Tail);
    assert_eq!(
        view.key(Key::Fold),
        Outcome::Edited,
        "the work the folded result stands in is opened, because a work drawn as one row draws no \
         result to read beside the answer"
    );
    view
}

fn calls() -> Vec<Entry> {
    vec![
        prompt("which calls settle well?"),
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
        prompt("and the one that is still out?"),
        Entry::Turn { turn: 5 },
        call(5, "bash", r#"{"command": "make check STAGES=test"}"#),
    ]
}

fn call(turn: u32, name: &str, arguments: &str) -> Entry {
    Entry::Call {
        turn,
        id: None,
        name: SecretSet::default().redact(name),
        arguments: SecretSet::default().redact(arguments),
    }
}

fn result(turn: u32, name: &str, exit: Exit, text: &str, bytes: u64, elapsed_ms: u64) -> Entry {
    Entry::Result {
        turn,
        id: None,
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

fn prompt(text: &str) -> Entry {
    Entry::Prompt {
        text: SecretSet::default().redact(text),
    }
}

fn end() -> Entry {
    Entry::Stop { stop: Stop::Final }
}

pub fn assert_pointer_draws_the_same_cells(name: &str, cols: u16, rows: u16) {
    let fixture = format!("{name}-{cols}x{rows}");
    let mut pointing = named(name, Capabilities::default().reporting(true));
    let taken = crate::frame::painted(&mut pointing, cols, rows);
    taken.assert_caret(&fixture);
    let mut blind = named(name, Capabilities::default().reporting(false));
    let none = crate::frame::painted(&mut blind, cols, rows);
    none.assert_caret(&fixture);

    assert_eq!(
        taken.text(name),
        none.text(name),
        "a pointer is never a cell: the frame {fixture} drawn where the terminal reports one is \
         the frame drawn where it reports none"
    );
    println!("\ncheck: frame {fixture} pointer ... ok");
}

fn drawn(mut view: View, cols: u16, rows: u16) -> View {
    crate::frame::painted(&mut view, cols, rows);
    view
}

#[test]
fn a_frame_whose_cards_query_row_and_placed_caret_disagree_is_no_fixture() {
    let mut view = named("files", Capabilities::default());
    let frame = crate::frame::painted(&mut view, 120, 40);
    frame.assert_caret("files-120x40");

    let drawn = frame
        .query_row()
        .expect("the card draws the row its query is typed at");
    let (column, row) = drawn;
    assert_eq!(
        frame.told().caret(),
        Some(Position::new(column, row)),
        "a frame whose cells draw a query row places its cursor one column past the glyphs typed \\
         at it"
    );
    assert!(
        crate::frame::carets_agree(Some(drawn), Some(Position::new(column + 1, row))).is_err(),
        "a pass that placed the cursor elsewhere than the query row is drawn is no fixture"
    );
    assert!(
        crate::frame::carets_agree(Some(drawn), None).is_err(),
        "and a query row the pass placed no cursor in at all is no fixture either"
    );
    let mut view = named("keys", Capabilities::default());
    let keys = crate::frame::painted(&mut view, 120, 40);
    assert_eq!(
        keys.query_row(),
        None,
        "a card that takes no glyph draws no query row for a caret to wait in"
    );
}

#[test]
fn the_rails_facts_come_off_the_fixture_session_and_the_document_it_recorded() {
    let document = document();
    let facts = railed(&document);

    assert_eq!(
        facts.session.as_deref(),
        Some(GUID),
        "the session the rail draws is the one the fixture's own session was named by"
    );
    assert_eq!(
        facts.temp.as_deref(),
        Some(session().temp()),
        "and its temp is that session's, not a path written down here"
    );
    assert_eq!(
        facts.workspace.as_deref(),
        Some(document.workspace.as_path()),
        "the workspace is the tree the document recorded the run standing in"
    );
    assert_eq!(
        facts.loaded.names(),
        LOADED_SKILLS,
        "the skills are the ones the run loaded, in the order it loaded them"
    );
    assert_eq!(
        facts.ctx.map(|ctx| (ctx.size, ctx.window)),
        Some((48_200, Some(CONTEXT_WINDOW))),
        "the size is the newest ask's, measured against the window the fixture names"
    );
    assert_eq!(
        facts.cache,
        Some(88),
        "and the cache beside it is that ask's hit rate, not the share of the summed spend"
    );
}

#[test]
fn a_fact_the_document_changes_reaches_the_rail_with_no_edit_to_the_fixture() {
    let changed = SessionDocument {
        wire: Wire::Openai,
        model: String::from("claude-opus-4-1"),
        branch: Some(String::from("spec/2-the-document")),
        turns: 31,
        thinking: false,
        enforcement: Enforcement::Unsafe,
        ..document()
    };
    let facts = railed(&changed);

    assert_eq!(
        facts.wire.as_deref(),
        Some("openai"),
        "the wire the rail draws is the one the document records"
    );
    assert_eq!(
        facts.model.as_deref(),
        Some("claude-opus-4-1"),
        "and its model is the served name the document names"
    );
    assert_eq!(
        facts.branch.as_deref(),
        Some("spec/2-the-document"),
        "a branch the document records is the one drawn beside the workspace"
    );
    assert_eq!(
        facts.turns,
        Some((31, 250)),
        "the turns follow the document's count against the cap it dialled"
    );
    assert_eq!(
        facts.think.as_deref(),
        Some("off"),
        "a document that records no thinking is a rail that draws none"
    );
    assert_eq!(
        facts.sandbox,
        Some(Enforcement::Unsafe),
        "and the enforcement follows the one the document says held"
    );
}

#[test]
fn the_rail_frames_draw_no_run_state_and_the_fixture_that_draws_one_says_so() {
    assert_eq!(
        railed(&document()).state,
        Some(RunState::Stopped(Stop::Final)),
        "the facts derived from the record carry the stop the document recorded"
    );
    assert_eq!(
        facts().state,
        None,
        "and a frame that draws no state row states none, leaving that row to the fixtures that \
         say a run is live or has stopped"
    );
}

#[test]
fn the_facts_a_frame_draws_name_the_fixture_session_and_the_temp_it_stands_on() {
    let facts = facts();
    let session = session();

    assert_eq!(
        facts.session.as_deref(),
        Some(GUID),
        "the session a frame draws is the one the fixture names"
    );
    assert_eq!(
        facts.temp.as_deref(),
        Some(session.temp()),
        "and its temp is that session's own, not a second session's"
    );
    assert!(
        session.temp().ends_with(format!("sessions/{GUID}/tmp")),
        "the temp is named by the GUID and by nothing drawn from urandom: {}",
        session.temp().display()
    );
}

#[test]
fn the_record_keeps_the_boundary_the_tree_is_bound_by() {
    let document = document();
    let tree = std::fs::canonicalize(tree()).expect("the tree resolves");

    assert_eq!(
        document.policy.writable,
        vec![tree.display().to_string()],
        "the writable set the record keeps is the boundary's, over the tree the run stands in"
    );
    assert_eq!(
        document.enforcement,
        Enforcement::Bubblewrap,
        "and the enforcement it keeps is the one that boundary holds"
    );
    assert_eq!(
        document.policy.timeout_seconds, DEFAULT_TIMEOUT_SECONDS,
        "the timeout is production's default, which no literal here can drift from"
    );
    assert_eq!(
        document.policy.command_budget_ms, DEFAULT_COMMAND_BUDGET_MS,
        "so is the command budget"
    );
    assert_eq!(
        document.policy.output_budget_bytes, DEFAULT_OUTPUT_BUDGET_BYTES,
        "and the output budget"
    );
    assert_eq!(
        document.dials.max_turns, DEFAULT_MAX_TURNS,
        "and the turn cap the record dials is the one a run with no cap set is bound by"
    );
}

#[test]
fn the_ground_the_frames_judge_by_is_that_boundary_and_the_session_temp() {
    let tree = std::fs::canonicalize(tree()).expect("the tree resolves");
    let temp = std::fs::canonicalize(session().temp()).expect("the temp resolves");
    let mut places = vec![tree, temp];
    places.sort();

    assert_eq!(
        ground().places(),
        places,
        "a frame judges a path by the writable set the boundary holds and the temp the run writes in"
    );
}

#[test]
fn a_config_that_binds_a_run_differently_moves_what_the_fixture_draws() {
    let changed = Config {
        timeout: Some(7),
        max_turns: Some(9),
        ..Config::default()
    };
    let document = document_of(&changed);

    assert_eq!(
        document.policy.timeout_seconds, 7,
        "a timeout the config sets is the one the record keeps"
    );
    assert_eq!(
        railed(&document).turns,
        Some((6, 9)),
        "and the turns row of the rail draws the run's count against the cap the config set"
    );
    assert_eq!(
        document.turns, 6,
        "while the count the record holds is the run's own, untouched by the cap"
    );
}
