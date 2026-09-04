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
