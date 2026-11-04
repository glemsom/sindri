//! `/settings`: the one config file, edited through the one schema. The frame
//! stages fields in memory and writes nothing until the user saves, `Esc`
//! throwing the whole draft away; a secret the file holds is never handed back,
//! so no field can show one and no frame can leak one. It is also the modal the
//! front door opens by itself when the run has no key, which is why `Missing` —
//! what is absent, in the words the transcript and the composer use — lives
//! here rather than in a second module both would have to know. The model row
//! completes against the names the wire serves, ranked by the composer's own
//! ranking, and it is a field first: a name nothing offered is kept and saved.
//! That row also folds the names open under itself, which is the one place the
//! modal's `Enter` means something other than save: on a row nobody has typed
//! in it asks to choose from the names the wire offered, and on the fold it
//! takes the name it is on; a row someone has typed in is their draft, so `Enter`
//! writes it, exactly as it does on every other row.
//!
//! The provider row is a choice the same way: `Enter` on it offers the wires the
//! run itself parses — `Provider::ALL`, so the modal cannot name a wire the run
//! would refuse — and the one taken is what a save writes. A wire chosen is a
//! new `Serving`, so the names under the model row are asked for again, and the
//! listing for the wire the user has left is dropped where it lands. The address
//! row moves the same `Serving`: the names are asked for the address typed in
//! it, once the row is whole — a keystroke there is a draft and asks nothing —
//! so the list a human is offered and the list a run at that address would be
//! served are one answer.
//!
//! The extra headers row folds the same way, into the table itself: `Enter` on
//! it opens one `Name: value` line per header with `+ add a header` under the
//! last, `a` adds a line, `d` removes the one under the cursor, `Ctrl-U` empties
//! it, `Enter` hands that line its own keys, and `Esc` shuts the fold with the
//! draft beside it. What the fold draws is the file's text and never a resolved
//! one, and the save is the row's own: a table built this way is written by the
//! one path a pasted table already was. A line a save would refuse is refused
//! while it is typed, in the run's own sentence (`table_refusal`), so the table's
//! check is one check asked early rather than a second opinion.
//!
//! Every row is one field (`row`): a value, a cursor in it and what a save makes
//! of it, so a keystroke means the same thing in the address, the model name,
//! the header table and a secret. `←→`, `Home` and `End` move that cursor inside
//! the row, a typed character lands at it, `Backspace` and `Delete` take the
//! cells either side of it and a paste lands there; `↑↓`, `Tab` and `BackTab`
//! walk the rows, and a row nobody has typed in still writes nothing. A narrow
//! row is scrolled to the cursor when it is drawn, so the field holds no
//! scroll of its own.

mod headers;
mod list;
mod missing;
mod row;
mod served;

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::config::{Document, KEYS, Key, Provider, refusal, toggle_shown};

use crate::overlay::Overlay;
use crate::rail::Launch;

pub use headers::Fold;
pub(crate) use headers::counted;
pub use list::List;
pub use missing::Missing;
pub use row::{Edit, Field};
pub(crate) use row::{one_line, whole};
pub use served::{Served, Serving};

/// A table the run would refuse, and the line it refuses: the sentence a save
/// would state, word for word, and the line of the headers row's value it is
/// about — counted the way the fold draws its lines. One value, so the sentence
/// in the foot and the line drawn as refused cannot point at two places.
#[derive(Debug, Clone)]
pub struct Refused {
    /// The line the sentence is about, as the fold numbers its lines.
    pub line: usize,
    /// The run's own sentence for it.
    pub said: String,
}

/// What one key did to the modal: the four things it can ask for, and the two
/// ways of asking for nothing. `Staying` is the modal taking the key and staying
/// — a field edited, a cursor moved — and `Held` is a key it holds nothing for,
/// which is the one answer that leaves the frame as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Staying,
    Held,
    Discard,
    Save,
    Quit,
    CancelRun,
}

#[derive(Debug, Clone)]
pub struct Settings {
    document: Document,
    fields: Vec<Field>,
    /// The words the provider row chooses from: the wires the run itself
    /// parses, read off `Provider::ALL` so a fourth wire is one word here and
    /// the same word there, never a list the modal keeps of its own.
    wires: Vec<String>,
    chosen: usize,
    /// The completion of the row that offers words: the served names a
    /// fragment in the model row reaches, the wires a fragment in the provider
    /// row reaches, and the one a `Tab` takes. Empty while the row is, because
    /// the row is a field before it is ever a list.
    list: List,
    /// Where the headers row's table stands: shut, or open on one of its lines
    /// with the keys the fold's or that line's. The fold holds no cursor of its
    /// own — the line under it is the field's cursor — so it is only ever a
    /// statement of whose the keys are.
    headers: Fold,
    error: Option<String>,
    missing: Option<Missing>,
}

/// The config file's document: at the path the frame was pointed at where it was
/// pointed at one, and in the schema's own place otherwise. `/settings` and
/// `/model` read the file through it, so the file one of them saves is the file
/// it read.
pub(crate) fn document(path: Option<&Path>) -> Result<Document, String> {
    let loaded = match path {
        Some(path) => Document::load_at(path),
        None => Document::load(),
    };
    loaded.map_err(|error| error.message().to_string())
}

/// The one writer of the config file: a document staged with one key's value and
/// saved atomically, or the run's own words for why it refused — staged on a
/// copy, so a value the schema refuses leaves the file exactly as it was. The
/// model screen's one name goes through it, and the modal's whole form is saved
/// by [`commit`] beside it.
pub(crate) fn write(document: &Document, key: Key, value: &str) -> Result<Document, String> {
    let mut staged = document.clone();
    staged
        .set(key, value)
        .map_err(|error| format!("{}: {}", key.name(), error.message()))?;
    commit(staged)
}

/// The save itself: the one place the frame hands a document to the disk, so the
/// file is written by one writer whatever settled the values in it.
fn commit(staged: Document) -> Result<Document, String> {
    staged.save().map_err(|error| error.message().to_string())?;
    Ok(staged)
}

impl Settings {
    pub fn open() -> Result<Settings, String> {
        document(None).map(Settings::at)
    }

    pub fn at_path(path: &Path) -> Result<Settings, String> {
        document(Some(path)).map(Settings::at)
    }

    pub fn at(document: Document) -> Settings {
        let fields = KEYS
            .iter()
            .map(|key| Field::new(*key, document.editable(*key)))
            .collect();
        Settings {
            document,
            fields,
            wires: Provider::ALL
                .iter()
                .map(|wire| wire.as_str().to_string())
                .collect(),
            chosen: 0,
            list: List::default(),
            headers: Fold::Shut,
            error: None,
            missing: None,
        }
    }

    /// The modal a first run opens by itself: its cursor on the row the run
    /// has nothing for, and, above the rows, the four facts that row needs —
    /// where a value comes from, where it is written, that the file is `0600`,
    /// and that a credential is never shown again.
    pub fn staged(mut self, missing: Missing) -> Settings {
        self.chosen = KEYS
            .iter()
            .position(|key| *key == missing.row())
            .unwrap_or_default();
        self.missing = Some(missing);
        self
    }

    pub fn path(&self) -> &Path {
        self.document.path()
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Where the extra headers row's fold stands: what the row draws and what
    /// the hint line says the keys do.
    pub fn headers(&self) -> Fold {
        self.headers
    }

    pub fn chosen(&self) -> usize {
        self.chosen
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Why the modal opened without being asked, when it did.
    pub fn missing(&self) -> Option<&Missing> {
        self.missing.as_ref()
    }

    /// What the run makes of the headers table as it stands, while that row is
    /// the one the keys are typing in: the sentence the line a save would refuse
    /// earns, with the line it belongs to. It is the save's own check —
    /// `refusal` is the one the file is judged by — asked on every frame rather
    /// than latched on a keystroke, so a line corrected is a line cleared and a
    /// line broken is a line refused without the row ever being left. `None` on
    /// every other row, because a refusal belongs to the row being typed and
    /// never follows the cursor around the modal, and `None` once a save has
    /// spoken until the next keystroke, because the foot has one sentence at a
    /// time.
    pub fn table_refusal(&self) -> Option<Refused> {
        if self.chosen_key() != Some(Key::ExtraHeaders) || self.error.is_some() {
            return None;
        }
        let (line, said) = refusal(&self.field(Key::ExtraHeaders)?.text)?;
        Some(Refused { line, said })
    }

    pub fn staged_secret(&self) -> bool {
        self.fields
            .iter()
            .any(|field| field.key.secret() && !field.text.is_empty())
    }

    /// One key, and what it did. The model row's list is ranked before the key
    /// is read, so what `Tab` takes and what `↑↓` steps are the names the field
    /// holds at that keystroke — and a row with no list under it is the plain
    /// field it always was. A fold standing open under the model row takes its
    /// own keys first, so no other row and no other key learns that it is there.
    pub fn key(&mut self, key: KeyEvent, served: &Served) -> Outcome {
        self.refresh(served);
        let outcome = self.pressed(key, served);
        // And ranked again once the key has landed, so a fold typed into is the
        // names the fragment reaches on the frame it is drawn on rather than one
        // keystroke later.
        self.refresh(served);
        outcome
    }

    /// The key itself, already ranked against the names the frame holds: what
    /// the frame sees once `key` has put the list back in step with the row.
    fn pressed(&mut self, key: KeyEvent, served: &Served) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if !ctrl && let Some(outcome) = self.fold_key(key.code) {
            return outcome;
        }
        // The headers row's own fold takes its keys before the modal sees one: a
        // table is a list of lines, and `a`, `d` and `Ctrl-U` there are a line's
        // business rather than a character's.
        if let Some(outcome) = headers::key(self, key) {
            return outcome;
        }
        match key.code {
            KeyCode::Esc => Outcome::Discard,
            KeyCode::Enter => match self.open_fold(served) {
                true => Outcome::Staying,
                false => Outcome::Save,
            },
            KeyCode::Up | KeyCode::BackTab if self.step_match(true) => Outcome::Staying,
            KeyCode::Down if self.step_match(false) => Outcome::Staying,
            KeyCode::Tab if self.take_match(served) => Outcome::Staying,
            KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
                if self.slide(key.code) =>
            {
                Outcome::Staying
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.focus(false);
                Outcome::Staying
            }
            KeyCode::Down | KeyCode::Tab => {
                self.focus(true);
                Outcome::Staying
            }
            KeyCode::Char('q') if ctrl => Outcome::Quit,
            KeyCode::Char('c') if ctrl => Outcome::CancelRun,
            KeyCode::Char('u') if ctrl => {
                self.clear();
                Outcome::Staying
            }
            KeyCode::Backspace => {
                self.backspace();
                Outcome::Staying
            }
            KeyCode::Delete => {
                self.delete();
                Outcome::Staying
            }
            KeyCode::Char(' ') if self.flip() => Outcome::Staying,
            KeyCode::Char(ch) if !ctrl => {
                self.insert(ch);
                Outcome::Staying
            }
            // A key no field and no row of the modal takes: nothing of the
            // frame moved with it.
            _ => Outcome::Held,
        }
    }

    /// The model row's completion: the names the fragment typed there reaches.
    pub fn list(&self) -> &List {
        &self.list
    }

    /// The list the fragment names now, ranked off the words the row with the
    /// cursor offers. The frame calls it when its facts change — a listing
    /// landing, the modal opening, a wire chosen — and `key` calls it before
    /// every keystroke, so what is drawn and what a `Tab` takes are read off the
    /// same words.
    pub(crate) fn refresh(&mut self, served: &Served) {
        let key = self.chosen_key();
        let typed = text_of(&self.fields, key);
        let offered = offered(key, served, &self.wires);
        self.list.refresh(offered, typed);
    }

    /// A key while the words stand open under the row with the cursor: `↑↓` move
    /// inside them, `Enter` takes the word the fold is on and shuts it, and `Esc`
    /// shuts it and keeps the draft — the discarding `Esc` is the modal's, and it
    /// is the one after this. `None` is every other key, which the modal takes as
    /// it always did.
    fn fold_key(&mut self, code: KeyCode) -> Option<Outcome> {
        if !self.list.folded() || !self.offering() {
            return None;
        }
        match code {
            KeyCode::Esc => {
                self.list.close();
                Some(Outcome::Staying)
            }
            // A fold nothing stands under holds no name to take, so this is the
            // row's own save again: a name the list does not hold still saves.
            KeyCode::Enter => Some(match self.take_folded() {
                true => Outcome::Staying,
                false => Outcome::Save,
            }),
            KeyCode::Up | KeyCode::BackTab => {
                self.list.step(true);
                Some(Outcome::Staying)
            }
            KeyCode::Down => {
                self.list.step(false);
                Some(Outcome::Staying)
            }
            _ => None,
        }
    }

    /// `Enter` on a row that offers words while they stand shut: the file's own
    /// value in a row nobody has typed in asks to be chosen from, so the words
    /// open under it — the names the wire serves under the model row, the wires
    /// the run parses under the provider row. A row someone has typed in is
    /// their draft and `Enter` writes it, and a row whose offer is empty — a
    /// wire that named nothing — is left the field it has always been: there is
    /// nothing to fold out, and nothing to open on.
    fn open_fold(&mut self, served: &Served) -> bool {
        let untouched = self
            .fields
            .get(self.chosen)
            .is_some_and(|field| field.edit == Edit::Untouched);
        if !untouched {
            return false;
        }
        // The headers row folds the table itself open rather than words: there
        // is always something to open, because a table holding nothing still has
        // the blank line the first header is typed on.
        if self.chosen_key() == Some(Key::ExtraHeaders) {
            if let Some(field) = self.fields.get_mut(self.chosen) {
                field.onto(0);
            }
            self.headers = Fold::Choosing;
            return true;
        }
        let key = self.chosen_key();
        let offered = offered(key, served, &self.wires);
        if offered.is_empty() {
            return false;
        }
        let current = text_of(&self.fields, key).to_string();
        self.list.open(offered, &current);
        true
    }

    /// `Enter` while the names stand open: the one the fold is on, in the field,
    /// and the fold shut — the row is the draft the save writes from then on.
    fn take_folded(&mut self) -> bool {
        let Some(name) = self.list.take().map(str::to_owned) else {
            return false;
        };
        self.list.close();
        self.edit(Edit::Value, |field| field.set(name));
        true
    }

    /// `↑` or `↓`: the matches step, and the cursor stays on the row whose field
    /// is being typed. A step with no match left to move to is the cursor's, so
    /// a row holding a list is still a row the arrows walk past — and a fragment
    /// nothing reaches has no list at all.
    fn step_match(&mut self, up: bool) -> bool {
        self.offering() && self.list.step(up)
    }

    /// `Tab`: the match the list is on, in the field. It is written on save
    /// like a typed name, because the row's text is the value and the list is
    /// only what the fragment reaches. `→` is the cursor's, so a row with names
    /// under it keeps its way to the cell the next character lands on.
    fn take_match(&mut self, served: &Served) -> bool {
        if !self.offering() {
            return false;
        }
        let Some(name) = self.list.take().map(str::to_owned) else {
            return false;
        };
        self.list.close();
        self.edit(Edit::Value, |field| field.set(name));
        self.refresh(served);
        true
    }

    /// Whether the row with the cursor offers words to choose from: the model
    /// row the names the wire serves, the provider row the wires the run
    /// parses. The list stands under that row, the keys walk it there, and the
    /// arrows are the list's only while it holds something.
    pub(crate) fn offering(&self) -> bool {
        matches!(self.chosen_key(), Some(Key::Model | Key::Provider))
    }

    /// The key of the row with the cursor.
    pub(crate) fn chosen_key(&self) -> Option<Key> {
        self.fields.get(self.chosen).map(|field| field.key)
    }

    /// The field of a key's row.
    fn field(&self, key: Key) -> Option<&Field> {
        self.fields.iter().find(|field| field.key == key)
    }

    /// The wire the provider row names, where it names one the run accepts:
    /// what the names under the model row answer for, and what a save would
    /// write. `None` while the row is empty or holds a word the run refuses,
    /// which is a word no listing is asked for.
    fn wire(&self) -> Option<Provider> {
        Provider::parse(&self.field(Key::Provider)?.text).ok()
    }

    /// The address this modal's rows state, where they state one: the address
    /// the names under the model row are asked at, and the one a save would
    /// write. A row nobody has typed in leaves `launch` speaking — the address
    /// a run would use comes from the file or the environment, and a value in
    /// the environment is no row of this modal's — a row typed in states the
    /// text in it, and a row `Ctrl-U` emptied states no address at all, which
    /// is the wire's own default rather than the one the launch left behind.
    pub(crate) fn address(&self, launch: Option<String>) -> Option<String> {
        match self.field(Key::BaseUrl) {
            Some(field) if field.edit != Edit::Untouched => {
                let text = field.text.trim();
                (!text.is_empty()).then(|| text.to_string())
            }
            _ => launch,
        }
    }

    pub fn focus(&mut self, down: bool) {
        self.chosen = crate::overlay::step(self.chosen, 1, self.fields.len(), !down);
        // The table is the headers row's, so walking off it shuts the fold and
        // keeps the draft: the keys the row has outside it are the modal's again.
        if self.chosen_key() != Some(Key::ExtraHeaders) {
            self.headers = Fold::Shut;
        }
    }

    /// A screenful at a time: what `PgUp`/`PgDn` and the wheel move the cursor
    /// by in a modal whose rows do not all fit. A page off a row that offers
    /// words shuts the fold it stands on: the words are that row's, and a page
    /// is a key the fold never took.
    pub fn page(&mut self, rows: usize, up: bool) {
        self.chosen = crate::overlay::step(self.chosen, rows, self.fields.len(), up);
        if !self.offering() {
            self.list.close();
        }
        if self.chosen_key() != Some(Key::ExtraHeaders) {
            self.headers = Fold::Shut;
        }
    }

    pub fn insert(&mut self, ch: char) {
        self.edit(Edit::Value, |field| field.insert(ch));
    }

    /// The cursor keys: `←→`, `Home` and `End` move the cursor inside the row
    /// it is on and never between rows, because `↑↓`, `Tab` and `BackTab` are
    /// what walk those. A move is no edit — a row nobody has typed in still
    /// writes nothing on save — so the field's own `Edit` is left as it was.
    /// The answer is whether the key was one of the four.
    pub fn slide(&mut self, code: KeyCode) -> bool {
        let Some(field) = self.fields.get_mut(self.chosen) else {
            return false;
        };
        match code {
            KeyCode::Left => field.left(),
            KeyCode::Right => field.right(),
            KeyCode::Home => field.home(),
            KeyCode::End => field.end(),
            _ => return false,
        }
        true
    }

    /// `Delete`: the character the cursor stands on, removed and the value
    /// closed over it. The cursor stays on the cell it was on, which now holds
    /// what followed — the mirror of `Backspace`, which leaves it on the cell
    /// the removed one stood on.
    pub fn delete(&mut self) {
        self.edit(Edit::Value, |field| {
            field.delete();
        });
    }

    /// A paste into the row with the cursor: the clipboard's text as one edit
    /// of that field, and never a keystroke — the row is the field, so a paste
    /// that a run would be asked on is a value typed one letter at a time and
    /// not a key a row holds nothing for. It is ranked again once it has
    /// landed, so the names under the model row are the fragment's the frame
    /// after it, exactly as `key` leaves them. A row of lines keeps the
    /// clipboard's breaks, because there one line is one header and dropping
    /// them would paste two headers as one value.
    pub fn paste(&mut self, text: &str, served: &Served) -> bool {
        let at_end = self.fields.get(self.chosen).is_some_and(Field::at_end);
        let text = match self.chosen_key().is_some_and(Key::lines) {
            true => whole(text, at_end),
            false => one_line(text),
        };
        if text.is_empty() {
            return false;
        }
        self.edit(Edit::Value, |field| field.insert_str(&text));
        self.refresh(served);
        true
    }

    pub fn backspace(&mut self) {
        self.edit(Edit::Value, |field| {
            field.backspace();
        });
    }

    /// Ctrl-U: the field with the cursor, emptied — and the key unset, so the
    /// environment or the default speaks instead. For the writable paths the
    /// default is the shipped floor; the other way to empty the field, one
    /// character at a time, writes the empty list, which is a value.
    pub fn clear(&mut self) {
        self.edit(Edit::Unset, |field| field.clear());
    }

    /// The table the extra headers row's fold draws: the row with the cursor,
    /// because the fold only ever stands open on that one.
    fn table(&mut self) -> Option<&mut Field> {
        self.fields.get_mut(self.chosen)
    }

    /// The line of that row the fold's cursor is on: the row's own cursor, read
    /// where the fold is about to move it.
    fn table_line(&self) -> Option<usize> {
        self.fields.get(self.chosen).map(Field::line)
    }

    /// One line of the table changed, and the row a draft from the moment the
    /// text moved and not before: a fold opened and shut again writes nothing,
    /// and neither does a blank line added to a table that already ended in one.
    /// A line's own check is not remembered here: `table_refusal` asks the run's
    /// check of the text as it stands on the frame that draws it, so a refusal
    /// cannot outlive the line that earned it.
    fn table_edit(&mut self, change: impl FnOnce(&mut Field)) {
        let Some(field) = self.fields.get_mut(self.chosen) else {
            return;
        };
        let before = field.text.clone();
        change(field);
        if field.text == before {
            return;
        }
        field.edit = Edit::Value;
        self.error = None;
    }

    /// A keystroke in the field with the cursor, and what a save makes of it.
    fn edit(&mut self, edit: Edit, change: impl FnOnce(&mut Field)) {
        if let Some(field) = self.fields.get_mut(self.chosen) {
            change(field);
            field.edit = edit;
            self.error = None;
        }
    }

    pub fn flip(&mut self) -> bool {
        let on = match self.fields.get(self.chosen) {
            Some(field) if field.key.toggled() => toggle_shown(&field.text) == "on",
            _ => return false,
        };
        let text = if on { "false" } else { "true" }.to_string();
        self.edit(Edit::Value, |field| field.set(text));
        true
    }

    pub fn save(&mut self) -> Result<(), String> {
        // Staged on a copy: a document that refuses one value must not be
        // left holding the ones before it.
        let mut staged = self.document.clone();
        for field in self
            .fields
            .iter()
            .filter(|field| field.edit != Edit::Untouched)
        {
            if field.edit == Edit::Unset {
                staged.unset(field.key);
                continue;
            }
            staged
                .set(field.key, &field.text)
                .map_err(|error| format!("{}: {}", field.key.name(), error.message()))?;
        }
        self.document = commit(staged)?;
        self.error = None;
        Ok(())
    }

    pub fn failed(&mut self, message: String) {
        self.error = Some(message);
    }

    pub fn document(&self) -> &Document {
        &self.document
    }
}

/// Whether this keystroke only drafts the address: a character typed into the
/// base URL row, taken back on either side of the cursor, or the row emptied.
/// A draft asks nothing, and the listing on hand stands until the cursor leaves
/// the row it belongs to.
fn drafting(settings: &Settings, key: KeyEvent) -> bool {
    let typing = matches!(
        key.code,
        KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
    );
    typing && settings.chosen_key() == Some(Key::BaseUrl)
}

/// The fragment typed in a row: the list under a row is ranked off that row's
/// own text and never off another row's, because the fragment is what a `Tab`
/// there would take.
fn text_of(fields: &[Field], key: Option<Key>) -> &str {
    fields
        .iter()
        .find(|field| Some(field.key) == key)
        .map_or("", |field| field.text.as_str())
}

/// The words a row offers when it is asked to choose: the names the wire serves
/// on the model row, the wires the run parses on the provider row, and nothing
/// at all on a row that is a field and nothing else.
fn offered<'a>(key: Option<Key>, served: &'a Served, wires: &'a [String]) -> &'a [String] {
    match key {
        Some(Key::Model) => served.names(),
        Some(Key::Provider) => wires,
        _ => &[],
    }
}

impl crate::app::App {
    /// A key while the settings modal has the screen, and whether the modal
    /// took it: a key it stays on and does nothing with leaves the frame as it
    /// was.
    pub(crate) fn settings_key(&mut self, key: KeyEvent) -> bool {
        let Some(Overlay::Settings(settings)) = self.overlay.as_mut() else {
            return false;
        };
        let outcome = settings.key(key, &self.served);
        // The wire the provider row names and the address the base URL row
        // states are what the names under the model row answer for, so a key
        // that moved either asks again: an ask is a `Serving`, the wire and the
        // address together, and the listing for the pair the user has left is
        // dropped by the rule already in place. An address still being typed is
        // asked once it is whole, because one request per letter of a draft is a
        // request for an address nobody has finished.
        let serving = serving_of(settings, self.rail.launch());
        if !drafting(settings, key) && self.served.want(serving) {
            settings.refresh(&self.served);
        }
        match outcome {
            Outcome::Staying => true,
            Outcome::Held => false,
            Outcome::Discard => {
                self.overlay = None;
                true
            }
            Outcome::Save => {
                self.save_settings();
                true
            }
            Outcome::Quit => {
                self.quit = true;
                true
            }
            Outcome::CancelRun => self.cancel_or_clear(),
        }
    }

    pub(crate) fn save_settings(&mut self) {
        let Some(Overlay::Settings(settings)) = self.overlay.as_mut() else {
            return;
        };
        match settings.save() {
            Ok(()) => {
                let path = settings.path().display().to_string();
                // The file is read back as what a run would read it as, so a
                // row that was absent and now is not stops being said — and a
                // row that is still absent keeps being said.
                let still = settings
                    .missing()
                    .and_then(|missing| missing.satisfied(settings.document()));
                self.overlay = None;
                self.missing = still;
                self.note_info(format!(
                    "settings saved to {path}; the next prompt uses them"
                ));
                self.reload = true;
            }
            Err(message) => settings.failed(message),
        }
    }

    /// `/settings`, or — with what the front door found absent — the modal it
    /// opens by itself: the same modal, on the row the run has nothing for, and
    /// saying why it is there.
    pub(crate) fn open_settings(&mut self, missing: Option<Missing>) {
        let opened = match &self.config {
            Some(path) => Settings::at_path(path),
            None => Settings::open(),
        };
        match opened {
            Ok(settings) => {
                let mut settings = match missing {
                    Some(missing) => settings.staged(missing),
                    None => settings,
                };
                self.ask_wire(&settings);
                settings.refresh(&self.served);
                self.show(Overlay::Settings(Box::new(settings)));
            }
            Err(message) => self.note_error(format!("settings: {message}")),
        }
    }

    /// The ask the modal makes as it opens: the names the wire its provider row
    /// names serves. A modal reopened on the same wire and base URL keeps the
    /// answer it already has — asking again is a keystroke of its own — and one
    /// reopened on another drops it and asks for the wire it now speaks.
    fn ask_wire(&mut self, settings: &Settings) {
        self.served.want(serving_of(settings, self.rail.launch()));
    }
}

/// The ask a modal makes: the wire its provider row names — or the wire the
/// launch speaks while the row is empty or holds a word the run refuses — at
/// the address its base URL row states, or the launch's while that row is one
/// nobody has typed in, because a listing answers for one wire at one address
/// and the two travel as one `Serving`.
fn serving_of(settings: &Settings, launch: &Launch) -> Serving {
    let wire = settings
        .wire()
        .map_or_else(|| launch.provider.clone(), |wire| wire.as_str().to_string());
    Serving::of(wire, settings.address(launch.base_url.clone()))
}
