use sindri_core::config::{self, Config, ConfigError, FieldKind, Touched, collection_text};
use sindri_core::redact::MASK;

use crate::composer::Key;
use crate::ink::{Faces, Ink, Look};
use crate::kept::{Kept, Move, Narrow};
use crate::list::{self, Beside, ScreenRow};
use crate::models::Models;
use crate::pen;

mod fields;
use fields::{FIELDS, grouped};
use sindri_core::provider::served::Listing;
use sindri_core::wire::Wire;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    rows: Vec<Row>,
    narrow: Narrow,
    refusal: Option<String>,
    editor: Option<Editor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Editor {
    Text {
        before: String,
        touched: bool,
    },
    Choices {
        values: Vec<&'static str>,
        narrow: Narrow,
    },
    Collection {
        before: String,
        touched: bool,
        items: Vec<String>,
        narrow: Narrow,
        input: Option<(Option<usize>, String, Option<String>)>,
    },
    Credential(Narrow),
    Model(Models),
}

#[derive(Clone, Debug)]
struct Row {
    key: &'static config::Field,
    text: String,
    touched: bool,
    before: String,
}

impl PartialEq for Row {
    fn eq(&self, other: &Row) -> bool {
        self.key.name == other.key.name
            && self.text == other.text
            && self.before == other.before
            && self.touched == other.touched
    }
}

impl Eq for Row {}

impl Settings {
    pub fn of(config: &Config) -> Settings {
        Settings {
            rows: FIELDS
                .iter()
                .map(|field| {
                    Config::field(field.key).expect("settings field belongs to the schema")
                })
                .map(|key| Row {
                    key,
                    text: key.shown(config),
                    before: key.shown(config),
                    touched: false,
                })
                .collect(),
            narrow: Narrow::default(),
            refusal: None,
            editor: None,
        }
    }

    pub fn refresh(&mut self) -> bool {
        if let Some(Editor::Model(models)) = &mut self.editor {
            *models = Models::new();
            true
        } else {
            false
        }
    }

    pub fn editing(&self) -> bool {
        self.editor.is_some()
    }

    pub fn listing(&mut self, listing: Listing) {
        let current = self
            .rows
            .iter()
            .find(|row| row.key.name == "model")
            .map(|row| row.text.clone());
        if let Some(Editor::Model(models)) = &mut self.editor {
            *models = std::mem::take(models).served_by(Some(listing));
            if let Some(at) = models
                .rows()
                .iter()
                .position(|(id, _)| Some(*id) == current.as_deref())
            {
                for _ in 0..at {
                    models.walked(Move::Next);
                }
            }
        }
    }

    pub fn draft(&self, base: &Config) -> Result<Config, ConfigError> {
        let mut draft = base.clone();
        for row in self.rows.iter().filter(|row| row.touched) {
            let value = if row.text.trim().is_empty() {
                Config::default()
            } else {
                row.key.read(row.text.trim())?
            };
            match row.key.name {
                "provider" => draft.provider = value.provider,
                "model" => draft.model = value.model,
                "route" => draft.route = value.route,
                "base_url" => draft.base_url = value.base_url,
                "api_key" => draft.api_key = value.api_key,
                _ => {}
            }
        }
        Ok(draft)
    }

    fn wire(&self) -> Option<Wire> {
        self.rows
            .iter()
            .find(|row| row.key.name == "provider")
            .and_then(|row| Wire::from_word(&row.text))
    }

    pub fn activate(&mut self) -> bool {
        if let Some(editor) = self.editor.take() {
            let value = match editor {
                Editor::Text { before, touched } => {
                    let row = self.standing();
                    let result = if row.text.trim().is_empty() {
                        Ok(Config::default())
                    } else {
                        row.key.read(row.text.trim())
                    };
                    match result {
                        Ok(config) => {
                            let value = if row.key.kind == FieldKind::Credential {
                                row.text.clone()
                            } else {
                                row.key.shown(&config)
                            };
                            self.replace(value);
                        }
                        Err(error) => {
                            self.refusal = Some(error.to_string());
                            self.editor = Some(Editor::Text { before, touched });
                        }
                    }
                    return false;
                }
                Editor::Credential(narrow) => {
                    if narrow.mark() == 1 {
                        self.replace(String::new());
                    } else {
                        let row = self.standing();
                        self.editor = Some(Editor::Text {
                            before: row.text.clone(),
                            touched: row.touched,
                        });
                        self.standing().text.clear();
                    }
                    return false;
                }
                Editor::Collection {
                    before,
                    touched,
                    mut items,
                    mut narrow,
                    mut input,
                } => {
                    if let Some((index, text, name)) = input.take() {
                        if self.standing().key.kind == FieldKind::Headers && name.is_none() {
                            input = Some((
                                index,
                                index
                                    .and_then(|i| {
                                        items[i]
                                            .split_once(':')
                                            .map(|(_, value)| value.trim().to_owned())
                                    })
                                    .unwrap_or_default(),
                                Some(text),
                            ));
                        } else {
                            let original = (index, text.clone(), name.clone());
                            let value = name.map(|name| format!("{name}: {text}")).unwrap_or(text);
                            let mut candidate = items.clone();
                            if let Some(index) = index {
                                candidate[index] = value;
                            } else {
                                candidate.push(value);
                            }
                            let text = collection_text(&candidate);
                            match self.standing().key.read(&text) {
                                Ok(_) => {
                                    items = candidate;
                                    self.replace(text);
                                }
                                Err(error) => {
                                    self.refusal = Some(error.to_string());
                                    input = Some(original);
                                }
                            }
                        }
                    } else {
                        let at = narrow.mark();
                        if at > items.len() * 2 {
                            return false;
                        }
                        if at == items.len() * 2 {
                            input = Some((None, String::new(), None));
                        } else if at % 2 == 1 {
                            items.remove(at / 2);
                            self.replace(collection_text(&items));
                            narrow.pressed(at.min(items.len() * 2), items.len() * 2 + 2);
                        } else if let Some(value) = items.get(at / 2) {
                            let text = if self.standing().key.kind == FieldKind::Headers {
                                value
                                    .split_once(':')
                                    .map(|(name, _)| name)
                                    .unwrap_or(value)
                                    .to_owned()
                            } else {
                                value.clone()
                            };
                            input = Some((Some(at / 2), text, None));
                        }
                    }
                    self.editor = Some(Editor::Collection {
                        before,
                        touched,
                        items,
                        narrow,
                        input,
                    });
                    return false;
                }
                Editor::Choices { values, narrow } => {
                    values.get(narrow.mark()).map(|v| (*v).to_owned())
                }
                Editor::Model(models) => models.chosen().map(str::to_owned),
            };
            if let Some(value) = value {
                self.replace(value);
            }
            return false;
        }
        let Some(row) = self.rows.get(self.current()) else {
            return false;
        };
        self.editor = if matches!(row.key.kind, FieldKind::Paths | FieldKind::Headers) {
            Some(Editor::Collection {
                before: row.text.clone(),
                touched: row.touched,
                items: row
                    .key
                    .read(&row.text)
                    .ok()
                    .map(|config| {
                        if row.key.kind == FieldKind::Headers {
                            config.extra_headers.unwrap_or_default()
                        } else {
                            config
                                .extra_writable_paths
                                .unwrap_or_default()
                                .iter()
                                .map(|path| path.display().to_string())
                                .collect()
                        }
                    })
                    .unwrap_or_default(),
                narrow: Narrow::default(),
                input: None,
            })
        } else if row.key.kind == FieldKind::Credential {
            Some(Editor::Credential(Narrow::default()))
        } else if row.key.name == "model" {
            Some(Editor::Model(Models::new()))
        } else if let Some(values) = row.key.choices(self.wire()) {
            if values.is_empty() {
                return false;
            }
            let mut narrow = Narrow::default();
            for _ in 0..values.iter().position(|v| *v == row.text).unwrap_or(0) {
                narrow.walked(Move::Next, values.len());
            }
            Some(Editor::Choices { values, narrow })
        } else {
            Some(Editor::Text {
                before: row.text.clone(),
                touched: row.touched,
            })
        };
        matches!(self.editor, Some(Editor::Model(_)))
    }

    fn replace(&mut self, value: String) {
        let row = self.standing();
        row.text = value;
        row.touched = true;
        self.refusal = None;
        if self.wire() == Some(Wire::Anthropic)
            && let Some(row) = self.rows.iter_mut().find(|row| row.key.name == "route")
            && !row.text.is_empty()
        {
            row.text.clear();
            row.touched = true;
        }
    }

    pub fn cancel_editor(&mut self) -> bool {
        let Some(editor) = self.editor.take() else {
            return false;
        };
        if let Editor::Text { before, touched }
        | Editor::Collection {
            before, touched, ..
        } = editor
        {
            self.standing().text = before;
            self.standing().touched = touched;
        }
        self.refusal = None;
        true
    }

    pub fn rows(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.rows
            .iter()
            .map(|row| (row.key.name, row.text.as_str()))
    }

    pub fn current(&self) -> usize {
        self.narrow.mark()
    }

    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    pub fn body(&self, width: u16) -> Vec<ScreenRow> {
        match &self.editor {
            Some(Editor::Model(models)) => {
                let mut rows = models.body();
                rows.extend(list::refusal(
                    "Ctrl-F: refresh; Ctrl-U: enter identifier manually; Esc: cancel",
                    width,
                ));
                if let Some(reason) = self.refusal() {
                    rows.extend(list::refusal(reason, width));
                }
                return rows;
            }
            Some(Editor::Credential(narrow)) => {
                return editor_menu(vec!["Replace".into(), "Remove".into()], narrow.mark());
            }
            Some(Editor::Collection {
                items,
                narrow,
                input,
                ..
            }) => {
                let mut rows = if let Some((_, text, name)) = input {
                    let label = if self.rows[self.current()].key.kind == FieldKind::Headers {
                        if name.is_some() {
                            "Header value"
                        } else {
                            "Header name"
                        }
                    } else {
                        "Path"
                    };
                    let mut row = list::whole(&format!("{label}: {text}"), Look::plain(Ink::Plain));
                    row.caret = Some(pen::width(&row.spans));
                    vec![row]
                } else {
                    let mut labels = Vec::new();
                    for item in items {
                        labels.push(format!("Edit {item}"));
                        labels.push(format!("Remove {item}"));
                    }
                    labels.push("Add".into());
                    labels.push("Done".into());
                    editor_menu(labels, narrow.mark())
                };
                rows.extend(list::refusal(
                    "Enter confirm · Esc restore · Ctrl-U clears input",
                    width,
                ));
                if let Some(reason) = self.refusal() {
                    rows.extend(list::refusal(reason, width));
                }
                return rows;
            }
            Some(Editor::Choices { values, narrow }) => {
                return list::List {
                    query: None,
                    said: None,
                    items: list::columned(
                        values.iter().map(|v| {
                            (
                                if self.rows[self.current()].key.kind == FieldKind::Boolean {
                                    match *v {
                                        "true" => "On",
                                        "false" => "Off",
                                        _ => "Use inherited value",
                                    }
                                } else if v.is_empty() {
                                    "Automatic"
                                } else {
                                    *v
                                },
                                "",
                            )
                        }),
                        Ink::Plain,
                        Beside::Row,
                    ),
                    selected: narrow.mark(),
                    marker: list::MARK,
                    chooses: true,
                }
                .rows();
            }
            _ => {}
        }
        let mut rows = vec![list::whole(
            &format!(
                "Next run · {} pending {}",
                self.changed(),
                self.change_word()
            ),
            Look::plain(Ink::Muted),
        )];
        let mut group = "";
        for (at, row) in self.rows.iter().enumerate() {
            let field = &FIELDS[at];
            if field.group != group {
                rows.push(list::whole("", Look::plain(Ink::Dim)));
                rows.push(list::whole(
                    field.group,
                    Look::new(Ink::Accent, Faces::BOLD),
                ));
                group = field.group;
            }
            let selected = at == self.current();
            let look = if selected {
                Look::new(Ink::Accent, Faces::REVERSED)
            } else {
                Look::plain(Ink::Plain)
            };
            let editing = selected && matches!(self.editor, Some(Editor::Text { .. }));
            let value = if editing && row.key.name != "api_key" {
                row.text.clone()
            } else {
                self.value(row)
            };
            let marker = if selected { "› " } else { "  " };
            let dirty = if row.touched && row.text != row.before {
                " *"
            } else {
                ""
            };
            let mut lines = if width < 44 {
                vec![
                    ScreenRow::cut(vec![(format!("{marker}{}{dirty}", field.label), look)])
                        .choosing(at),
                    ScreenRow::cut(vec![(format!("  {value}"), look)]).choosing(at),
                ]
            } else {
                vec![
                    ScreenRow::cut(vec![
                        (marker.to_owned(), look),
                        (
                            pen::pad(field.label, 23),
                            if selected {
                                look
                            } else {
                                Look::plain(Ink::Muted)
                            },
                        ),
                        (format!("{value}{dirty}"), look),
                    ])
                    .choosing(at),
                ]
            };
            if editing {
                let line = lines.last_mut().expect("a field has a value row");
                line.caret = Some(pen::width(&line.spans));
            }
            rows.extend(lines);
        }
        rows.extend(self.footer(width));
        rows
    }

    pub fn changed(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.touched && row.text != row.before)
            .count()
    }

    fn change_word(&self) -> &'static str {
        if self.changed() == 1 {
            "change"
        } else {
            "changes"
        }
    }

    pub fn switch(&self) -> bool {
        self.rows
            .get(self.current())
            .is_some_and(|row| row.key.switch())
    }

    fn value(&self, row: &Row) -> String {
        if row.key.name == "api_key" && !row.text.is_empty() {
            return "Configured · hidden".into();
        }
        if row.key.name == "route" {
            if self.wire() == Some(Wire::Anthropic) {
                return "Not applicable".into();
            }
            if row.text.is_empty() {
                let model = self
                    .rows
                    .iter()
                    .find(|row| row.key.name == "model")
                    .map(|row| row.text.as_str())
                    .unwrap_or("");
                let url = self
                    .rows
                    .iter()
                    .find(|row| row.key.name == "base_url")
                    .map(|row| row.text.as_str())
                    .unwrap_or("");
                let endpoint = self
                    .wire()
                    .map(sindri_core::provider::endpoint::Endpoint::own)
                    .map(|endpoint| {
                        if url.is_empty() {
                            endpoint
                        } else {
                            endpoint.based(url)
                        }
                    });
                return match endpoint.and_then(|endpoint| endpoint.route_of(model)) {
                    Some(route) => format!("Automatic → {route}"),
                    None => "Automatic".into(),
                };
            }
        }
        if row.text.is_empty() {
            return "Not set".into();
        }
        if row.key.switch() {
            return match row.text.as_str() {
                "true" => "[on]".into(),
                "false" => "[off]".into(),
                _ => row.text.clone(),
            };
        }
        if let Ok(value) = row.text.parse::<u64>() {
            let units: &[(u64, &str)] = match row.key.kind {
                FieldKind::DurationSeconds => &[(3600, "h"), (60, "min")],
                FieldKind::DurationMilliseconds => {
                    &[(3_600_000, "h"), (60_000, "min"), (1000, "s")]
                }
                FieldKind::ByteSize => &[(1 << 30, "GiB"), (1 << 20, "MiB"), (1 << 10, "KiB")],
                _ => &[],
            };
            if let Some((scale, unit)) = units
                .iter()
                .find(|(scale, _)| value >= *scale && value.is_multiple_of(*scale))
            {
                return format!("{} {unit}", grouped(&(value / scale).to_string()));
            }
        }
        let unit = match row.key.name {
            "context_window" | "output_headroom" | "tool_result_headroom" => " tokens",
            "timeout" => " seconds",
            "command_budget_ms" => " ms",
            "output_budget_bytes" => " bytes",
            _ => "",
        };
        let value = if !unit.is_empty() {
            grouped(&row.text)
        } else {
            row.text.clone()
        };
        format!("{value}{unit}")
    }

    fn footer(&self, width: u16) -> Vec<ScreenRow> {
        let mut rows = vec![list::whole("", Look::plain(Ink::Dim))];
        if let Some(field) = FIELDS.get(self.current()) {
            for line in crate::rows::wrap(&format!("{} · {}", field.label, field.help), width) {
                rows.push(list::whole(&line, Look::plain(Ink::Muted)));
            }
        }
        let hints = if matches!(self.editor, Some(Editor::Text { .. })) {
            if width < 44 {
                "Enter confirm · Esc restore"
            } else {
                "Enter confirms · Esc restores · Ctrl-U clears"
            }
        } else if width < 44 {
            if self.switch() {
                "↑↓ move · Space toggle · Esc cancel"
            } else {
                "↑↓ move · Enter edit · Esc cancel"
            }
        } else if self.switch() {
            "↑↓ move · Space toggle · Enter edit · Esc cancel · Tab next"
        } else {
            "↑↓ move · Enter edit · Esc cancel · Tab next"
        };
        if self.rows.iter().any(|row| {
            row.touched && row.text != row.before && matches!(row.key.name, "provider" | "base_url")
        }) {
            rows.extend(list::refusal(
                "Endpoint changed: check the model, address and credentials before Apply",
                width,
            ));
        }
        if let Some(reason) = self.refusal() {
            rows.extend(list::refusal(reason, width));
        }
        for (at, text) in [
            (
                self.rows.len(),
                if width < 44 {
                    format!("Apply {} {} · next run", self.changed(), self.change_word())
                } else {
                    format!(
                        "Apply {} {} · changes affect the next run",
                        self.changed(),
                        self.change_word()
                    )
                },
            ),
            (self.rows.len() + 1, "Cancel · discard changes".into()),
        ] {
            let selected = at == self.current();
            let ink = if at == self.rows.len() && self.changed() == 0 {
                Ink::Dim
            } else {
                Ink::Accent
            };
            let look = Look::new(
                ink,
                if selected {
                    Faces::REVERSED
                } else {
                    Faces::PLAIN
                },
            );
            rows.push(
                ScreenRow::cut(vec![(
                    format!("{}{text}", if selected { "› " } else { "  " }),
                    look,
                )])
                .choosing(at),
            );
        }
        rows.push(ScreenRow::cut(vec![(
            hints.into(),
            Look::plain(Ink::Muted),
        )]));
        rows
    }

    pub fn frame(&self, width: u16, height: u16) -> Vec<ScreenRow> {
        let mut rows = self.body(width);
        let height = usize::from(height);
        if rows.len() <= height {
            return rows;
        }
        if matches!(
            self.editor,
            Some(Editor::Collection { .. } | Editor::Credential(_))
        ) {
            let selected = rows
                .iter()
                .position(|row| {
                    row.spans
                        .first()
                        .is_some_and(|(text, _)| text.starts_with(list::MARK))
                })
                .unwrap_or(0);
            let start = selected.saturating_sub(height.saturating_sub(2));
            return rows.drain(start..).take(height).collect();
        }
        if matches!(self.editor, Some(Editor::Model(_) | Editor::Choices { .. })) {
            let selected = rows
                .iter()
                .position(|row| {
                    row.spans
                        .first()
                        .is_some_and(|(text, _)| text.starts_with(list::MARK))
                })
                .unwrap_or(0);
            let start = selected.saturating_sub(height.saturating_sub(1));
            return rows.drain(start..).take(height).collect();
        }
        let footer = self.footer(width);
        rows.truncate(rows.len() - footer.len());
        let mut footer = footer;
        if footer.len() + 4 > height {
            let actions = footer.split_off(footer.len().saturating_sub(3));
            let room = height.saturating_sub(4 + actions.len());
            let reason = self
                .refusal()
                .map(|reason| crate::rows::wrap(reason, width))
                .unwrap_or_default();
            footer = reason
                .into_iter()
                .take(room)
                .map(|line| list::whole(&line, Look::plain(Ink::Err)))
                .collect();
            footer.extend(actions);
        }
        if height <= footer.len() {
            return footer
                .into_iter()
                .rev()
                .take(height)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
        }
        let room = height - footer.len();
        let selected = rows
            .iter()
            .rposition(|row| row.choice == Some(self.current()))
            .unwrap_or(0);
        let start = selected.saturating_sub(room.saturating_sub(2));
        let end = (start + room.saturating_sub(1)).min(rows.len());
        let count = rows.len() - (end - start);
        let mut visible: Vec<_> = rows.drain(start..end).collect();
        visible.push(list::whole(
            &format!("⋯ {count} rows kept back · ↑↓ scroll"),
            Look::plain(Ink::Dim),
        ));
        visible.extend(footer);
        visible
    }

    pub fn touched(&self) -> Touched {
        let mut touched = Touched::new();
        for row in self
            .rows
            .iter()
            .filter(|row| row.touched && row.text != row.before)
        {
            touched.set(row.key.name, row.text.as_str());
        }
        touched
    }

    pub fn refuse(&mut self, reason: &str) {
        if let Some(at) = self
            .rows
            .iter()
            .position(|row| reason.contains(row.key.name))
        {
            self.narrow.pressed(at, self.rows.len() + 2);
        }
        self.refusal = Some(reason.to_owned());
    }

    pub fn clear(&mut self) -> bool {
        if let Some(Editor::Collection {
            input: Some((_, text, _)),
            ..
        }) = &mut self.editor
        {
            text.clear();
            self.refusal = None;
            return true;
        }
        if matches!(
            self.editor,
            Some(Editor::Credential(_) | Editor::Collection { .. })
        ) {
            return false;
        }
        if matches!(self.editor, Some(Editor::Model(_))) {
            self.editor = Some(Editor::Text {
                before: self.standing().text.clone(),
                touched: self.standing().touched,
            });
            return true;
        }
        if matches!(self.editor, Some(Editor::Choices { .. })) {
            return false;
        }
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
        if matches!(
            self.editor,
            Some(Editor::Collection { .. } | Editor::Credential(_))
        ) {
            return self.key(Key::Char(' '));
        }
        if matches!(self.editor, Some(Editor::Model(_) | Editor::Choices { .. })) {
            return self.key(Key::Char(' '));
        }
        if !self.has_row() {
            return false;
        }
        if !self.standing().key.switch() {
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
        if let Some(Editor::Collection { input, .. }) = &mut self.editor {
            let Some((_, text, _)) = input else {
                return false;
            };
            let changed = match key {
                Key::Char(c) if !c.is_control() => {
                    text.push(c);
                    true
                }
                Key::Backspace => text.pop().is_some(),
                _ => false,
            };
            if changed {
                self.refusal = None;
            }
            return changed;
        }
        if matches!(self.editor, Some(Editor::Credential(_))) {
            return false;
        }
        if let Some(Editor::Model(models)) = &mut self.editor {
            return models.key(key);
        }
        if matches!(self.editor, Some(Editor::Choices { .. })) {
            return false;
        }
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

    fn backspace(&mut self) -> bool {
        let masked = self.unmask();
        let erased = self.standing().text.pop().is_some() || masked;
        if erased {
            self.standing().touched = true;
            self.refusal = None;
        }
        erased
    }

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
        if self.rows.is_empty() {
            return false;
        }
        match &mut self.editor {
            Some(Editor::Model(models)) => models.walked(way),
            Some(Editor::Choices { values, narrow }) => narrow.walked(way, values.len()),
            Some(Editor::Credential(narrow)) => narrow.walked(way, 2),
            Some(Editor::Collection {
                items,
                narrow,
                input,
                ..
            }) => input.is_none() && narrow.walked(way, items.len() * 2 + 2),
            Some(Editor::Text { .. }) => false,
            None => self.narrow.walked(way, self.rows.len() + 2),
        }
    }

    fn typed(&mut self, key: Key) -> bool {
        if self.editor.is_none() {
            return false;
        }
        self.key(key)
    }

    fn pressed(&mut self, at: usize) -> bool {
        match &mut self.editor {
            Some(Editor::Model(models)) => models.pressed(at),
            Some(Editor::Choices { values, narrow }) => narrow.pressed(at, values.len()),
            Some(Editor::Credential(narrow)) => narrow.pressed(at, 2),
            Some(Editor::Collection {
                items,
                narrow,
                input,
                ..
            }) => input.is_none() && narrow.pressed(at, items.len() * 2 + 2),
            Some(Editor::Text { .. }) => false,
            None => self.narrow.pressed(at, self.rows.len() + 2),
        }
    }
}

#[cfg(test)]
mod tests;

fn editor_menu(labels: Vec<String>, selected: usize) -> Vec<ScreenRow> {
    list::List {
        query: None,
        said: None,
        items: list::columned(
            labels.iter().map(|label| (label.as_str(), "")),
            Ink::Plain,
            Beside::Row,
        ),
        selected,
        marker: list::MARK,
        chooses: true,
    }
    .rows()
}
