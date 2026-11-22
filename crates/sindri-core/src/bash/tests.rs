use tempfile::TempDir;

use super::*;
use crate::config::Config;

struct Ground {
    _root: TempDir,
    path: PathBuf,
}

impl Ground {
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let path = std::fs::canonicalize(root.path()).expect("the ground resolves");
        Ground { _root: root, path }
    }

    fn directory(&self, name: &str) -> PathBuf {
        let directory = self.path.join(name);
        std::fs::create_dir_all(&directory).expect("the directory");
        std::fs::canonicalize(&directory).expect("the directory resolves")
    }
}

fn boundary(ground: &Ground, extra: &[&str], enforcement: Enforcement) -> Boundary {
    let workspace = ground.directory("workspace");
    let config = Config {
        extra_writable_paths: Some(
            extra
                .iter()
                .map(|name| ground.directory(name))
                .collect::<Vec<PathBuf>>(),
        ),
        ..Config::default()
    };
    Boundary::from_config(&config, &workspace, enforcement).expect("the boundary holds")
}

const TEMP: &str = "/run/sindri/7f3c/tmp";

#[test]
fn the_sandbox_argv_is_the_boundary_and_nothing_else() {
    let ground = Ground::new();
    let boundary = boundary(&ground, &["zdata", "adata"], Enforcement::Bubblewrap);
    let workspace = ground.path.join("workspace");
    let adata = ground.path.join("adata");
    let zdata = ground.path.join("zdata");

    let argv = invocation(&boundary, Path::new(TEMP), "echo hi")
        .expect("the boundary renders an argv")
        .argv();

    let expected: Vec<String> = vec![
        "bwrap",
        "--unshare-all",
        "--share-net",
        "--die-with-parent",
        "--new-session",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--bind",
        &adata.display().to_string(),
        &adata.display().to_string(),
        "--bind",
        &workspace.display().to_string(),
        &workspace.display().to_string(),
        "--bind",
        &zdata.display().to_string(),
        &zdata.display().to_string(),
        "--bind",
        TEMP,
        TEMP,
        "--clearenv",
        "--setenv",
        "TMPDIR",
        TEMP,
        "--setenv",
        "PATH",
        "/usr/local/bin:/usr/bin:/bin",
        "--",
        "/bin/sh",
        "-c",
        "echo hi",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    assert_eq!(argv, expected);
}

#[test]
fn the_unsafe_argv_is_the_shell_alone() {
    let ground = Ground::new();
    let boundary = boundary(&ground, &["adata"], Enforcement::Unsafe);

    let argv = invocation(&boundary, Path::new(TEMP), "echo hi")
        .expect("the boundary renders an argv")
        .argv();

    assert_eq!(argv, vec!["/bin/sh", "-c", "echo hi"]);
}

#[test]
fn a_relative_session_temp_is_a_usage_error() {
    let ground = Ground::new();
    let boundary = boundary(&ground, &[], Enforcement::Bubblewrap);

    let error = invocation(&boundary, Path::new("relative/tmp"), "echo hi")
        .expect_err("a relative temp names no directory a sandbox could mount");

    assert_eq!(
        error.exit_code(),
        2,
        "a bad call is a usage error, not a best effort"
    );
    assert!(
        error.to_string().contains("relative/tmp"),
        "the refusal names the path to fix: {error}"
    );
}

#[test]
fn an_invalid_writable_set_is_a_usage_error_not_a_best_effort() {
    let ground = Ground::new();
    let workspace = ground.directory("workspace");
    let data = ground.directory("data");
    let inner = data.join("inner");
    std::fs::create_dir_all(&inner).expect("the inner directory");
    let (data_name, inner_name) = (data.display().to_string(), inner.display().to_string());

    let broken: Vec<(&str, Vec<PathBuf>, &str)> = vec![
        ("duplicate", vec![data.clone(), data.clone()], &data_name),
        ("nested", vec![data.clone(), inner.clone()], &inner_name),
        ("root", vec![PathBuf::from("/")], "/"),
        ("relative", vec![PathBuf::from("data")], "data"),
    ];

    for (shape, paths, named) in broken {
        let config = Config {
            extra_writable_paths: Some(paths),
            ..Config::default()
        };
        let refused = Boundary::from_config(&config, &workspace, Enforcement::Bubblewrap)
            .expect_err(&format!("a {shape} writable set must be refused"));
        let error = BashError::from(refused);

        assert_eq!(
            error.exit_code(),
            2,
            "a {shape} writable set is a usage error, not a best effort"
        );
        assert!(
            error.to_string().contains(named),
            "the {shape} refusal names the path that broke it: {error}"
        );
    }
}
