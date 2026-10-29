//! The published description of the sandbox: the text the model reads and
//! `sindri doctor` prints. Derived from the policy, never written by hand.

use super::SandboxPolicy;
use super::argv::LOCALE;

impl SandboxPolicy {
    /// The human-and-model-facing description (ADR-0002). One derivation, so it
    /// cannot drift from what `to_argv` enforces.
    pub fn describe(&self) -> String {
        let mut out = String::from("sandbox policy\n");
        if self.is_unsafe() {
            out.push_str(
                "  enforcement: none — --yolo-unsafe runs commands with your own privileges,\n\
                 \x20              outside bubblewrap; the whole host filesystem is writable\n",
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

        let temp = self.session_temp();
        out.push_str(&format!(
            "  session temp: {} (TMPDIR, TEMP and TMP)\n",
            temp.display()
        ));
        out.push_str(&format!(
            "  home: {} (writable and empty — created for the run)\n",
            temp.join("home").display()
        ));
        out.push_str(&format!(
            "  environment: cleared, then PATH, HOME, TMPDIR and LANG={LOCALE} set explicitly\n"
        ));
        out.push_str(
            "  isolation: own pid, ipc, uts and cgroup namespaces; /proc lists only this sandbox\n",
        );
        out.push_str("  read-only: the rest of the host filesystem, readable but not writable\n");
        out
    }
}
