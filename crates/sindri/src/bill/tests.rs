use super::*;

#[test]
fn a_draft_that_names_no_file_and_opens_with_no_command_bills_nothing() {
    let files = Files::default();

    assert_eq!(
        Bill::of("the draft bills nothing however long it runs", &files),
        None,
        "a draft of any length that names no file and opens with no command bills nothing"
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
        Some(vec![String::from("1 file")]),
        "a word naming one file of the walk is one file"
    );
    assert_eq!(
        Bill::of("read src/rows.rs and src/view.rs", &files).map(|bill| bill.parts()),
        Some(vec![String::from("2 files")]),
        "two words naming two files are two files"
    );
}

#[test]
fn a_file_the_draft_names_twice_is_one_file_of_material() {
    let root = workspace(&["src/rows.rs"]);
    let files = Files::walk(root.path());

    assert_eq!(
        Bill::of("read src/rows.rs then src/rows.rs", &files).map(|bill| bill.parts()),
        Some(vec![String::from("1 file")]),
        "the same file named twice is one file of material, however often the draft says it"
    );
}

#[test]
fn a_draft_that_opens_with_a_command_bills_one_command() {
    let files = Files::default();

    assert_eq!(
        Bill::of("/stats", &files).map(|bill| bill.parts()),
        Some(vec![String::from("1 command")]),
        "a draft the frame answers itself bills the one command it opens with"
    );
    assert_eq!(
        Bill::of("/s", &files),
        None,
        "a word that is no command bills no command"
    );
}

#[test]
fn the_files_the_draft_names_are_billed_before_the_command_it_opens_with() {
    let root = workspace(&["src/rows.rs"]);
    let files = Files::walk(root.path());

    assert_eq!(
        Bill::of("/stats src/rows.rs", &files).map(|bill| bill.parts()),
        Some(vec![String::from("1 file"), String::from("1 command")]),
        "the files a draft names come first, and the command it opens with after them"
    );
}
