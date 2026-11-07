# A mode is one screen and one key table

`?` has to open something, the queue has to share `↑` with the draft, and the settings modal,
`/model` and the `@` picker were all heading for the same frame. Left as a `match` on keys in the
terminal's `answer`, each of them would have added a second place a key is decided, and "no key is
bound twice" would have stayed an opinion.

So a **mode** is a screen that answers keys, each mode owns one **key table**, and the frame is an
explicit stack of modes: `Composer` at the base, because a prompt is what the front end is for,
and a modal (`Keys` today) opened over it. `Stack::act` asks the mode on top and **never** the
modes under it, so a key resolves in exactly one mode — a glyph pressed at a screen that binds no
glyph does nothing, rather than editing a draft nobody can see. A mode's table names the keys
worth explaining; the keys it does not name fall to the mode's fallback, which for the composer is
the composer's own map and for a screen is nothing. A mode added later is a variant, a table and a
row in `Mode::ALL`.

The table is data rather than a `match` for one reason: the `?` screen is painted **from the same
rows that answer the keys**, so the screen that explains the keymap cannot drift from it, and
`? keys` is never a phantom. A test walks the tables and fails if one key appears twice in one of
them.

Two keys needed a rule, and both rules read where the caret is rather than which screen is up:

- **`?` is a key only while the draft is empty.** The composer is a text box, and a product whose
  prompt box could not hold a question mark would be a product you cannot ask a question of. With
  a draft, `?` is a glyph; the keys are one `Ctrl-C` away.
- **`↑` is one row bound to one act.** On the composer's first line the thing above the caret is
  the queue, so it recalls the newest queued prompt; anywhere else it moves the caret a row up.
  That decision is a single arm in `View::up` reading `Composer::on_first_line`, so the queue adds
  to one resolution instead of a second one.

A screen is the whole frame: its bands are not painted and the caret is not placed, because a
draft drawn under a screen a key cannot reach would be a lie about what the keys do.

One wager is in the decoder rather than in a table: a bare `ESC` byte that arrives alone is the
`Esc` key, and a sequence that opens with `ESC` brings the bytes that say which sequence it is.
The alternative — waiting for a byte a keyboard may never send — is a screen nobody can close.
