use super::*;

#[test]
fn a_draft_bills_every_character_it_holds_and_nothing_for_a_draft_holding_none() {
    let files = Files::default();

    assert_eq!(
        Bill::of("the draft bills every char", &files).map(|bill| bill.parts()),
        Some(vec![String::from("26 chars")]),
        "a draft bills every character it holds, said in words"
    );
    assert_eq!(
        Bill::of("", &files),
        None,
        "an empty draft bills nothing at all"
    );
}

fn workspace(files: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a temp workspace");
    for file in files {
        let path = root.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the directories a file sits in");
        }
        std::fs::write(&path, "").expect("a file");
    }
    root
}

#[test]
fn one_file_is_a_file_and_two_are_files() {
    let root = workspace(&["src/rows.rs", "src/view.rs"]);
    let files = Files::walk(root.path());

    assert_eq!(
        Bill::of("read src/rows.rs", &files).map(|bill| bill.parts()),
        Some(vec![String::from("1 file"), String::from("16 chars")]),
        "a word naming one file of the walk is one file"
    );
    assert_eq!(
        Bill::of("read src/rows.rs and src/view.rs", &files).map(|bill| bill.parts()),
        Some(vec![String::from("2 files"), String::from("32 chars")]),
        "two words naming two files are two files"
    );
}

#[test]
fn a_draft_that_opens_with_a_command_bills_one_command() {
    let files = Files::default();

    assert_eq!(
        Bill::of("/stats", &files).map(|bill| bill.parts()),
        Some(vec![String::from("1 command"), String::from("6 chars")]),
        "a draft the frame answers itself bills the one command it opens with"
    );
    assert_eq!(
        Bill::of("/s", &files).map(|bill| bill.parts()),
        Some(vec![String::from("2 chars")]),
        "a word that is no command bills no command"
    );
}
