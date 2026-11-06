//! Proves the `open` tool decides inside the call: what may be handed to the
//! host's browser, what is refused, and that every refusal names the allowlist
//! the model has to correct itself against.
//!
//! The escape this module exists for is a symlink to `/`: the string the model
//! wrote is inside the writable set and carries an allowed extension, so a check
//! on that string alone lets it through. Every path is therefore judged twice —
//! as written, and as it resolves — and the tests below fail if either check
//! goes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use sindri_core::config::Config;
use sindri_core::open::{EXTENSIONS, Ground, SCHEMES};
use sindri_core::policy::{Boundary, Enforcement};
use tempfile::TempDir;

/// The three places a judgement needs: a writable workspace, the session temp
/// beside it, and a directory outside both that nothing may be opened from.
struct Tree {
    /// Held so the directories outlive the ground that names them.
    root: TempDir,
}

impl Tree {
    /// A tree of the three directories, each canonical, so a judgement compares
    /// the filesystem's spelling of a path with the filesystem's spelling.
    fn new() -> Tree {
        let root = tempfile::tempdir().expect("a temp tree");
        for name in ["workspace", "tmp", "outside"] {
            std::fs::create_dir_all(root.path().join(name)).expect("the directory");
        }
        Tree { root }
    }

    /// A directory of the tree, canonicalised the way the boundary resolves one.
    fn directory(&self, name: &str) -> PathBuf {
        std::fs::canonicalize(self.root.path().join(name)).expect("the directory resolves")
    }

    /// The workspace: the one writable path, and the directory a relative target
    /// is read from — the first of the tree's directories, so canonicalising the
    /// temp does not trail the workspace before it.
    fn workspace(&self) -> PathBuf {
        self.directory("workspace")
    }

    /// The session temp, which §5 puts beside the writable set.
    fn temp(&self) -> PathBuf {
        self.directory("tmp")
    }

    /// A directory the run may not read a target from.
    fn outside(&self) -> PathBuf {
        self.directory("outside")
    }

    /// The boundary a run standing in the workspace holds.
    fn boundary(&self) -> Boundary {
        Boundary::from_config(
            &Config::default(),
            &self.workspace(),
            Enforcement::Bubblewrap,
        )
        .expect("the boundary holds")
    }

    /// The judgement a run makes: the boundary in force, the session temp and the
    /// directory a relative path is read from.
    fn ground(&self) -> Ground {
        Ground::new(&self.boundary(), &self.temp(), &self.workspace()).expect("the ground resolves")
    }

    /// A file at `path` holding `body`, written through any directory that must
    /// exist first.
    fn file(&self, path: &Path, body: &str) -> PathBuf {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the file's directory");
        }
        std::fs::write(path, body).expect("the file");
        path.to_path_buf()
    }

    /// A symlink at `link` pointing at `target`, which need not exist.
    fn symlink(&self, link: &Path, target: &Path) {
        std::os::unix::fs::symlink(target, link).expect("the symlink");
    }

    /// A file in the workspace named `name`, holding enough to be a file.
    fn in_workspace(&self, name: &str) -> PathBuf {
        self.file(&self.workspace().join(name), "body")
    }
}

/// The one argv word a decision hands the front end, as a string a test can read.
fn word(opening: &sindri_core::open::Opening) -> String {
    opening
        .word()
        .into_string()
        .expect("a judgement is one UTF-8 argv word")
}

#[test]
fn a_url_on_localhost_opens_as_the_one_target() {
    let tree = Tree::new();

    let opening = tree
        .ground()
        .judge(r#"{"url":"http://localhost:5173/index.html"}"#)
        .expect("localhost is the one authority a run may name");

    assert_eq!(word(&opening), "http://localhost:5173/index.html");
}

#[test]
fn a_url_naming_another_authority_is_refused_and_the_refusal_names_localhost() {
    let tree = Tree::new();

    let refusal = tree
        .ground()
        .judge(r#"{"url":"https://example.com/page.html"}"#)
        .expect_err("the browser is on this machine, so no other host may be opened");

    let said = refusal.to_string();
    assert!(
        said.contains("example.com") && said.contains("localhost"),
        "the refusal says what was refused and what is allowed: {said}"
    );
}

#[test]
fn a_url_whose_scheme_is_outside_the_allowlist_is_refused_naming_both_schemes() {
    let tree = Tree::new();

    for url in ["file:///etc/passwd", "javascript:alert(1)"] {
        let refusal = tree
            .ground()
            .judge(&format!(r#"{{"url":"{url}"}}"#))
            .expect_err("only a web page may be opened");
        let said = refusal.to_string();
        for allowed in SCHEMES {
            assert!(
                said.contains(allowed),
                "{url} is refused naming `{allowed}` so the model can correct it: {said}"
            );
        }
    }
}

#[test]
fn an_opening_is_one_argv_word_and_never_a_shell_string() {
    let tree = Tree::new();
    tree.in_workspace("my notes.html");

    let opening = tree
        .ground()
        .judge(r#"{"path":"my notes.html"}"#)
        .expect("a file with spaces is still one target");

    assert_eq!(
        opening.word(),
        OsString::from(tree.workspace().join("my notes.html")),
        "the target is one word, so nothing downstream has to quote it"
    );
}

#[test]
fn a_file_in_the_session_temp_opens_because_the_temp_is_part_of_the_boundary() {
    let tree = Tree::new();
    let target = tree.file(&tree.temp().join("report.md"), "body");

    let opening = tree
        .ground()
        .judge(&format!(r#"{{"path":"{}"}}"#, target.display()))
        .expect("the session temp is a place a target may come from");

    assert_eq!(opening.word(), target.into_os_string());
}

#[test]
fn an_extension_outside_the_allowlist_is_refused_naming_every_allowed_one() {
    let tree = Tree::new();
    tree.in_workspace("script.sh");

    let refusal = tree
        .ground()
        .judge(r#"{"path":"script.sh"}"#)
        .expect_err("a shell script is not something to hand a browser");

    let said = refusal.to_string();
    assert!(
        said.contains(".sh"),
        "the refusal names the extension it refused: {said}"
    );
    for allowed in EXTENSIONS {
        assert!(
            said.contains(allowed),
            "the refusal names `{allowed}`, every one of them, so one turn is enough: {said}"
        );
    }
}

#[test]
fn a_path_outside_the_writable_set_and_the_temp_is_refused_naming_both_places() {
    let tree = Tree::new();
    let target = tree.file(&tree.outside().join("notes.md"), "body");

    let refusal = tree
        .ground()
        .judge(&format!(r#"{{"path":"{}"}}"#, target.display()))
        .expect_err("nothing outside the boundary may be opened");

    let said = refusal.to_string();
    assert!(
        said.contains(&tree.workspace().display().to_string())
            && said.contains(&tree.temp().display().to_string()),
        "the refusal names the places a target may come from: {said}"
    );
}

#[test]
fn a_symlink_to_the_root_is_refused_naming_the_path_it_resolved_to() {
    let tree = Tree::new();
    tree.symlink(&tree.workspace().join("link.html"), Path::new("/"));

    let refusal = tree
        .ground()
        .judge(r#"{"path":"link.html"}"#)
        .expect_err("a link to `/` is the escape this check exists for");

    let said = refusal.to_string();
    assert!(
        said.contains("resolves to `/`"),
        "the refusal names the path it resolved to and not the string that was written: {said}"
    );
    assert!(
        said.contains("outside the writable paths"),
        "and containment on the resolved path is the check that refused it, which is why a link \
         to `/` cannot pass on its name: {said}"
    );
}

#[test]
fn a_path_written_outside_the_set_is_refused_even_when_it_resolves_back_inside() {
    let tree = Tree::new();
    let inside = tree.in_workspace("notes.md");
    tree.symlink(&tree.outside().join("there.md"), &inside);

    let refusal = tree
        .ground()
        .judge(&format!(
            r#"{{"path":"{}"}}"#,
            tree.outside().join("there.md").display()
        ))
        .expect_err("both checks have to pass, and this one fails the first");

    let said = refusal.to_string();
    assert!(
        said.contains("there.md") && said.contains("outside the writable paths"),
        "the string is judged before anything is followed, so a path written outside is refused: {said}"
    );
}

#[test]
fn a_symlink_inside_the_writable_set_to_a_file_outside_is_refused() {
    let tree = Tree::new();
    let secret = tree.file(&tree.outside().join("secret.txt"), "body");
    tree.symlink(&tree.workspace().join("twin.txt"), &secret);

    let refusal = tree
        .ground()
        .judge(r#"{"path":"twin.txt"}"#)
        .expect_err("both ends carry an allowed extension, so only the resolved path can say no");

    let said = refusal.to_string();
    assert!(
        said.contains(&secret.display().to_string()),
        "a link out of the set is refused even when its name looks like a text file: {said}"
    );
}

#[test]
fn a_symlink_whose_resolved_path_lacks_an_allowed_extension_is_refused() {
    let tree = Tree::new();
    tree.in_workspace("script.sh");
    tree.symlink(
        &tree.workspace().join("notes.md"),
        &tree.workspace().join("script.sh"),
    );

    let refusal = tree
        .ground()
        .judge(r#"{"path":"notes.md"}"#)
        .expect_err("the resolved path is the one opened, so its extension is the one judged");

    let resolved = tree.workspace().join("script.sh");
    let said = refusal.to_string();
    assert!(
        said.contains(&resolved.display().to_string()) && said.contains(".sh"),
        "the refusal names the path it resolved to and the extension that path carries: {said}"
    );
    assert!(
        said.contains("resolves"),
        "and says the check that refused was the one on the resolved path: {said}"
    );
}

#[test]
fn a_directory_carrying_an_allowed_extension_is_refused_because_a_target_is_a_file() {
    let tree = Tree::new();
    std::fs::create_dir_all(tree.workspace().join("site.html")).expect("the directory");

    let refusal = tree
        .ground()
        .judge(r#"{"path":"site.html"}"#)
        .expect_err("a browser is handed a file, not a directory");

    assert!(
        refusal.to_string().contains("not a regular file"),
        "the refusal says what a target has to be: {refusal}"
    );
}

#[test]
fn a_call_naming_no_target_or_two_is_refused_and_the_refusal_names_both_keys() {
    let tree = Tree::new();

    let neither = tree
        .ground()
        .judge(r#"{"url":"","path":""}"#)
        .expect_err("an empty value is the same as naming nothing");
    let said = neither.to_string();
    assert!(
        said.contains("url") && said.contains("path"),
        "the refusal names both keys exactly one of which is a target: {said}"
    );

    let both = tree
        .ground()
        .judge(r#"{"url":"http://localhost/","path":"notes.html"}"#)
        .expect_err("one target is the product, so two are refused");
    let said = both.to_string();
    assert!(
        said.contains("http://localhost/") && said.contains("notes.html"),
        "the refusal names both targets it was given: {said}"
    );
}
