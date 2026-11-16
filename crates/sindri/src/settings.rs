use sindri_core::config::{Config, ConfigError, Environment, Flags, Touched};
use sindri_core::redact::MASK;

use crate::composer::Key;
use crate::ink::Ink;
use crate::kept::{Kept, Move, Narrow};
use crate::list::{self, Beside, ScreenRow};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    rows: Vec<Row>,
    narrow: Narrow,
    refusal: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    key: &'static str,
    text: String,
    touched: bool,
}

impl Settings {
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
            narrow: Narrow::default(),
            refusal: None,
        }
    }

    pub fn rows(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.rows.iter().map(|row| (row.key, row.text.as_str()))
    }

    pub fn current(&self) -> usize {
        self.narrow.mark()
    }

    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// The rows the screen's card holds: the config's keys with the values the visit carries, the row
    /// the reader is on marked, and under them the refusal that turned the visit back. A value too
    /// wide for the frame is cut at its right edge rather than dropped, as the picker's paths are.
    pub fn body(&self, width: u16) -> Vec<ScreenRow> {
        let keys: Vec<(&str, &str)> = self.rows().collect();
        let mut rows = list::List {
            query: None,
            said: None,
            items: list::columned(keys.into_iter(), Ink::Muted, Beside::Row),
            selected: self.current(),
            marker: list::MARK,
            chooses: false,
        }
        .rows();
        if let Some(reason) = self.refusal() {
            rows.extend(list::refusal(reason, width));
        }
        rows
    }

    pub fn touched(&self) -> Touched {
        let mut touched = Touched::new();
        for row in self.rows.iter().filter(|row| row.touched) {
            touched.set(row.key, row.text.as_str());
        }
        touched
    }

    pub fn refuse(&mut self, reason: &str) {
        self.refusal = Some(reason.to_owned());
    }

    pub fn clear(&mut self) -> bool {
        if !self.has_row() {
            return false;
        }
        let row = self.standing();
        if row.text.is_empty() {
            return false;
        }
        row.text.clear();
        row.touched = true;
        self.refusal = None;
        true
    }

    pub fn space(&mut self) -> bool {
        if !self.has_row() {
            return false;
        }
        if !Config::TOGGLES.contains(&self.standing().key) {
            return self.write(' ');
        }
        let row = self.standing();
        row.text = match row.text.as_str() {
            "true" => String::from("false"),
            _ => String::from("true"),
        };
        row.touched = true;
        self.refusal = None;
        true
    }

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

    fn has_row(&self) -> bool {
        self.rows.get(self.current()).is_some()
    }

    /// The row the keys act on, which is the row the mark stands on.
    fn standing(&mut self) -> &mut Row {
        let at = self.narrow.mark();
        &mut self.rows[at]
    }

    fn write(&mut self, glyph: char) -> bool {
        self.unmask();
        let row = self.standing();
        row.text.push(glyph);
        row.touched = true;
        self.refusal = None;
        true
    }

    /// A backspace on a row holding the mask unsets it whole: the secret is not something
    /// this screen may hold even for one keystroke.
    fn backspace(&mut self) -> bool {
        let masked = self.unmask();
        let erased = self.standing().text.pop().is_some() || masked;
        if erased {
            self.standing().touched = true;
            self.refusal = None;
        }
        erased
    }

    /// The mask is what the file holds and not what a reader typed: the first glyph replaces
    /// it, and the row holds no value a save may write until one does.
    fn unmask(&mut self) -> bool {
        let row = self.standing();
        let masked = row.text == MASK;
        if masked {
            row.text.clear();
        }
        masked
    }
}

impl Kept for Settings {
    fn walked(&mut self, way: Move) -> bool {
        self.narrow.walked(way, self.rows.len())
    }

    fn typed(&mut self, key: Key) -> bool {
        self.key(key)
    }

    /// A visit's rows hang no choice, so no press is ever read as one.
    fn pressed(&mut self, _at: usize) -> bool {
        false
    }
}

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
    use super::*;

    fn decided() -> Config {
        Config {
            model: Some(String::from("claude-sonnet-4-5")),
            thinking: Some(true),
            api_key: Some(String::from("sk-not-a-real-key")),
            ..Config::default()
        }
    }

    fn shown(settings: &Settings, key: &str) -> String {
        settings
            .rows()
            .find(|(name, _)| *name == key)
            .map(|(_, text)| text.to_owned())
            .unwrap_or_else(|| panic!("the screen has a row for {key}"))
    }

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
            settings.walked(Move::Next);
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
        assert!(!settings.walked(Move::Next));
        assert!(
            settings.touched().is_empty(),
            "a visit to a screen that was never opened is a file with nothing to write"
        );
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
             path: only the schema's own toggles flip"
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
