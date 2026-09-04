-- The room, which is one table.
--
-- `id` is what everything pages from, so it has to be monotonic and never reused. AUTOINCREMENT
-- rather than plain rowid is the difference: without it SQLite may hand a deleted row's id to a
-- new message, and a reader holding that id as a watermark would then never see it.
--
-- There is no delete in the worker, so this should not arise. It is written this way anyway,
-- because the watermark is the one thing a reader cannot recover if it goes backwards.
CREATE TABLE IF NOT EXISTS said (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  who  TEXT    NOT NULL,
  text TEXT    NOT NULL,
  at   INTEGER NOT NULL
);

-- Every read is "everything after an id, in order", so that is what is indexed.
CREATE INDEX IF NOT EXISTS said_by_id ON said (id);

-- Everybody let into the room after it was set up.
--
-- Separate from the PASSWORDS secret rather than replacing it. The secret is the founders and it
-- changes only through the Cloudflare account. This table changes from inside the room, so the
-- owner can let somebody in without a deploy. `whoIsAsking` reads the secret first, which is why
-- a row here can never take over a founder's name however it got written.
--
-- `hash` is UNIQUE because two people sharing a password would make the lookup ambiguous, and
-- whoever asked second would find themselves posting under the other one's name.
CREATE TABLE IF NOT EXISTS person (
  name  TEXT    PRIMARY KEY,
  hash  TEXT    NOT NULL UNIQUE,
  added INTEGER NOT NULL
);

-- People waiting to be let in.
--
-- This is the one thing in the room a stranger can write to, so it is the one place that needs a
-- ceiling. `settled` is 0 waiting, 1 let in, 2 turned down. Turned down rows are kept rather
-- than deleted, for the same reason messages are: the record is worth more than the tidiness.
CREATE TABLE IF NOT EXISTS asked (
  id      INTEGER PRIMARY KEY AUTOINCREMENT,
  name    TEXT    NOT NULL,
  hash    TEXT    NOT NULL,
  why     TEXT    NOT NULL DEFAULT '',
  at      INTEGER NOT NULL,
  settled INTEGER NOT NULL DEFAULT 0
);

-- The owner's list is always "waiting, oldest first", so that is what is indexed.
CREATE INDEX IF NOT EXISTS asked_waiting ON asked (settled, id);
