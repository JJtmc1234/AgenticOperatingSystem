# An instruction pointing at something unreachable is worse than no instruction

On 2026 08 29 every agent was told, in the standing brief every rank receives:

> Before you do anything else, read ~/Projects/MEMORY/README.md and ~/Projects/MEMORY/INDEX.md.
> If you are about to touch JJ's Gmail, read ~/Projects/MEMORY/work/mail.md first.

Then an agent was asked to send a test email. It refused, and it was right to:

> I stopped before sending. Your standing rule is that I read
> `~/Projects/MEMORY/work/mail.md` before I send anything, and that read is blocked.

An agent runs in its own folder under `Projects/army/<lead>/<agent>/`. `Projects/MEMORY` is a
sibling directory, and reading outside the working directory needs saying so explicitly. So the
new rule did not merely fail to help. It actively stopped the feature it was written to govern.

## Why this is the dangerous shape

An instruction the model cannot satisfy has two outcomes and both are bad.

- A careful agent refuses, and the capability silently stops working.
- A less careful one decides the rule cannot have meant that, invents what the file probably
  said, and acts on the invention. That one is worse, because it looks like it worked.

This agent took the first path and said exactly what was blocked, which is the only reason the
bug was found in minutes rather than in a week of quietly wrong sends.

## The fix

`--add-dir` for the shared memory folder, added in `claude::args_with` so it reaches every path
that starts an agent, and in the panel's own `ask_agent`. It resolves `$HOME/Projects/MEMORY`
and passes nothing at all when the folder is absent.

It goes **before** `--allowedTools`, which is variadic. Nothing goes after that list. See
`lessons/tool-lists.md`.

## The rule

When you write an instruction that names a file, a path, a command or a tool, check the agent
you are writing it for can actually reach it. Grant the access in the same change that writes
the instruction, never in a later one.

The general form: a rule and the ability to obey it are one change, not two.
