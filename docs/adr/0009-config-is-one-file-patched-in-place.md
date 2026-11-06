# One config file, every key optional, patched in place

Every key in `config.toml` is an `Option<T>`, so *unset* is a real state — that is what lets
"an empty value unsets the key so the environment speaks" mean anything. Resolution is
file, then `SINDRI_*` environment, then flags, with **flags winning**. `deny_unknown_fields`
turns §11's "unknown key is an error naming the key and the file" into a serde error rather
than a hand-written parser.

Saving uses **`toml_edit`** to patch only the touched keys, so a user's comments, key order
and untouched values survive a visit to the settings modal. The write is a temp file in the
same directory, `fsync`, `rename`, mode `0600`: the rename is what makes it atomic, not the
write.

The two run budgets (`command_budget_ms`, `output_budget_bytes`) are keys like the rest. A
one-hour wall clock is a product default, not a law.
