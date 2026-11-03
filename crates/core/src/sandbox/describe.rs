//! The published description of the sandbox: the text the model reads and
//! `sindri doctor` prints. Derived from the policy, never written by hand —
//! except the network line, which has one value, so it is a constant below.

use super::SandboxPolicy;
use super::argv::LOCALE;

/// The network is the one thing bwrap leaves alone: the sandbox bounds the
/// filesystem, not the network, and there is no second posture to name.
const NETWORK: &str = concat!(
    "shared — outbound network works; ",
    "the sandbox bounds the filesystem, not the network"
);

impl SandboxPolicy {
    /// The human-and-model-facing description. One derivation, so it
    /// cannot drift from what `to_argv` enforces.
    pub fn describe(&self) -> String {
        let mut out = String::from("sandbox policy\n");
        if self.is_unsafe() {
            out.push_str(
                "  enforcement: none — --yolo-unsafe runs commands with your own privileges,\n\
                 \x20              outside bubblewrap; the whole host filesystem is writable\n",
            );
            out.push_str(
                "  environment: inherited from the caller, as is the working directory;\n\
                 \x20              nothing is remapped and nothing is cleared\n",
            );
            out.push_str(&format!("  network: {NETWORK}\n"));
            return out;
        }
        // The boundary is two halves in one line: the host root, readable but not
        // writable, and the writable set. Stated once, so no later line disagrees.
        out.push_str(
            "  enforcement: bubblewrap — read-only host root (readable, not writable), \
             the writable set mounted read-write\n",
        );
        out.push_str(&format!("  network: {NETWORK}\n"));

        out.push_str("  writable set:\n");
        for path in self.writable().paths() {
            out.push_str(&format!("    {}\n", path.display()));
        }

        // Every line below that states a value names the variables holding it,
        // and all of them read the one list the argv enforces.
        let environment = self.environment();
        let names: Vec<&str> = environment.iter().map(|(name, _)| *name).collect();
        let temp_path = self.session_temp().display().to_string();
        out.push_str(&format!(
            "  session temp: {temp_path} ({})\n",
            and_list(&variables_holding(&environment, &temp_path))
        ));
        match self.home() {
            Some(home) => {
                let home_path = home.display().to_string();
                out.push_str(&format!(
                    "  home: {home_path} ({} — the host's own, read-only; write to $TMPDIR instead)\n",
                    and_list(&variables_holding(&environment, &home_path))
                ));
            }
            // The absence is the host's, so it is said about the host: nothing
            // was taken away by this policy and nothing is remapped in its place.
            None => out.push_str(
                "  home: none — the host has no HOME, so none is set; write to $TMPDIR instead\n",
            ),
        }
        out.push_str(&format!(
            "  environment: cleared, then {} set explicitly; the locale is {LOCALE}\n",
            and_list(&names),
        ));
        out.push_str(
            "  isolation: own pid, ipc, uts and cgroup namespaces; /proc lists only this sandbox\n",
        );
        out
    }
}

/// The names in `environment` whose value is `value`: the temp path's, HOME's.
fn variables_holding(environment: &[(&'static str, String)], value: &str) -> Vec<&'static str> {
    environment
        .iter()
        .filter(|(_, held)| held == value)
        .map(|(name, _)| *name)
        .collect()
}

/// `a`, `a and b`, `a, b and c`: the description is a sentence, not a listing.
fn and_list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
