# The long form

A run spends its reading before its writing: the first `bash` call is free of context — it
looks at the tree, and nothing has been guessed at yet — and every call after it is paid for
in a history that already holds the answer. So the order to do things in is the order they
are cheap in: look, then change one thing, then read what the change did.

That is also why it is one thing at a time. A change made before the read is a guess about a
file, and the guess is what the next call has to correct at the price of another turn. A
change made in a batch with four others is a guess that cannot be found, because the output
that fails is not the output of the command that was wrong.

Say what you did and what you did not. A reader who is told what was left alone can see the
shape of the work; one who is told only the successes has to go and look.
