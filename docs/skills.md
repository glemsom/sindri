# Skills in the terminal

Invoke a skill as a first-class slash command: `/skill-name` starts an agent turn
using that skill's instructions. You can include a request after the name:

```text
/web-access
/web-access Read https://example.com and summarize the page.
```

Type `/` or part of a skill name to find it in the commands card. **Tab completes**
the selected name without starting work; **Enter invokes** it. After completing a
name, you can add an optional request before pressing Enter.

Invoking an already-loaded skill is valid: each invocation starts another turn.
Loading instructions and asking the agent to act are distinct operations; an
already-loaded marker does not mean the skill cannot be invoked again.

Built-in slash commands win name collisions. For example, `/help` opens help even
if a skill is named `help`. This command-name rule is separate from skill scope
precedence: repository skills override user skills, which override bundled skills
of the same name.

## Load-only compatibility

The legacy `/skill:name` spelling remains available to load a skill's instructions
without starting an agent turn. It is not equivalent to `/name`, and does not take
an optional request. A colliding skill can still be loaded with `/skill:help`.
Loading an already-loaded skill does not start work or reload its instructions.

Skills that disallow human invocation are not available as user slash commands.
The model's ability to load a skill is controlled separately. `/new` clears the
conversation and its loaded skills.
