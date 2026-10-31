//! The published description of the sandbox: the text the model reads and
//! `sindri doctor` prints. Derived from the policy, never written by hand.

use super::SandboxPolicy;
use super::argv::LOCALE;

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
            out.push_str(&format!("  network: {}\n", self.network().describe()));
            return out;
        }
        out.push_str(
            "  enforcement: bubblewrap — read-only host root, writable set mounted read-write\n",
        );
        out.push_str(&format!("  network: {}\n", self.network().describe()));

        out.push_str("  writable set:\n");
        for path in self.writable().paths() {
            out.push_str(&format!("    {}\n", path.display()));
        }

        // Every line below that states a value names the variables holding it,
        // and all of them read the one list the argv enforces.
        let environment = self.environment();
        let names: Vec<&str> = environment.iter().map(|(name, _)| *name).collect();
        let temp = self.session_temp();
        let temp_path = temp.display().to_string();
        let home_path = temp.join("home").display().to_string();
        out.push_str(&format!(
            "  session temp: {temp_path} ({})\n",
            and_list(&variables_holding(&environment, &temp_path))
        ));
        out.push_str(&format!(
            "  home: {home_path} ({} — writable and empty, created for the run)\n",
            and_list(&variables_holding(&environment, &home_path))
        ));
        out.push_str(&format!(
            "  environment: cleared, then {} set explicitly; the locale is {LOCALE}\n",
            and_list(&names),
        ));
        out.push_str(
            "  isolation: own pid, ipc, uts and cgroup namespaces; /proc lists only this sandbox\n",
        );
        out.push_str("  read-only: the rest of the host filesystem, readable but not writable\n");
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
