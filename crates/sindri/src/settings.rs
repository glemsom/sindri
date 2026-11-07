//! The settings screen: the config file's keys as one list, the value each of them holds
//! as the file and the environment resolved it, and a visit that writes only what a reader
//! touched (§12, ADR-0009).
//!
//! Nothing here keeps a schema of its own. The keys are `Config::KEYS`, the toggles are
//! `Config::TOGGLES`, the text a row starts holding is `Config::text`, and what `Enter`
//! hands out is a `Touched` — the keys a reader wrote into, with the text each was left
//! holding. The write is the core's (`Config::save_touched`), which parses that text,
//! refuses a value that will not do while naming its key, and patches the file in place
//! through one owner-only rename. So this module decides what a key does to a row and
//! nothing about what a file is, and the frame that paints the rows is `view`'s — the same
//! split the picker makes (ADR-0005).
//!
//! Two rules are worth saying out loud, because both are about a row rather than a file:
//!
//! - **An untouched row is never written.** A row is seeded with the value in force, which
//!   may have come from the environment or a flag, so writing it back would move a value
//!   nobody typed into the file and change which layer speaks. Only a row a reader wrote
//!   into — a glyph, a `Ctrl-U`, a `Space` — is in the patch, which is what "only touched
//!   fields are written" costs as code.
//! - **A secret is a mask, not a key.** The `api_key` row shows `Config::text`'s mask while
//!   the file holds a key, and a row holding the mask is a row nobody has written into yet:
//!   the first glyph or backspace replaces the mask rather than appending to it, so a mask
//!   can never be saved as a key (§11, ADR-0011).

use sindri_core::config::{Config, ConfigError, Environment, Flags, Touched};
use sindri_core::redact::MASK;

use crate::composer::Key;

/// The keys of the config file as the rows of one screen, the row a key would act on, and
/// the reason the last write did not land.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// One row per key of the schema, in the order the schema names them.
    rows: Vec<Row>,
    /// Which row a typed glyph, `Ctrl-U` and `Space` act on.
    current: usize,
    /// What the last write was refused for, if it was: the reason, naming the key, that the
    /// screen says under its rows until the reader writes something else.
    refusal: Option<String>,
}

/// One key of the schema and the text the row holds for it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    /// The key as the file spells it.
    key: &'static str,
    /// What the row holds: the value in force until a reader writes into it, the mask while
    /// a secret is set, and whatever the reader has typed since.
    text: String,
    /// Whether this row is in the patch. A visit that read the screen and typed nothing is
    /// a visit that writes nothing, however many rows it looked at.
    touched: bool,
}

impl Settings {
    /// The screen a reader opens on the config as it resolved: every key in force, from
    /// whichever layer spoke last, so the list is the truth rather than the file's half of
    /// it.
    pub fn of(config: &Config) -> Settings {
        Settings {
            rows: Config::KEYS
                .iter()
                .map(|key| Row {
                    key,
                    text: config.text(key),
                    touched: false,
                })
                .collect(),
            current: 0,
            refusal: None,
        }
    }

    /// The rows as the screen paints them: the key as the file spells it, and the text it
    /// holds.
    pub fn rows(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.rows.iter().map(|row| (row.key, row.text.as_str()))
    }

    /// Which row the keys act on.
    pub fn current(&self) -> usize {
        self.current
    }

    /// Why the last write was refused, if it was: the reason a reader has to see, naming
    /// the key that has to change (§11).
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// What `Enter` hands the one save: the rows a reader wrote into, and no others.
    pub fn touched(&self) -> Touched {
        let mut touched = Touched::new();
        for row in self.rows.iter().filter(|row| row.touched) {
            touched.set(row.key, row.text.as_str());
        }
        touched
    }

    /// Records why the write this screen asked for did not land. The rows are left as the
    /// reader typed them, because the value that has to change is still on the screen.
    pub fn refuse(&mut self, reason: &str) {
        self.refusal = Some(reason.to_owned());
    }

    /// `↓` and `Tab`: the row below, and the first row below the last. The list is a ring
    /// rather than a wall, because a screen holding every key of a file is a list a reader
    /// walks to its end and back.
    pub fn down(&mut self) -> bool {
        if self.rows.is_empty() {
            return false;
        }
        self.current = (self.current + 1) % self.rows.len();
        true
    }

    /// `↑` and `Shift+Tab`: the row above, and the last row above the first.
    pub fn up(&mut self) -> bool {
        if self.rows.is_empty() {
            return false;
        }
        self.current = (self.current + self.rows.len() - 1) % self.rows.len();
        true
    }

    /// `Ctrl-U`: the row's value goes, and an empty value is a key the save unsets — which
    /// is the whole point of emptying one, so the layer below speaks again (§11).
    ///
    /// A row already empty is a row with nothing to empty: the key is not pressed for
    /// nothing, and nothing is repainted.
    pub fn clear(&mut self) -> bool {
        if !self.has_row() {
            return false;
        }
        let row = &mut self.rows[self.current];
        if row.text.is_empty() {
            return false;
        }
        row.text.clear();
        row.touched = true;
        self.refusal = None;
        true
    }

    /// `Space`: flips a toggle, and is a glyph everywhere else — because `thinking` is a
    /// switch and a path or a header is text, and a screen that could not hold a space in a
    /// path would be a screen that cannot name one.
    ///
    /// A toggle with no value flips on: unset is a state of its own and `true` is what a
    /// reader pressing a switch that is neither on nor off means. `Ctrl-U` is how it goes
    /// back to unset.
    pub fn space(&mut self) -> bool {
        if !self.has_row() {
            return false;
        }
        if !Config::TOGGLES.contains(&self.rows[self.current].key) {
            return self.write(' ');
        }
        let row = &mut self.rows[self.current];
        row.text = match row.text.as_str() {
            "true" => String::from("false"),
            _ => String::from("true"),
        };
        row.touched = true;
        self.refusal = None;
        true
    }

    /// The screen's own map, which is the glyph floor's: a glyph widens the row and a
    /// backspace takes one back. A value is typed rather than edited, so there is no caret
    /// here to move and the arrows are the list's.
    pub fn key(&mut self, key: Key) -> bool {
        if !self.has_row() {
            return false;
        }
        match key {
            Key::Char(glyph) if !glyph.is_control() => self.write(glyph),
            Key::Backspace => self.backspace(),
            _ => false,
        }
    }

    /// Whether there is a row for a key to act on at all: a screen a frame never opened is a
    /// screen with no rows, and a key pressed at one is not a key that changes anything.
    fn has_row(&self) -> bool {
        self.rows.get(self.current).is_some()
    }

    /// Writes `glyph` at the end of the row the keys act on.
    fn write(&mut self, glyph: char) -> bool {
        self.unmask();
        let row = &mut self.rows[self.current];
        row.text.push(glyph);
        row.touched = true;
        self.refusal = None;
        true
    }

    /// A backspace erases the glyph before the end of the row. On a row holding the mask the
    /// whole mask goes: the secret is not something this screen may hold even for one
    /// keystroke, so a backspace there unsets the row rather than nibbling a key nobody can
    /// see.
    fn backspace(&mut self) -> bool {
        let masked = self.unmask();
        let erased = self.rows[self.current].text.pop().is_some() || masked;
        if erased {
            self.rows[self.current].touched = true;
            self.refusal = None;
        }
        erased
    }

    /// Takes the mask off the row the keys act on, saying whether it was wearing one: a row
    /// holding the mask holds what the file already has and not what a reader typed, so the
    /// first glyph replaces the mask rather than being appended to it.
    fn unmask(&mut self) -> bool {
        let row = &mut self.rows[self.current];
        let masked = row.text == MASK;
        if masked {
            row.text.clear();
        }
        masked
    }
}

/// Writes what a settings modal touched, and hands back the config the next prompt runs on:
/// the core's one save (owner-only, atomic, only the touched keys) and then the whole
/// precedence resolved again — file, then environment, then the launch's own flags — so a
/// prompt sent after a visit runs on the values the visit left rather than the ones the
/// launch was handed (§11, ADR-0009).
///
/// A save that is refused returns the refusal and no config: a bad value is never armed for
/// the next run, and nothing was written (§11).
pub fn persist(
    environment: &impl Environment,
    touched: &Touched,
    flags: Flags,
) -> Result<Config, ConfigError> {
    Config::save_touched(environment, touched)?;
    Config::resolve(environment, flags)
}

#[cfg(test)]
mod tests {
    //! The screen's own rules, one test each: the rows the schema gives it, the value a row
    //! starts holding, the three keys that change a value, the two that walk the list, and
    //! the one thing `Enter` may hand out — the rows a reader wrote into and no others. None
    //! of them opens a file: what a save makes of a patch is `config`'s, and the tests in
    //! `tests/settings.rs` are where the two meet.

    use super::*;

    /// The config a screen opens on: two keys decided, the way a file or the environment
    /// would have left them, and a secret among them.
    fn decided() -> Config {
        Config {
            model: Some(String::from("claude-sonnet-4-5")),
            thinking: Some(true),
            api_key: Some(String::from("sk-not-a-real-key")),
            ..Config::default()
        }
    }

    /// The text the screen holds for `key`.
    fn shown(settings: &Settings, key: &str) -> String {
        settings
            .rows()
            .find(|(name, _)| *name == key)
            .map(|(_, text)| text.to_owned())
            .unwrap_or_else(|| panic!("the screen has a row for {key}"))
    }

    /// The screen with `key`'s row under the keys, walked to as a reader walks it.
    fn at(settings: &mut Settings, key: &str) {
        for _ in 0..settings.rows().count() {
            if settings
                .rows()
                .nth(settings.current())
                .map(|(name, _)| name)
                == Some(key)
            {
                return;
            }
            settings.down();
        }
        panic!("{key} is a key of the schema, so the screen has a row for it");
    }

    #[test]
    fn the_screen_walks_every_key_the_schema_has_in_the_order_the_schema_names_them() {
        let settings = Settings::of(&Config::default());
        let rows: Vec<&str> = settings.rows().map(|(key, _)| key).collect();

        assert_eq!(
            rows,
            Config::KEYS.to_vec(),
            "the list is the schema's own keys, so a key the schema gains is a row here with \
             nothing to remember"
        );
    }

    #[test]
    fn a_row_starts_holding_the_value_in_force_whichever_layer_spoke_it() {
        let settings = Settings::of(&decided());

        assert_eq!(shown(&settings, "model"), "claude-sonnet-4-5");
        assert_eq!(
            shown(&settings, "thinking"),
            "true",
            "a toggle reads as the word it will be written as"
        );
        assert_eq!(
            shown(&settings, "timeout"),
            "",
            "a key nothing speaks for is a row with nothing in it, because unset is a state"
        );
    }

    #[test]
    fn a_secret_is_a_mask_and_the_first_glyph_replaces_it_rather_than_piling_onto_it() {
        let mut settings = Settings::of(&decided());
        at(&mut settings, "api_key");
        assert_eq!(
            shown(&settings, "api_key"),
            MASK,
            "a frame may not hold a secret, so the row says a key is set and not which"
        );

        settings.key(Key::Char('n'));

        assert_eq!(
            shown(&settings, "api_key"),
            "n",
            "the mask is what the file holds, not what the reader typed, so a glyph \
             replaces it: a mask saved as a key would be a key of bullets"
        );
    }

    #[test]
    fn a_screen_no_frame_opened_holds_no_row_and_no_key_changes_one() {
        let mut settings = Settings::default();

        assert!(!settings.clear());
        assert!(!settings.space());
        assert!(!settings.key(Key::Char('x')));
        assert!(!settings.down());
        assert!(
            settings.touched().is_empty(),
            "a visit to a screen that was never opened is a file with nothing to write"
        );
    }

    #[test]
    fn the_list_is_a_ring_so_the_last_key_is_one_press_from_the_first() {
        let mut settings = Settings::of(&Config::default());

        assert_eq!(settings.current(), 0, "a screen opens on its first key");
        settings.up();
        assert_eq!(
            settings.current(),
            Config::KEYS.len() - 1,
            "the row above the first is the last, because a list of fourteen keys is walked \
             rather than climbed"
        );
        settings.down();
        assert_eq!(settings.current(), 0);
    }

    #[test]
    fn ctrl_u_empties_the_value_of_the_row_a_key_acts_on() {
        let mut settings = Settings::of(&decided());
        at(&mut settings, "model");

        assert!(settings.clear());
        assert_eq!(
            shown(&settings, "model"),
            "",
            "an emptied value is a key handed back to the environment, so the row that held \
             the file's model holds nothing"
        );
        assert_eq!(
            settings.touched().rows().collect::<Vec<_>>(),
            vec![("model", "")],
            "and it is the one row the visit will write"
        );
    }

    #[test]
    fn ctrl_u_on_a_row_already_empty_is_not_a_key_that_repaints() {
        let mut settings = Settings::of(&Config::default());
        at(&mut settings, "timeout");

        assert!(!settings.clear());
        assert!(
            settings.touched().is_empty(),
            "there was nothing to empty, so there is nothing to write"
        );
    }

    #[test]
    fn space_flips_a_toggle_from_unset_to_on_and_back_off_again() {
        let mut settings = Settings::of(&Config::default());
        at(&mut settings, "ssh_agent");

        settings.space();
        assert_eq!(
            shown(&settings, "ssh_agent"),
            "true",
            "a toggle neither on nor off is what a reader pressing the switch means"
        );
        settings.space();
        assert_eq!(shown(&settings, "ssh_agent"), "false");
        assert_eq!(
            settings.touched().rows().collect::<Vec<_>>(),
            vec![("ssh_agent", "false")]
        );
    }

    #[test]
    fn space_on_a_key_that_is_not_a_toggle_is_a_glyph() {
        let mut settings = Settings::of(&Config::default());
        at(&mut settings, "extra_writable_paths");
        settings.key(Key::Char('/'));
        settings.key(Key::Char('a'));

        settings.space();
        settings.key(Key::Char('b'));

        assert_eq!(
            shown(&settings, "extra_writable_paths"),
            format!("/a{}b", ' '),
            "a path may hold a space, and a screen that could not type one could not name a \
             path: only the schema's three toggles flip"
        );
    }

    #[test]
    fn a_glyph_lands_in_the_row_the_keys_act_on_and_leaves_every_other_row_alone() {
        let mut settings = Settings::of(&Config::default());
        at(&mut settings, "model");
        settings.key(Key::Char('a'));
        settings.key(Key::Char('b'));
        settings.key(Key::Backspace);
        at(&mut settings, "max_turns");
        settings.key(Key::Char('4'));

        assert_eq!(shown(&settings, "model"), "a");
        assert_eq!(
            settings.touched().rows().collect::<Vec<_>>(),
            vec![("max_turns", "4"), ("model", "a")],
            "the patch is the rows a reader wrote into, in the schema's order"
        );
        assert_eq!(
            shown(&settings, "base_url"),
            "",
            "a row nobody reached is a row with nothing in it"
        );
    }

    #[test]
    fn a_backspace_erases_the_last_glyph_and_takes_a_whole_mask_with_it() {
        let mut settings = Settings::of(&decided());
        at(&mut settings, "model");
        settings.key(Key::Backspace);
        assert_eq!(shown(&settings, "model"), "claude-sonnet-4-");

        at(&mut settings, "api_key");
        settings.key(Key::Backspace);
        assert_eq!(
            shown(&settings, "api_key"),
            "",
            "a secret is not something a screen may hold even between two keystrokes, so a \
             backspace on the mask unsets the row whole rather than nibbling it"
        );
    }

    #[test]
    fn a_refusal_is_said_until_the_reader_writes_something_else() {
        let mut settings = Settings::of(&decided());
        at(&mut settings, "max_turns");
        settings.key(Key::Char('x'));

        settings.refuse("bad value in max_turns: `x` will not parse");
        assert!(
            settings
                .refusal()
                .is_some_and(|reason| reason.contains("max_turns")),
            "a refusal names the key a reader has to go and fix"
        );

        settings.key(Key::Char('1'));
        assert_eq!(
            settings.refusal(),
            None,
            "the reason was about the value that was there, and it has been written over"
        );
    }
}
