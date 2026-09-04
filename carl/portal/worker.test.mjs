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

/// A D1 stand in that keeps the room in an array.
///
/// Enough to answer the two statements the worker makes, and no more. The point is to exercise
/// the worker's decisions, not to reimplement SQLite.
function fakeDb() {
  const rows = [];
  let next = 1;
  return {
    rows,
    prepare(sql) {
      let bound = [];
      const self = {
        bind(...args) {
          bound = args;
          return self;
        },
        async first() {
          const [who, text, at] = bound;
          const row = { id: next++, who, text, at };
          rows.push(row);
          return row;
        },
        async all() {
          const after = bound[0];
          return { results: rows.filter((r) => r.id > after) };
        },
      };
      assert.ok(sql.includes("said"), "the worker asked for something other than the room");
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
  assert.deepEqual(e.DB.rows.map((r) => r.who), ["JJ", "Carl"]);
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
  assert.equal(e.DB.rows.length, 0);
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
  assert.equal(e.DB.rows.length, 0);
});

test("the sender's clock is not consulted", async () => {
  const e = await env();
  // A message claiming to be from 1970, which would sort to the top of the room forever.
  await worker.fetch(say("carl-password", { text: "hello", at: 0 }), e);
  const now = Math.floor(Date.now() / 1000);
  assert.ok(Math.abs(e.DB.rows[0].at - now) < 5, `stored ${e.DB.rows[0].at}, now ${now}`);
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
