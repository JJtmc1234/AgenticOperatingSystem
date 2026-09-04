// Who the room knows, and how somebody new comes to be one of them.
//
// There are two sources of a name and they are deliberately different things. `PASSWORDS` is the
// founders, set as a deployed secret and changed only by somebody with the Cloudflare account.
// The `person` table is everybody let in since, and it is changed from inside the room by the
// owner. Nothing in the table can shadow the secret, because the secret is looked at first.
//
// **A name is still earned by a password.** Adding a person adds a hash and the name it sits
// against. It does not add a way to choose a name at request time, and `whoIsAsking` still
// returns whichever name the presented hash was filed under.

/// Hex sha256, which is how both the secret and the table store them.
export async function hashOf(text) {
  const bytes = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/// A fixed length compare. The hashes are all the same length so this leaks nothing useful, and
/// the alternative invites a timing argument nobody wants to have about a room.
const same = (a, b) => a.length === b.length && a === b;

function founders(env) {
  try {
    return JSON.parse(env.PASSWORDS);
  } catch {
    return {};
  }
}

/// Who is asking, from the password alone.
///
/// Returns null rather than throwing, so every route refuses the same way.
export async function whoIsAsking(request, env) {
  const header = request.headers.get("Authorization") || "";
  const token = header.replace(/^Bearer\s+/i, "").trim();
  if (!token) return null;
  const presented = await hashOf(token);

  // The secret first. A row in the table can then never take over a founder's name, whatever
  // ends up in the table and however it got there.
  for (const [name, hash] of Object.entries(founders(env))) {
    if (same(presented, hash)) return name;
  }

  const row = await env.DB.prepare("SELECT name FROM person WHERE hash = ?").bind(presented).first();
  return row ? row.name : null;
}

/// Whoever may let people in. One person, named in the config rather than inferred from rank,
/// because "the first name in PASSWORDS" would quietly move the moment that object is reordered.
export const isOwner = (who, env) => who === (env.OWNER || "JJ");

/// A name has to be free everywhere before it can be given out.
///
/// Two people under one name is the failure this exists to stop. The room's whole worth is that
/// a line saying "Hunter" was written by Hunter, and a second Hunter would break that quietly
/// rather than loudly.
async function nameIsFree(env, name) {
  const wanted = name.toLowerCase();
  for (const taken of Object.keys(founders(env))) {
    if (taken.toLowerCase() === wanted) return false;
  }
  const row = await env.DB.prepare("SELECT name FROM person WHERE lower(name) = ?").bind(wanted).first();
  return !row;
}

/// A password already in use would make the hash ambiguous, and whoever asked second would find
/// themselves posting under somebody else's name.
async function hashIsFree(env, hash) {
  for (const taken of Object.values(founders(env))) {
    if (same(hash, taken)) return false;
  }
  const row = await env.DB.prepare("SELECT name FROM person WHERE hash = ?").bind(hash).first();
  return !row;
}

const NAME_SHAPE = /^[\p{L}\p{N}][\p{L}\p{N} '\-]{0,23}$/u;

/// Somebody asking to be let in. The only route in the room that needs no password, so
/// everything it accepts is checked here rather than trusted.
export async function ask(env, body) {
  const name = (body.name || "").toString().trim();
  const password = (body.password || "").toString();
  const why = (body.why || "").toString().trim().slice(0, 200);

  if (!NAME_SHAPE.test(name)) {
    return { error: "a name is 1 to 24 letters, numbers, spaces, hyphens or apostrophes", status: 400 };
  }
  // Long enough to be worth having. This is an identity in a room that keeps everything, not a
  // login to a forum.
  if (password.length < 8) return { error: "a password needs at least 8 characters", status: 400 };
  if (password.length > 200) return { error: "that password is too long", status: 400 };

  // A cap rather than real rate limiting. This route is open to anybody who has the address, and
  // without a ceiling one afternoon of nonsense would bury a real request in the owner's list.
  const waiting = await env.DB.prepare("SELECT COUNT(*) AS n FROM asked WHERE settled = 0").first();
  if (waiting && waiting.n >= 20) {
    return { error: "there are already too many requests waiting. Try later", status: 429 };
  }

  if (!(await nameIsFree(env, name))) return { error: "somebody already goes by that name here", status: 409 };

  const hash = await hashOf(password);
  if (!(await hashIsFree(env, hash))) return { error: "pick a different password", status: 409 };

  const already = await env.DB.prepare(
    "SELECT id FROM asked WHERE lower(name) = ? AND settled = 0",
  )
    .bind(name.toLowerCase())
    .first();
  if (already) return { error: "that name is already waiting to be let in", status: 409 };

  await env.DB.prepare("INSERT INTO asked (name, hash, why, at, settled) VALUES (?, ?, ?, ?, 0)")
    .bind(name, hash, why, Math.floor(Date.now() / 1000))
    .run();
  return { asked: true };
}

/// What the owner sees. The hash never leaves the server, because it is the one thing in the row
/// that is worth stealing and the owner has no use for it.
export async function pending(env) {
  const { results } = await env.DB.prepare(
    "SELECT id, name, why, at FROM asked WHERE settled = 0 ORDER BY id ASC LIMIT 50",
  ).all();
  return results || [];
}

/// Letting somebody in. The name and the hash are taken from the request as it was made, never
/// from the approval, so the owner cannot be talked into approving one name and creating another.
export async function letIn(env, id) {
  const row = await env.DB.prepare("SELECT id, name, hash FROM asked WHERE id = ? AND settled = 0")
    .bind(id)
    .first();
  if (!row) return { error: "no request waiting with that id", status: 404 };

  // Checked again here, not only when it was asked. Two requests for the same name can both be
  // waiting, and approving the second one after the first would otherwise put two people in the
  // room under one name.
  if (!(await nameIsFree(env, row.name))) {
    await env.DB.prepare("UPDATE asked SET settled = 2 WHERE id = ?").bind(id).run();
    return { error: "somebody goes by that name now, so this request was turned down", status: 409 };
  }
  if (!(await hashIsFree(env, row.hash))) {
    await env.DB.prepare("UPDATE asked SET settled = 2 WHERE id = ?").bind(id).run();
    return { error: "that password is already in use, so this request was turned down", status: 409 };
  }

  await env.DB.prepare("INSERT INTO person (name, hash, added) VALUES (?, ?, ?)")
    .bind(row.name, row.hash, Math.floor(Date.now() / 1000))
    .run();
  await env.DB.prepare("UPDATE asked SET settled = 1 WHERE id = ?").bind(id).run();
  return { letIn: row.name };
}

export async function turnDown(env, id) {
  const row = await env.DB.prepare("SELECT id, name FROM asked WHERE id = ? AND settled = 0")
    .bind(id)
    .first();
  if (!row) return { error: "no request waiting with that id", status: 404 };
  await env.DB.prepare("UPDATE asked SET settled = 2 WHERE id = ?").bind(id).run();
  return { turnedDown: row.name };
}
