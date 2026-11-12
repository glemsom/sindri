# One session lasts a whole conversation

A launch in the TUI opens one session on its first prompt and keeps it until the launch ends: one
directory, one temp, one record that grows with every prompt, `sessions list` showing one entry where
it showed one per prompt, and the temp emptied when the conversation ends rather than after every
prompt. The type is `sindri_core::session::Session`, held by the `Launch` the front end opens, and
`Launch::drive` borrows it instead of consuming it: the launch a prompt starts is the launch the
prompt before it ran on, handed from the run's thread back to the frame when the run stops.

**A session is how a conversation is recorded, and a conversation outlives a run.** ADR-0021 keeps
the prompts a launch has had answered in `Conversation` and hands them to the run that joins next; the
session is that same lifetime written down — the record of those prompts and their turns, the temp
their commands worked in. The words stay the frame's, because `/new` is answered there; the session is
the launch's, opened by the first prompt and outliving every run it drives. A session per prompt could
say what one run asked and nothing about what the reader was doing across them, and a temp that went
at every stop would be scratch a command could not keep anything in. The readers that show a session —
the rail, `/stats`, `sessions list` — were each reading the right facts off the wrong lifetime: the
session, the temp, the turns and the spend they draw are the launch's, not the run's.

**The numbering, and the one thing that stays the run's.** The transcript and the document count the
session's turns: a prompt after `hi my name is Glenn` takes the turn after the one that prompt ended,
so a record reads as one conversation and never restarts at one. What the session held before is what
the next run's turns are numbered after, and `Record::began` says a run is in flight again — clearing
the end the run before reached, which was never this run's. A status document stays the run's: it
names the turn of the run that said it, because a status is the run's own word about what it is doing
and never a count of the conversation.

**A config that decides a run supersedes the session.** A session's record names one wire, one model,
one route, one thinking, one boundary and one set of dials, so a save that names another — `/model`, a
settings visit — is the end of the session it was made in: the launch closes it and the prompt after
opens the next, because two models' turns under one `"model"` would make the record a lie, and a dial
read at open would otherwise never take effect. Only the two keys a frame reads, `status` and `face`,
decide no run, and a save that touches one of those leaves the session where it is. The conversation
is not emptied by a save: it is the reader's words, and `/new` is the one command that empties it.

**The cap is a run's, and the count is the session's.** `max_turns` still counts the turns one run may
take, while the turns the rail and the document state are the conversation's, so the pair the rail
draws reads the conversation's turns against what a run may take: a launch of many prompts can draw a
count past its cap without any one run reaching it. Making the cap the conversation's was the other
way to keep the pair honest, and it was refused: it would take a reader's allowance away for a reason
nothing asked of the launch.

## What a reader is held to

- One launch, one session, however many prompts it is asked; batch mode is one run and one session per
  run, as it was.
- A file a command writes into the session temp survives the prompt after it and is gone when the
  launch ends, and the temp is emptied no sooner.
- `/new` empties the conversation and never the session: the record keeps every prompt the launch was
  asked, including the ones `/new` superseded.
- An interrupted launch still settles its record — the stop reason, the end, the temp emptied — as an
  interrupted run did, because the run's own stop is what writes them and the launch's end is what
  empties the temp. A launch quit under a live run is the one ending that settles nothing: it is left
  saying `running`, with no end and its temp kept, because the run it was waiting on never stopped.
- `Launch::drive` borrows the launch and `Launch::close` — the temp going — belongs to the launch's
  end, so no run can end a session another run is holding.
