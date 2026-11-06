//! A bench that decides wording: it prices, section by section, the preamble §9
//! re-sends every turn and the tool descriptions the model's schema carries, so
//! the cost of a sentence is read before it is written rather than guessed at
//! after, and two wordings can be chosen between on evidence.
//!
//! THE ESTIMATOR IS AN ESTIMATE, NOT A TOKEN COUNT. Tokens are counted with
//! `tiktoken-rs`'s cl100k_base: a real byte-pair tokenizer, the GPT-4 family's,
//! deterministic to the byte. It is not the tokenizer of every model Sindri talks
//! to — an Anthropic or a Copilot model splits words its own way — so the absolute
//! number is an estimate. It is the *same* estimate on every run and every
//! machine, which is what a choice between two wordings needs and all this bench
//! claims.
//!
//! WHAT IS PRICED is what a run pays on every turn: §9's three sections and, when
//! the repository root carries one, its `AGENTS.md`; then one row per tool a run
//! with the built-in catalog registers. Every byte of the rendered message
//! belongs to exactly one section row, so a section added to the preamble is a
//! row nobody can forget to price.
//!
//! Paths are replaced by `<workspace>` before anything is counted: the tree this
//! bench is compiled in is a fair thing to measure and its absolute path is not,
//! because a deeper checkout would otherwise cost more than a shorter sentence.
//! The user's own skills are left out for the same reason — `$HOME` is nobody's
//! wording — so the catalog measured is the built-ins the binary ships.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::preamble::Preamble;
use sindri_core::skills::Catalog;
use sindri_core::tools::ToolSet;
use tiktoken_rs::CoreBPE;

/// The ceiling on the words a run pays every turn, in cl100k_base tokens: §9's
/// three sections plus every tool description, summed.
///
/// THE NUMBER IS THE TABLE'S, NOT A WISH. When this bench landed the rows it
/// budgets summed to 376. The budget is that plus about a tenth — thirty-four
/// tokens, a sentence or two of real prose — and no more, so a sentence that
/// earns its place fits and a paragraph that does not, does not. Raising it is
/// allowed; raising it without saying in this comment what bought the tokens is
/// exactly what the budget exists to prevent.
///
/// Outside the number, deliberately: the project's own `AGENTS.md`, which §8
/// puts in the message but no sentence of Sindri's cuts. It is measured and
/// printed so nobody has to guess what it costs, and left out of the total so a
/// repository with more house rules than this one does not fail a bench about
/// wording.
const BUDGET_TOKENS: usize = 410;

/// The markers the priced sections of the preamble open with, in render order:
/// §9's three, then the fence §8 wraps the project's own words in — which is
/// absent from a tree whose repository root carries no `AGENTS.md`.
const SECTION_MARKERS: [(&str, &str); 4] = [
    ("persona", "## Persona"),
    ("principles", "## Principles"),
    ("environment", "## Environment"),
    ("project instructions", "<project_instructions>"),
];

/// Whether a row's tokens count toward the budget, and why not when they do not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Counted {
    /// Part of what a turn pays, so the budget covers it.
    TowardBudget,
    /// A row repeating rows already counted: the preamble's total is its own
    /// sections summed.
    RepeatOfRows,
    /// The repository's own words, which no sentence of Sindri's cuts.
    ProjectsOwnWords,
}

impl Counted {
    /// The word the table's last column prints for this row.
    fn label(self) -> &'static str {
        match self {
            Counted::TowardBudget => "budgeted",
            Counted::RepeatOfRows => "already counted",
            Counted::ProjectsOwnWords => "not budgeted",
        }
    }

    /// Whether the row's tokens are part of the budgeted total.
    fn counts(self) -> bool {
        self == Counted::TowardBudget
    }
}

/// One row of the table: a named portion of what a turn costs, and its price.
struct Price {
    /// The name the row prints under, and the name a failure quotes.
    name: String,
    /// The cl100k_base tokens `text` costs, as the estimator counts them.
    tokens: usize,
    /// The exact bytes those tokens are counted from — the one number here that
    /// is not an estimate.
    bytes: usize,
    /// Whether the row counts toward the budget.
    counted: Counted,
}

impl Price {
    /// The price of `text`, named `name`.
    fn of(name: impl Into<String>, text: &str, counted: Counted) -> Price {
        let tokens = tokenizer().encode_with_special_tokens(text).len();
        Price {
            name: name.into(),
            tokens,
            bytes: text.len(),
            counted,
        }
    }

    /// The row as the table prints it.
    fn print(&self) {
        println!(
            "bench: {:<22} {:>5} tok {:>6} B  {}",
            self.name,
            self.tokens,
            self.bytes,
            self.counted.label()
        );
    }
}

/// Everything one turn re-sends, priced: the preamble as it renders for this
/// tree, and every tool a run with the built-in catalog registers.
struct Measure {
    /// The preamble section by section, then the whole of it.
    preamble: Vec<Price>,
    /// One row per registered tool, named after the tool.
    tools: Vec<Price>,
}

impl Measure {
    /// Prices the wording this tree would really send: its own `AGENTS.md`, its
    /// own boundary, and the skills the binary ships.
    fn take() -> Measure {
        let root = repo_root();
        let rendered = normalize(&Preamble::for_workspace(&root).render(), &root);

        let boundary = Boundary::from_config(&Config::default(), &root, Enforcement::Bubblewrap)
            .expect("the boundary of a tree whose config names no exception holds");
        let catalog = Catalog::discover(&root, None);

        let mut preamble: Vec<Price> = sections(&rendered)
            .into_iter()
            .map(|(name, text)| {
                let counted = if name == PROJECT_INSTRUCTIONS {
                    Counted::ProjectsOwnWords
                } else {
                    Counted::TowardBudget
                };
                Price::of(name, text, counted)
            })
            .collect();
        preamble.push(Price::of(
            "preamble total",
            &rendered,
            Counted::RepeatOfRows,
        ));

        let tools = ToolSet::new(&boundary, &catalog)
            .tools()
            .iter()
            .map(|tool| {
                Price::of(
                    format!("tool {}", tool.name()),
                    tool.description(),
                    Counted::TowardBudget,
                )
            })
            .collect();

        Measure { preamble, tools }
    }

    /// Every row, in the order the table prints them.
    fn rows(&self) -> impl Iterator<Item = &Price> {
        self.preamble.iter().chain(self.tools.iter())
    }

    /// What the budget covers: the budgeted rows, summed. A tokenizer that merged
    /// two rows into one token would make the message itself a little cheaper
    /// than this sum, never dearer, so the budget is never quietly generous.
    fn budgeted_total(&self) -> usize {
        self.rows()
            .filter(|row| row.counted.counts())
            .map(|row| row.tokens)
            .sum()
    }

    /// The table: every row with its price, then the total against the budget.
    /// Printed under `--nocapture`, which is how `make bench` runs it.
    fn print(&self) {
        println!(
            "bench: estimator cl100k_base (tiktoken-rs), a proxy for every model: token counts are estimates"
        );
        for row in self.rows() {
            row.print();
        }
        let total = self.budgeted_total();
        println!(
            "bench: {:<22} {:>5} tok          of {} — {} {}",
            "budgeted total",
            total,
            BUDGET_TOKENS,
            BUDGET_TOKENS.abs_diff(total),
            if total > BUDGET_TOKENS {
                "over"
            } else {
                "to spare"
            }
        );
    }
}

/// The section that is measured but never budgeted, because it is the
/// repository's own words rather than Sindri's.
const PROJECT_INSTRUCTIONS: &str = "project instructions";

/// The estimator, loaded once per test binary: the BPE's ranks are megabytes and
/// parsing them twice would be the only slow thing in this bench.
fn tokenizer() -> &'static CoreBPE {
    static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();
    TOKENIZER.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("the cl100k_base ranks, compiled into the crate")
    })
}

/// The repository root this bench prices: the tree it is compiled in, so the
/// `AGENTS.md` measured is the one a run here would really be sent.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root above this crate")
}

/// `text` with this machine's checkout path taken out of it: the table is a
/// property of the wording, and where a tree happens to live is not.
fn normalize(text: &str, root: &Path) -> String {
    text.replace(&root.display().to_string(), "<workspace>")
}

/// The preamble cut into the sections §9 and §8 name, each slice starting at its
/// marker and ending where the next one starts, so the slices concatenated are
/// the whole message and no byte is left unpriced.
fn sections(rendered: &str) -> Vec<(&'static str, &str)> {
    let mut starts: Vec<(&'static str, usize)> = SECTION_MARKERS[..3]
        .iter()
        .map(|(name, marker)| {
            let offset = rendered
                .find(marker)
                .unwrap_or_else(|| panic!("§9's {name} section opens with {marker:?}"));
            (*name, offset)
        })
        .collect();
    let (fence, fence_marker) = SECTION_MARKERS[3];
    if let Some(offset) = rendered.find(fence_marker) {
        starts.push((fence, offset));
    }
    assert_eq!(
        starts[0].1, 0,
        "the persona opens the preamble, so nothing comes before the first priced row"
    );
    assert!(
        starts.windows(2).all(|pair| pair[0].1 < pair[1].1),
        "the sections arrive in the order §9 renders them, and none of them is empty"
    );

    starts
        .iter()
        .enumerate()
        .map(|(index, (name, start))| {
            let end = starts.get(index + 1).map_or(rendered.len(), |next| next.1);
            (*name, &rendered[*start..end])
        })
        .collect()
}

#[test]
fn the_shipped_wording_stays_within_the_token_budget() {
    let measure = Measure::take();
    measure.print();

    let total = measure.budgeted_total();
    let dearest = measure
        .rows()
        .filter(|row| row.counted.counts())
        .max_by_key(|row| row.tokens)
        .expect("a budgeted row");
    assert!(
        total <= BUDGET_TOKENS,
        "the wording a turn re-sends costs {total} tokens (a cl100k_base estimate), {} over the \
         {BUDGET_TOKENS}-token budget, and the dearest row is {} at {} tokens. Cut a sentence, or \
         raise BUDGET_TOKENS in crates/sindri-core/tests/wording_bench.rs and say what bought it.",
        total - BUDGET_TOKENS,
        dearest.name,
        dearest.tokens
    );
}

#[test]
fn every_byte_of_the_preamble_belongs_to_a_priced_section() {
    let rendered = Preamble::for_workspace(&repo_root()).render();

    let rebuilt: String = sections(&rendered)
        .into_iter()
        .map(|(_, text)| text)
        .collect();

    assert_eq!(
        rebuilt, rendered,
        "the table's rows must cover the rendered preamble, and cover it once"
    );
}

#[test]
fn every_registered_tool_is_priced_by_name() {
    let root = repo_root();
    let boundary = Boundary::from_config(&Config::default(), &root, Enforcement::Bubblewrap)
        .expect("the boundary of a tree whose config names no exception holds");
    let tools = ToolSet::new(&boundary, &Catalog::discover(&root, None));
    let priced = Measure::take();

    for tool in tools.tools() {
        let name = format!("tool {}", tool.name());
        let row = priced
            .tools
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("a registered {name} has a row of its own"));
        assert_eq!(
            row.tokens,
            tokenizer()
                .encode_with_special_tokens(tool.description())
                .len(),
            "{name} is priced from the description it really carries"
        );
    }
    assert!(
        tools.get("skill").is_some(),
        "the built-in catalog registers the skill tool, so the table prices two tools"
    );
}
