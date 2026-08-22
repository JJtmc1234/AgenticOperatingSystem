//! The capability server driven the way a client drives it, over stdio.
//!
//! The unit tests check each gate on its own. These check that a request actually travels
//! through all four of them and comes out with the right answer, which is a different claim
//! and the one that would still be false if any of the wiring were wrong.

use std::collections::BTreeMap;
use std::io::Cursor;

use aos_core::{AgentId, Policy, RiskTier, Verdict};
use aos_mcp::{Root, Scope, Server};
use serde_json::{Value, json};

/// One server, kept alive across several requests.
///
/// Needed because a plan and its commit are two separate calls to the same server. A helper
/// that spins up a fresh server per batch cannot test plan then commit at all, it can only
/// test that a plan from a dead session is refused, which is a different and much weaker
/// claim.
struct Session {
    server: Server,
}

impl Session {
    fn new(scope: &Scope, policy: Policy) -> Self {
        Self {
            server: Server::new(
                scope.clone(),
                policy,
                AgentId::new("claude-code").unwrap(),
                None,
            ),
        }
    }

    fn send(&mut self, request: Value) -> Value {
        let mut out = Vec::new();
        self.server
            .serve(Cursor::new(format!("{request}\n").into_bytes()), &mut out)
            .unwrap();
        serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap()
    }
}

/// Pulls the plan id out of what the server said to do next.
fn plan_id_in(said: &str) -> String {
    said.split("plan_id set to ")
        .nth(1)
        .unwrap_or_else(|| panic!("no plan id offered in: {said}"))
        .split_whitespace()
        .next()
        .unwrap()
        .trim_end_matches('.')
        .to_string()
}

/// Runs a batch of requests through a server and returns the replies.
fn talk(scope: &Scope, policy: Policy, requests: &[Value]) -> Vec<Value> {
    let lines: String = requests
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<Vec<_>>()
        .join("");

    let mut out = Vec::new();
    let mut server = Server::new(
        scope.clone(),
        policy,
        AgentId::new("claude-code").unwrap(),
        None,
    );
    server
        .serve(Cursor::new(lines.into_bytes()), &mut out)
        .unwrap();

    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn call(id: u32, name: &str, args: Value) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": name, "arguments": args }
    })
}

fn text_of(reply: &Value) -> String {
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn failed(reply: &Value) -> bool {
    reply["result"]["isError"] == json!(true)
}

fn policy(write: Verdict, destructive: Verdict) -> Policy {
    let mut tiers = BTreeMap::new();
    tiers.insert(RiskTier::Read, Verdict::Allow);
    tiers.insert(RiskTier::Write, write);
    tiers.insert(RiskTier::System, Verdict::Deny);
    tiers.insert(RiskTier::Destructive, destructive);
    Policy {
        tiers,
        agents: BTreeMap::new(),
        plan_ttl_secs: 300,
    }
}

/// A workspace an agent may both read and change all of.
///
/// The wide grant, which is what most of these tests are about: they check the gates rather than
/// the scope, and a narrower one would only make every path in them longer. The narrow grant has
/// tests of its own further down.
fn workspace() -> (tempfile::TempDir, Scope) {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("notes")).unwrap();
    std::fs::write(d.path().join("notes/a.txt"), "hello").unwrap();
    let root = Root::open(d.path()).unwrap();
    let scope = Scope::granting(root, d.path()).unwrap();
    (d, scope)
}

#[test]
fn the_catalogue_lists_every_tool_with_its_tier() {
    let (_d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Prompt, Verdict::Prompt),
        &[json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})],
    );
    let tools = out[0]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"read_file"));
    assert!(names.contains(&"delete_file"));
    assert!(
        !names
            .iter()
            .any(|n| n.contains("shell") || n.contains("exec")),
        "{names:?}"
    );
}

/// Read is the only tier that happens in one step, so this is the shortest path through the
/// server and the one everything else is measured against.
#[test]
fn reading_happens_in_one_step() {
    let (_d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Prompt, Verdict::Prompt),
        &[call(1, "read_file", json!({ "path": "notes/a.txt" }))],
    );
    assert!(!failed(&out[0]));
    assert_eq!(text_of(&out[0]), "hello");
}

/// The heart of phase 3. A write does not happen on the first call, it comes back as a plan,
/// and quoting that plan back is what makes it happen.
#[test]
fn writing_takes_two_calls_and_the_first_changes_nothing() {
    let (d, r) = workspace();
    let mut s = Session::new(&r, policy(Verdict::Prompt, Verdict::Prompt));

    let planned = s.send(call(
        1,
        "write_file",
        json!({ "path": "new.txt", "text": "written" }),
    ));
    let said = text_of(&planned);
    assert!(said.contains("needs agreeing to first"), "{said}");
    assert!(
        !d.path().join("new.txt").exists(),
        "planning must not write anything"
    );

    let id = plan_id_in(&said);
    let done = s.send(call(
        2,
        "write_file",
        json!({ "path": "new.txt", "text": "written", "plan_id": id }),
    ));
    assert!(!failed(&done), "{}", text_of(&done));
    assert_eq!(
        std::fs::read_to_string(d.path().join("new.txt")).unwrap(),
        "written"
    );
}

/// A plan is single use. Otherwise agreeing to one deletion agrees to every future one.
#[test]
fn a_plan_cannot_be_spent_twice() {
    let (d, r) = workspace();
    let mut s = Session::new(&r, policy(Verdict::Prompt, Verdict::Prompt));

    let id = plan_id_in(&text_of(&s.send(call(
        1,
        "delete_file",
        json!({ "path": "notes/a.txt" }),
    ))));

    let first = s.send(call(
        2,
        "delete_file",
        json!({ "path": "notes/a.txt", "plan_id": id.clone() }),
    ));
    assert!(!failed(&first), "{}", text_of(&first));
    assert!(!d.path().join("notes/a.txt").exists());

    std::fs::write(d.path().join("notes/a.txt"), "back again").unwrap();
    let second = s.send(call(
        3,
        "delete_file",
        json!({ "path": "notes/a.txt", "plan_id": id }),
    ));
    assert!(failed(&second), "a spent plan must not work twice");
    assert!(
        d.path().join("notes/a.txt").exists(),
        "the file must survive"
    );
}

/// The attack plan then commit exists to stop. A real, valid, unspent plan for a harmless
/// write, spent on a different file. This is the test that would still pass if the commit
/// checked only that the id existed.
#[test]
fn a_real_plan_cannot_be_spent_on_a_different_file() {
    let (d, r) = workspace();
    std::fs::write(d.path().join("keep.txt"), "important").unwrap();
    let mut s = Session::new(&r, policy(Verdict::Prompt, Verdict::Prompt));

    let id = plan_id_in(&text_of(&s.send(call(
        1,
        "write_file",
        json!({ "path": "scratch.txt", "text": "junk" }),
    ))));

    let stolen = s.send(call(
        2,
        "write_file",
        json!({ "path": "keep.txt", "text": "clobbered", "plan_id": id.clone() }),
    ));
    assert!(failed(&stolen), "{}", text_of(&stolen));
    assert_eq!(
        std::fs::read_to_string(d.path().join("keep.txt")).unwrap(),
        "important",
        "the file the plan was not about must be untouched"
    );

    // And the same id must not be usable for a different capability either.
    let switched = s.send(call(
        3,
        "delete_file",
        json!({ "path": "keep.txt", "plan_id": id }),
    ));
    assert!(failed(&switched));
    assert!(d.path().join("keep.txt").exists());
}

/// An id that was never issued must be refused rather than treated as a fresh plan.
#[test]
fn an_invented_plan_id_is_refused() {
    let (d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Prompt, Verdict::Prompt),
        &[call(
            1,
            "write_file",
            json!({ "path": "new.txt", "text": "x", "plan_id": "0000000000000000" }),
        )],
    );
    assert!(failed(&out[0]));
    assert!(!d.path().join("new.txt").exists());
}

/// The boundary, checked through the whole server rather than only in root.rs.
#[test]
fn a_path_outside_the_root_is_refused_at_every_tier() {
    let (_d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Allow, Verdict::Allow),
        &[
            call(1, "read_file", json!({ "path": "../../../etc/passwd" })),
            call(2, "read_file", json!({ "path": "/etc/passwd" })),
            call(
                3,
                "write_file",
                json!({ "path": "../escaped.txt", "text": "x" }),
            ),
            call(4, "delete_file", json!({ "path": "/etc/hosts" })),
        ],
    );
    for (i, reply) in out.iter().enumerate() {
        assert!(failed(reply), "request {} should have failed", i + 1);
    }
    assert!(std::path::Path::new("/etc/hosts").exists(), "still there");
}

/// A denied tier never reaches the plan stage. Offering a plan for something that can never
/// be committed would be a lie told politely.
#[test]
fn a_denied_tier_is_refused_without_a_plan() {
    let (d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Prompt, Verdict::Deny),
        &[call(1, "delete_file", json!({ "path": "notes/a.txt" }))],
    );
    let said = text_of(&out[0]);
    assert!(failed(&out[0]));
    assert!(said.contains("policy denies"), "{said}");
    assert!(!said.contains("plan_id"), "no plan should be offered");
    assert!(d.path().join("notes/a.txt").exists());
}

/// Allow means allow. A policy that says a tier needs no agreement must not then demand one.
#[test]
fn an_allowed_tier_happens_in_one_step() {
    let (d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Allow, Verdict::Allow),
        &[call(
            1,
            "write_file",
            json!({ "path": "direct.txt", "text": "no plan needed" }),
        )],
    );
    assert!(!failed(&out[0]), "{}", text_of(&out[0]));
    assert_eq!(
        std::fs::read_to_string(d.path().join("direct.txt")).unwrap(),
        "no plan needed"
    );
}

/// There is no shell, and asking for one says so rather than failing vaguely.
#[test]
fn there_is_no_way_to_run_a_command() {
    let (_d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Allow, Verdict::Allow),
        &[
            call(1, "shell", json!({ "command": "rm -rf /" })),
            call(2, "exec", json!({ "command": "id" })),
        ],
    );
    for reply in &out {
        assert!(failed(reply));
        assert!(
            text_of(reply).contains("never runs commands"),
            "{}",
            text_of(reply)
        );
    }
}

/// Replying to a notification is a protocol error at the client, and they arrive routinely,
/// so getting this wrong breaks the session rather than one call.
#[test]
fn a_notification_gets_no_reply() {
    let (_d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Allow, Verdict::Allow),
        &[
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
        ],
    );
    assert_eq!(out.len(), 1, "only the ping should be answered");
    assert_eq!(out[0]["id"], json!(1));
}

/// A refused file is news the model can act on. A JSON-RPC error would be swallowed by the
/// client and the model would only know that nothing happened.
#[test]
fn a_refusal_is_a_successful_call_carrying_bad_news() {
    let (_d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Allow, Verdict::Allow),
        &[call(1, "read_file", json!({ "path": "../escape" }))],
    );
    assert!(
        out[0].get("result").is_some(),
        "must not be a protocol error"
    );
    assert!(out[0].get("error").is_none());
    assert!(failed(&out[0]));
}

#[test]
fn an_unknown_method_is_a_protocol_error() {
    let (_d, r) = workspace();
    let out = talk(
        &r,
        policy(Verdict::Allow, Verdict::Allow),
        &[json!({"jsonrpc":"2.0","id":1,"method":"resources/list"})],
    );
    assert!(out[0].get("error").is_some());
}

/// A project with one task workspace inside it, which is what a lead grants a worker.
fn narrow() -> (tempfile::TempDir, Scope) {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("src")).unwrap();
    std::fs::create_dir_all(d.path().join("task")).unwrap();
    std::fs::write(d.path().join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::create_dir_all(d.path().join(".ssh")).unwrap();
    std::fs::write(d.path().join(".ssh/id_rsa"), "not yours").unwrap();

    let root = Root::open(d.path()).unwrap();
    let scope = Scope::granting(root, d.path().join("task")).unwrap();
    (d, scope)
}

/// The whole grant, driven the way a worker would drive it. Wide read, one place to write, and
/// nothing else reachable however the request is spelled.
#[test]
fn a_worker_can_read_the_project_and_write_only_its_own_workspace() {
    let (d, scope) = narrow();
    let mut session = Session::new(&scope, policy(Verdict::Prompt, Verdict::Prompt));

    let read = session.send(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"read_file","arguments":{"path":"src/main.rs"}}
    }));
    assert!(
        read["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("fn main")
    );

    let offered = session.send(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"write_file","arguments":{"path":"task/result.txt","text":"done"}}
    }));
    let plan = plan_id_in(offered["result"]["content"][0]["text"].as_str().unwrap());

    let done = session.send(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"write_file",
                  "arguments":{"path":"task/result.txt","text":"done","plan_id":plan}}
    }));
    assert!(done["error"].is_null(), "{done}");
    assert_eq!(
        std::fs::read_to_string(d.path().join("task/result.txt")).unwrap(),
        "done"
    );
}

/// Every way out of the workspace, refused at the far end of the protocol rather than only in
/// the unit tests. A gate that works in isolation and is not wired up is not a gate.
#[test]
fn nothing_reaches_outside_the_workspace_however_it_is_asked_for() {
    let (d, scope) = narrow();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "not yours").unwrap();
    std::os::unix::fs::symlink(outside.path(), d.path().join("task/elsewhere")).unwrap();

    for path in [
        "src/main.rs",               // readable, and still not writable
        "escape.txt",                // beside the workspace
        "../escape.txt",             // out of the project
        "/tmp/escape.txt",           // absolute
        "task/../../escape.txt",     // up and around
        "task/elsewhere/escape.txt", // through a link
        ".ssh/id_rsa",               // a secret inside the project
    ] {
        let out = talk(
            &scope,
            policy(Verdict::Allow, Verdict::Allow),
            &[json!({
                "jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"write_file","arguments":{"path":path,"text":"x"}}
            })],
        );
        let said = format!("{}", out[0]);
        assert!(said.contains("refused"), "{path} was not refused: {said}");
    }

    assert!(!outside.path().join("escape.txt").exists());
    assert!(!d.path().join("escape.txt").exists());
}

/// Allowed by policy and still refused by the scope, because they are different gates and a
/// worker whose lead widened the policy has not thereby been given the whole disk.
#[test]
fn a_policy_that_allows_everything_does_not_widen_the_scope() {
    let (_d, scope) = narrow();
    let out = talk(
        &scope,
        policy(Verdict::Allow, Verdict::Allow),
        &[json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"delete_file","arguments":{"path":"src/main.rs"}}
        })],
    );
    let said = format!("{}", out[0]);
    assert!(said.contains("refused"), "{said}");
}

/// There is no capability that runs anything, and none that reads the environment. Both of
/// those would be every tier at once, which is why neither can be described to a policy.
#[test]
fn there_is_no_way_to_run_a_command_or_read_the_environment() {
    let (_d, scope) = workspace();
    let out = talk(
        &scope,
        policy(Verdict::Allow, Verdict::Allow),
        &[json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})],
    );

    let tools = out[0]["result"]["tools"].as_array().unwrap();
    let listed = serde_json::to_string(tools).unwrap().to_lowercase();
    for word in [
        "shell", "exec", "bash", "command", "run_", "spawn", "env", "sudo", "process",
    ] {
        assert!(
            !listed.contains(word),
            "{word} appears in the catalogue: {listed}"
        );
    }

    for name in [
        "run",
        "exec",
        "shell",
        "bash",
        "getenv",
        "environment",
        "sudo",
    ] {
        let said = format!(
            "{}",
            talk(
                &scope,
                policy(Verdict::Allow, Verdict::Allow),
                &[json!({
                    "jsonrpc":"2.0","id":1,"method":"tools/call",
                    "params":{"name":name,"arguments":{}}
                })],
            )[0]
        );
        assert!(said.contains("no capability called"), "{name}: {said}");
    }
}

/// A plan for something that can never happen is a plan somebody may sit and agree to, and it
/// still will not happen. Worse, it answers "pending approval" to a request whose real answer is
/// "never", which tells an agent to keep trying.
#[test]
fn a_write_outside_the_workspace_is_refused_before_any_plan_is_offered() {
    let (_d, scope) = narrow();
    let out = talk(
        &scope,
        policy(Verdict::Prompt, Verdict::Prompt),
        &[json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"write_file","arguments":{"path":"src/main.rs","text":"x"}}
        })],
    );
    let said = out[0]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(said.contains("readable but not writable"), "{said}");
    assert!(
        !said.contains("plan_id set to"),
        "it was offered a plan: {said}"
    );
}
