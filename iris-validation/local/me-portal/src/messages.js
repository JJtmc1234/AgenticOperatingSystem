/**
 * The room itself. Append only, ordered, and small.
 *
 * D1 rather than KV. KV is eventually consistent, so two people posting at once
 * can read back a different order from each other, and a conversation whose
 * order depends on who is asking is not a conversation. A database that assigns
 * the number is the only thing that makes "since" mean anything.
 */

/** Longest message. Long enough for a paragraph, short enough that the room stays readable. */
export const MOST_CHARS = 4000;

/**
 * @typedef {{id: number, at: number, author: string, body: string}} Message
 */

/**
 * Everything after `since`, oldest first.
 *
 * `since` rather than a page number, because the page polls: it asks for what
 * it has not seen and gets nothing back when nothing has happened, which is the
 * common case and should cost the least.
 */
export async function after(db, since, most = 200) {
  const { results } = await db
    .prepare("SELECT id, at, author, body FROM messages WHERE id > ?1 ORDER BY id ASC LIMIT ?2")
    .bind(since, most)
    .all();
  return results ?? [];
}

/** The last `most`, oldest first, for somebody who has just walked in. */
export async function recent(db, most = 200) {
  const { results } = await db
    .prepare("SELECT id, at, author, body FROM messages ORDER BY id DESC LIMIT ?1")
    .bind(most)
    .all();
  return (results ?? []);
}

/**
 * Says one thing, as whoever the token said you are.
 *
 * The author is never taken from the request. It comes from the session, which
 * came from the password. A body that arrived with a name in it is ignored,
 * which is the difference between a name and a claim.
 */
export async function say(db, author, body, at) {
  const text = body.trim();
  if (!text) {
    throw new Refused("an empty message says nothing");
  }
  if (text.length > MOST_CHARS) {
    throw new Refused(`messages stop at ${MOST_CHARS} characters, this one was ${text.length}`);
  }

  const row = await db
    .prepare("INSERT INTO messages (at, author, body) VALUES (?1, ?2, ?3) RETURNING id, at, author, body")
    .bind(at, author, text)
    .first();
  if (!row) {
    throw new Refused("the message was not written");
  }
  return row;
}

/** Something the caller did wrong, as opposed to something that broke. */
export class Refused extends Error {}
