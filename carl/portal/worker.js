// The room. One conversation that people and agents are both in.
//
// Three routes and nothing else:
//
//   GET  /            the page a person opens
//   POST /say         put a message in the room
//   GET  /read?after= everything after an id
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

const PAGE_CACHE = "no-store";

/// Hex sha256, which is how PASSWORDS stores them.
async function hashOf(text) {
  const bytes = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/// Who is asking, from the password alone.
///
/// Returns null rather than throwing, so every route refuses the same way. Comparison is over
/// the hash rather than the password so a wrong guess never reaches a string compare against
/// the real one.
async function whoIsAsking(request, env) {
  const header = request.headers.get("Authorization") || "";
  const token = header.replace(/^Bearer\s+/i, "").trim();
  if (!token) return null;

  let people;
  try {
    people = JSON.parse(env.PASSWORDS);
  } catch {
    return null;
  }
  const presented = await hashOf(token);
  for (const [name, hash] of Object.entries(people)) {
    // Fixed length compare. The hashes are the same length so this leaks nothing useful, and
    // the alternative invites a timing argument nobody wants to have about a room.
    if (presented.length === hash.length && presented === hash) return name;
  }
  return null;
}

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

    const who = await whoIsAsking(request, env);
    if (!who) {
      // The same answer for a missing password and a wrong one. Telling them apart tells
      // somebody guessing which half they got right.
      return json({ error: "not a password this room knows" }, 401);
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

// The page, inline so the room is one file to deploy and cannot get out of step with its own
// API. It is small enough that a build step would cost more than it saves.
const ROOM_HTML = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>The room</title>
<style>
  :root { color-scheme: light dark; --line: #8884; }
  body { margin: 0; font: 15px/1.5 system-ui, sans-serif; display: flex; flex-direction: column; height: 100vh; }
  header { padding: 10px 14px; border-bottom: 1px solid var(--line); font-weight: 600; }
  header small { font-weight: 400; opacity: .7; }
  #room { flex: 1; overflow-y: auto; padding: 14px; }
  .said { margin: 0 0 10px; }
  .who { font-weight: 600; }
  .at { opacity: .55; font-size: 12px; margin-left: 6px; }
  .text { white-space: pre-wrap; overflow-wrap: anywhere; }
  form { display: flex; gap: 8px; padding: 10px; border-top: 1px solid var(--line); }
  input, button { font: inherit; padding: 9px 11px; border: 1px solid var(--line); border-radius: 8px; background: transparent; color: inherit; }
  #text { flex: 1; }
  button { cursor: pointer; }
  #trouble { padding: 0 14px 10px; color: #c33; }
</style>
</head>
<body>
<header>The room <small>JJ, Hunter, Atlas and Carl. Everything here is kept.</small></header>
<div id="room"></div>
<div id="trouble"></div>
<form id="send">
  <input id="password" type="password" placeholder="your password" autocomplete="current-password">
  <input id="text" placeholder="say something" autocomplete="off">
  <button>Send</button>
</form>
<script>
  const room = document.getElementById("room");
  const trouble = document.getElementById("trouble");
  const password = document.getElementById("password");
  const text = document.getElementById("text");
  let seen = 0;

  // The password stays in the tab. Kept so a refresh does not log you out mid conversation,
  // and never sent anywhere but this room's own API.
  password.value = sessionStorage.getItem("portal") || "";
  password.addEventListener("change", () => sessionStorage.setItem("portal", password.value));

  function show(said) {
    const when = new Date(said.at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    const el = document.createElement("p");
    el.className = "said";
    const who = document.createElement("span");
    who.className = "who";
    who.textContent = said.who;
    const at = document.createElement("span");
    at.className = "at";
    at.textContent = when;
    const body = document.createElement("span");
    body.className = "text";
    // textContent, not innerHTML. The room has agents writing into it and a message is data.
    body.textContent = " " + said.text;
    el.append(who, at, document.createElement("br"), body);
    room.append(el);
    room.scrollTop = room.scrollHeight;
  }

  async function poll() {
    if (!password.value) return;
    try {
      const res = await fetch("/read?after=" + seen, {
        headers: { Authorization: "Bearer " + password.value },
      });
      if (res.status === 401) { trouble.textContent = "That password is not one this room knows."; return; }
      if (!res.ok) { trouble.textContent = "The room answered " + res.status + "."; return; }
      trouble.textContent = "";
      for (const said of await res.json()) { show(said); seen = said.id; }
    } catch (e) {
      trouble.textContent = "Could not reach the room.";
    }
  }

  document.getElementById("send").addEventListener("submit", async (e) => {
    e.preventDefault();
    const words = text.value.trim();
    if (!words || !password.value) return;
    text.value = "";
    try {
      const res = await fetch("/say", {
        method: "POST",
        headers: { Authorization: "Bearer " + password.value, "Content-Type": "application/json" },
        body: JSON.stringify({ text: words }),
      });
      if (!res.ok) {
        trouble.textContent = res.status === 401 ? "That password is not one this room knows." : "The room refused that.";
        text.value = words;   // handed back rather than lost
        return;
      }
      await poll();
    } catch {
      trouble.textContent = "Could not reach the room.";
      text.value = words;
    }
  });

  poll();
  setInterval(poll, 3000);
</script>
</body>
</html>`;
