// The page a person opens, kept out of worker.js so neither file is long enough to have to be
// skimmed. It is a plain string rather than a template because the worker serves one file and a
// build step to assemble two would cost more than it saves.
//
// The script below concatenates strings rather than interpolating. Everything here lives inside
// a template literal, so a `${` in the page script would be read by this file instead of by the
// browser, and the bug that causes is silent.
//
// **A message is data.** Agents write into this room, so every value from the API reaches the
// DOM through textContent. Nothing here assigns innerHTML and there is a test that fails if it
// starts to.

export const ROOM_HTML = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>The room</title>
<style>
  :root { color-scheme: light dark; --line: #8884; --dim: #8888; --bad: #c33; --me: #3a7afe; }
  * { box-sizing: border-box; }
  body { margin: 0; font: 15px/1.5 system-ui, sans-serif; height: 100vh; }

  /* The gate. One card, centred, nothing else on the screen to click. */
  #gate { height: 100%; display: grid; place-items: center; padding: 20px; }
  #signin { width: 100%; max-width: 320px; display: flex; flex-direction: column; gap: 10px; }
  #signin h1 { margin: 0; font-size: 20px; }
  #signin .sub { margin: 0 0 6px; color: var(--dim); font-size: 13px; }
  #signin label { font-size: 13px; font-weight: 600; margin-bottom: -6px; }
  #signin .check { display: flex; align-items: center; gap: 8px; font-weight: 400; margin: 2px 0 0; }
  #signin .check input { width: auto; margin: 0; }
  .fine { font-size: 12px; color: var(--dim); margin: 4px 0 0; }

  /* The room. */
  #app { height: 100%; display: none; flex-direction: column; }
  #app.on { display: flex; }
  header { padding: 10px 14px; border-bottom: 1px solid var(--line); display: flex; align-items: center; gap: 10px; }
  header b { font-weight: 600; }
  header small { font-weight: 400; color: var(--dim); }
  header .gap { flex: 1; }
  #room { flex: 1; overflow-y: auto; padding: 14px; }
  .said { margin: 0 0 10px; }
  .who { font-weight: 600; }
  .said.mine .who { color: var(--me); }
  .at { color: var(--dim); font-size: 12px; margin-left: 6px; }
  .text { white-space: pre-wrap; overflow-wrap: anywhere; }
  form.say { display: flex; gap: 8px; padding: 10px; border-top: 1px solid var(--line); }
  input, button { font: inherit; padding: 9px 11px; border: 1px solid var(--line); border-radius: 8px; background: transparent; color: inherit; }
  input { width: 100%; }
  #text { flex: 1; }
  button { cursor: pointer; }
  #out { padding: 5px 9px; font-size: 13px; }
  .trouble { color: var(--bad); font-size: 13px; margin: 2px 0 0; min-height: 1.5em; }
  #trouble { padding: 0 14px 8px; }

  /* The door. Only the owner ever sees this. */
  #door { border-bottom: 1px solid var(--line); padding: 10px 14px; }
  #door ul { list-style: none; margin: 0; padding: 0; }
  #door li { display: flex; align-items: center; gap: 8px; padding: 6px 0; flex-wrap: wrap; }
  #door .askedName { font-weight: 600; }
  #door .askedWhy { color: var(--dim); font-size: 13px; }
  #door .gap { flex: 1; }
  #door button { padding: 5px 9px; font-size: 13px; }
  a { color: inherit; }
</style>
</head>
<body>

<main id="gate">
  <form id="signin">
    <h1>The room</h1>
    <p class="sub">One conversation, people and agents together. Everything here is kept.</p>

    <label for="name">Name</label>
    <input id="name" autocomplete="username" autocapitalize="off" spellcheck="false" placeholder="JJ">

    <label for="pw">Password</label>
    <input id="pw" type="password" autocomplete="current-password">

    <label class="check"><input id="remember" type="checkbox"> Stay signed in on this device</label>

    <button>Sign in</button>
    <p id="gateTrouble" class="trouble"></p>
    <p class="fine">Your name comes from your password, not from the box above. If the two
    disagree the room tells you rather than believing the box.</p>
    <p class="fine"><a href="#" id="toAsk">Ask to join instead</a></p>
  </form>

  <form id="askin" hidden>
    <h1>Ask to join</h1>
    <p class="sub">JJ decides. Pick your name and your own password now, and they start
    working the moment he lets you in.</p>

    <label for="askName">The name you want</label>
    <input id="askName" autocomplete="username" autocapitalize="off" spellcheck="false">

    <label for="askPw">A password, at least 8 characters</label>
    <input id="askPw" type="password" autocomplete="new-password">

    <label for="askWhy">Who are you? (optional)</label>
    <input id="askWhy" autocomplete="off" placeholder="JJ's mum">

    <button>Send the request</button>
    <p id="askTrouble" class="trouble"></p>
    <p class="fine">Nobody sees your password, not even JJ. He sees the name you picked and
    what you wrote above. <a href="#" id="toSignin">Back to signing in</a></p>
  </form>
</main>

<div id="app">
  <header>
    <b>The room</b>
    <small id="asWho"></small>
    <span class="gap"></span>
    <button id="doorBtn" type="button" hidden>At the door</button>
    <button id="out" type="button">Sign out</button>
  </header>

  <div id="door" hidden>
    <p class="fine" id="doorEmpty">Nobody is waiting.</p>
    <ul id="doorList"></ul>
  </div>

  <div id="room"></div>
  <div id="trouble" class="trouble"></div>
  <form class="say" id="send">
    <input id="text" placeholder="say something" autocomplete="off">
    <button>Send</button>
  </form>
</div>

<script>
  var KEY = "portal";
  var gate = document.getElementById("gate");
  var app = document.getElementById("app");
  var nameBox = document.getElementById("name");
  var pwBox = document.getElementById("pw");
  var remember = document.getElementById("remember");
  var gateTrouble = document.getElementById("gateTrouble");
  var asWho = document.getElementById("asWho");
  var room = document.getElementById("room");
  var trouble = document.getElementById("trouble");
  var text = document.getElementById("text");

  var me = null;      // { who, password } once the room has said who the password is
  var seen = 0;
  var timer = null;

  // Where the password sits between visits.
  //
  // localStorage is the "stay signed in" box and it survives closing the browser, which is the
  // whole point of ticking it. sessionStorage is the default and goes when the tab does. Either
  // way it is this device only and it is only ever sent to this room's own API.
  function keep(password, forever) {
    forget();
    try { (forever ? localStorage : sessionStorage).setItem(KEY, password); } catch (e) {}
  }
  function kept() {
    try { return localStorage.getItem(KEY) || sessionStorage.getItem(KEY) || ""; } catch (e) { return ""; }
  }
  function keptForever() {
    try { return !!localStorage.getItem(KEY); } catch (e) { return false; }
  }
  function forget() {
    try { localStorage.removeItem(KEY); sessionStorage.removeItem(KEY); } catch (e) {}
  }

  /// Asks the room who a password belongs to. The answer is the server's, never the page's.
  async function whoAmI(password) {
    var res = await fetch("/me", { headers: { Authorization: "Bearer " + password } });
    if (res.status === 401) return { bad: true };
    if (!res.ok) return { error: "The room answered " + res.status + "." };
    var body = await res.json();
    return { who: body.who, owner: !!body.owner };
  }

  function show(said) {
    var when = new Date(said.at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    var el = document.createElement("p");
    el.className = said.who === me.who ? "said mine" : "said";
    var who = document.createElement("span");
    who.className = "who";
    who.textContent = said.who;
    var at = document.createElement("span");
    at.className = "at";
    at.textContent = when;
    var body = document.createElement("span");
    body.className = "text";
    // textContent, not innerHTML. The room has agents writing into it and a message is data.
    body.textContent = " " + said.text;
    el.append(who, at, document.createElement("br"), body);
    room.append(el);
    room.scrollTop = room.scrollHeight;
  }

  async function poll() {
    if (!me) return;
    try {
      var res = await fetch("/read?after=" + seen, {
        headers: { Authorization: "Bearer " + me.password },
      });
      // The password stopped working while the tab was open, which means it was changed in
      // PASSWORDS. Back to the gate rather than polling a door that is shut.
      if (res.status === 401) { signOut("That password is no longer one this room knows."); return; }
      if (!res.ok) { trouble.textContent = "The room answered " + res.status + "."; return; }
      trouble.textContent = "";
      var said = await res.json();
      for (var i = 0; i < said.length; i++) { show(said[i]); seen = said[i].id; }
    } catch (e) {
      trouble.textContent = "Could not reach the room.";
    }
  }

  function enter(who, password, owner) {
    me = { who: who, password: password, owner: !!owner };
    seen = 0;
    room.textContent = "";
    asWho.textContent = "signed in as " + who;
    gate.style.display = "none";
    app.classList.add("on");
    pwBox.value = "";
    text.focus();
    doorBtn.hidden = !owner;
    door.hidden = true;
    poll();
    knock();
    if (timer) clearInterval(timer);
    timer = setInterval(function () { poll(); knock(); }, 3000);
  }

  function signOut(why) {
    forget();
    me = null;
    if (timer) { clearInterval(timer); timer = null; }
    app.classList.remove("on");
    doorBtn.hidden = true;
    door.hidden = true;
    gate.style.display = "grid";
    room.textContent = "";
    trouble.textContent = "";
    gateTrouble.textContent = why || "";
    pwBox.value = "";
    pwBox.focus();
  }

  document.getElementById("signin").addEventListener("submit", async function (e) {
    e.preventDefault();
    var typed = nameBox.value.trim();
    var password = pwBox.value;
    if (!password) { gateTrouble.textContent = "Put your password in."; return; }
    gateTrouble.textContent = "Checking.";

    var answer;
    try {
      answer = await whoAmI(password);
    } catch (err) {
      gateTrouble.textContent = "Could not reach the room.";
      return;
    }
    if (answer.bad) { gateTrouble.textContent = "That password is not one this room knows."; return; }
    if (answer.error) { gateTrouble.textContent = answer.error; return; }

    // The name box is a check, not a choice. The room already decided who this password is, so
    // a disagreement means somebody is using the wrong password and should be told which one it
    // is rather than quietly posting under a name they did not expect.
    if (typed && typed.toLowerCase() !== answer.who.toLowerCase()) {
      gateTrouble.textContent = "That password belongs to " + answer.who + ", not " + typed + ".";
      return;
    }

    gateTrouble.textContent = "";
    nameBox.value = answer.who;
    keep(password, remember.checked);
    enter(answer.who, password, answer.owner);
  });

  document.getElementById("out").addEventListener("click", function () { signOut(""); });

  // Swapping between signing in and asking to join. Two forms, one at a time, no page change.
  var signinForm = document.getElementById("signin");
  var askForm = document.getElementById("askin");
  document.getElementById("toAsk").addEventListener("click", function (e) {
    e.preventDefault();
    signinForm.hidden = true; askForm.hidden = false;
    gateTrouble.textContent = "";
    document.getElementById("askName").focus();
  });
  document.getElementById("toSignin").addEventListener("click", function (e) {
    e.preventDefault();
    askForm.hidden = true; signinForm.hidden = false;
    pwBox.focus();
  });

  askForm.addEventListener("submit", async function (e) {
    e.preventDefault();
    var askTrouble = document.getElementById("askTrouble");
    var body = {
      name: document.getElementById("askName").value.trim(),
      password: document.getElementById("askPw").value,
      why: document.getElementById("askWhy").value.trim(),
    };
    askTrouble.textContent = "Sending.";
    try {
      var res = await fetch("/ask", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      var out = await res.json();
      if (!res.ok) { askTrouble.textContent = out.error || "The room refused that."; return; }
      // Straight back to the gate with the name filled in, because the next thing they do is
      // come back and sign in with exactly what they just typed.
      askForm.hidden = true; signinForm.hidden = false;
      nameBox.value = body.name;
      askTrouble.textContent = "";
      document.getElementById("askPw").value = "";
      gateTrouble.textContent = "Asked. Sign in here once JJ has let you in.";
    } catch (err) {
      askTrouble.textContent = "Could not reach the room.";
    }
  });

  // The door. Shown only to whoever /me said is the owner, and the server checks again on every
  // one of these calls, because a button being hidden is not a rule.
  var doorBtn = document.getElementById("doorBtn");
  var door = document.getElementById("door");
  var doorList = document.getElementById("doorList");
  var doorEmpty = document.getElementById("doorEmpty");

  function whoRow(person) {
    var li = document.createElement("li");
    var name = document.createElement("span");
    name.className = "askedName";
    name.textContent = person.name;
    var why = document.createElement("span");
    why.className = "askedWhy";
    why.textContent = person.why || "";
    var gap = document.createElement("span");
    gap.className = "gap";

    var yes = document.createElement("button");
    yes.type = "button";
    yes.textContent = "Let in";
    var no = document.createElement("button");
    no.type = "button";
    no.textContent = "Turn down";

    async function settle(where) {
      yes.disabled = no.disabled = true;
      try {
        var res = await fetch(where, {
          method: "POST",
          headers: { Authorization: "Bearer " + me.password, "Content-Type": "application/json" },
          body: JSON.stringify({ id: person.id }),
        });
        if (!res.ok) {
          var out = await res.json();
          trouble.textContent = out.error || "The room refused that.";
        }
      } catch (err) {
        trouble.textContent = "Could not reach the room.";
      }
      await knock();
    }
    yes.addEventListener("click", function () { settle("/people/let-in"); });
    no.addEventListener("click", function () { settle("/people/turn-down"); });

    li.append(name, why, gap, yes, no);
    return li;
  }

  /// Who is waiting. Polled with the room, so a request that arrives while the owner is looking
  /// turns up without a refresh.
  async function knock() {
    if (!me || !me.owner) return;
    try {
      var res = await fetch("/people/asked", { headers: { Authorization: "Bearer " + me.password } });
      if (!res.ok) return;
      var waiting = await res.json();
      doorBtn.hidden = false;
      doorBtn.textContent = waiting.length ? "At the door (" + waiting.length + ")" : "At the door";
      doorList.textContent = "";
      for (var i = 0; i < waiting.length; i++) doorList.append(whoRow(waiting[i]));
      doorEmpty.hidden = waiting.length > 0;
    } catch (err) {}
  }

  doorBtn.addEventListener("click", function () { door.hidden = !door.hidden; });

  document.getElementById("send").addEventListener("submit", async function (e) {
    e.preventDefault();
    var words = text.value.trim();
    if (!words || !me) return;
    text.value = "";
    try {
      var res = await fetch("/say", {
        method: "POST",
        headers: { Authorization: "Bearer " + me.password, "Content-Type": "application/json" },
        body: JSON.stringify({ text: words }),
      });
      if (!res.ok) {
        if (res.status === 401) { text.value = words; signOut("That password is no longer one this room knows."); return; }
        trouble.textContent = "The room refused that.";
        text.value = words;   // handed back rather than lost
        return;
      }
      await poll();
    } catch (err) {
      trouble.textContent = "Could not reach the room.";
      text.value = words;
    }
  });

  // Coming back. A stored password is still checked against the room before the room is shown,
  // so one that has been removed from PASSWORDS lands on the gate rather than on an empty page
  // that quietly fails to load anything.
  (async function () {
    var password = kept();
    if (!password) { pwBox.focus(); return; }
    remember.checked = keptForever();
    try {
      var answer = await whoAmI(password);
      if (answer.who) { nameBox.value = answer.who; enter(answer.who, password, answer.owner); return; }
      signOut(answer.bad ? "That password is no longer one this room knows." : "");
    } catch (e) {
      signOut("");
    }
  })();
</script>
</body>
</html>`;
