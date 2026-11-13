# The window outside the frame says what the run is doing

The terminal around the frame carries the run without a cell of the frame being spent on it:
`sindri::window::Window` is the one telling a launch makes — the status document `OSC 7501` a status
bar reads, the title `OSC 2` naming the workspace and the state, the progress mark `OSC 9;4` a
streaming run wears, and the notification `OSC 9` a run that ended while nobody was looking raises.
It replaced `Teller`, because a launch that knows the run is working is a launch that knows its title
and its mark, and the two sayings have one source: every one of them is built from the same `Status`
of the same `Told`. A state told to the status document and not to the title would be a title about a
run that has moved on. The focus the terminal reports rides the same type, and is the only thing it
is given rather than says.

**None of it is a cell.** The frame is the cells, and a title or a mark drawn as a cell would be a
column spent on something the terminal already has a place for. So the window is bytes written beside
the frame — through a second handle on the descriptor a frame is drawn on — and a reader's copy of the
frame takes the words of the run and no title of its own. That is also why a terminal that takes none
of these sequences is handed **the same frame it has always drawn**, and why the goldens of every
frame are untouched by this work.

**The title names the workspace and the state, and is written when the state changes.** The two words
are the status's own: `Status::workspace` is the directory name the status document already carries,
and `Phase::word` is the `idle`, `working`, `done`, `blocked` or `error` the document already
carries, so a title is not a second spelling of a fact. A turn inside one state names the same title,
and the window keeps the state it last wrote rather than repainting a title bar for nothing.

**The mark is indeterminate, and every way out of a run clears it.** `OSC 9;4;3` asks the terminal for
its own indeterminate progress indicator, because a turn is a count and never a share: turn 12 of 250
is not five percent of anything, which is the same reason the throttle says one state a second rather
than a percentage. `OSC 9;4;0` takes it away, and it is written by the stop, by a `status` key turned
off mid-run — a telling that has gone quiet can never say the stop — and by the way out, the panic
hook's included, because a mark outlives the process that set it and a reader would be left with a
terminal spinning for a run that is gone.

**A notification is raised only for a run that ended while nobody was looking.** A run the reader
watched end is a run they saw end, and a notification for it is noise on a screen they are already
reading. The body is `workspace: <the status's own line>` — `answered`, `failed`, `cancelled` — so the
words are the ones the status document carries and never the prompt or the answer. It is written
inside the state change into `done` or `error`, which is what makes it exactly one per run.

**Focus is asked for on the way in and given up on the way out.** `CSI ? 1004 h` asks the terminal
which of its windows is the one being read, and the report arrives as `CSI I` or `CSI O` — decoded as
`Event::Focus`, which is no key, so nothing a reader types is read as one. It is given up with the
screen, or a shell is left sending reports. Its one consequence for the frame is the accent edge: the
surface that takes the keys lets its accent go while nobody is reading it and takes it up again when
the reader comes back. A window that has never reported is the one being read, so a terminal that
takes none of these sequences draws the accent edge it always drew.

**The window is written where the terminal names itself, and never guessed at.** `window::speaks`
reads the names a terminal gives itself, as `link::speaks` does for a hyperlink, and writes nothing to
one that says nothing: a title a terminal would mangle is worse than the shell's own. A query is not
the answer, because a query is a read of the input a filter does not make. The `status` key gates the
whole window, because it is the reader's one way of saying their terminal is to be left quiet.

## What a reader is held to

- The title names the workspace and the state, and is written when the state changes and not on every
  turn.
- A streaming run wears the terminal's progress mark, and every stop clears it: the stop, a telling
  turned off, and the way out — a panic included.
- A run that ends while the window is unfocused raises one notification, and one that ends while it is
  focused raises none; the words are the status's own.
- A terminal that reports focus is asked to when the screen is taken and gives the report up on every
  way out.
- The accent edge of the surface that takes the keys recedes while the window is unfocused and returns
  when it is focused, and a terminal that has reported nothing keeps the edge it always had.
- A terminal that takes none of these sequences draws the same frame as before, and is handed the
  status document and no other byte.
