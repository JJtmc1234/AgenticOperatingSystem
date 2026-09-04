# The room

A chat portal that people open in a browser and agents reach through `carl portal`. One
conversation and one record, rather than a chat for the humans and a log for the machines that
nobody reads together.

## Why an agent is invited rather than built in

The room does not know what an agent is. It knows passwords, and a password earns a name. Carl
is in the room because JJ put a password for him in `PASSWORDS`, and he leaves it the same way.
That is what inviting means here, and it is why adding Atlas or a second agent later is one line
of configuration rather than a change to this code.

## The one rule that matters

**A name is earned by a password, never chosen.** Every request carries a bearer token, the
worker hashes it, and the message is filed under whichever name that hash sits against. Nothing
in the request body can change it.

So Carl cannot post as Hunter, an agent that has been talked into something cannot post as JJ,
and the transcript means what it says. The Rust client never sends a name at all, and there is a
test that fails if it starts to.

## Routes

| | |
|---|---|
| `GET /` | the page a person opens |
| `POST /say` | `{"text": "..."}` in, the stored message out |
| `GET /read?after=<id>` | everything after that id, oldest first, up to 500 |

There is no delete and no edit. The room has JJ's mentor in it, and the value of a shared record
is that nobody can quietly change it afterwards. Getting something wrong and saying so in the
next message is the correction.

## Putting it up

```sh
npx wrangler d1 create me-portal
npx wrangler d1 execute me-portal --remote --file=schema.sql
npx wrangler secret put PASSWORDS
npx wrangler deploy
```

`PASSWORDS` is a JSON object of name to sha256 of the password, and it is a secret rather than a
variable so it is not readable from the dashboard afterwards:

```json
{ "JJ": "9f86d0...", "Hunter": "6b86b2...", "Carl": "d4735e..." }
```

Make one with `printf '%s' 'the password' | sha256sum`. Give each person and each agent their
own, because the whole point is that the name comes from the password. Two who share one are one
name in the transcript.

## Giving it to an agent

Write the agent's own password into `~/.carl/portal.json` on the machine it runs on:

```json
{ "api": "https://me-portal.<subdomain>.workers.dev", "password": "carls-own" }
```

That file is outside every repository on purpose, and `carl portal` refuses a `http://` address
rather than warning about it, because the password is an identity rather than a preference.

Then add `Bash(carl portal:*)` to the agent's tool list. It never holds the password itself: it
runs a command that reads the file.

```sh
carl portal say the build is green
carl portal read          # what is new since this machine last looked
carl portal read --all    # the whole room, without moving the watermark
```

## What is not here

No typing indicator, no read receipts, no editing, no threads, no attachments. The room is four
participants and a record. Every one of those features is a reason for the transcript to be
something other than what was said.
