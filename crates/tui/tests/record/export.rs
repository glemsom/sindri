//! `/export`: the current thread as markdown, where it was written said out
//! loud, and a refusal to overwrite without a second ask.

use super::*;

#[test]
fn an_export_writes_the_thread_as_markdown_and_says_where_it_went() {
    let mut f = Fixture::three_records();
    let id = f.app.rail_guid();
    command(&mut f.app, "/export");

    let path = f.workspace.join(format!("sindri-{id}.md"));
    let written = std::fs::read_to_string(&path).expect("the export is written to the workspace");
    let said = notes(&f.app);
    assert!(
        said.contains(&path.display().to_string()),
        "the transcript says where it went: {said}"
    );
    assert!(
        said.contains(&format!("export: writing {}", path.display())),
        "and the path it was writing, before it wrote: {said}"
    );
    assert!(
        written.starts_with(&format!("# session {id}")),
        "the file names the record it came from: {written}"
    );
    assert!(
        written.contains("why is the gate red?")
            && written.contains("The record could not be read."),
        "the thread is in it: {written}"
    );
}

#[test]
fn an_export_goes_to_a_path_that_is_named() {
    let mut f = Fixture::three_records();
    let named = f.workspace.join("notes/gate.md");
    std::fs::create_dir_all(named.parent().expect("a parent")).expect("the directory");
    command(&mut f.app, &format!("/export {}", named.display()));

    let written = std::fs::read_to_string(&named).expect("the named path is written");
    assert!(written.contains("why is the gate red?"), "{written}");
    assert!(
        notes(&f.app).contains(&named.display().to_string()),
        "and the transcript says so: {}",
        notes(&f.app)
    );
}

#[test]
fn an_export_refuses_to_overwrite_a_file_without_a_second_ask() {
    let mut f = Fixture::three_records();
    let path = f.workspace.join(format!("sindri-{}.md", f.app.rail_guid()));
    std::fs::write(&path, "keep me\n").expect("a file that is already there");

    command(&mut f.app, "/export");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file is untouched"),
        "keep me\n",
        "a refusal writes nothing"
    );
    let said = notes(&f.app);
    assert!(
        said.contains(&path.display().to_string()) && said.contains("already exists"),
        "the refusal names the file it will not overwrite: {said}"
    );
    assert!(
        said.contains("/export again"),
        "and says how to say yes: {said}"
    );

    // The second ask is the confirmation, and only then is the file replaced.
    command(&mut f.app, "/export");
    let written = std::fs::read_to_string(&path).expect("the file is replaced");
    assert!(
        written.contains("why is the gate red?"),
        "the second ask overwrites: {written}"
    );
}

#[test]
fn a_session_exported_reads_back_identically_through_sessions_show() {
    let mut f = Fixture::three_records();
    let id = f.app.rail_guid();
    command(&mut f.app, "/export");
    let exported = std::fs::read_to_string(f.workspace.join(format!("sindri-{id}.md")))
        .expect("the export is written");

    // What `sindri sessions show <guid>` prints for the same record.
    let stored = sindri_core::record::Stored::load_in(&SessionDir::at(f.sessions.join(&id)))
        .expect("the record still reads");
    let shown = stored.render();
    let mut at = 0;
    for entry in &stored.entries {
        let text = match entry {
            sindri_core::record::Entry::User { text } => text,
            sindri_core::record::Entry::Assistant { text, .. } => text,
            sindri_core::record::Entry::ToolResult { content, .. } => content,
            sindri_core::record::Entry::ToolCall { command, .. } => {
                command.as_deref().unwrap_or_default()
            }
            sindri_core::record::Entry::Policy { description } => description,
        };
        assert!(
            shown.contains(text.trim()),
            "{text:?} is in what sessions show prints: {shown}"
        );
        if text.trim().is_empty() {
            continue;
        }
        let found = exported[at..]
            .find(text.trim())
            .unwrap_or_else(|| panic!("{text:?} is in the export, in order: {exported}"));
        at += found + text.trim().len();
    }
}

#[test]
fn the_session_id_is_copyable_out_of_the_rail_for_sessions_show() {
    let mut f = Fixture::three_records();
    let id = f.app.rail_guid();

    // Ctrl-G copies the id alone: what `sindri sessions show` takes.
    press(&mut f.app, KeyCode::Char('g'), KeyModifiers::CONTROL);
    assert_eq!(
        f.clipboard.taken(),
        vec![id.clone()],
        "the id, without the rail's column around it"
    );

    // And a click on the rail's session row does the same, like any other row
    // of the frame: a pointer is an accelerator, never the only way in.
    let rows = screen(&mut f.app, 110, 24);
    // The rail's session fact: the label, then the id wrapped across the
    // column's width. A click on any of those rows copies the whole id.
    let at = row_of(&rows, &id[..8]);
    let column = 110 - 32 + 4;
    click(&mut f.app, column, at as u16);
    assert_eq!(
        f.clipboard.taken().last(),
        Some(&id),
        "clicking the session row copies the id: {:?}",
        f.clipboard.taken()
    );
}
