// The room's own rules, checked without deploying it.
//
// The one worth checking hardest is that a name comes from a password and cannot be chosen. It
// is the whole security model, it is one `for` loop, and if it ever stops being true the
// transcript stops meaning anything.
//
//   node --test portal/worker.test.mjs

import { test } from "node:test";
import assert from "node:assert/strict";
import worker from "./worker.js";

const sha256 = async (text) => {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
};

/// A D1 stand in that keeps the room, the members and the requests in three arrays.
///
/// Enough to answer the statements the worker actually makes, and no more. It dispatches on the
/// table named in the SQL rather than reimplementing SQLite, so a query the worker has not been
/// written to make will fail loudly here instead of quietly returning nothing.
function fakeDb(seed = {}) {
  const said = [];
  const person = [...(seed.person || [])];
  const asked = [...(seed.asked || [])];
  let nextSaid = 1;
  let nextAsked = 1;

  return {
    said,
    person,
    asked,
    prepare(sql) {
      let bound = [];
      const one = () => {
        const q = sql.replace(/\s+/g, " ");

        if (q.includes("INSERT INTO said")) {
          const [who, text, at] = bound;
          const row = { id: nextSaid++, who, text, at };
          said.push(row);
          return row;
        }
        if (q.includes("FROM person") && q.includes("hash = ?")) {
          return person.find((p) => p.hash === bound[0]) || null;
        }
        if (q.includes("FROM person") && q.includes("lower(name)")) {
          return person.find((p) => p.name.toLowerCase() === bound[0]) || null;
        }
        if (q.includes("COUNT(*)") && q.includes("FROM asked")) {
          return { n: asked.filter((a) => a.settled === 0).length };
        }
        if (q.includes("FROM asked") && q.includes("lower(name)")) {
          return asked.find((a) => a.name.toLowerCase() === bound[0] && a.settled === 0) || null;
        }
        if (q.includes("FROM asked") && q.includes("id = ?")) {
          return asked.find((a) => a.id === bound[0] && a.settled === 0) || null;
        }
        throw new Error("the fake database was asked something it does not answer: " + q);
      };

      const self = {
        bind(...args) {
          bound = args;
          return self;
        },
        async first() {
          return one();
        },
        async run() {
          const q = sql.replace(/\s+/g, " ");
          if (q.includes("INSERT INTO asked")) {
            const [name, hash, why, at] = bound;
            asked.push({ id: nextAsked++, name, hash, why, at, settled: 0 });
            return { success: true };
          }
          if (q.includes("INSERT INTO person")) {
            const [name, hash, added] = bound;
            person.push({ name, hash, added });
            return { success: true };
          }
          if (q.includes("UPDATE asked SET settled")) {
            const settled = Number(q.match(/settled = (\d+)/)[1]);
            const row = asked.find((a) => a.id === bound[0]);
            if (row) row.settled = settled;
            return { success: true };
          }
          throw new Error("the fake database was told to run something it does not: " + q);
        },
        async all() {
          const q = sql.replace(/\s+/g, " ");
          if (q.includes("FROM said")) {
            return { results: said.filter((r) => r.id > bound[0]) };
          }
          if (q.includes("FROM asked")) {
            return {
              results: asked
                .filter((a) => a.settled === 0)
                .map(({ id, name, why, at }) => ({ id, name, why, at })),
            };
          }
          throw new Error("the fake database was asked to list something it does not: " + q);
        },
      };
      return self;
    },
  };
}

async function env() {
  return {
    DB: fakeDb(),
    PASSWORDS: JSON.stringify({
      JJ: await sha256("jj-password"),
      Hunter: await sha256("hunter-password"),
      Carl: await sha256("carl-password"),
    }),
  };
}

const say = (password, body) =>
  new Request("https://room/say", {
    method: "POST",
    headers: { Authorization: `Bearer ${password}`, "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

const read = (password, after = 0) =>
  new Request(`https://room/read?after=${after}`, {
    headers: { Authorization: `Bearer ${password}` },
  });

test("the name comes from the password, and the body cannot change it", async () => {
  const e = await env();
  // Carl's password, asking to be called Hunter. This is the attack the design exists for.
  const res = await worker.fetch(say("carl-password", { text: "hello", who: "Hunter" }), e);
  assert.equal(res.status, 200);
  const said = await res.json();
  assert.equal(said.who, "Carl");
});

test("two agents with different passwords get different names", async () => {
  const e = await env();
  await worker.fetch(say("jj-password", { text: "one" }), e);
  await worker.fetch(say("carl-password", { text: "two" }), e);
  assert.deepEqual(e.DB.said.map((r) => r.who), ["JJ", "Carl"]);
});

test("a wrong password and a missing one are refused the same way", async () => {
  const e = await env();
  const wrong = await worker.fetch(say("not-a-password", { text: "hello" }), e);
  const missing = await worker.fetch(
    new Request("https://room/say", { method: "POST", body: "{}" }),
    e,
  );
  assert.equal(wrong.status, 401);
  assert.equal(missing.status, 401);
  // Telling them apart tells somebody guessing which half they got right.
  assert.deepEqual(await wrong.json(), await missing.json());
  assert.equal(e.DB.said.length, 0);
});

test("reading returns everything after the watermark, oldest first", async () => {
  const e = await env();
  await worker.fetch(say("jj-password", { text: "one" }), e);
  await worker.fetch(say("hunter-password", { text: "two" }), e);
  await worker.fetch(say("carl-password", { text: "three" }), e);

  const all = await (await worker.fetch(read("jj-password", 0), e)).json();
  assert.deepEqual(all.map((r) => r.text), ["one", "two", "three"]);

  const since = await (await worker.fetch(read("jj-password", 1), e)).json();
  assert.deepEqual(since.map((r) => r.text), ["two", "three"]);
});

test("an empty or oversized message is refused rather than stored", async () => {
  const e = await env();
  assert.equal((await worker.fetch(say("jj-password", { text: "   " }), e)).status, 400);
  const huge = "x".repeat(4001);
  assert.equal((await worker.fetch(say("jj-password", { text: huge }), e)).status, 400);
  assert.equal(e.DB.said.length, 0);
});

test("the sender's clock is not consulted", async () => {
  const e = await env();
  // A message claiming to be from 1970, which would sort to the top of the room forever.
  await worker.fetch(say("carl-password", { text: "hello", at: 0 }), e);
  const now = Math.floor(Date.now() / 1000);
  assert.ok(Math.abs(e.DB.said[0].at - now) < 5, `stored ${e.DB.said[0].at}, now ${now}`);
});

test("the page is served without a password, and nothing else is", async () => {
  const e = await env();
  const page = await worker.fetch(new Request("https://room/"), e);
  assert.equal(page.status, 200);
  assert.match(page.headers.get("Content-Type"), /text\/html/);

  const nope = await worker.fetch(new Request("https://room/read"), e);
  assert.equal(nope.status, 401);
});

test("the page puts message text in as text rather than as markup", async () => {
  const e = await env();
  const html = await (await worker.fetch(new Request("https://room/"), e)).text();
  // Agents write into this room. A message is data, and the page has to treat it that way.
  assert.ok(html.includes("textContent"), "the page does not use textContent");
  // An assignment, not the bare word. The comment above the code says "not innerHTML", and a
  // substring check would fail on the comment explaining why it is safe.
  assert.doesNotMatch(html, /\.innerHTML\s*=/, "the page assigns innerHTML somewhere");
});

test("an unknown route says so rather than serving the page", async () => {
  const e = await env();
  const nowhere = new Request("https://room/nowhere", {
    headers: { Authorization: "Bearer jj-password" },
  });
  assert.equal((await worker.fetch(nowhere, e)).status, 404);
});

test("/me names whoever the password belongs to", async () => {
  const e = await env();
  const res = await worker.fetch(
    new Request("https://room/me", { headers: { Authorization: "Bearer carl-password" } }),
    e,
  );
  assert.equal(res.status, 200);
  // `owner` rides along so the page knows whether to offer the requests panel. Carl is not it.
  assert.deepEqual(await res.json(), { who: "Carl", owner: false });
});

test("/me is behind the same door as everything else", async () => {
  const e = await env();
  // The gate asks this before it shows anybody the room, so a wrong password has to be refused
  // here too. If it were readable without one it would confirm which names exist.
  for (const headers of [{}, { Authorization: "Bearer not-a-password" }]) {
    const res = await worker.fetch(new Request("https://room/me", { headers }), e);
    assert.equal(res.status, 401);
  }
});

test("the gate checks the name against the room rather than believing it", async () => {
  const e = await env();
  const html = await (await worker.fetch(new Request("https://room/"), e)).text();
  // The name box is a check, not a choice. The page has to ask /me and compare, because a page
  // that trusted the box would let somebody sign in as Hunter with Carl's password and be
  // surprised by every message they sent afterwards.
  assert.ok(html.includes("/me"), "the gate never asks the room who the password is");
  assert.ok(html.includes("belongs to"), "the gate does not say whose password it actually is");
});

test("a password is only ever stored on the device that typed it", async () => {
  const e = await env();
  const html = await (await worker.fetch(new Request("https://room/"), e)).text();
  // "Stay signed in" is localStorage and the default is sessionStorage. Neither leaves the
  // browser, and there is no third place for a password to end up.
  assert.ok(html.includes("localStorage"), "nothing survives closing the browser");
  assert.ok(html.includes("sessionStorage"), "the default outlives the tab");
  assert.doesNotMatch(html, /document\.cookie/, "the page puts a password in a cookie");
});

/// Asking to be let in, and the owner deciding.

const asking = (body) =>
  new Request("https://room/ask", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

const owned = (password, path, body) =>
  new Request("https://room" + path, {
    method: body === undefined ? "GET" : "POST",
    headers: { Authorization: `Bearer ${password}`, "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

test("somebody with no password can ask, and cannot do anything else", async () => {
  const e = await env();
  const asked = await worker.fetch(asking({ name: "Mum", password: "a good long one", why: "JJ asked me" }), e);
  assert.equal(asked.status, 200);
  assert.deepEqual(await asked.json(), { asked: true });
  assert.equal(e.DB.asked.length, 1);

  // Asking is not being in. Nothing else opens without a password that already works.
  for (const path of ["/read", "/me", "/people/asked"]) {
    assert.equal((await worker.fetch(new Request("https://room" + path), e)).status, 401);
  }
});

test("the password never comes back out of a request", async () => {
  const e = await env();
  await worker.fetch(asking({ name: "Mum", password: "a good long one" }), e);
  const list = await (await worker.fetch(owned("jj-password", "/people/asked"), e)).json();
  // The owner has no use for the hash and it is the only thing in the row worth stealing.
  assert.equal(list.length, 1);
  assert.equal(list[0].name, "Mum");
  assert.ok(!("hash" in list[0]), "the hash was handed to the owner");
});

test("only the owner sees or settles requests", async () => {
  const e = await env();
  await worker.fetch(asking({ name: "Mum", password: "a good long one" }), e);
  // Hunter is in the room and Carl is an agent in it. Neither decides who else gets in.
  for (const password of ["hunter-password", "carl-password"]) {
    assert.equal((await worker.fetch(owned(password, "/people/asked"), e)).status, 403);
    assert.equal((await worker.fetch(owned(password, "/people/let-in", { id: 1 }), e)).status, 403);
  }
  assert.equal(e.DB.person.length, 0, "somebody who is not the owner let a person in");
});

test("being let in is what makes a password work", async () => {
  const e = await env();
  await worker.fetch(asking({ name: "Mum", password: "a good long one" }), e);

  // Before: the password is not one the room knows.
  assert.equal((await worker.fetch(read("a good long one"), e)).status, 401);

  const done = await worker.fetch(owned("jj-password", "/people/let-in", { id: 1 }), e);
  assert.equal(done.status, 200);
  assert.deepEqual(await done.json(), { letIn: "Mum" });

  // After: it works, and it carries the name from the request rather than any name sent later.
  const mine = await worker.fetch(say("a good long one", { who: "JJ", text: "hello all" }), e);
  assert.equal((await mine.json()).who, "Mum");
});

test("turning somebody down leaves the password not working", async () => {
  const e = await env();
  await worker.fetch(asking({ name: "Mum", password: "a good long one" }), e);
  const done = await worker.fetch(owned("jj-password", "/people/turn-down", { id: 1 }), e);
  assert.deepEqual(await done.json(), { turnedDown: "Mum" });
  assert.equal(e.DB.person.length, 0);
  assert.equal((await worker.fetch(read("a good long one"), e)).status, 401);
  // And it is off the list rather than sitting there to be approved by accident later.
  assert.deepEqual(await (await worker.fetch(owned("jj-password", "/people/asked"), e)).json(), []);
});

test("a name already in the room cannot be asked for", async () => {
  const e = await env();
  // The whole worth of the record is that a line saying Hunter was written by Hunter.
  for (const name of ["Hunter", "hunter", "CARL"]) {
    const res = await worker.fetch(asking({ name, password: "a good long one" }), e);
    assert.equal(res.status, 409, `${name} was allowed`);
  }
  assert.equal(e.DB.asked.length, 0);
});

test("a password already in use cannot be asked for", async () => {
  const e = await env();
  // Two people on one hash means the second one posts under the first one's name.
  const res = await worker.fetch(asking({ name: "Mum", password: "hunter-password" }), e);
  assert.equal(res.status, 409);
  assert.equal(e.DB.asked.length, 0);
});

test("a rubbish name or a thin password is refused before it is stored", async () => {
  const e = await env();
  for (const body of [
    { name: "", password: "a good long one" },
    { name: "   ", password: "a good long one" },
    { name: "<script>alert(1)</script>", password: "a good long one" },
    { name: "Mum", password: "short" },
    { name: "M".repeat(25), password: "a good long one" },
  ]) {
    const res = await worker.fetch(asking(body), e);
    assert.equal(res.status, 400, JSON.stringify(body) + " was accepted");
  }
  assert.equal(e.DB.asked.length, 0);
});

test("the same name cannot be waiting twice", async () => {
  const e = await env();
  await worker.fetch(asking({ name: "Mum", password: "a good long one" }), e);
  const again = await worker.fetch(asking({ name: "mum", password: "a different long one" }), e);
  assert.equal(again.status, 409);
  assert.equal(e.DB.asked.length, 1);
});

test("the waiting list has a ceiling", async () => {
  const e = await env();
  // The one route a stranger can write to. Without a cap, an afternoon of nonsense buries the
  // real request the owner was waiting for.
  for (let i = 0; i < 20; i++) {
    const res = await worker.fetch(asking({ name: "Person" + i, password: "a good long one " + i }), e);
    assert.equal(res.status, 200);
  }
  const over = await worker.fetch(asking({ name: "One more", password: "a good long one x" }), e);
  assert.equal(over.status, 429);
  assert.equal(e.DB.asked.length, 20);
});

test("a request cannot be settled twice", async () => {
  const e = await env();
  await worker.fetch(asking({ name: "Mum", password: "a good long one" }), e);
  assert.equal((await worker.fetch(owned("jj-password", "/people/let-in", { id: 1 }), e)).status, 200);
  // The second one finds nothing waiting, rather than adding a second Mum.
  assert.equal((await worker.fetch(owned("jj-password", "/people/let-in", { id: 1 }), e)).status, 404);
  assert.equal(e.DB.person.length, 1);
});

test("the owner is named in the config rather than guessed", async () => {
  const e = await env();
  e.OWNER = "Hunter";
  assert.equal((await (await worker.fetch(owned("hunter-password", "/me"), e)).json()).owner, true);
  assert.equal((await (await worker.fetch(owned("jj-password", "/me"), e)).json()).owner, false);
});
