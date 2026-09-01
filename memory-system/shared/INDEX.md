# Index

Every file in this folder, what is in it, and when to read it. Read `README.md` first, then the
rows here that match what you are about to do.

If you are about to touch mail, a machine setting, or anything JJ will read, there is a row for
it. Grep this file for a keyword before deciding something is not written down.

## Always

| File | What it holds |
|---|---|
| `README.md` | How this folder works, the org chart, what to do before you act. Read every time. |
| `INDEX.md` | This file. |
| `people/jj.md` | Who JJ is, how he wants to be written to, what he has already corrected. |

## Before you send or read mail

| File | What it holds |
|---|---|
| `work/mail.md` | **Read before touching Gmail.** Send rules, the gibberish auto reply, loop protections, who must never get a machine reply. |
| `work/mail-sent.md` | The log. Append a line for every message an agent sends. |
| `people/contacts.md` | Hunter, school, investors. Who is never auto replied to. |

## Before you report or write anything down

| File | What it holds |
|---|---|
| `work/reporting.md` | What to say, what to leave out, the 20 line cap on timed reports. |
| `work/action-items.md` | How `Projects/ACTION-ITEMS.md` works. Append to the bottom, never recite it in chat. |
| `work/committing.md` | Everything gets committed. Unrelated work gets its own commit, never left unstaged. |

## Before you touch the machine

| File | What it holds |
|---|---|
| `machine/tensor.md` | The host. Network, groups, sudo policy, and the open SSH exposure. |
| `machine/processes.md` | What runs, which services cost money to restart, and how to spot a stale process. |
| `machine/virtualbox.md` | `wundows`, the isolation audit, and how to delete a VM without hitting the wrong one. |

## Project background

| File | What it holds |
|---|---|
| `projects/multiverse.md` | Multiverse Enterprises is software only. The holoprojector is a design. |
| `projects/carl.md` | The Rust codebase, the chain of command, how a turn reaches a screen. |
| `projects/factorio.md` | Paused. The API facts that were expensive to learn. |
| `projects/aos.md` | Hunter's homework issues and what is outstanding. |

## Your own folder

The shared folder is not the only memory you have, and until now nothing said so here. Your own
files are indexed below in the order you read them. `<you>` is your name, and your folder is
`Projects/army/<your lead>/<you>/` or `Projects/army/<you>/` if you are a lead.

| File | What it holds | When to read it |
|---|---|---|
| `CLAUDE.md` | Who you are, your lead, your reports, what you never do. | Loaded for you every turn. You do not have to open it. |
| `MEMORY.md` | The detailed procedure for your job. Starts with its own index. | Read its index, then the rows that match the work. Never the whole file. |
| `memory/summary.md` | What you are carrying right now, and where the rest of it is. | Every turn, first. It is short on purpose. |
| `memory/learned.md` | Rules you worked out or were corrected on. Promoted, not drafted. | Before deciding something you may already have decided once. |
| `memory/rules.md` | **Superseded.** Its contents moved into `learned.md`. | Never. It is kept so the migration can be checked, not to be worked from. |

Two files and two indexes, and they point at different things on purpose. This one covers what
every agent shares. The one at the top of your `MEMORY.md` covers your own job. If you are
reading a whole file to find one rule, one of the two indexes is missing a row and adding it is
part of the work.

## Mistakes that have already been made

Read these when you are about to do the thing they are about. They are not stories, they are
the reason a rule exists.

| File | The mistake |
|---|---|
| `lessons/broken-primitives.md` | Ten agents deployed on primitives that had never worked. Prove one worker first. |
| `lessons/check-before-claiming.md` | Said a mail was never sent because drafts was empty. It had been sent five days earlier. |
| `lessons/read-state.md` | Reported the same read notices three times and buried a test due the next day. |
| `lessons/fonts-and-glyphs.md` | Fixed a missing glyph twice by picking a different missing glyph. |
| `lessons/tool-lists.md` | Capability belongs in the tool list. Also, `--allowedTools` eats a trailing prompt. |
| `lessons/reachable-instructions.md` | Told every agent to read a folder none of them could open. A rule and the ability to obey it are one change. |

## Keywords

For grepping when you do not know the filename.

`gmail` `email` `send` `reply` `gibberish` `auto reply` `loop` -> `work/mail.md`
`hunter` `school` `miss candi` `a16z` `investor` -> `people/contacts.md`
`dashes` `semicolons` `style` `writing` `tone` -> `people/jj.md`
`report` `slack` `timer` `20 lines` -> `work/reporting.md`
`commit` `push` `git` `worktree` `stash` -> `work/committing.md`
`ssh` `firewall` `ufw` `port` `network` -> `machine/tensor.md`
`virtualbox` `wundows` `vm` `nat` `isolation` -> `machine/virtualbox.md`
`stale` `process` `orphan` `systemd` `timer` `restart` -> `machine/processes.md`
`rcon` `factorio` `mining` `furnace` `status code` -> `projects/factorio.md`
`panel` `egui` `stream` `thinking` `chunk` `say` -> `projects/carl.md`
`holoprojector` `hardware` -> `projects/multiverse.md`
`glyph` `font` `unicode` `square` `box` -> `lessons/fonts-and-glyphs.md`
`unsent` `missing` `not found` `zero` -> `lessons/check-before-claiming.md`
`add-dir` `permission denied` `cannot read` `blocked` `outside` -> `lessons/reachable-instructions.md`
`my job` `procedure` `how do i` `steps` -> your own `MEMORY.md`, read its index first
`what am i doing` `carrying` `outstanding` -> your own `memory/summary.md`
`already decided` `corrected` `rule` `learned` -> your own `memory/learned.md`

## Adding to this folder

One subject per file. Split a file once it passes roughly 150 lines. Add a row here in the same
turn you add the file, because an unindexed file is one nobody reads. Say what changed in your
commit message.
