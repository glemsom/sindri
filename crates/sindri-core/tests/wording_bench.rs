use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::preamble::Preamble;
use sindri_core::skills::Catalog;
use sindri_core::tools::{FrontEnd, ToolSet};
use tiktoken_rs::CoreBPE;

const BUDGET_TOKENS: usize = 680;

const SECTION_MARKERS: [(&str, &str); 4] = [
    ("persona", "## Persona"),
    ("principles", "## Principles"),
    ("environment", "## Environment"),
    ("project instructions", "<project_instructions>"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Counted {
    TowardBudget,
    RepeatOfRows,
    ProjectsOwnWords,
}

impl Counted {
    fn label(self) -> &'static str {
        match self {
            Counted::TowardBudget => "budgeted",
            Counted::RepeatOfRows => "already counted",
            Counted::ProjectsOwnWords => "not budgeted",
        }
    }

    fn counts(self) -> bool {
        self == Counted::TowardBudget
    }
}

struct Price {
    name: String,
    tokens: usize,
    bytes: usize,
    counted: Counted,
}

impl Price {
    fn of(name: impl Into<String>, text: &str, counted: Counted) -> Price {
        let tokens = tokenizer().encode_with_special_tokens(text).len();
        Price {
            name: name.into(),
            tokens,
            bytes: text.len(),
            counted,
        }
    }

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

struct Measure {
    preamble: Vec<Price>,
    tools: Vec<Price>,
}

impl Measure {
    fn take() -> Measure {
        let root = repo_root();
        let temp = tempfile::tempdir().expect("session temp");
        let probe_boundary = Boundary::from_config(&Config::default(), &root, Enforcement::Unsafe)
            .expect("a probe boundary");
        let helpers =
            sindri_core::helpers::Helpers::probe(&probe_boundary, temp.path(), Path::new("bwrap"));
        let rendered = normalize(
            &Preamble::for_workspace(&root).helpers(helpers).render(),
            &root,
        );

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

        let tools = ToolSet::new(&boundary, &catalog, FrontEnd::Terminal)
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

    fn rows(&self) -> impl Iterator<Item = &Price> {
        self.preamble.iter().chain(self.tools.iter())
    }

    /// A tokenizer that merged two rows into one token would make the message cheaper than this
    /// sum, never dearer, so the budget is never quietly generous.
    fn budgeted_total(&self) -> usize {
        self.rows()
            .filter(|row| row.counted.counts())
            .map(|row| row.tokens)
            .sum()
    }

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

const PROJECT_INSTRUCTIONS: &str = "project instructions";

fn tokenizer() -> &'static CoreBPE {
    static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();
    TOKENIZER.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("the cl100k_base ranks, compiled into the crate")
    })
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root above this crate")
}

fn normalize(text: &str, root: &Path) -> String {
    text.replace(&root.display().to_string(), "<workspace>")
}

fn sections(rendered: &str) -> Vec<(&'static str, &str)> {
    let mut starts: Vec<(&'static str, usize)> = SECTION_MARKERS[..3]
        .iter()
        .map(|(name, marker)| {
            let offset = rendered
                .find(marker)
                .unwrap_or_else(|| panic!("the {name} section opens with {marker:?}"));
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
        "the sections arrive in the order the rule renders them, and none of them is empty"
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
    let tools = ToolSet::new(
        &boundary,
        &Catalog::discover(&root, None),
        FrontEnd::Terminal,
    );
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
