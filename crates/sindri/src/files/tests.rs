use super::*;

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
fn a_word_that_names_a_file_of_the_walk_is_a_file_the_walk_holds() {
    let root = workspace(&["src/rows.rs"]);
    let files = Files::walk(root.path());

    assert!(
        files.names("src/rows.rs"),
        "the walk holds the one file the workspace stands on"
    );
    assert!(
        !files.names("src/nowhere.rs"),
        "a word the walk does not hold names no file"
    );
    assert!(
        !Files::default().names("src/rows.rs"),
        "a value walking no workspace holds nothing"
    );
}
