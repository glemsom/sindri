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

Only `/name [request]` invokes a skill. The former `/skill:name` and `/skill name`
forms are ordinary unknown slash prompt text: they neither load nor invoke skills
and have no completion rows. There is no reserved skill name beyond actual
built-in command collisions.

Skills that disallow human invocation are not available as user slash commands.
The model's ability to load a skill is controlled separately. `/new` clears the
conversation and its loaded skills.
