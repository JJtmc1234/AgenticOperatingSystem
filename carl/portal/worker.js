// The room. One conversation that people and agents are both in.
//
// Three routes and nothing else:
//
//   GET  /            the page a person opens
//   POST /say         put a message in the room
//   GET  /read?after= everything after an id
//   GET  /me          which name the password earned, and whether it may let people in
//   POST /ask         somebody asking to join, the one route that needs no password
//   GET  /people/asked, POST /people/let-in, POST /people/turn-down   the owner's three
//
// There is no delete and no edit, on purpose. The room has JJ's mentor in it and the value of a
// shared record is that nobody can quietly change it afterwards. Getting something wrong and
// saying so in the next message is the correction.
//
// **A name is earned by a password, never chosen.** Every request carries a bearer token, the
// worker hashes it and looks for a matching hash in PASSWORDS, and the name attached to the
// message is the one that hash is filed under. Nothing in the request body can change it, so
// Carl cannot post as Hunter and a compromised agent cannot post as JJ. This is the whole
// security model and it is why the client never sends a name.

import { ROOM_HTML } from "./page.js";
import { ask, isOwner, letIn, pending, turnDown, whoIsAsking } from "./people.js";

const PAGE_CACHE = "no-store";

const json = (value, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json; charset=utf-8", "Cache-Control": PAGE_CACHE },
  });

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/" && request.method === "GET") {
      return new Response(ROOM_HTML, {
        headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": PAGE_CACHE },
      });
    }

    // The only route that needs no password, because somebody who has none is exactly who uses
    // it. Everything it accepts is checked in people.js rather than trusted.
    if (url.pathname === "/ask" && request.method === "POST") {
      let body;
      try {
        body = await request.json();
      } catch {
        return json({ error: "that was not json" }, 400);
      }
      const out = await ask(env, body);
      return out.error ? json({ error: out.error }, out.status) : json(out);
    }

    const who = await whoIsAsking(request, env);
    if (!who) {
      // The same answer for a missing password and a wrong one. Telling them apart tells
      // somebody guessing which half they got right.
      return json({ error: "not a password this room knows" }, 401);
    }

    if (url.pathname === "/me" && request.method === "GET") {
      // What the gate asks before it shows anybody the room. It returns the name the password
      // earned rather than confirming a name that was offered, so the page has nothing to check
      // against except the room's own answer.
      return json({ who, owner: isOwner(who, env) });
    }

    // Letting people in. Owner only, and the check is here rather than in the page, because a
    // page can be edited by whoever is looking at it and this cannot.
    if (url.pathname.startsWith("/people/")) {
      if (!isOwner(who, env)) return json({ error: "only the owner lets people in" }, 403);

      if (url.pathname === "/people/asked" && request.method === "GET") {
        return json(await pending(env));
      }

      const settle = url.pathname === "/people/let-in" ? letIn
        : url.pathname === "/people/turn-down" ? turnDown
        : null;
      if (settle && request.method === "POST") {
        let body;
        try {
          body = await request.json();
        } catch {
          return json({ error: "that was not json" }, 400);
        }
        const id = Number.parseInt(body.id, 10);
        if (!Number.isFinite(id)) return json({ error: "which request?" }, 400);
        const out = await settle(env, id);
        return out.error ? json({ error: out.error }, out.status) : json(out);
      }
    }

    if (url.pathname === "/say" && request.method === "POST") {
      let body;
      try {
        body = await request.json();
      } catch {
        return json({ error: "that was not json" }, 400);
      }
      const text = (body.text || "").toString().trim();
      if (!text) return json({ error: "nothing to say" }, 400);
      if (text.length > 4000) return json({ error: "too long for one message" }, 400);

      const at = Math.floor(Date.now() / 1000);
      // The server's clock, never the sender's. An agent with a wrong clock would otherwise
      // reorder the room for everybody reading it.
      const row = await env.DB.prepare(
        "INSERT INTO said (who, text, at) VALUES (?, ?, ?) RETURNING id, who, text, at",
      )
        .bind(who, text, at)
        .first();
      return json(row);
    }

    if (url.pathname === "/read" && request.method === "GET") {
      const after = Number.parseInt(url.searchParams.get("after") || "0", 10);
      const from = Number.isFinite(after) && after > 0 ? after : 0;
      const { results } = await env.DB.prepare(
        "SELECT id, who, text, at FROM said WHERE id > ? ORDER BY id ASC LIMIT 500",
      )
        .bind(from)
        .all();
      return json(results || []);
    }

    return json({ error: "no such route" }, 404);
  },
};
