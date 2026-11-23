use std::path::{Path, PathBuf};

use sindri_core::policy::{Enforcement, PolicyRecord};
use sindri_core::provider::Route;
use sindri_core::records::{Dials, SessionDocument, Usage};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Budget, State as RunState, Stop};
use sindri_core::session::Session;
use sindri_core::wire::Wire;

use super::*;
use crate::pen;
use crate::readback::{column_of, inks};
use crate::rows::margins;

const ROOM: usize = (FLOOR - FIELD) as usize;

const COLUMNS: u16 = FLOOR;

const SCREEN: u16 = 81;

#[test]
fn the_facts_a_launch_holds_before_a_run_are_what_a_prompt_is_pointed_at() {
    let facts = Facts::boot(boot());

    assert_eq!(
        (
            facts.wire.as_deref(),
            facts.model.as_deref(),
            facts.workspace.as_deref(),
            facts.branch.as_deref(),
            facts.sandbox,
        ),
        (
            Some("anthropic"),
            Some("claude-sonnet-4-5"),
            Some(Path::new("/srv/workspace")),
            Some("main"),
            Some(Enforcement::Bubblewrap),
        ),
        "the wire, the model, the workspace, the branch and the enforcement are the \
             launch's own facts"
    );
    assert_eq!(DASH, "—", "the mark a fact owed is drawn as is an em dash");
    assert_eq!(
        (facts.session.as_deref(), facts.temp.as_deref()),
        (Some(DASH), Some(Path::new(DASH))),
        "a session and a temp are a run's: neither exists yet, so each is an em dash and \
             never a guess"
    );
    assert_eq!(
        (
            facts.think,
            facts.turns,
            facts.ctx,
            facts.cache,
            facts.loaded.names().len()
        ),
        (None, None, None, None, 0),
        "a launch knows no thinking, no turn count, no size, no hit rate and no skill: they \
             arrive with a run, and no drawing guesses at one"
    );
}

#[test]
fn a_session_is_named_by_the_first_eight_of_its_guid_and_whole_when_it_is_shorter() {
    assert_eq!(
        short_id("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"),
        "9f2c7a41",
        "no rail column holds a GUID of thirty-six"
    );
    assert_eq!(
        short_id("9f2c"),
        "9f2c",
        "a name already short is left whole"
    );
    assert_eq!(short_id(""), "", "and no name at all is no columns of one");
}

#[test]
fn the_facts_a_session_holds_are_the_ones_a_frame_draws_it_by() {
    let root = tempfile::tempdir().expect("a temp state home");
    let session = Session::create(root.path()).expect("a session");
    let document = document(session.guid());

    let facts = Facts::of(&session, &document);

    assert_eq!(
        facts.wire.as_deref(),
        Some("anthropic"),
        "the dialect the document was written for, in the wire's own word"
    );
    assert_eq!(facts.model.as_deref(), Some("claude-sonnet"));
    assert_eq!(facts.turns, Some((3, 250)), "the turns opened, and the cap");
    assert_eq!(facts.sandbox, Some(Enforcement::Bubblewrap));
    assert_eq!(
        facts.workspace.as_deref(),
        Some(Path::new("/home/glemsom/src/sindri")),
        "the directory the run stood in"
    );
    assert_eq!(facts.branch.as_deref(), Some("spec/1-capability"));
    assert_eq!(
        facts.session.as_deref(),
        Some(session.guid()),
        "the session's own GUID, which the rail draws short"
    );
    assert_eq!(
        facts.temp.as_deref(),
        Some(session.temp()),
        "and the temp the sandbox maps to TMPDIR"
    );
    assert_eq!(
        (facts.ctx, facts.cache, facts.loaded.names().len()),
        (None, None, 0),
        "a document holds what the run spent summed over every turn, so the size of one \
             ask and the cache's hit rate are handed in and not read off it"
    );
}

#[test]
fn the_facts_a_record_wrote_join_the_facts_the_frame_was_handed() {
    let document = SessionDocument {
        route: Some(Route::Chat),
        usage: Usage {
            input: Some(48_200),
            output: Some(3_100),
            ..Usage::default()
        },
        end: Some(String::from("2026-01-01T00:04:00Z")),
        stopped: RunState::Stopped(Stop::Final),
        ..document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")
    };

    let facts = Facts::default()
        .recorded(&document)
        .loading([String::from("bash")].into_iter().collect());

    assert_eq!(
        (
            facts.route,
            facts.spend,
            facts.state.as_ref().map(RunState::word)
        ),
        (Some(Route::Chat), document.usage, Some("final")),
        "the route the run took, what it spent and the state it stopped in are a record's \
             own facts"
    );
    assert_eq!(
        (
            facts.commit.as_deref(),
            facts.start.as_deref(),
            facts.end.as_deref()
        ),
        (
            Some("16becb5"),
            Some("2026-01-01T00:00:00Z"),
            Some("2026-01-01T00:04:00Z")
        ),
        "and so are the commit it stood on and the instants it was written at"
    );
    assert_eq!(
        facts.loaded.names(),
        ["bash"],
        "and the names it loaded, whole"
    );
    assert!(
        facts.wire.is_none() && facts.model.is_none(),
        "a record is no source of the facts the frame was handed: it adds the ones it wrote"
    );
}

fn boot() -> Boot {
    Boot {
        wire: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-5")),
        workspace: PathBuf::from("/srv/workspace"),
        branch: Some(String::from("main")),
        enforcement: Enforcement::Bubblewrap,
    }
}

fn git(workspace: &Path, args: &[&str]) {
    let ran = std::process::Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "sindri")
        .env("GIT_AUTHOR_EMAIL", "sindri@example.invalid")
        .env("GIT_COMMITTER_NAME", "sindri")
        .env("GIT_COMMITTER_EMAIL", "sindri@example.invalid")
        .status()
        .expect("git should run");
    assert!(ran.success(), "git {args:?} failed");
}

#[test]
fn the_boot_facts_of_a_config_are_the_run_a_prompt_will_ask_for() {
    let root = tempfile::tempdir().expect("a temp workspace");
    git(root.path(), &["init", "--quiet", "-b", "the-branch"]);
    std::fs::write(root.path().join("a.txt"), "one file\n").expect("a file");
    git(root.path(), &["add", "a.txt"]);
    git(root.path(), &["commit", "--quiet", "-m", "one commit"]);
    let config = Config {
        provider: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-5")),
        ..Config::default()
    };

    let boot = Boot::of(&config, Enforcement::Bubblewrap, root.path());

    assert_eq!(
        boot.wire,
        Some(Wire::Anthropic),
        "the dialect the config names"
    );
    assert_eq!(boot.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(boot.workspace, root.path());
    assert_eq!(
        boot.branch.as_deref(),
        Some("the-branch"),
        "read off the repository, which a boot frame asks before any session exists"
    );
    assert_eq!(boot.enforcement, Enforcement::Bubblewrap);
}

#[test]
fn a_config_that_names_no_run_and_a_workspace_that_is_no_checkout_give_no_fact_at_all() {
    let root = tempfile::tempdir().expect("a temp workspace");
    let config = Config {
        provider: None,
        model: None,
        ..Config::default()
    };

    let boot = Boot::of(&config, Enforcement::Bubblewrap, root.path());

    assert_eq!(
        (boot.wire, boot.model, boot.branch),
        (None, None, None),
        "the splash screen names nothing it cannot know, and a directory that is no checkout \
             has no branch to name"
    );
}

#[test]
fn the_size_and_the_cache_a_turns_report_carries_are_the_facts_a_frame_draws() {
    let root = tempfile::tempdir().expect("a temp state home");
    let session = Session::create(root.path()).expect("a session");
    let document = document(session.guid());
    let asked = Usage {
        input: Some(48_200),
        cached_input: Some(12_050),
        ..Usage::default()
    };
    let uncached = Usage {
        input: Some(48_200),
        ..Usage::default()
    };

    let facts = Facts::of(&session, &document).measured(&asked, None);

    assert_eq!(
        facts.ctx,
        Some(Ctx {
            size: 48_200,
            window: None
        }),
        "the input the turn carried, and no window until a reader names one"
    );
    assert_eq!(
        facts.cache,
        Some(25),
        "a quarter of every input token came from the provider's own cache"
    );

    assert_eq!(
        Facts::of(&session, &document)
            .measured(&uncached, None)
            .cache,
        None,
        "a report that names no cached token has no hit rate to draw"
    );
}

#[test]
fn a_report_that_states_no_size_measures_nothing() {
    let root = tempfile::tempdir().expect("a temp state home");
    let session = Session::create(root.path()).expect("a session");
    let document = document(session.guid());
    let reported = Usage {
        input: Some(48_200),
        cached_input: Some(4_820),
        ..Usage::default()
    };

    assert_eq!(
        Facts::of(&session, &document)
            .measured(&reported, Some(200_000))
            .ctx,
        Some(Ctx {
            size: 48_200,
            window: Some(200_000)
        }),
        "the window a reader named, beside the size this report measured"
    );

    assert_eq!(
        (
            Facts::of(&session, &document)
                .measured(&Usage::default(), Some(200_000))
                .ctx,
            Facts::of(&session, &document)
                .measured(&Usage::default(), None)
                .cache
        ),
        (None, None),
        "a report that carries no token is no size to be a share of a window and no \
             input to make a hit rate of, so the row is absent rather than nought"
    );
}

#[test]
fn the_facts_a_session_holds_say_whether_the_model_thinks() {
    for (thinking, word) in [(true, "high"), (false, "off")] {
        let root = tempfile::tempdir().expect("a temp state home");
        let session = Session::create(root.path()).expect("a session");
        let mut document = document(session.guid());
        document.thinking = thinking;

        let facts = Facts::of(&session, &document);

        assert_eq!(
            facts.think.as_deref(),
            Some(word),
            "a document that says {thinking} is drawn as {word} and never a third word"
        );
    }
}

#[test]
fn the_facts_of_a_run_take_the_dialect_its_document_names() {
    let root = tempfile::tempdir().expect("a temp state home");
    let session = Session::create(root.path()).expect("a session");

    for (wire, word) in [
        (Wire::Anthropic, "anthropic"),
        (Wire::Openai, "openai"),
        (Wire::Copilot, "copilot"),
    ] {
        let mut document = document(session.guid());
        document.wire = wire;

        let facts = Facts::of(&session, &document);

        assert_eq!(
            facts.wire.as_deref(),
            Some(word),
            "the dialect the document names, in the word the rail draws it by"
        );
    }
}

#[test]
fn a_pair_holding_a_wide_glyph_still_lands_its_slash_beside_the_slash_above_it() {
    let shared = Halves::of(["9 / 250", "日本語 / 2"].into_iter());

    let narrow = padded("9 / 250", shared);
    let wide = padded("日本語 / 2", shared);
    let half = |spelling: &str| {
        let (one, _) = spelling.split_once('/').expect("a pair");
        pen::cells(one.trim_end())
    };

    assert_eq!(
        half(&narrow),
        half(&wide),
        "a half holding three wide glyphs draws in more columns than a half holding one \
             digit, so it is padded by the columns it leaves and not by the glyphs it is made \
             of: {narrow:?} and {wide:?}"
    );
    assert_eq!(
        half(&wide),
        u16::try_from(shared.left).expect("a half width"),
        "and the half is exactly its group's width, so the two slashes stand in one column"
    );
}

#[test]
fn a_path_keeps_the_components_that_fit_and_marks_the_elision_in_front() {
    assert_eq!(
        tail("/glemsom/src/sindri", ROOM),
        "/glemsom/src/sindri",
        "a path that fits is drawn whole, with no mark to explain"
    );
    assert_eq!(
        tail("/very/long/path/that/cannot/fit/whole", ROOM),
        "…/cannot/fit/whole",
        "a path that does not fit keeps its tail and says the head went"
    );
    assert_eq!(
        tail("/the-name-that-will-not-fit-anywhere", ROOM),
        "",
        "no component fits the column, so the fact has no value to draw"
    );
    assert_eq!(
        tail("/w/日本語", 8),
        "…/日本語",
        "a component of wide glyphs takes the columns it draws in, so the tail that fits \
             eight is the one whose glyphs take eight and never the one whose count does"
    );
}

#[test]
fn a_served_name_keeps_the_segments_that_fit_and_marks_the_cut_at_its_tail() {
    assert_eq!(
        head("claude-sonnet-4-5-20250929", ROOM),
        "claude-sonnet-4-5…",
        "a dated name keeps the family a reader can name, and marks the date stamp it \
             left behind"
    );
    assert_eq!(
        head("claude-sonnet-4-5", ROOM),
        "claude-sonnet-4-5",
        "a name that fits is drawn whole, with no mark to explain"
    );
    assert_eq!(
        head("anthropic.claude-3-5-sonnet-20241022-v2:0", ROOM),
        "anthropic.claude-3-5…",
        "a gateway name is cut at a segment boundary, and never mid-segment"
    );
    assert_eq!(
        head("claude-sonnet-4-5-20250929", "claude".len() + 1),
        "claude…",
        "the mark takes a column of its own, so the head stops one short of the room"
    );
    assert_eq!(
        head("aservednamethatnobreaksanywhere", ROOM),
        "",
        "a name whose first segment does not fit beside the mark has no head to keep"
    );
}

#[test]
fn a_served_name_holding_a_wide_glyph_is_cut_by_the_columns_it_draws_in() {
    let wide = "claude-sonnet-日本語-日本-20250929";

    let cut = head(wide, ROOM);

    assert_eq!(
        cut, "claude-sonnet-日本語…",
        "a segment of six glyphs is twelve columns, so the second one leaves the head no \
             room: the cut is by what the glyphs draw in and not by how many there are"
    );
    assert!(
        pen::cells(&cut) <= u16::try_from(ROOM).expect("a value column"),
        "and the head it keeps draws in no more columns than the value column holds"
    );
    assert_eq!(
        head("日本語日本語日本語日本語", ROOM),
        "",
        "a name of wide glyphs and no segment to break at is wider than its column and \
             keeps no head at all, rather than a head wider than the rail"
    );
}

fn document(guid: &str) -> SessionDocument {
    SessionDocument {
        session: guid.to_owned(),
        wire: Wire::Anthropic,
        model: String::from("claude-sonnet"),
        enforcement: Enforcement::Bubblewrap,
        policy: PolicyRecord {
            enforcement: Enforcement::Bubblewrap,
            writable: vec![String::from("/home/glemsom/src/sindri")],
            timeout_seconds: 120,
            command_budget_ms: 3_600_000,
            output_budget_bytes: 8 * 1024 * 1024,
            description: String::from("the boundary the model was told about"),
        },
        workspace: PathBuf::from("/home/glemsom/src/sindri"),
        branch: Some(String::from("spec/1-capability")),
        commit: Some(String::from("16becb5")),
        dials: Dials {
            max_turns: 250,
            command_budget_ms: 3_600_000,
            output_budget_bytes: 8 * 1024 * 1024,
        },
        start: String::from("2026-01-01T00:00:00Z"),
        end: Some(String::from("2026-01-01T00:04:00Z")),
        turns: 3,
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

fn band(width: u16, height: u16) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

const SERVED: &str = "claude-sonnet-4-5-20250929";

const GATEWAY: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

const CUT: &str = "claude-sonnet-4-5…";

fn frame() -> Facts {
    Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from(SERVED)),
        think: Some(String::from("high")),
        turns: Some((3, 250)),
        ctx: Some(Ctx {
            size: 48_200,
            window: None,
        }),
        cache: Some(88),
        workspace: Some(PathBuf::from(
            "/home/glemsom/Documents/git/sindri/.worktrees/46-rail",
        )),
        branch: Some(String::from("spec/1-capability")),
        loaded: ["bash", "records"].into_iter().collect(),
        sandbox: Some(Enforcement::Bubblewrap),
        session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
        temp: Some(PathBuf::from(
            "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
        )),
        ..Facts::default()
    }
}

fn spent() -> Facts {
    Facts {
        spend: Usage {
            input: Some(48_200),
            output: Some(3_100),
            total: Some(51_300),
            ..Usage::default()
        },
        ..frame()
    }
}

fn wire_refused() -> Stop {
    Stop::Error(SecretSet::default().redact("the wire went away"))
}

fn live() -> Facts {
    Facts {
        state: Some(RunState::Running),
        ..frame()
    }
}

fn painted_shape(facts: &Facts, shape: Shape, area: Rect, marker: Option<&Pulse>) -> Buffer {
    let palette = Palette::TRUECOLOR;
    let mut buffer = Buffer::empty(area);
    pen::claim(&mut buffer, area, palette.ground(None));
    draw(
        &rows(facts, shape, area, marker),
        area,
        &mut buffer,
        &palette,
    );
    buffer
}

#[test]
fn the_rail_s_first_row_says_the_run_is_running_and_steps_with_the_marker() {
    let first = |at: Option<u64>| {
        laid(
            &live(),
            Shape::Ledger,
            band(COLUMNS, 19),
            at.map(Pulse::live).as_ref(),
        )
        .into_iter()
        .next()
        .expect("the rail's first row")
    };

    assert_eq!(
        first(Some(0)),
        stated(&[(0, "state     "), (10, "⠋ running")]),
        "a live run's first row says the state it is in, under the glyph the frame is on"
    );
    assert_eq!(
        first(Some(160)),
        stated(&[(0, "state     "), (10, "⠹ running")]),
        "and two steps of eighty milliseconds later the row has stepped with the frame"
    );
    assert_eq!(
        first(None),
        stated(&[(0, "state     "), (10, "running")]),
        "a rail handed no marker says the state and no glyph, never a step it has not taken"
    );

    let buffer = painted_shape(
        &live(),
        Shape::Ledger,
        band(COLUMNS, 19),
        Some(&Pulse::live(0)),
    );
    assert_eq!(
        Ink::of(
            buffer
                .cell((MARGIN + FIELD, 0))
                .expect("the state's first cell"),
            &Palette::TRUECOLOR
        ),
        Some(Ink::Accent),
        "the row says something is happening, so it is read in the accent ink"
    );
}

#[test]
fn a_stopped_run_s_row_is_the_stop_s_own_word_under_the_mark_that_stop_earns() {
    for (stop, glyph, ink) in [
        (Stop::Final, '✓', Ink::Ok),
        (Stop::MaxTurns, '✗', Ink::Err),
        (Stop::Budget(Budget::Clock), '✗', Ink::Err),
        (Stop::Cancelled, '✗', Ink::Err),
        (wire_refused(), '✗', Ink::Err),
    ] {
        let facts = Facts {
            state: Some(RunState::Stopped(stop.clone())),
            ..frame()
        };
        let area = band(COLUMNS, 19);
        let value = format!("{glyph} {}", stop.word());

        assert_eq!(
            laid(&facts, Shape::Ledger, area, None)[0],
            stated(&[(0, "state     "), (10, value.as_str())]),
            "a run stopped on {} says so in its own word, under the mark that stop wears",
            stop.word()
        );
        assert_eq!(
            Ink::of(
                painted_shape(&facts, Shape::Ledger, area, None)
                    .cell((MARGIN + FIELD, 0))
                    .expect("the state's first cell"),
                &Palette::TRUECOLOR
            ),
            Some(ink),
            "and the mark is read in the ink that stop earns"
        );
    }
}

#[test]
fn a_live_run_wears_no_stop_s_mark_whatever_the_frame_s_marker_settled_on() {
    let area = band(COLUMNS, 19);
    let settled = Pulse::settled(&Stop::MaxTurns, 1_240);

    assert_eq!(
        laid(&live(), Shape::Ledger, area, Some(&settled))[0],
        stated(&[(0, "state     "), (10, "running")]),
        "a run that is still going says running, and never the word its marker settled on"
    );
    assert_eq!(
        Ink::of(
            painted_shape(&live(), Shape::Ledger, area, Some(&settled))
                .cell((MARGIN + FIELD, 0))
                .expect("the state's first cell"),
            &Palette::TRUECOLOR
        ),
        Some(Ink::Accent),
        "and it is read in the accent, so the row and the head never say two steps"
    );
}

#[test]
fn the_state_is_the_last_fact_the_rail_gives_up_and_the_sandbox_outlives_it() {
    let facts = Facts {
        state: Some(RunState::Running),
        sandbox: Some(Enforcement::Bubblewrap),
        ..Facts::default()
    };

    assert_eq!(
        laid(&facts, Shape::Ledger, band(COLUMNS, 3), None),
        [
            stated(&[(0, "state     "), (10, "running")]),
            stated(&[]),
            stated(&[(0, "sandbox   "), (10, "bwrap")]),
        ],
        "a three-row rail holds the run's state, the air under it and the posture that \
             confines it"
    );
    assert_eq!(
        laid(&facts, Shape::Ledger, band(COLUMNS, 2), None),
        [
            stated(&[(0, "sandbox   "), (10, "bwrap")]),
            stated(&[(0, "+1 more")]),
        ],
        "and a two-row rail gives the state up before the sandbox, which never goes"
    );
}

#[test]
fn the_strip_carries_the_state_beside_the_size_and_lets_it_go_after_every_fact_it_may() {
    let facts = Facts {
        state: Some(RunState::Running),
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        ..frame()
    };
    let marker = Pulse::live(0);
    let strip = |width: u16| {
        glyphs(
            &painted_shape(&facts, Shape::Strip, band(width, 1), Some(&marker)),
            0,
        )
    };

    assert_eq!(
        strip(80),
        margined_row(
            "ctx █░░░░░ · state ⠋ running · model claude-sonnet-4-5… · sandbox bwrap",
            80
        ),
        "a strip under the rail's threshold carries the run's state beside the size it \
             watches, and holds it where the branch it carries no room for has gone"
    );
    assert_eq!(
        strip(40),
        margined_row("ctx █░░░░░ · state ⠋ running", 40),
        "and a narrower one keeps the state where the model has gone, and gives up the count \
             of what went beside it because the margin leaves no room for the two"
    );
    assert_eq!(
        strip(27),
        margined_row("ctx █░░░░░ · +12 more", 27),
        "a strip too narrow to hold the size and the state beside it keeps the size alone, \
             because the state is the last fact the strip lets go and never the first"
    );

    for width in 32..NARROW {
        let row = strip(width);
        assert!(
            row.contains("state ⠋ running"),
            "a {width}-column strip holds the run's size and its state, so it says what \
                 the run is doing: {row:?}"
        );
        assert!(
            pen::cells(&row) <= width,
            "and no fact is laid past the {width} columns it was given: {row:?}"
        );
    }
}

fn stated(parts: &[(u16, &str)]) -> Vec<(u16, String)> {
    parts
        .iter()
        .map(|(at, text)| (*at, String::from(*text)))
        .collect()
}

fn laid(
    facts: &Facts,
    shape: Shape,
    area: Rect,
    marker: Option<&Pulse>,
) -> Vec<Vec<(u16, String)>> {
    rows(facts, shape, area, marker)
        .iter()
        .map(|row| {
            row.parts()
                .iter()
                .map(|(at, part)| {
                    let drawn = match part {
                        Part::Run(text, _) => text.clone(),
                        Part::Value(value) => value.text().to_owned(),
                    };
                    (*at, drawn)
                })
                .collect()
        })
        .collect()
}

fn placed(facts: &Facts, shape: Shape, area: Rect) -> Vec<(u16, Vec<(u16, String)>)> {
    rows(facts, shape, area, None)
        .iter()
        .map(|row| {
            (
                row.y(),
                row.parts()
                    .iter()
                    .map(|(at, part)| {
                        let glyphs = match part {
                            Part::Run(text, _) => text.clone(),
                            Part::Value(value) => value.text().to_owned(),
                        };
                        (*at, glyphs)
                    })
                    .collect(),
            )
        })
        .collect()
}

fn read(facts: &Facts, area: Rect) -> Vec<String> {
    placed(facts, Shape::Screen, area)
        .into_iter()
        .map(|(_, parts)| {
            let glyphs: Vec<&str> = parts.iter().map(|(_, glyphs)| glyphs.as_str()).collect();
            if glyphs
                .iter()
                .all(|run| run.chars().all(|glyph| glyph == HAIRLINE))
            {
                return String::from("rule");
            }
            glyphs
                .iter()
                .flat_map(|run| run.split_whitespace())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

fn run() -> Facts {
    Facts {
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        ..frame()
    }
    .recorded(&document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"))
    .loading(["bash", "records"].into_iter().collect())
}

fn painted(facts: &Facts, area: Rect) -> Buffer {
    let palette = Palette::TRUECOLOR;
    let mut buffer = Buffer::empty(area);
    pen::claim(&mut buffer, area, palette.ground(None));
    draw(
        &rows(facts, Shape::Screen, area, None),
        area,
        &mut buffer,
        &palette,
    );
    buffer
}

fn margined_row(text: &str, width: u16) -> String {
    let mut row = format!("{}{text}", pen::blank(MARGIN));
    row.push_str(&pen::blank(width.saturating_sub(pen::cells(&row))));
    row
}

fn bare_row(value: &str, width: u16) -> String {
    margined_row(value, width)
}

fn hung_row(label: &str, value: &str, width: u16) -> String {
    let at = width
        .saturating_sub(margins())
        .saturating_sub(pen::cells(value));
    margined_row(
        &format!(
            "{}{}{value}",
            pen::pad(label, FIELD),
            pen::blank(at.saturating_sub(FIELD))
        ),
        width,
    )
}

fn said_row(label: &str, value: &str, width: u16) -> String {
    margined_row(&format!("{}{value}", pen::pad(label, FIELD)), width)
}

fn glyphs(buffer: &Buffer, y: u16) -> String {
    (buffer.area.x..buffer.area.right())
        .map(|x| {
            buffer
                .cell((x, y))
                .unwrap_or_else(|| panic!("row {y}, column {x} is outside the band"))
                .symbol()
                .to_owned()
        })
        .collect()
}

#[test]
fn a_fact_the_run_knows_nothing_about_is_not_drawn_and_its_block_goes_with_it() {
    let document = SessionDocument {
        route: None,
        branch: None,
        commit: None,
        end: None,
        usage: Usage::default(),
        stopped: RunState::Running,
        thinking: false,
        model: String::from("claude-sonnet"),
        ..document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")
    };
    let facts = Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet")),
        think: Some(String::from("off")),
        workspace: Some(PathBuf::from("/home/glemsom/src/sindri")),
        sandbox: Some(Enforcement::Bubblewrap),
        ..Facts::default()
    }
    .recorded(&document);

    let rows = read(&facts, band(81, 40));

    for gone in [
        "route", "meters", "spend", "branch", "commit", "exit", "end", "skills", "session",
    ] {
        assert!(
            !rows
                .iter()
                .any(|row| row == gone || row.starts_with(&format!("{gone} "))),
            "the run knows no {gone}, so the screen draws no row for it: {rows:?}"
        );
    }
    assert!(
        rows.contains(&String::from("think off")),
        "whether the model thinks is a fact the run always knows: {rows:?}"
    );
    assert!(
        rows.contains(&String::from("stopped running")),
        "a run still going says so, and says nothing about when it ended: {rows:?}"
    );
}

#[test]
fn a_narrow_column_cuts_a_path_at_its_head_and_a_name_at_its_tail_and_draws_no_other_part() {
    let facts = Facts {
        model: Some(String::from(GATEWAY)),
        ..frame()
    }
    .recorded(&document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"))
    .loading(
        [String::from("a-skill-name-that-runs-past-any-column-here")]
            .into_iter()
            .collect(),
    );
    let area = band(40, 40);

    let drawn = read(&facts, area);

    assert!(
        drawn.contains(&String::from("model anthropic.claude-3-5…")),
        "a served name too long for its column keeps its head and marks the tail: {drawn:?}"
    );
    assert!(
        drawn.contains(&String::from("a-skill-name-that-runs-past-any…")),
        "a loaded name is cut the same way: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|row| row.starts_with("session ")),
        "the GUID is thirty-six columns and the row holds twenty-six, so it is not drawn in \
             part: {drawn:?}"
    );
    for row in rows(&facts, Shape::Screen, area, None) {
        for (at, part) in row.parts() {
            assert!(
                usize::from(*at) + usize::from(part.columns()) <= usize::from(area.width),
                "every row of a narrow column lands inside it: {at} for {} columns in a \
                     {}-column band",
                part.columns(),
                area.width
            );
        }
    }
}

#[test]
fn a_block_is_drawn_whole_and_what_the_band_cannot_hold_is_counted_under_its_last_column() {
    let facts = Facts { ..frame() }
        .recorded(&document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"))
        .loading([String::from("bash")].into_iter().collect());
    let area = band(120, 12);

    let laid = placed(&facts, Shape::Screen, area);
    let names: Vec<(&str, u16)> = laid
        .iter()
        .filter_map(|(_, parts)| match parts.as_slice() {
            [(x, glyphs)] if glyphs.chars().any(char::is_alphabetic) => Some((glyphs.as_str(), *x)),
            _ => None,
        })
        .collect();

    assert_eq!(
        names,
        [
            ("run", 0),
            ("meters", 0),
            ("spend", 59),
            ("place", 59),
            ("+8 more", 59),
        ],
        "the blocks flow into the columns whole, a block that does not fit moves on rather \
             than being split, and the count of what went stands under the last column"
    );
    let (y, parts) = laid.last().cloned().expect("the band counts what went");
    assert_eq!(
        parts,
        vec![(59, String::from("+8 more"))],
        "the count of what went belongs to the last column, under its last row: {laid:?}"
    );
    assert_eq!(
        y,
        laid[laid.len() - 2].0 + 1,
        "and the screen's rows are contiguous, so the card drawn on them is as tall as the \
             rows it holds: {laid:?}"
    );
}

#[test]
fn a_band_wide_enough_for_one_column_draws_one_however_wide_it_is() {
    let facts = run();

    for width in [84u16, 120, 200, 320] {
        let rows = read(&facts, band(width, 54));

        assert!(
            rows.contains(&String::from(
                "session 9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"
            )),
            "a {width}-column band holds the whole screen in one column, so it draws one \
                 and the GUID whole: columns are there for the room they add and never to \
                 narrow a fact: {rows:?}"
        );
        assert_eq!(
            placed(&facts, Shape::Screen, band(width, 54))
                .iter()
                .filter_map(|(_, parts)| match parts.as_slice() {
                    [(_, glyphs)] if glyphs == "session" => Some(()),
                    _ => None,
                })
                .count(),
            1,
            "and one column of the blocks is all a {width}-column band draws"
        );
    }
}

#[test]
fn no_row_of_a_screen_is_laid_past_the_band_it_is_drawn_in() {
    for width in 20..=200u16 {
        for height in 1..=20 {
            for row in rows(&run(), Shape::Screen, band(width, height), None) {
                assert!(
                    row.y() < height,
                    "a row stands on row {} of a band {height} high",
                    row.y()
                );
                for (at, part) in row.parts() {
                    assert!(
                        usize::from(*at) + usize::from(part.columns()) <= usize::from(width),
                        "a part runs from {at} for {} columns on a {width}-column band",
                        part.columns()
                    );
                }
            }
        }
    }
}

#[test]
fn a_band_s_rows_are_drawn_at_the_origin_it_is_handed() {
    let facts = run();
    let laid = rows(
        &facts,
        Shape::Screen,
        Rect {
            x: 4,
            y: 2,
            ..band(81, u16::MAX)
        },
        None,
    );
    let area = Rect::new(0, 0, 100, 24);
    let at = Rect::new(14, 4, 81, 20);
    let palette = Palette::TRUECOLOR;
    let mut buffer = Buffer::empty(area);

    draw(&laid, at, &mut buffer, &palette);

    assert_eq!(
        (laid[0].y(), laid[0].parts()[0].0),
        (0, 0),
        "a shape lays its rows from the band's own origin, so its first row is row 0 whatever row \
             and column the band it was handed sits at, and its first word opens in the margin"
    );
    let start = at.x.saturating_add(MARGIN);
    assert_eq!(
        glyphs(&buffer, at.y).trim_end(),
        format!("{}run", pen::blank(start)),
        "the run block's name is drawn from the column the door is handed and the frame's \
             margin, and its first glyph stands at column {start}"
    );
    let blank = |y: u16, from: u16, to: u16| {
        (from..to).all(|x| buffer.cell((x, y)).expect("a cell of the band").symbol() == " ")
    };
    assert!(
        (0..at.y).all(|y| blank(y, 0, area.width)),
        "no row stands above the origin row, so every cell of the four rows over it is blank"
    );
    assert!(
        blank(at.y, 0, start),
        "no row starts left of the origin column, so the cells before column {start} are blank"
    );
}

#[test]
fn the_door_draws_no_row_past_the_rows_the_band_gives_it() {
    let facts = run();
    let laid = rows(&facts, Shape::Screen, band(81, u16::MAX), None);
    let area = band(81, 6);
    let palette = Palette::TRUECOLOR;
    let mut buffer = Buffer::empty(area);

    draw(&laid, band(81, 1), &mut buffer, &palette);

    assert_eq!(
        glyphs(&buffer, 0).trim_end(),
        format!("{}run", pen::blank(MARGIN)),
        "the first row of the screen's layout is drawn from the origin and the frame's margin, \
             however few rows the band holds"
    );
    assert!(
        (1..area.height).all(|y| glyphs(&buffer, y).trim().is_empty()),
        "a band one row tall is handed one row, so every row under the origin is blank"
    );
}

#[test]
fn the_screen_draws_the_facts_the_ledger_holds_and_never_reads_one_off_the_record_again() {
    let elsewhere = Facts {
        wire: Some(String::from("openai")),
        model: Some(String::from("gpt-5")),
        think: Some(String::from("off")),
        branch: Some(String::from("main")),
        ..frame()
    }
    .recorded(&document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"));
    let drawn = read(&elsewhere, band(81, 40));

    for held in ["wire openai", "model gpt-5", "think off", "branch main"] {
        assert!(
            drawn.contains(&String::from(held)),
            "the record names another wire, another model and another branch, and the \
                 screen draws what the frame holds: {drawn:?}"
        );
    }

    let blank = Facts {
        model: None,
        ..frame()
    }
    .recorded(&document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"));
    let drawn = read(&blank, band(81, 40));

    assert!(
        !drawn.iter().any(|row| row.starts_with("model ")),
        "a served name the frame does not hold has no row, whatever the record names: \
             {drawn:?}"
    );
}

#[test]
fn a_name_with_no_label_of_its_own_starts_the_row_it_stands_on() {
    let area = band(81, 40);

    let buffer = painted(&run(), area);

    let names: Vec<String> = (0..area.height)
        .map(|y| glyphs(&buffer, y))
        .filter(|row| row.trim() == "bash" || row.trim() == "records")
        .collect();
    assert_eq!(
        names,
        [bare_row("bash", 81), bare_row("records", 81)],
        "a name with no label of its own is no number, so it starts the row it stands on \
             with no field and no run of blanks in front of it"
    );
}

#[test]
fn a_column_is_never_narrower_than_a_whole_one_and_a_screen_handed_nothing_draws_no_row() {
    for width in 20..=200u16 {
        let mut starts: Vec<u16> = rows(&run(), Shape::Screen, band(width, 40), None)
            .iter()
            .filter_map(|row| match row.parts() {
                [(x, Part::Run(label, Ink::Dim))]
                    if !label.chars().all(|glyph| glyph == HAIRLINE) =>
                {
                    Some(*x)
                }
                _ => None,
            })
            .collect();
        starts.sort_unstable();
        starts.dedup();
        let holds = usize::from((width.saturating_add(COLUMN_GAP)) / (MIN_COLUMN + COLUMN_GAP));

        assert!(
            starts.len() <= holds.max(1),
            "a {width}-column band holds {holds} whole columns, and the screen draws \
                 {}: {starts:?}",
            starts.len()
        );
        for start in starts {
            assert!(
                start == 0 || start + MIN_COLUMN <= width,
                "a column starts at {start} on a {width}-column band, and forty columns is \
                     the least this screen draws one at"
            );
        }
    }

    assert_eq!(
        rows(&Facts::default(), Shape::Screen, band(120, 40), None),
        [],
        "a screen handed no fact and no record has nothing to place"
    );
}

#[test]
fn a_screen_row_sets_its_label_dim_and_its_value_plain_where_the_value_hangs() {
    let area = band(81, 40);

    let buffer = painted(&run(), area);

    let wire = glyphs(&buffer, 1);
    assert_eq!(
        wire,
        said_row("wire", "anthropic", 81),
        "the label in its field and the word reading on from it"
    );
    assert_eq!(
        inks(&buffer, 1, &Palette::TRUECOLOR),
        [(Ink::Plain, 2), (Ink::Dim, 10), (Ink::Plain, 69)],
        "the field is `dim` and the value is the fact, so it is `plain`"
    );
    assert_eq!(
        column_of(&glyphs(&buffer, 2), '/'),
        None,
        "the model's row is no pair"
    );
    assert_eq!(
        column_of(&glyphs(&buffer, 7), '/'),
        column_of(&glyphs(&buffer, 8), '/'),
        "the turns above the size line their slash up with it, so the digits either side \
             are a column"
    );
    assert_eq!(
        glyphs(&buffer, 5),
        margined_row(
            &pen::filled(area.width.saturating_sub(margins()), '─'),
            area.width
        ),
        "the rule spans the column between the run's last fact and the name of the block \
             under it"
    );
    assert!(
        glyphs(&buffer, 4).trim().starts_with("think"),
        "the fact above the rule is the last of the run block"
    );
    assert_eq!(glyphs(&buffer, 6).trim(), "meters");
    assert_eq!(
        inks(&buffer, 5, &Palette::TRUECOLOR),
        [(Ink::Plain, 2), (Ink::Dim, 77), (Ink::Plain, 2)],
        "the rule between two blocks is drawn in the ink a block's edge is read in"
    );
}

#[test]
fn a_share_of_a_window_is_a_meter_and_a_hit_rate_is_no_meter_at_all() {
    let area = band(81, 40);

    let buffer = painted(&run(), area);

    let empty = u16::try_from(METER - 1).expect("the meter's cells fit a row");
    let meter = format!("{FULL_CELL}{}", pen::filled(empty, EMPTY_CELL));
    assert_eq!(
        glyphs(&buffer, 8),
        hung_row("ctx", &format!("{meter} 48.2k / 200k"), 81),
        "a quarter of the window fills one of the meter's six cells, beside the size it is \
             a share of"
    );
    assert_eq!(
        read(&run(), area)[9],
        "cache 88%",
        "a hit rate has no ceiling to be a share of, so it is the number and no meter"
    );
}

#[test]
fn the_fact_that_says_the_run_is_moving_wears_the_accent() {
    let area = band(81, 40);
    let moved = SessionDocument {
        end: None,
        ..document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")
    };

    for (stopped, ink) in [
        (RunState::Running, Ink::Accent),
        (RunState::Stopped(Stop::Final), Ink::Plain),
    ] {
        let facts = Facts { ..frame() }.recorded(&SessionDocument {
            stopped,
            ..moved.clone()
        });
        let buffer = painted(&facts, area);
        let row = (0..area.height)
            .find(|y| glyphs(&buffer, *y).trim_start().starts_with("stopped"))
            .expect("the screen draws the state the run stopped in");
        let at = (MARGIN + FIELD, row);

        assert_eq!(
            Ink::of(
                buffer.cell(at).expect("a cell of the state"),
                &Palette::TRUECOLOR
            ),
            Some(ink),
            "a run still going is the one fact on the screen that says something is \
                 happening, so it is read in the accent and not in the plain ink"
        );
    }
}

#[test]
fn every_fact_a_run_holds_has_its_row_in_the_block_the_screen_reads_it_in() {
    assert_eq!(
        read(&run(), band(81, 40)),
        [
            "run",
            "wire anthropic",
            "model claude-sonnet-4-5-20250929",
            "route chat",
            "think high",
            "rule",
            "meters",
            "turns 3 / 250",
            "ctx 48.2k / 200k",
            "cache 88%",
            "rule",
            "spend",
            "input 48.2k",
            "output 3.1k",
            "total 51.3k",
            "cached 12k",
            "reasoning 900",
            "rule",
            "place",
            "workspace /home/glemsom/Documents/git/sindri/.worktrees/46-rail",
            "branch spec/1-capability",
            "commit 16becb5",
            "rule",
            "stop",
            "stopped final",
            "exit 0",
            "start 2026-01-01T00:00:00Z",
            "end 2026-01-01T00:04:00Z",
            "rule",
            "confine",
            "sandbox bubblewrap",
            "rule",
            "skills",
            "bash",
            "records",
            "rule",
            "session",
            "session 9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61",
            "temp …/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
        ],
        "every fact the run holds has its row, whole, in the block the screen reads it in: the \
             route it took, what it spent, the commit it stood on, the state it stopped in with its \
             instants, the names it loaded, and the whole of what a rail column shortens"
    );
}

#[test]
fn the_screen_names_a_block_and_lays_a_row_for_each_fact_with_its_value_on_the_hanging_edge() {
    let facts = Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from(SERVED)),
        think: Some(String::from("high")),
        ..Facts::default()
    };

    assert_eq!(
        placed(&facts, Shape::Screen, band(81, 40)),
        [
            (0, stated(&[(0, "run")])),
            (1, stated(&[(0, "wire      "), (10, "anthropic")])),
            (2, stated(&[(0, "model     "), (10, SERVED)])),
            (3, stated(&[(0, "think     "), (10, "high")])),
        ],
        "a block is named in the column's first row and each fact below it carries its value \
             on the column's last column, whole where the column holds it"
    );
}

#[test]
fn the_ledger_lays_its_blocks_in_the_order_they_are_read_with_air_between_two() {
    assert_eq!(
        laid(&frame(), Shape::Ledger, band(COLUMNS, 17), None),
        [
            stated(&[(0, "wire      "), (10, "anthropic")]),
            stated(&[(0, "model     "), (10, CUT)]),
            stated(&[(0, "think     "), (10, "high")]),
            stated(&[]),
            stated(&[(0, "turns     "), (10, "3 / 250")]),
            stated(&[(0, "ctx       "), (10, "48.2k")]),
            stated(&[(0, "cache     "), (10, "88%")]),
            stated(&[]),
            stated(&[(0, "workspace "), (10, "…/46-rail")]),
            stated(&[(0, "branch    "), (10, "spec/1-capability")]),
            stated(&[]),
            stated(&[(0, "skills    "), (10, "2")]),
            stated(&[]),
            stated(&[(0, "sandbox   "), (10, "bwrap")]),
            stated(&[]),
            stated(&[(0, "session   "), (10, "9f2c7a41")]),
            stated(&[(0, "temp      "), (10, "…/9f2c7a41/tmp")]),
        ],
        "six blocks, an empty row between two of them, and nothing given up at this height"
    );

    let rules: Vec<usize> = laid(&frame(), Shape::Ledger, band(COLUMNS, 17), None)
        .iter()
        .enumerate()
        .filter(|(_, parts)| {
            parts
                .iter()
                .any(|(_, glyphs)| glyphs.chars().all(|glyph| glyph == HAIRLINE))
        })
        .map(|(row, _)| row)
        .collect();
    assert!(
        rules.is_empty(),
        "the rail draws a rule on rows {rules:?}: the blocks are parted by air, and the \
             column's only rule is the one under the face"
    );
}

#[test]
fn the_ledgers_foot_says_what_the_run_spent_on_the_frames_last_rows() {
    let rows = placed(&spent(), Shape::Ledger, band(COLUMNS, 24));

    assert_eq!(
        &rows[rows.len() - 3..],
        [
            (21, stated(&[(0, "input     "), (10, "48.2k")])),
            (22, stated(&[(0, "output    "), (10, "3.1k")])),
            (23, stated(&[(0, "total     "), (10, "51.3k")])),
        ],
        "the spend stands on the band's last three rows, read from the top down, with the total \
             on the frame's own floor"
    );
}

#[test]
fn the_foot_is_parted_from_the_blocks_above_it_by_one_row_of_air() {
    let rows = placed(&spent(), Shape::Ledger, band(COLUMNS, 21));

    assert_eq!(
        rows[16],
        (16, stated(&[(0, "temp      "), (10, "…/9f2c7a41/tmp")])),
        "the blocks above the foot end on row 16, as they do on a rail that knows no spend"
    );
    assert_eq!(
        rows[17],
        (18, stated(&[(0, "input     "), (10, "48.2k")])),
        "and the foot takes the band's last three rows from row 18, so the row above it is the \
             air that parts the two"
    );
}

#[test]
fn a_band_with_no_room_for_the_foot_keeps_the_total_and_counts_the_two_it_gave_up() {
    let rows = placed(&spent(), Shape::Ledger, band(COLUMNS, 20));

    assert_eq!(
        rows[17],
        (17, stated(&[(0, "+2 more")])),
        "a band with room for one row of the spend counts the two it gave up under the blocks"
    );
    assert_eq!(
        rows[18],
        (19, stated(&[(0, "total     "), (10, "51.3k")])),
        "and keeps the total on its floor, because that is the fact a reader glances at"
    );
}

#[test]
fn a_full_rail_keeps_the_foot_on_the_band_it_just_fits() {
    let rows = placed(&run(), Shape::Ledger, band(COLUMNS, 24));

    assert_eq!(
        rows.last(),
        Some(&(23, stated(&[(0, "total     "), (10, "51.3k")]))),
        "every fact a frame can hold and a spend beside them leave the total on the floor, so the \
         foot costs the blocks above it no row they held"
    );
}

#[test]
fn a_ledger_that_knows_no_spend_gives_the_rows_at_its_foot_back_to_air() {
    let rows = laid(&frame(), Shape::Ledger, band(COLUMNS, 24), None);

    assert_eq!(
        rows.last(),
        Some(&stated(&[(0, "temp      "), (10, "…/9f2c7a41/tmp")])),
        "a rail that knows no spend draws no row at its foot: the block it ends on is the last \
             row it lays"
    );
}

#[test]
fn the_strip_lays_the_facts_it_carries_on_one_row_with_one_gap_between_two() {
    assert_eq!(
        laid(&frame(), Shape::Strip, band(40, 1), None),
        [stated(&[
            (0, "ctx "),
            (4, "48.2k"),
            (9, " · "),
            (12, "model "),
            (18, CUT),
        ])],
        "the strip's one row reads the facts of its own list, and the room left is not \
             enough for the count of the eight it does not carry"
    );
}

#[test]
fn a_band_too_short_gives_facts_up_in_the_sacrifice_order_and_the_last_row_counts_what_went() {
    assert_eq!(
        laid(&frame(), Shape::Ledger, band(COLUMNS, 8), None),
        [
            stated(&[(0, "wire      "), (10, "anthropic")]),
            stated(&[(0, "model     "), (10, CUT)]),
            stated(&[(0, "think     "), (10, "high")]),
            stated(&[]),
            stated(&[(0, "turns     "), (10, "3 / 250")]),
            stated(&[]),
            stated(&[(0, "sandbox   "), (10, "bwrap")]),
            stated(&[(0, "+7 more")]),
        ],
        "temp, cache, session, branch, skills, ctx and workspace went, in that order, and the \
             foot of the ledger counts the seven"
    );
}

#[test]
fn every_fact_of_a_ledger_opens_its_value_on_the_field_and_a_sheet_hangs_its_figures() {
    let facts_of = |rows: &[Vec<(u16, String)>]| -> Vec<(String, u16, String)> {
        rows.iter()
            .filter_map(|parts| match parts.as_slice() {
                [(0, label), (at, glyphs)] if !label.trim().is_empty() => {
                    Some((label.trim().to_owned(), *at, glyphs.clone()))
                }
                _ => None,
            })
            .collect()
    };

    let rail = facts_of(&laid(&frame(), Shape::Ledger, band(COLUMNS, 17), None));
    assert_eq!(
        rail.iter()
            .map(|(label, _, _)| label.as_str())
            .collect::<Vec<&str>>(),
        [
            "wire",
            "model",
            "think",
            "turns",
            "ctx",
            "cache",
            "workspace",
            "branch",
            "skills",
            "sandbox",
            "session",
            "temp",
        ],
        "the rail states every fact the run holds on a row of its own, under its label"
    );
    let spend = laid(&spent(), Shape::Ledger, band(COLUMNS, 24), None);
    let foot = facts_of(&spend[spend.len() - 3..]);
    assert_eq!(
        foot.iter()
            .map(|(label, _, _)| label.as_str())
            .collect::<Vec<&str>>(),
        ["input", "output", "total"],
        "the rows at the foot of the ledger state what the run spent, under the same labels"
    );

    let stated = facts_of(&laid(&run(), Shape::Ledger, band(COLUMNS, 17), None));
    assert!(
        stated.iter().any(|(label, _, _)| label == "state"),
        "the rail's first row states what the run is doing, whose value opens on the field too"
    );

    for (label, at, glyphs) in rail.iter().chain(&foot).chain(&stated) {
        assert_eq!(
            *at, FIELD,
            "{label}'s {glyphs:?} opens in {at}: a ledger opens every value, the spend's own \
             included, on the field its label is set in, so its facts read as one list under a \
             single left rule"
        );
        assert!(
            usize::from(*at) + usize::from(pen::cells(glyphs)) <= usize::from(COLUMNS - margins()),
            "{label}'s {glyphs:?} reaches past the column the frame's margin leaves: no value \
             of the rail hangs on that edge any more"
        );
    }
    let model = rail
        .iter()
        .find(|(label, _, _)| label == "model")
        .expect("the model's row of the rail");
    assert_eq!(
        usize::from(model.1) + usize::from(pen::cells(&model.2)),
        usize::from(COLUMNS - margins()),
        "the model's {:?} is cut to the field and fills it to the margin: a value too long for \
         the field gives way at its tail where it always gave way",
        model.2
    );

    let sheet_rows: Vec<Vec<(u16, String)>> = placed(&spent(), Shape::Screen, band(SCREEN, 40))
        .into_iter()
        .map(|(_, parts)| parts)
        .collect();
    let sheet = facts_of(&sheet_rows);
    for label in ["turns", "ctx", "cache", "input", "output", "total"] {
        let (_, at, glyphs) = sheet
            .iter()
            .find(|(fact, _, _)| fact == label)
            .unwrap_or_else(|| panic!("the {label} row of the sheet"));
        assert_eq!(
            usize::from(*at) + usize::from(pen::cells(glyphs)),
            usize::from(SCREEN - margins()),
            "{label}'s {glyphs:?} does not end on its column's last column: a sheet lays its \
             facts in several columns, so a figure hangs on that edge and its digits line up \
             with the column beside it"
        );
    }
    for (label, at, _) in sheet.iter().filter(|(fact, _, _)| {
        !["turns", "ctx", "cache", "input", "output", "total"].contains(&fact.as_str())
    }) {
        assert_eq!(
            *at, FIELD,
            "{label}'s value starts in {at}: a word of the sheet opens on the field its label \
             is set in, as every value of the ledger does"
        );
    }
}

#[test]
fn a_name_that_goes_for_want_of_room_is_drawn_whole_once_the_rail_grew_to_hold_it() {
    for width in 20..=200u16 {
        let rail = Rail::of(width);
        let room = match rail {
            Rail::Ledger => Rail::columns(width),
            Rail::Strip => width,
        };
        let drawn_whole = match rail {
            Rail::Ledger => {
                value_room(room.saturating_sub(margins())) >= pen::cells(UNNAMED) as usize
            }
            Rail::Strip => false,
        };
        let expected = match drawn_whole {
            true => stated(&[(0, "model     "), (FIELD, UNNAMED)]),
            false => stated(&[(0, "+1 more")]),
        };

        let drawn = laid(&unnamed(), rail.into(), band(room, 2), None);
        assert_eq!(
            drawn,
            [expected],
            "at {width} columns the {rail:?} draws a name with no hyphen to break at as \
                 {drawn:?}: a name no column of it holds whole is given up and counted, and one a \
                 grown rail holds is drawn whole and never in part"
        );
    }
}

const UNNAMED: &str = "aservednamethatnobreaksanywhere";

fn unnamed() -> Facts {
    Facts {
        model: Some(String::from(UNNAMED)),
        ..Facts::default()
    }
}

#[test]
fn no_row_is_laid_past_the_band_it_is_drawn_in() {
    for width in 20..=200u16 {
        let rail = Rail::of(width);
        let room = match rail {
            Rail::Ledger => Rail::columns(width),
            Rail::Strip => width,
        };
        for height in 1..=20 {
            for facts in [frame(), spent(), run()] {
                assert_laid_inside(&facts, rail.into(), band(room, height), room);
            }
        }
    }
}

fn assert_laid_inside(facts: &Facts, shape: Shape, area: Rect, room: u16) {
    let laid = rows(facts, shape, area, None);

    assert!(
        laid.len() <= usize::from(area.height),
        "the {shape:?} lays {} rows on a band {} high and {room} wide",
        laid.len(),
        area.height
    );
    for row in &laid {
        for (at, part) in row.parts() {
            assert!(
                usize::from(*at) + usize::from(part.columns()) <= usize::from(room),
                "the {shape:?} lays a part running from {at} for {} columns on a band {room} \
                     wide.",
                part.columns()
            );
        }
    }
}

#[test]
fn the_rail_holds_the_frames_right_edge_down_to_the_row_above_the_composer() {
    let area = band(120, 40);
    let share = split(&frame(), area);
    assert_eq!(
        share.bands,
        band(120 - CEILING, 40),
        "the transcript is laid in the frame less the rail's columns, and the frame's own \
             height"
    );
    assert_eq!(share.rail, Some(Rail::Ledger));

    let mut transcript = band(120 - CEILING, 37);
    assert_eq!(
        share.rail_band(area, 37, &mut transcript),
        Some((
            Rail::Ledger,
            RailBand::of(Rect {
                x: 120 - CEILING,
                ..band(CEILING, 37)
            })
        )),
        "the rail's band is the frame's own floor at its right edge, down to the composer's head \
             row and no row past it"
    );
    assert_eq!(
        transcript,
        band(120 - CEILING, 37),
        "so the transcript is handed back the band it was laid in"
    );

    let mut transcript = band(NARROW - FLOOR, 21);
    assert_eq!(
        split(&frame(), band(NARROW, 37)).rail_band(band(NARROW, 37), 21, &mut transcript),
        Some((
            Rail::Ledger,
            RailBand::of(Rect {
                x: NARROW - FLOOR,
                ..band(FLOOR, 21)
            })
        )),
        "and the narrowest frame that holds a ledger is the transcript's own floor beside the \
             rail's"
    );

    assert_eq!(
        share.rail_band(area, area.y, &mut transcript),
        None,
        "a composer headed by the frame's own first row leaves the rail no row, so no rail is \
             drawn at all"
    );
}

#[test]
fn a_strip_takes_the_transcripts_last_row_and_leaves_the_bands_their_columns() {
    let area = band(NARROW - 1, 24);
    let share = split(&frame(), area);
    assert_eq!(
        share.bands, area,
        "a frame below the ledger's floor gives the bands every column it holds"
    );
    assert_eq!(share.rail, Some(Rail::Strip));

    let mut transcript = band(NARROW - 1, 21);
    assert_eq!(
        share.rail_band(area, 21, &mut transcript),
        Some((
            Rail::Strip,
            RailBand::of(Rect {
                y: 20,
                height: 1,
                ..area
            })
        )),
        "below that the strip takes the transcript's band's last row and no columns"
    );
    assert_eq!(
        transcript,
        band(NARROW - 1, 20),
        "which is the one row the transcript gives up for it"
    );

    let mut nothing = band(NARROW - 1, 0);
    assert_eq!(
        share.rail_band(area, 21, &mut nothing),
        None,
        "and a band with no row to give hands the strip none"
    );
}

#[test]
fn the_band_under_a_picture_centres_it_and_lays_the_facts_beneath_the_rows_it_takes() {
    let area = band(COLUMNS, 37);
    assert_eq!(
        RailBand::of(area).with_picture(19, 11),
        RailBand {
            area,
            picture: Some(Rect {
                x: 7,
                y: 0,
                width: 19,
                height: 11,
            }),
            facts: Rect {
                y: 12,
                height: 25,
                ..area
            },
        },
        "a nineteen-cell picture is centred in the rail's columns past the one the frame's seam \
             stands in, one row under it is kept for the rule that closes the face, and the facts \
             take every row under that"
    );
    assert_eq!(
        RailBand::of(area).picture,
        None,
        "and a band with no picture drawn over it reserves no row"
    );
}

#[test]
fn a_picture_larger_than_the_band_is_cut_to_it_rather_than_laid_outside_the_rail() {
    let area = band(COLUMNS, 4);
    let band = RailBand::of(area).with_picture(40, 9);

    assert_eq!(
        band.picture,
        Some(Rect {
            x: 1,
            width: 31,
            ..area
        }),
        "a picture wider and taller than the band is cut to the columns it has past the seam"
    );
    assert_eq!(
        band.facts.height, 0,
        "and leaves the facts no row, so none is laid outside the rail"
    );
}

#[test]
fn a_frame_that_knows_no_fact_leaves_the_whole_frame_to_the_bands() {
    let area = band(120, 37);
    assert_eq!(
        split(&Facts::default(), area),
        Share {
            bands: area,
            rail: None,
        },
        "a ledger with nothing in it is not drawn, and nothing narrows for it"
    );
    let mut transcript = area;
    assert_eq!(
        Share {
            bands: area,
            rail: None
        }
        .rail_band(area, area.bottom(), &mut transcript),
        None,
        "and a frame with no rail cuts no band of its own"
    );
    assert_eq!(transcript, area, "so the transcript keeps the whole frame");
}

#[test]
fn a_taller_band_keeps_the_blocks_where_they_stood_and_lays_the_foot_on_its_floor() {
    let frame_high = placed(&spent(), Shape::Ledger, band(COLUMNS, 40));
    let band_high = placed(&spent(), Shape::Ledger, band(COLUMNS, 37));
    let floor = |rows: &[(u16, Vec<(u16, String)>)]| rows.last().map(|(row, _)| *row);

    assert_eq!(
        &frame_high[..17],
        &band_high[..17],
        "the rows a rail is handed past the blocks it holds are air: every block keeps the row \
             it stood on"
    );
    assert_eq!(
        (floor(&frame_high), floor(&band_high)),
        (Some(39), Some(36)),
        "and the foot stands on the floor of the band it is handed rather than under the blocks"
    );
}

#[test]
fn the_rail_s_column_is_a_fact_about_the_frame_between_a_floor_and_a_ceiling() {
    assert_eq!(
        Rail::columns(NARROW),
        FLOOR,
        "the narrowest frame that holds a ledger takes the rail's floor and no less"
    );
    assert!(
        Rail::columns(NARROW + 24) > FLOOR,
        "and a wider frame gives the rail more of itself, so the column is a fact about \
             the frame and not a width written down"
    );
    assert_eq!(
        Rail::columns(400),
        CEILING,
        "but never more than the ceiling, however wide the real terminal is"
    );
    assert_eq!(
        Rail::columns(20),
        20,
        "and a frame narrower than the floor is handed all of itself, so the column is never \
             wider than the band it is drawn in"
    );
}

#[test]
fn a_wide_frame_draws_the_whole_served_name_where_the_narrowest_ledger_cuts_it_to_its_head() {
    let facts = Facts {
        model: Some(String::from(SERVED)),
        ..Facts::default()
    };

    assert_eq!(
        model_drawn(&facts, NARROW),
        CUT,
        "the rail's floor leaves a value column narrower than a dated name, and the head \
             with its mark is what is drawn"
    );
    assert_eq!(
        model_drawn(&facts, NARROW + 60),
        SERVED,
        "and a wider frame's rail holds the served name whole: the value column is the \
             rail's own, so widening the rail is what stops the cut"
    );
}

fn model_drawn(facts: &Facts, frame: u16) -> String {
    let area = band(frame, 24);
    let share = split(facts, area);
    let mut transcript = share.bands;
    let (shape, rail) = share
        .rail_band(area, area.bottom(), &mut transcript)
        .expect("a rail in a frame that knows a fact");
    rows(facts, shape.into(), rail.facts, None)
        .iter()
        .flat_map(Row::parts)
        .find_map(|(_, part)| match part {
            Part::Value(value) => Some(value.text().to_owned()),
            Part::Run(..) => None,
        })
        .expect("the model's own row")
}

#[test]
fn the_width_a_frame_loses_its_ledger_at_is_the_rails_floor_beside_the_transcripts() {
    assert_eq!(
        NARROW,
        FLOOR + TRANSCRIPT,
        "the threshold is derived: the narrowest frame that holds a ledger is the rail's own \
             floor beside the columns the transcript keeps"
    );
    assert_eq!(
        Rail::of(NARROW),
        Rail::Ledger,
        "so the narrowest frame the transcript still has its floor in holds the ledger"
    );
    assert_eq!(
        Rail::of(NARROW - 1),
        Rail::Strip,
        "and one column under it is the strip, which costs the transcript no columns"
    );
    assert_eq!(Rail::of(20), Rail::Strip, "the narrowest frame there is");
    assert_eq!(Rail::of(200), Rail::Ledger, "the widest");
}

#[test]
fn the_transcript_moves_a_column_at_a_time_and_gives_up_the_rail_only_where_the_shape_changes() {
    let mut crossing = None;
    for width in 21..=200u16 {
        let narrow = split(&frame(), band(width - 1, 40)).bands.width;
        let wide = split(&frame(), band(width, 40)).bands.width;
        if Rail::of(width - 1) != Rail::of(width) {
            crossing = Some((width, narrow, wide));
            continue;
        }
        assert!(
            wide.abs_diff(narrow) <= 1,
            "at {width} columns the transcript is {wide} wide where {narrow} was a column \
                 ago: a keystroke reflows a whole rail's worth of prose"
        );
    }
    let (at, strip, ledger) = crossing.expect("a width where the strip becomes the ledger");
    assert_eq!(
        at, NARROW,
        "the transcript reflows at the one width the shape changes at"
    );
    assert_eq!(
        Rail::columns(at),
        FLOOR,
        "the rail appears at its narrowest and never at a grown column, so the one reflow a \
             resize to this width costs is the least a rail can cost"
    );
    assert!(
        strip - ledger < Rail::columns(at),
        "at {at} columns the transcript gives up {} of its {strip} columns, which is the \
             rail's whole column of {}: what a keystroke at this width reflows is less than the \
             rail it makes room for",
        strip - ledger,
        Rail::columns(at)
    );
}

#[test]
fn the_ledger_gives_the_foot_up_first_and_the_sandbox_never_goes() {
    assert_eq!(
        SACRIFICE.map(Kind::label),
        [
            "input",
            "output",
            "total",
            "temp",
            "cache",
            "session",
            "branch",
            "skills",
            "ctx",
            "workspace",
            "turns",
            "wire",
            "think",
            "model",
            "state"
        ],
        "the sacrifice order, the first to go first: the foot, then every block above it, and \
             the state the last fact it lets go"
    );
    assert!(
        !SACRIFICE.contains(&Kind::Sandbox),
        "the sandbox posture is kept at every width, so no loop over the order can reach it"
    );
}

#[test]
fn the_strip_carries_its_own_list_in_the_order_a_reader_watches_it() {
    assert_eq!(
        STRIP_FACTS.map(Kind::label),
        ["ctx", "state", "model", "sandbox", "branch"],
        "the thing you watch, what it is doing, the thing you are, the thing that confines \
             you, and the thing you are standing on"
    );

    let tail = STRIP_FACTS.map(Kind::label);
    let tail = &tail[1..];
    assert_eq!(
        STRIP_SACRIFICE.map(Kind::label),
        [tail[3], tail[2], tail[1], tail[0]],
        "and what the strip gives up is that list without its head, read from the tail, so \
             the run's size is the one fact it never goes without and the state is the last it \
             lets go"
    );
    assert!(
        !STRIP_FACTS.contains(&Kind::Turns),
        "turns stays out of the list: the strip has no room to say both how full the \
             conversation is and how many turns it took to fill"
    );
}

#[test]
fn no_served_name_is_laid_in_part_at_any_width_between_twenty_and_two_hundred() {
    for name in [
        "claude-sonnet-4-5",
        SERVED,
        "anthropic.claude-3-5-sonnet-20241022-v2:0",
    ] {
        assert_no_width_lays_a_name_in_part(name);
    }
}

fn assert_no_width_lays_a_name_in_part(name: &str) {
    let mut cut = 0;
    for width in 20..=200u16 {
        let rail = Rail::of(width);
        let room = match rail {
            Rail::Ledger => Rail::columns(width),
            Rail::Strip => width,
        };
        let shape = Shape::from(rail);
        let facts = Facts {
            model: Some(String::from(name)),
            ..Facts::default()
        };
        let fits_in = match rail {
            Rail::Ledger => value_room(Rail::columns(width)),
            Rail::Strip => VALUE,
        };
        let value = rows(&facts, shape, band(room, 1), None)
            .iter()
            .flat_map(Row::parts)
            .find_map(|(_, part)| match part {
                Part::Value(value) => Some(value.text().to_owned()),
                Part::Run(..) => None,
            });
        let Some(value) = value else {
            continue;
        };

        assert!(
            value == name || value.ends_with('…'),
            "at {width} columns {name:?} is laid as {value:?}, which is neither the whole \
                 name nor a name with the mark that says the rest went"
        );
        assert!(
            name.starts_with(value.trim_end_matches('…')),
            "at {width} columns {name:?} is laid as {value:?}: the head survives and the \
                 tail is what goes"
        );
        assert!(
            Fact::named(Kind::Model, value.clone())
                .fitted(fits_in)
                .is_some(),
            "at {width} columns {name:?} is laid as {value:?}, which no row of the \
                 {shape:?} has room for"
        );
        if value.ends_with('…') {
            cut += 1;
        }
    }
    if drawn(name) <= VALUE {
        assert_eq!(
            cut, 0,
            "{name:?} fits a value column, so no width in the sweep may cut it"
        );
    } else {
        assert!(
            cut > 0,
            "{name:?} is wider than the value column, so some width in the sweep lays it cut"
        );
    }
}

#[test]
fn a_row_states_the_kind_of_the_fact_it_lays_and_the_air_between_blocks_states_none() {
    let laid = rows(&frame(), Shape::Ledger, band(COLUMNS, 19), None);

    assert_eq!(
        laid.iter().map(Row::kind).take(5).collect::<Vec<_>>(),
        [
            Some(Kind::Wire),
            Some(Kind::Model),
            Some(Kind::Think),
            None,
            Some(Kind::Turns)
        ],
        "the rail's rows state the facts they lay, and the row of air that parts two blocks \
         states nothing"
    );
}

#[test]
fn a_row_that_lays_no_one_fact_states_no_kind() {
    assert_eq!(
        hairline_row(band(COLUMNS, 1)).kind(),
        None,
        "the rule that heads a block states no fact of its own"
    );
    assert_eq!(
        rows(&frame(), Shape::Strip, band(COLUMNS, 1), None)[0].kind(),
        None,
        "a strip is one row carrying every fact it holds, so it states no one of them"
    );
    let short = rows(&spent(), Shape::Ledger, band(COLUMNS, 6), None);
    assert!(
        short.last().is_some_and(|row| row.kind().is_none()
            && row
                .parts()
                .iter()
                .any(|(_, part)| matches!(part, Part::Run(text, _) if text.starts_with('+')))),
        "the count of the facts a rail could not hold states no fact of its own"
    );
}
