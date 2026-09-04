# The memory system

How ten agents remember things between conversations, and how they find what they remember
without carrying all of it in every prompt.

The problem this solves is not storage. It is that a prompt has a budget, an agent that is told
to read everything reads nothing, and a rule an agent cannot find is a rule that does not exist.
So the whole design is about **indexes**, and the rule the indexes enforce is written where it
will be read: add the row in the same turn you add the file.

## Two layers, two indexes

```
shared/                every agent reads this, every turn
  README.md            how the folder works, the org chart, what to do before acting
  INDEX.md             every file, one row each, plus keywords for grepping
  people/  work/  machine/  projects/  lessons/

agents/<lead>/<agent>/ one agent's own
  CLAUDE.md            loaded every turn. Identity, chain of command, what it never does
  MEMORY.md            on demand. The handbook for its job, opening with its own index
  memory/summary.md    what it is carrying right now
  memory/learned.md    rules it worked out or was corrected on
```

`CLAUDE.md` is small and always loaded. `MEMORY.md` is large and never loaded: it is read when
its own index says a section is relevant. That split is the point. Miles's handbook is 8 kB
across ten sections, and before it had an index the instruction was "read it before email work",
which meant the whole file every time.

## What each index is for

`shared/INDEX.md` covers what every agent shares. The index at the top of each `MEMORY.md`
covers that agent's own job. Neither is decoration:

- Every file in `shared/` has a row. Checked, both directions.
- Every section in a `MEMORY.md` has a row. Checked, both directions.
- A file that is superseded stays indexed, marked **never read**. `memory/rules.md` is the
  example: its contents moved into `learned.md`, and the code keeps it so the migration can be
  audited. An index that silently dropped it would leave a file in every agent folder that
  looks current.

## What learning means here

`learned.md` is promoted, not drafted. A pattern becomes a rule on the third separate sighting.
A correction from JJ or the agent's lead becomes one at once. Nothing an agent writes into its
own memory grants it anything: rank, reporting line and tool list come from the compiled
organisation, never from a file the agent can edit. That separation is deliberate and is tested.

`lessons/` is different again. Each file is a mistake that was actually made and the rule it
produced. They are not stories. They are the reason a rule exists, kept so the rule survives
somebody deciding it looks unnecessary.

## Held back from this public copy

Three files are stubs here, and one line is redacted. This repository is public, and those
carry either a route into a personal machine or a child's teachers by name and address:

| File | Why |
|---|---|
| `shared/machine/tensor.md` | says exactly how the machine is exposed |
| `shared/machine/virtualbox.md` | the internal network and what it reaches |
| `shared/people/contacts.md` | teachers and their addresses |
| `shared/work/mail.md` | one line of school addresses, redacted in place |
| `agents/olivia/miles/brief.md` | one line of school addresses, redacted in place |
| `ACTION-ITEMS.md` | teachers, classes and when they meet, which together say where a child is |

The four in `shared/` keep their rows in `INDEX.md`. An index that quietly omits a file it
knows about teaches the reader that the index is incomplete, which costs more than the omission
saves. The other two are not in that index because it only covers `shared/`, so this table is
where they are named.

The complete system, with those files intact, is the private repository
`JJtmc1234/army-memory`.

## Where the code is

In `../carl`, in this repository. The agents, the ranks, the delegation checks and the
promotion engine used to be the separate `JJtmc1234/carl`, and were brought in with their
history in September 2026 because one project in four repositories is four places to look for
one answer.

This folder is still the memory those agents read rather than the runtime that reads it. The
split is worth keeping inside one repository: memory is text a person can check, and the
runtime is code a test can check.

## Why army-memory is still a separate repository

Its history is the reason. The files above are stubs at the tip of this branch, but they are
real in every commit that private repository ever made, and importing that history here would
publish what the stubs exist to hold back. So the content is mirrored and the history is not.
