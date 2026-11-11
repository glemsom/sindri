//! The run's facts and the one way each of them is spelled, so a fact reads the same wherever it
//! is said: the wire, the served name, thinking, the turns, the window and its meter, the cache, the
//! workspace, the branch, the skills, the sandbox, the session and the temp.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use sindri_core::policy::Enforcement;
use sindri_core::provider::Usage;
use sindri_core::records::SessionDocument;
use sindri_core::session::Session;
use sindri_core::wire::Wire;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen;

/// The columns `text` draws in, a wide glyph counted as the two cells it takes: a value is
/// measured by what a row shows and never by how many glyphs it is made of.
fn drawn(text: &str) -> usize {
    usize::from(pen::cells(text))
}

/// The frame width the ledger gives way at: a hundred-column frame is the last that holds one.
pub const NARROW: u16 = 100;

/// The two shapes a fact is read in: the ledger's column beside a transcript, which has room for
/// a pair beside its meter, and the strip along a band's bottom row, which has room for the meter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Ledger,
    Strip,
}

impl Shape {
    pub fn of(width: u16) -> Shape {
        if width < NARROW {
            Shape::Strip
        } else {
            Shape::Ledger
        }
    }
}

/// The value a fact a launch can already name but a run has yet to fill is drawn as: an em dash
/// says the fact is owed and never that it is empty.
pub(crate) const DASH: &str = "—";

/// The columns a session's GUID is drawn by: no rail column holds a whole one.
const SHORT: usize = 8;

/// The cells a meter is drawn in: a share of a window is a picture, and six cells is as fine as
/// one row of a rail can say it.
pub(crate) const METER: usize = 6;

/// The cell a meter is full of, and the cell it has yet to fill.
pub(crate) const FULL_CELL: char = '█';
pub(crate) const EMPTY_CELL: char = '░';

/// The share of a window at which the rail raises its voice: a run that full is one turn from
/// being no room at all.
const FULL: u8 = 90;

/// The run's size as the last request reported it, and never as the next one will be: whatever
/// tool results the turn produced are not in it yet, so a run one turn from a full window is
/// that and no more exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub size: u64,
    pub window: Option<u64>,
}

impl Ctx {
    /// The size and the meter of the share of its window, from one derivation so a row's words and
    /// its meter cannot disagree. The ledger says the size against its window; the strip keeps only
    /// the meter, because its one row is shared with three other facts.
    pub(crate) fn spelled(&self, shape: Shape) -> Value {
        let Some(fill) = self.fill() else {
            return Value::new(tokens(self.size));
        };
        let text = match shape {
            Shape::Ledger => pair(tokens(self.size), tokens(fill.window)),
            Shape::Strip => String::new(),
        };
        Value::metered(text, fill)
    }

    /// The share of the window the run fills. A window of no tokens is no share of anything, and the
    /// ink is set here and nowhere else, so no paint site holds a second opinion.
    fn fill(&self) -> Option<Fill> {
        let window = self.window?;
        let percent = share(self.size, window)?;
        Some(Fill {
            percent,
            window,
            ink: if percent >= FULL {
                Ink::Accent
            } else {
                Ink::Plain
            },
        })
    }
}

/// The share of a window a run fills, as the meter that draws it: the percent the pair of numbers
/// cannot say in one glance, and the ink the filled cells are read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fill {
    percent: u8,
    window: u64,
    ink: Ink,
}

impl Fill {
    /// The cells the share fills, rounded to the nearest of the meter's six: a cell is a sixth of
    /// the window, and a share that fills no sixth of it is a meter with no cell lit.
    fn cells(&self) -> usize {
        (usize::from(self.percent) * METER + 50) / 100
    }

    fn draw(&self, buffer: &mut Buffer, x: u16, y: u16, palette: &Palette) -> u16 {
        let filled = self.cells();
        let mut x = x;
        for cell in 0..METER {
            let (glyph, ink) = match cell < filled {
                true => (FULL_CELL, self.ink),
                false => (EMPTY_CELL, Ink::Dim),
            };
            pen::glyph(buffer, x, y, glyph, ink, palette);
            x = x.saturating_add(1);
        }
        x
    }
}

/// One of the run's facts, by the word a label field holds it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Wire,
    Model,
    Think,
    Turns,
    Ctx,
    Cache,
    Workspace,
    Branch,
    Skills,
    Sandbox,
    Session,
    Temp,
}

impl Kind {
    /// The word the label field holds, so no two drawings can call one fact two things.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Kind::Wire => "wire",
            Kind::Model => "model",
            Kind::Think => "think",
            Kind::Turns => "turns",
            Kind::Ctx => "ctx",
            Kind::Cache => "cache",
            Kind::Workspace => "workspace",
            Kind::Branch => "branch",
            Kind::Skills => "skills",
            Kind::Sandbox => "sandbox",
            Kind::Session => "session",
            Kind::Temp => "temp",
        }
    }
}

/// What a launch knows before its first run: the wire and the model the config names, the
/// workspace and the branch it stands on, and the enforcement the policy will hold. A session, a
/// temp and a turn count belong to a run and are not here.
#[derive(Debug, Clone)]
pub struct Boot {
    pub wire: Option<Wire>,
    pub model: Option<String>,
    pub workspace: PathBuf,
    pub branch: Option<String>,
    pub enforcement: Enforcement,
}

/// The facts a run holds, as the core keeps them. A fact nobody can state is `None` and is not
/// drawn, never guessed at; a fact the launch names but a run has yet to fill is an em dash, which
/// is a value the frame holds and not an absence. No fact at all and the frame draws no rail.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    /// The dialect the run asks over, in the wire's own word: `anthropic`, `openai` or `copilot`,
    /// and never a fourth.
    pub wire: Option<String>,
    /// The served name of the model, whole here and cut to its head wherever the rail draws it.
    pub model: Option<String>,
    pub think: Option<String>,
    pub turns: Option<(u32, u32)>,
    pub ctx: Option<Ctx>,
    /// How much of the run's input the provider served from its own cache: a hit rate and not a
    /// fill, so it gets no meter and no threshold.
    pub cache: Option<u8>,
    pub workspace: Option<PathBuf>,
    pub branch: Option<String>,
    /// Nothing produces this fact yet: a caller passes it in until its producer lands.
    pub skills: Option<usize>,
    pub sandbox: Option<Enforcement>,
    /// The run's own, and an em dash on a frame that has not run yet.
    pub session: Option<String>,
    /// The run's own, and an em dash on a frame that has not run yet.
    pub temp: Option<PathBuf>,
}

impl Facts {
    /// The facts a run holds, read off the two values the core keeps them in: the session the run is
    /// writing, and the document the record rewrites as facts land.
    pub fn of(session: &Session, document: &SessionDocument) -> Facts {
        Facts {
            wire: Some(document.wire.word().to_owned()),
            model: Some(document.model.clone()),
            think: Some(if document.thinking { "high" } else { "off" }.to_owned()),
            turns: Some((document.turns, document.dials.max_turns)),
            sandbox: Some(document.enforcement),
            workspace: Some(document.workspace.clone()),
            branch: document.branch.clone(),
            session: Some(session.guid().to_owned()),
            temp: Some(session.temp().to_path_buf()),
            ..Facts::default()
        }
    }

    /// The facts of the frame a launch opens on, before a session and a document exist. A run's
    /// own facts — its session, its temp, its turns — are not known yet, and each is an em dash
    /// rather than a guess at what the run will hold.
    pub fn boot(boot: Boot) -> Facts {
        Facts {
            wire: boot.wire.map(|wire| wire.word().to_owned()),
            model: boot.model,
            workspace: Some(boot.workspace),
            branch: boot.branch,
            sandbox: Some(boot.enforcement),
            session: Some(String::from(DASH)),
            temp: Some(PathBuf::from(DASH)),
            ..Facts::default()
        }
    }

    /// The size one ask's report measured, and the part of its input the provider served from cache.
    /// The document cannot hand these in: the `usage` it keeps is every ask summed, and a sum of the
    /// inputs grows with each turn while the conversation's size is what the newest request carried.
    pub fn measured(mut self, ask: &Usage, window: Option<u64>) -> Facts {
        let Some(size) = ask.input else {
            return self;
        };
        self.ctx = Some(Ctx { size, window });
        self.cache = ask.cached_input.and_then(|cached| share(cached, size));
        self
    }
}

/// A value as a row draws it: its own glyphs, the meter a share of a window is drawn as beside
/// them, and the ink the glyphs are read in. The rule lives here and nowhere else, so the rail's
/// ledger and strip and the stats screen that widens them place and colour a value alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Value {
    text: String,
    fill: Option<Fill>,
    ink: Ink,
}

impl Value {
    pub(crate) fn new(text: impl Into<String>) -> Value {
        Value {
            text: text.into(),
            fill: None,
            ink: Ink::Plain,
        }
    }

    pub(crate) fn metered(text: impl Into<String>, fill: Fill) -> Value {
        Value {
            fill: Some(fill),
            ..Value::new(text)
        }
    }

    /// The same value in another ink, for the one fact that says something is moving: the eye lands
    /// on it without reading a word.
    pub(crate) fn inked(mut self, ink: Ink) -> Value {
        self.ink = ink;
        self
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn meter(&self) -> Option<Fill> {
        self.fill
    }

    /// The same value spelled in another cut: a column that narrows a value's glyphs keeps the
    /// meter beside them and the ink they are read in.
    pub(crate) fn respelled(&self, text: String) -> Value {
        Value {
            text,
            ..self.clone()
        }
    }

    /// The value's pair in the half widths its group shares, so a slash lands in one column.
    pub(crate) fn align(&mut self, halves: Halves) {
        self.text = padded(&self.text, halves);
    }

    /// The columns the value takes on a row, the column between a meter and its glyphs included.
    pub(crate) fn columns(&self) -> usize {
        let glyphs = drawn(&self.text);
        match (self.fill, glyphs) {
            (None, glyphs) => glyphs,
            (Some(_), 0) => METER,
            (Some(_), glyphs) => METER + 1 + glyphs,
        }
    }

    pub(crate) fn draw(&self, buffer: &mut Buffer, x: u16, y: u16, palette: &Palette) -> u16 {
        let mut x = x;
        if let Some(fill) = self.fill {
            x = fill.draw(buffer, x, y, palette);
            if !self.text.is_empty() {
                x = pen::run(buffer, x, y, " ", self.ink, palette);
            }
        }
        pen::run(buffer, x, y, &self.text, self.ink, palette)
    }
}

/// A number in two parts, a space either side of its slash: the digits read as a share of a whole
/// and not as one word, and the slash gets a column of its own to be read in.
pub(crate) fn pair(left: impl std::fmt::Display, right: impl std::fmt::Display) -> String {
    format!("{left} / {right}")
}

/// The share of a whole that a part is, in whole percent and capped at a hundred: a part past
/// its whole has no larger share to show, and a share of nothing is no percentage at all.
fn share(part: u64, whole: u64) -> Option<u8> {
    if whole == 0 {
        return None;
    }
    let percent = (u128::from(part) * 100 + u128::from(whole) / 2) / u128::from(whole);
    Some(u8::try_from(percent.min(100)).unwrap_or(100))
}

/// How much of a run's input the provider served from its own cache, in whole percent: a hit rate
/// and not a fill, so it draws no meter and has no ceiling to be a share of.
pub(crate) fn hit_rate(percent: u8) -> String {
    format!("{percent}%")
}

/// The size of a run in spelled tokens: `948`, `48.2k`, `1.02M`, because a rail column has no
/// room to make six digits read as one number.
pub(crate) fn tokens(count: u64) -> String {
    if count < 1_000 {
        return count.to_string();
    }
    // Tenths of a thousand, and hundredths of a million: the first carries three figures up to
    // nine hundred and ninety-nine thousand, and the second once rounding takes a count past
    // four.
    let tenths = count.saturating_add(50) / 100;
    if tenths < 10_000 {
        return scaled(tenths, 1, "k");
    }
    scaled(count.saturating_add(5_000) / 10_000, 2, "M")
}

/// An amount in a unit's smallest shown fraction, drawn as that unit's whole part, a point and
/// what is left of the fraction. A zero left dangling goes, so two hundred thousand is `200k`
/// and not `200.0k`.
fn scaled(parts: u64, places: u32, unit: &str) -> String {
    let divisor = 10u64.pow(places);
    let whole = parts / divisor;
    let fraction = parts % divisor;
    if fraction == 0 {
        return format!("{whole}{unit}");
    }
    let decimals = format!("{fraction:0width$}", width = places as usize);
    format!("{whole}.{}{unit}", decimals.trim_end_matches('0'))
}

/// The row that says how many facts the room could not hold, in the one wording both shapes and
/// the screen that widens it count them by.
pub(crate) fn more(count: usize) -> String {
    format!("+{count} more")
}

/// The columns a group's pairs share: the widest half either side of the slash, so a block's
/// numbers are read as a column of numbers and a column of wholes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Halves {
    left: usize,
    right: usize,
}

impl Halves {
    pub(crate) fn of<'a>(spellings: impl Iterator<Item = &'a str>) -> Halves {
        spellings
            .filter_map(|spelling| spelling.split_once(" / "))
            .fold(Halves::default(), |shared, (one, two)| Halves {
                left: shared.left.max(drawn(one)),
                right: shared.right.max(drawn(two)),
            })
    }
}

/// `3 / 250` in its group's half widths: both halves hang on the slash, and the right one is
/// filled to the left, so no value carries a space the hanging edge would have to swallow.
pub(crate) fn padded(spelling: &str, shared: Halves) -> String {
    let Some((one, two)) = spelling.split_once(" / ") else {
        return spelling.to_owned();
    };
    format!("{} / {}", hung(one, shared.left), hung(two, shared.right))
}

/// `text` hung from `columns`, padded on the left by the columns it does not draw in: a half
/// holding a wide glyph spends two columns a glyph, so it is padded by what it leaves behind.
fn hung(text: &str, columns: usize) -> String {
    let room = columns.saturating_sub(drawn(text));
    let mut hung = pen::blank(u16::try_from(room).unwrap_or(u16::MAX));
    hung.push_str(text);
    hung
}

/// The tail of a path in at most `columns`, with `…/` in front when the head did not fit. Nothing
/// is ever cut mid-component: a tail that cannot hold even the last component is empty, which
/// drops the fact.
pub(crate) fn tail(path: &str, columns: usize) -> String {
    if drawn(path) <= columns {
        return path.to_owned();
    }
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    for start in 0..parts.len() {
        let tail = format!("…/{}", parts[start..].join("/"));
        if drawn(&tail) <= columns {
            return tail;
        }
    }
    String::new()
}

/// The head of a served name in at most `columns`: whole hyphen-separated segments, and never a
/// part of one. A name keeps its head, where the family lives, and gives up its stamps, because
/// widening the rail only moves the width a name fails at. A head that fits no mark is empty.
pub(crate) fn head(name: &str, columns: usize) -> String {
    if drawn(name) <= columns {
        return name.to_owned();
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut width = 0;
    for (index, part) in name.split('-').enumerate() {
        let next = width + drawn(part) + usize::from(index > 0);
        if next + 1 > columns {
            break;
        }
        kept.push(part);
        width = next;
    }
    if kept.is_empty() {
        return String::new();
    }
    format!("{}…", kept.join("-"))
}

/// A session's GUID by its first eight columns, or whole when it is shorter: no rail column holds
/// thirty-six.
pub(crate) fn short_id(guid: &str) -> &str {
    match guid.char_indices().nth(SHORT) {
        Some((end, _)) => &guid[..end],
        None => guid,
    }
}

#[cfg(test)]
mod tests {
    //! The facts a run holds and the one way each is spelled: the value a launch's frame is owed
    //! and a run has yet to fill, the size in spelled tokens, the head of a name and the tail of a
    //! path, each measured by the columns it draws in.

    use std::path::{Path, PathBuf};

    use sindri_core::policy::{Enforcement, PolicyRecord};
    use sindri_core::provider::Usage as Report;
    use sindri_core::records::{Dials, SessionDocument, Usage};
    use sindri_core::run::State as RunState;
    use sindri_core::session::Session;
    use sindri_core::wire::Wire;

    use super::*;
    use crate::pen;

    /// The columns a rail's row leaves a value: its thirty-two less the ten its label field holds.
    const ROOM: usize = 22;

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
                facts.skills
            ),
            (None, None, None, None, None),
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
            (facts.ctx, facts.cache, facts.skills),
            (None, None, None),
            "a document holds what the run spent summed over every turn, so the size of one \
             ask and the cache's hit rate are handed in and not read off it"
        );
    }

    /// The one launch a boot frame is drawn from: a workspace that is no checkout of this test's,
    /// so a fact nobody can know yet has nowhere to be read from but the rail itself.
    fn boot() -> Boot {
        Boot {
            wire: Some(Wire::Anthropic),
            model: Some(String::from("claude-sonnet-4-5")),
            workspace: PathBuf::from("/srv/workspace"),
            branch: Some(String::from("main")),
            enforcement: Enforcement::Bubblewrap,
        }
    }

    #[test]
    fn the_size_and_the_cache_a_turns_report_carries_are_the_facts_a_frame_draws() {
        let root = tempfile::tempdir().expect("a temp state home");
        let session = Session::create(root.path()).expect("a session");
        let document = document(session.guid());
        let asked = Report {
            input: Some(48_200),
            cached_input: Some(12_050),
            ..Report::default()
        };
        let uncached = Report {
            input: Some(48_200),
            ..Report::default()
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
        let reported = Report {
            input: Some(48_200),
            cached_input: Some(4_820),
            ..Report::default()
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
                    .measured(&Report::default(), Some(200_000))
                    .ctx,
                Facts::of(&session, &document)
                    .measured(&Report::default(), None)
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
            end: None,
            turns: 3,
            usage: Usage::default(),
            stopped: RunState::Running,
            route: None,
            thinking: false,
        }
    }
}
