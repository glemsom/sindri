# Glossary

Sindri's vocabulary. Use these words exactly; do not drift to synonyms.

## Agent

**turn** — one round trip to the model: our request, its response, the tool results we feed
back. Bounded by the turn cap (`max_turns`, default 250).

**tool call** — one request from the model to run a tool — `bash`, `skill` or `open` — plus the
structured tool result returned. `bash` is the one that runs anything: one call carrying a whole
pipeline or script the model composed. The code's word once its arguments are read is `Asked`.

**open** — the third tool: one **target** handed to the host's opener, for the human to look at.
It runs nothing inside the sandbox: the core parses the target and refuses it, and only a frame
registers it — **batch mode**, having no window, has no `open` at all: an absence, not a refusal.

**target** — what one `open` call names, and exactly one of them: `url`, an `http://` or `https://`
address, or `path`, a file (`Target`, whose two shapes are `Url` and `Path`). A `file:` URI is a
second spelling of `path` and not a second door: it runs the same guard and is refused in the
identical words. A `path` is guarded against the **sandbox policy**'s own sets and never a second
list — inside the **writable set** or the **session temp**, as given and as resolved, a regular
file, its extension read again on the resolved name so a link cannot promise one file and deliver
another.

**started** — the word an `open` result carries when the host was asked and the opener had not
refused it when the quarter-second probe window closed; `failed:` with the opener's own reason
when it exited non-zero inside that window. Neither says a human looked.

**answer** — the final assistant text of the last turn. The product. Markdown read into the
frame: prose, fences drawn as panels, links that keep their destinations, and a **picture** with
its **caption**.

**stopped** — why the loop ended: `final`, `max_turns`, `budget`, `cancelled` or `error`. The set
is the loop's and it is closed: `Stopped` holds those five and no sixth. The **record** writes one
word more — `running` — and that is its word for a run still going, not a way the loop stops.

**budget** — what one prompt's commands may spend before the run stops: an hour of command time
and 8 MiB of output fed back, spent inside a script where the turn cap cannot count it. A tool
result is re-sent every turn, so the bytes counted are the conversation's.

**cancel** — Ctrl-C (SIGINT): one flag the loop and the executor share. The loop stops between
turns, the command's process group is killed, stdout stays empty, exit is 130. In the **frame** a
cancel is answered the moment the key lands, and the state clears when the run has stopped.

## Sandbox

**sandbox policy** — the value holding writable set and temp mapping: the only source
of both enforcement and description.

**writable set** — the paths mounted read-write besides the session temp: the workspace
plus **extras**.

**extras** — user-configured extra writable paths.

**session temp** — per-run writable scratch, mapped to `TMPDIR` in the sandbox, and
mounted as `HOME` beneath it.

**description** — the model-facing text derived from the sandbox policy, also printed by
`sindri doctor`. One sentence may be appended to it that the policy does not derive: the
**pointer** to the **catalog**, added only where the model may see one.

**pointer** — the one sentence appended to the **description** saying this environment's
instructions are behind `skill`, so a model reading the schema it reaches for first learns the
**catalog** is there. Not a **skill** body's, and not the preamble's: the preamble is re-sent every
turn, and the pointer is paid for only where there is a catalog the model may see.

**yolo-unsafe** — `--yolo-unsafe`: the policy whose enforcement is `none`, so commands run
with the caller's own privileges, outside bubblewrap, inheriting its environment and
working directory. Never persisted: asked for by the invocation that makes it.

**doctor** — `sindri doctor`: verifies bwrap, the binary set and the mount plan, prints
the description, and lists the **skill** **catalog**.

## Run

**boot** — turning the flags and the process's own facts into a **session** ready to start
(`crates/cli/src/boot.rs`), and the one line a front end says when it cannot. Not a synonym for
session: a boot ends where a session begins.

**instructions** — a repository's own house rules: the `AGENTS.md` at its root, found through the
workspace's git root — or the workspace itself where that is no repository — and read once at
**boot**. They travel beside the preamble and never as a turn of their own, so the **transcript**
stays the user's words; the read is capped by a byte count stated in one place, and a file past it
arrives cut with a marker rather than silently short. A repository with no `AGENTS.md` says
nothing. The code's word for the file is `Taken`: `Absent`, `Found` or `Unreadable`.

**session** — one run, identified by a GUID, recorded under
`$XDG_STATE_HOME/sindri/sessions/<guid>/`.

**record** — the files one **session** leaves in its directory: what `sindri sessions show` and
`/export` read, and what the frame never writes directly. A record is a session read back, added to
and never rewritten. Two other things go by that word: the specification's *record* is what a
terminal stores from a **status** report, and `agent::Report` is what one run did.

**transcript** — the ordered history of messages, tool calls and tool results of a
session.

**window** — the rows of the **transcript** the frame shows at once. It has a place in the
**thread** rather than an offset from its **tail**: a reader on the tail follows what arrives, and a
reader scrolled back keeps the row they are reading while a run appends below them, told how much
by the **badge**. That place is an entry and the rows into it, so a block that grows or folds above
the window moves no row under the reader; `Enter` on a call is a place too.

**badge** — the new-rows badge, `↓ 4 new rows`, against the footer's right edge: how many rows
arrived below a **window** that is off the **tail**. Only what arrived after the reader took their
place counts: going back, reaching the first turn and folding a call all take the rows below the
window as read. It says `1 new row` for one, and clears at the tail.

**tail** — the newest row of the **transcript**, and the place a **window** follows
from unless a reader has scrolled back or engaged with a call; `End` is the key that
takes it again.

**spill file** — where an over-long tool result's full text is written when the
model-facing copy is truncated head+tail.

**batch mode** — `-b`: the Unix-filter front end. stdout is the answer only; stderr is
silent unless `-v`, but for a fault the run cannot leave unsaid — a repository config it
refused, an `AGENTS.md` it could not read; `--format json` changes the output shape.

**reasoning** — provider-side thinking tokens, off by default, shown on stderr with `-v`.

**status** — what a run tells the terminal it is doing: one `OSC 7501` report holding one of five
states — `idle`, `working`, `done`, `blocked`, `error` — one coarse line of our own words, and the
run's identity, `app=sindri` with the workspace's own name as `title`. The code's word for the set
is `sindri_core::status::State`, and one report is made by the pure encoder `status::report`, which
returns the bytes or nothing at all: a report past a ceiling the specification sets is one a
terminal discards whole, so it is not sent rather than shortened. The mapping is `Stopped`'s and it
is closed — `final`, `max_turns` and `budget` report `done`, `error` reports `error`, and
`cancelled` reports `idle`, which is what an interrupted program owes its terminal. `blocked` is no
turn's word: a turn never waits on a human, so the frame alone sends it, with `kind=auth`, while
the settings modal is asking for a key — a run with no credential cannot answer, and that is the
one genuine blockage sindri has. There is no `progress`, since a turn is not a fraction of anything,
and no `id=`, since several runs are several ptys and so several root records. `msg` is coarse —
`Doing`'s five, thinking, running a command, loading a skill, opening a target and writing the
answer, plus the turn count — and never the shell command, the prompt or the answer, because a
report is a channel no redaction in the record guards. A frame that leaves its screen says `clear`
first, which is not one of the five: `done` and `error` outlive a shell prompt by design, and the
frame's own record must not outlive the frame. `status::clear` is that report, and `status::Told`
is the gate both front ends share — a state the terminal has already been told is not said again,
and none is said twice in a second. One setting silences every report — `status` in the config,
`SINDRI_STATUS` over it — and both front ends read it there: a courtesy is the human's to decline,
once.


## Skills

**skill** — a directory holding a `SKILL.md`, or a **built-in skill** with none at all: the
frontmatter's `name` and `description`, the two flags that say who may reach for it, and the body
beneath the fence that **activation** returns. A skill no version of the specification knows how to
read is a **refusal**, never a silence.

**description ceiling** — the character budget a **skill**'s `description` may spend —
`DESCRIPTION_CEILING`, 200 characters, read as characters and not bytes. The catalog sits in the
`skill` tool's description, so every description is paid for every turn, and one is a routing key
rather than a summary of the body: past the ceiling it is a **refusal** naming the line to cut.

**skill scope** — one of the three places a **scan** reads, and the three are ordered by
nearness: `builtin`, compiled into the binary; `user`, under the home directory at
`~/.agents/skills`; and `repo`, at every `.agents/skills` between the workspace and its repository
root. The specification governs what is inside a skill directory and says nothing about where one
lives, so these paths are ours. The code's word for it is `Scope`: `Builtin`, `User`, `Repo`.

**built-in skill** — a **skill** shipped inside the binary rather than installed to a path: its
`SKILL.md` is compiled in, so there is no directory to be absent, stale or unwritable. It is the
lowest **skill scope**, so a `user` skill of the same name shadows it and a `repo` skill shadows
them both. Its scope is a field and not an inference, so `sindri doctor` prints it, with
`(compiled in)` where a scanned skill's directory would go.

**activation** — one `skill` call's result: the body with the frontmatter stripped, in a named
`<skill_content>` wrapper, with the skill's directory and its file listing beside it so a relative
reference in the body resolves; a **built-in skill** has no directory, so its files are sent with
the body instead. Once per **session** and no more: a second call is `already_loaded` with no body,
because a tool result is sent again every turn. `/skill:name` asks for the same activation.

**loaded set** — the skills one conversation has been told, held with the history and cleared with
it: a body is in force while the **transcript** still carries it. Two identical calls in one turn's
wave are one load, and a load by hand claims the name as the tool's own call does. The code's word
is `Loaded`, and a frame builds its ledger from the `SkillLoaded` event.

**found** — what one **skill scope**'s scan hands back: its skills and its refusals, and no
shadowed list, because a scope has no nearer one to lose a name to. The code's word is `Found`,
and `skills::scan` is the function that returns one.

**fold** — the one function that makes the **catalog** of every scope's **found**, in precedence
order: a name already held is not replaced, and the skill that would have taken it is recorded as
**shadowing**. It is the only place a shadow is made, and it reads each found once. The code's
word is `fold`, and `skills::fold` is it.

**catalog** — what a **fold** makes: every **skill**, every **refusal**, and the shadowed names.
Sorted by name, so two scans of one tree say the same thing.

**refusal** — a `SKILL.md` the scan or the built-in table would not load, held with where it came
from and the line that broke it: no description, no YAML, a description past the **description
ceiling**, a link to nothing. Where it came from is its file — or, a **built-in skill** being
compiled in and having none, the **skill scope** that carries it, which is the word `sindri doctor`
prints in place of a path. The code's word for the two is `Where`: `File` and `Scope`. A refusal is
a diagnostic, not a failure — the rest of the **catalog** still holds.

**user-only skill** — one the model may not see: `disable-model-invocation: true`. It is absent
from the **catalog** the `skill` tool carries and from that tool's schema, and a human reaches it
from the **composer** alone; `sindri doctor` still prints it, because a scan found it.

**model-only skill** — one a human may not type: `user-invocable: false`. It is in the catalog and
callable, and absent from **completion**. A skill that says both is one nobody may run, and the
report says `none` for it.

**shadowing** — one name two of the **skill scope**s carry. The nearer one's skill holds it and the
farther one is recorded as shadowed, by its scope, its name and its directory where it has one, so
a reader sees both what won and what it displaced. The **fold** makes them, so a repository's skill
is recorded over the home directory's, and both over a built-in's.

## Terminal UI

**frame** — bare `sindri` on a terminal: the alternate-screen interface as a whole.

**rail** — the right column: a ledger in two blocks, split by a hairline. Above the rule, what a run
is spending — cache hit percentage, turns against the cap, tokens, context meter. Below it, what the
run *is*: its model, workspace, git branch, session GUID and session temp path. Labels sit in a
faint field and the value hangs off the last column, so every figure ends on one edge and is
compared down it, and a value too long for its room is shortened rather than wrapped. It is chrome,
not a **surface**: no fill, and what a row copies is the fact, never the leader that drew it.

**mark** — the cells a drag lights and the words that leave with them: a wash on the rows a copy
takes, from the column the drag started on to the column it ended on, the rows between in full and
a wide glyph at either edge whole, in one **ink** and only as a surface, so the ink under it still
reads as itself. One function computes the spans and both the mark and the copy read it
(`mark::span`), so the mark can never light a cell the copy does not take — but a **picture**'s
**block** is marked whole, graphic and **caption** together, its words taken once on the row the
block starts at. It stands on the rows it took until the reader's next gesture clears it.

**pulse** — the frame's own clock, and the only thing drawn without being asked: a tick of ~120 ms
while a run is live or a completion is walking, and nothing at all while the frame is still. It puts
a braille marker at the head of the newest answer with the run's elapsed time, settling on `✓` or
`✗`, freezes each call's duration as it ends, and moves the rail's output and context figures.

**walk** — a rule lit from one end: the theme interpolates two named stops, one step per cell, and
names the pairs a renderer may interpolate. It is how a border gets a gradient without a picture
and without a colour a renderer wrote down. A picture's pixels are held the same way, one at a
time.

**surface** — one of the three tiers of nearness the frame paints on: **canvas**, **panel**,
**raised**, in that order. A renderer asks the theme for the tier it draws on, and the frame fills
the canvas before the layout, so no renderer writes a background of its own: the theme names every
colour the frame paints. A renderer that *places* a picture paints the canvas under it first.

**ink** — the foreground a word is drawn in, and the theme's as surely as a **surface** is: body
ink is prose, a prompt, a call's command and a table's cells, and every other ink is a named colour
or a **walk**. A cell emptied by `Clear` has no span to bring one, so the surface names the ink it
asks for; nothing is left at the terminal's default foreground.

**canvas** — the lowest **surface**, and the theme's own: the frame fills the whole area with it
before anything is drawn, so a resize, a scrolled region or a gap a picture's escape leaves behind
is still the theme's and never the terminal's. A code word in prose is drawn straight on it.

**panel** — the **surface** the frame draws on, one tier above the canvas: the **composer**, a
fenced code block, the **queued prompt** panel, the band a turn's prompt sits on. The **rail** is
chrome on the canvas, not one.

**raised** — the **surface** an overlay is drawn on, one tier above a panel; the frame
is dimmed beneath it. The rail's sandbox **chip** sits on it too, a tier above the
canvas the rest of the rail is drawn on.

**role** — one of the seven inks a fence is painted in — `PLAIN`, `COMMENT`, `KEYWORD`, `FUNCTION`,
`TYPE`, `NUMBER`, `STRING` — each a colour the theme names and each clearing 4.5:1 on the **panel**
a fence sits on, because code is read as body text. Which scope is which role is a fact about
languages; which colour a role is painted in is the theme's.

**chip** — the rail's two statements that are not label-and-value pairs, drawn as **raised** boxes
on the edge the values hang off: the sandbox posture, `⬢ bwrap · 1 writable`, and — only when there
is one — the skills in force, `◆ house-style`. A chip is shortened to its head when its column
runs out. The posture's ink is the boundary's state, `GOOD` inside `bwrap` and `BAD` outside it.

**composer** — the input at the bottom: 2–6 rows, `Shift+Enter` for a newline, `@` for a path
**completion**, `/` for a **command**. With no text in it at all it carries the placeholder `Ask, or
drop a log here · @ path · / command`, which a draft — a space is a draft — never sits behind. It is
laid out in the **transcript**'s column and not across the terminal, so the panel ends where the
prose does and no cell of the **rail**'s column is its own.

**first frame** — the frame before the first prompt: an empty **thread** is not a blank page, but
the line naming the wire, the model, the workspace and the sandbox, one dim sentence of the
contract, and two prompts that can be typed as they read. Nothing it says is a figure, because no
run has spent any.

**queued prompt** — a prompt sent while a run has the frame: it waits in the queue and runs when
the run stops, in the order it was typed. `↑` takes the newest back into the **composer**, `Ctrl-X`
drops it. The **panel** above the composer holds them whole, titled `queued` when one waits and
`3 queued` when more, cut at six rows with that count standing for the rest.

**thread** — the growing conversation of one TUI session; `/new` starts over. Laid out as a
document: two blank rows between turns and none inside one, the **gutter** a turn's answer and calls
hang off, and the prompt at full strength on its own band. A thread shorter than the column is
written on its floor, so the newest row is the one above the **composer**.

**gutter** — a left edge the frame draws beside a thing that hangs off it: in the **thread**, the
two columns a turn's answer and its calls hang off; in a fenced code block, the `│` down its left.

**turn header** — the row that opens a turn: `YOU <hh:mm> <workspace>`.

**completion** — the offer the **composer** makes above itself, one column and one set of keys for
two triggers. `@` offers workspace paths: a proximity-ranked background walk, eight rows, `.git`
skipped and `.gitignore` deliberately not honoured. `/` offers the **command** catalogue in the one
place a command can be sent, ranked by the name being typed with the names it starts first; once
`/skill:` or `/skill ` is typed the offers are **skill** names. `Tab` and `Enter` take the marked
offer, and `Esc` dismisses it and leaves the draft as typed.

**block** — the rows a **picture** takes of an answer: its graphic's rows together with its
**caption**, or with the `‹›` that says what it could not be drawn as (`Viewport::block`). A drag
that reaches one takes it whole, by **mark**'s rule, which is the one entry that tells it.

**picture** — a markdown image the frame draws: a **block** of its own in the answer, drawn only
where the terminal speaks Kitty graphics, from a file in the workspace and never from a URL, at most
twenty rows — the face's own allowance — and whole in the **window** or not at all. A diagram is a
picture whose source is a fence's body, and twice that, forty, because it is drawn top to bottom.
Where a picture is not a block — a table cell, a list item, a heading — a mermaid fence is the
**panel** it was, and a fence that will not parse is that panel with the parser's own words under
it. A fence the stream has not closed is that panel too, its body not yet one to draw.

**caption** — the words and `‹src›` a **picture** is drawn under, `a chart ‹plot.png›`: the alt
text's words, the destination in `‹›` as a link keeps its own, and the reason in their place when
there is no picture. A mermaid fence has no destination and its source is not printed, so its
caption is `‹mermaid›`, and a diagram's **block** copies as the fence body instead, because that
name is not a diagram.

**export** — `/export [path]`: the **thread** as one markdown file, at the path named or
`sindri-<guid>.md` in the workspace, read out of the record so it says what `sindri
sessions show` prints. A file already there is not overwritten without a second ask.

**settings modal** — the `/settings` overlay: one row per key of the config schema. `Enter` saves,
`Esc` discards, `Ctrl-U` empties the field, `Space` flips a toggle. The rows scroll with the cursor
while the `file` line, the warning, a refused save's message and the hints are pinned outside it,
so the row a save failed on is never the one that fell off.

**binding** — one row of the **help overlay**: the keys, and the mouse gesture that is their
accelerator, that do one thing — and what that thing is. One list holds them all, and nothing on it
is reachable only with a pointer. The one row that is not a binding is the **command** catalogue's:
the commands share a row, because one name there is several.

**command** — one of the `/` names the **composer** sends: the name, the one line the **help
overlay** says of it, and what it opens. One catalogue (`COMMANDS`) holds them all, in the order the
frame says them; the parser, the refusal of an unknown name and the help's row all read it, so a
seventh command is added there and nowhere else. `/skill` is the one name that takes an argument
from a fixed set and the one a colon may end — `/skill:house-style` is `/skill house-style`.

**help overlay** — `?`'s list of every **binding**: the keys in their column, each description
wrapped under them, the box sized from the wrapped rows. A list longer than the box scrolls by a
screenful and says where it is (`3 of 12`) and which way there is more; `Esc` and `Enter` close it.

**from environment** — the API key field's default, and where emptying it returns: the
credential comes from `SINDRI_API_KEY`, the provider's variable, and nothing is written.

**door** — the frame's front door: the one place a run with no credential opens the interface
instead of exiting, with the **settings modal** staged on the `api_key` row.

**card** — the failure drawn instead of a frame when a `doctor` check fails, holding the check and
the report whole: sized from the rows that report wrapped into, paged with `PgUp`/`PgDn` when the
screen is shorter, and left only by a key — a resize lays it out again rather than dismissing it.