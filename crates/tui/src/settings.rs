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
//! listing for the wire the user has left is dropped where it lands.

mod list;
mod served;

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::config::{Document, KEYS, Key, Provider, toggle_shown};

use crate::overlay::Overlay;
use crate::rail::Launch;

pub use list::List;
pub use served::{Served, Serving};

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
pub struct Field {
    pub key: Key,
    pub text: String,
    /// What a save makes of this field. An untouched field is written as
    /// nothing — a secret the file holds is never handed back, so writing one
    /// empty would erase it — and an empty field is two things rather than
    /// one: the value it does not hold, and the absence `Ctrl-U` makes of it.
    pub edit: Edit,
}

/// What one save does with one field, so two flags cannot disagree about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// No keystroke has been in this field: the file keeps what it had.
    Untouched,
    /// The field's text is the value, the empty one included — which for the
    /// writable paths is the explicit `[]`, and not the floor.
    Value,
    /// `Ctrl-U`: the key leaves the file, so the environment or the run's
    /// default speaks instead — the shipped floor, for the writable paths.
    Unset,
}

/// What the front door found absent when it opened the frame itself: the run
/// has no credential, or no model, or neither — and the words for that absence,
/// which the transcript, the composer and a refused prompt each read. It holds
/// no value: the modal says where a key comes from, never what one is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// The wire the run speaks: its variable is where the credential comes
    /// from, and a local server needs none.
    provider: Provider,
    /// Whether the credential is one of the absent rows.
    key: bool,
    /// Whether the model is.
    model: bool,
    /// What the transcript has the reader type: the row, and every variable
    /// that sets it.
    ways: String,
    /// The same variables in the room the composer has, one row: it leaves the
    /// model to `/settings`.
    composer: String,
    /// What `/settings` does for it: `saves one`, `names one`, `sets both`.
    acted: &'static str,
}

impl Missing {
    /// What a resolved config could not find, when it found all of it: `None`
    /// is a run that can send a prompt, and there is nothing to say. The two
    /// flags are the one `match` on an absence, and they are kept, so the words
    /// a reader sees and the rows they describe are read off the same pair.
    pub fn of(provider: Provider, key: bool, model: bool) -> Option<Missing> {
        if !key && !model {
            return None;
        }
        let key_vars = Self::key_vars_for(provider);
        let (ways, composer, acted) = match (key, model) {
            (true, false) => (
                format!("one in the settings modal, or set {key_vars}"),
                key_vars,
                "saves one",
            ),
            (false, true) => (
                "one in the settings modal, or set SINDRI_MODEL".to_string(),
                "SINDRI_MODEL".to_string(),
                "names one",
            ),
            _ => (
                format!("them in the settings modal, or set {key_vars} and SINDRI_MODEL"),
                key_vars,
                "sets both",
            ),
        };
        Some(Missing {
            provider,
            key,
            model,
            ways,
            composer,
            acted,
        })
    }

    /// The credential's two variables, in the wire's own terms: the one fact
    /// every sentence about a missing key has to say, spelled once.
    pub fn key_vars(&self) -> String {
        Self::key_vars_for(self.provider)
    }

    fn key_vars_for(provider: Provider) -> String {
        format!("SINDRI_API_KEY or {}", provider.api_key_env())
    }

    /// The absence in words: `no key`, `no model`, `no key and no model` —
    /// read off the two flags rather than stored beside them, so the words and
    /// the rows they describe cannot drift apart. `of` never builds the pair
    /// with neither, which is why the last arm is the both-absent one.
    fn what(&self) -> &'static str {
        match (self.key, self.model) {
            (true, false) => "no key",
            (false, true) => "no model",
            _ => "no key and no model",
        }
    }

    /// Whether the credential is one of the absent rows: what the modal's key
    /// row says, and what the composer cannot do.
    pub fn wants_key(&self) -> bool {
        self.key
    }

    /// The row the modal opens on: the key when there is none, because a run
    /// with no credential is the failure a frame answers with a field.
    pub fn row(&self) -> Key {
        if self.key { Key::ApiKey } else { Key::Model }
    }

    /// The transcript's one line: what is absent and every way to set it —
    /// said once, when the frame opens, and never on a frame.
    pub fn said(&self) -> String {
        format!("{}: type {}", self.what(), self.ways)
    }

    /// What the composer says where the prompt would be: a run with no key
    /// cannot send one, and the frame says why rather than pretending.
    pub fn composer(&self) -> String {
        format!(
            "{} · a prompt cannot be sent · /settings, or set {}",
            self.what(),
            self.composer
        )
    }

    /// A prompt sent anyway: the draft stays where it was, and this is the one
    /// line the transcript gets for the keystroke.
    pub fn refused(&self) -> String {
        format!(
            "not sent: this run has {} — /settings {}",
            self.what(),
            self.acted
        )
    }

    /// What is still absent once a save has landed: a row the file now carries
    /// is filled in, and the rest is not. A key in the environment is not this
    /// frame's to see, so the file is what a save can answer for.
    pub fn satisfied(&self, document: &Document) -> Option<Missing> {
        let key = self.key && !document.is_set(Key::ApiKey);
        let model = self.model && !document.is_set(Key::Model);
        Missing::of(self.provider, key, model)
    }
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
    error: Option<String>,
    missing: Option<Missing>,
}

impl Settings {
    pub fn open() -> Result<Settings, String> {
        match Document::load() {
            Ok(document) => Ok(Settings::at(document)),
            Err(error) => Err(error.message().to_string()),
        }
    }

    pub fn at_path(path: &Path) -> Result<Settings, String> {
        Document::load_at(path)
            .map(Settings::at)
            .map_err(|error| error.message().to_string())
    }

    pub fn at(document: Document) -> Settings {
        let fields = KEYS
            .iter()
            .map(|key| Field {
                key: *key,
                text: document.editable(*key),
                edit: Edit::Untouched,
            })
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
        match key.code {
            KeyCode::Esc => Outcome::Discard,
            KeyCode::Enter => match self.open_fold(served) {
                true => Outcome::Staying,
                false => Outcome::Save,
            },
            KeyCode::Up | KeyCode::BackTab if self.step_match(true) => Outcome::Staying,
            KeyCode::Down if self.step_match(false) => Outcome::Staying,
            KeyCode::Tab | KeyCode::Right if self.take_match(served) => Outcome::Staying,
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
        self.edit(Edit::Value, |field| field.text = name);
        true
    }

    /// `↑` or `↓`: the matches step, and the cursor stays on the row whose field
    /// is being typed. A step with no match left to move to is the cursor's, so
    /// a row holding a list is still a row the arrows walk past — and a fragment
    /// nothing reaches has no list at all.
    fn step_match(&mut self, up: bool) -> bool {
        self.offering() && self.list.step(up)
    }

    /// `Tab` or `→`: the match the list is on, in the field. It is written on
    /// save like a typed name, because the row's text is the value and the list
    /// is only what the fragment reaches.
    fn take_match(&mut self, served: &Served) -> bool {
        if !self.offering() {
            return false;
        }
        let Some(name) = self.list.take().map(str::to_owned) else {
            return false;
        };
        self.list.close();
        self.edit(Edit::Value, |field| field.text = name);
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
    fn chosen_key(&self) -> Option<Key> {
        self.fields.get(self.chosen).map(|field| field.key)
    }

    /// The wire the provider row names, where it names one the run accepts:
    /// what the names under the model row answer for, and what a save would
    /// write. `None` while the row is empty or holds a word the run refuses,
    /// which is a word no listing is asked for.
    fn wire(&self) -> Option<Provider> {
        let field = self
            .fields
            .iter()
            .find(|field| field.key == Key::Provider)?;
        Provider::parse(&field.text).ok()
    }

    pub fn focus(&mut self, down: bool) {
        self.chosen = crate::overlay::step(self.chosen, 1, self.fields.len(), !down);
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
    }

    pub fn insert(&mut self, ch: char) {
        self.edit(Edit::Value, |field| field.text.push(ch));
    }

    pub fn backspace(&mut self) {
        self.edit(Edit::Value, |field| {
            field.text.pop();
        });
    }

    /// Ctrl-U: the field with the cursor, emptied — and the key unset, so the
    /// environment or the default speaks instead. For the writable paths the
    /// default is the shipped floor; the other way to empty the field, one
    /// character at a time, writes the empty list, which is a value.
    pub fn clear(&mut self) {
        self.edit(Edit::Unset, |field| field.text.clear());
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
        self.edit(Edit::Value, |field| field.text = text);
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
        staged.save().map_err(|error| error.message().to_string())?;
        self.document = staged;
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
        // The wire the provider row names is the one the names under the model
        // row answer for, so a key that moved that row asks again: an ask is a
        // `Serving`, the wire and the address together, and the listing for the
        // wire the user has left is dropped by the rule already in place.
        let serving = serving_of(settings, self.rail.launch());
        if self.served.want(serving) {
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
/// the address the launch holds, because a listing answers for one wire at one
/// address and the two travel as one `Serving`.
fn serving_of(settings: &Settings, launch: &Launch) -> Serving {
    let wire = settings
        .wire()
        .map_or_else(|| launch.provider.clone(), |wire| wire.as_str().to_string());
    Serving::of(wire, launch.base_url.clone())
}
