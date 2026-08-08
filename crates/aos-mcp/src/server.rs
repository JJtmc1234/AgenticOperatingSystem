//! The loop, and the four gates every call goes through.
//!
//! ```text
//!   tools/call
//!     |
//!     +-- does the capability exist            tools.rs, and no shell is in the table
//!     +-- what does the policy say about it    policy.rs, by tier and by agent
//!     +-- was it agreed to                     plan.rs, for anything above Read
//!     +-- is the path inside the root          root.rs, checked after resolution
//!     |
//!   do it, and append what happened to the log
//! ```
//!
//! They are separate because they fail for different reasons and a caller needs to know
//! which. "Denied by policy" and "that path is outside the root" are both refusals and
//! nothing else about them is the same.

use std::io::{BufRead, Write};

use aos_core::{AgentId, AgentSpec, Ledger, Plan, PlanId, PlanLedger, Policy, RiskTier, Verdict};
use serde_json::{Value, json};

use crate::{Error, Result, Root, files, rpc, tools};

pub struct Server {
    root: Root,
    policy: Policy,
    plans: PlanLedger,
    ledger: Option<Ledger>,
    /// Which agent this server is answering for, so the policy can single it out.
    agent: AgentId,
}

impl Server {
    pub fn new(root: Root, policy: Policy, agent: AgentId, ledger: Option<Ledger>) -> Self {
        let ttl = policy.plan_ttl_secs;
        Self {
            root,
            policy,
            plans: PlanLedger::new(ttl),
            ledger,
            agent,
        }
    }

    /// Reads until stdin closes.
    ///
    /// Nothing is ever written to stdout except a reply. A stray print becomes a parse error
    /// at the other end and looks like the server crashing.
    pub fn serve(&mut self, input: impl BufRead, mut output: impl Write) -> Result<()> {
        for line in input.lines() {
            let line = line?;
            let Some(msg) = rpc::parse(&line) else {
                continue;
            };
            // A notification must never be answered. Replying to one is a protocol error at
            // the client, and they arrive routinely, so this would break the session rather
            // than a single call.
            let Some(id) = msg.id.clone() else {
                continue;
            };

            let reply = match msg.method.as_str() {
                "initialize" => rpc::result(&id, self.hello()),
                "tools/list" => rpc::result(&id, tools::listing()),
                "tools/call" => rpc::result(&id, self.call(&msg.params)),
                "ping" => rpc::result(&id, json!({})),
                other => rpc::error(
                    &id,
                    rpc::METHOD_NOT_FOUND,
                    &format!("this server does not do {other}"),
                ),
            };

            writeln!(output, "{reply}")?;
            output.flush()?;
        }
        Ok(())
    }

    fn hello(&self) -> Value {
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "aos-files", "version": env!("CARGO_PKG_VERSION") }
        })
    }

    /// One `tools/call`, through all four gates.
    fn call(&mut self, params: &Value) -> Value {
        let Some(name) = params.get("name").and_then(|n| n.as_str()) else {
            return rpc::tool_error("the call named no tool");
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        // Gate one. A capability that is not in the table does not exist, and there is no
        // shell in the table.
        let Some(tool) = tools::find(name) else {
            // Recorded even though nothing exists to run. An agent reaching for a capability
            // that is not there is the most interesting line in an audit log, because it says
            // what something tried to do rather than what it managed.
            self.record(name, RiskTier::Read, "no such capability");
            return rpc::tool_error(format!(
                "there is no capability called {name}. This server does files and never runs \
                 commands."
            ));
        };

        // Gate two. What the policy says about that much damage, for this agent.
        match self.policy.verdict(&self.agent, tool.tier) {
            Verdict::Deny => {
                self.record(name, tool.tier, "denied by policy");
                return rpc::tool_error(format!(
                    "{name} is tier {} and the policy denies that for {}. Nothing was done.",
                    tool.tier, self.agent
                ));
            }
            Verdict::Allow | Verdict::Prompt => {}
        }

        // Gate three. Anything above Read has to have been agreed to first.
        if !tool.tier.allows_implicit_commit()
            && self.policy.verdict(&self.agent, tool.tier) == Verdict::Prompt
        {
            match self.plan_gate(name, tool.tier, &args) {
                Gate::Planned(text) => return rpc::tool_ok(text),
                Gate::Refused(text) => return rpc::tool_error(text),
                Gate::GoAhead => {}
            }
        }

        // Gate four is inside each of these, because resolving is the boundary and every
        // capability resolves its own arguments through the root.
        match self.run(name, &args) {
            Ok(text) => {
                self.record(name, tool.tier, "done");
                rpc::tool_ok(text)
            }
            Err(e) => {
                self.record(name, tool.tier, &format!("failed: {e}"));
                rpc::tool_error(e.to_string())
            }
        }
    }

    /// Plan then commit, for one call.
    fn plan_gate(&mut self, name: &str, tier: RiskTier, args: &Value) -> Gate {
        let spec = spec_for(name, args);
        let now = now();

        let Some(quoted) = args.get("plan_id").and_then(|p| p.as_str()) else {
            // No plan quoted, so make one and describe it. Nothing happens yet.
            return match self.plans.propose(&spec, tier, now) {
                Ok(plan) => {
                    self.record(name, tier, &format!("planned {}", plan.id));
                    Gate::Planned(describe(&plan, name, args))
                }
                Err(e) => Gate::Refused(format!("could not make a plan: {e}")),
            };
        };

        let id = PlanId::quoted(quoted);
        // Checked against the plan, not against the request that arrived with it. Otherwise
        // planning something harmless and committing something dangerous would work, which
        // is the entire thing plan then commit exists to stop.
        match self.plans.commit(&id, &spec, now) {
            Ok(_) => Gate::GoAhead,
            Err(e) => {
                self.record(name, tier, &format!("refused commit {quoted}: {e}"));
                Gate::Refused(format!(
                    "that plan cannot be carried out: {e}. Ask again with no plan_id to get a \
                     fresh plan."
                ))
            }
        }
    }

    fn run(&self, name: &str, args: &Value) -> Result<String> {
        let s = |key: &str| -> Result<String> {
            args.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(|| Error::Refused(format!("{name} needs a {key}")))
        };

        match name {
            "list_dir" => {
                let p = self.root.existing(&s("path").unwrap_or_default())?;
                files::list_dir(&self.root, &p)
            }
            "read_file" => {
                let p = self.root.existing(&s("path")?)?;
                files::read_file(&self.root, &p)
            }
            "find" => {
                let limit = args
                    .get("limit")
                    .and_then(|l| l.as_u64())
                    .unwrap_or(200)
                    .min(2_000) as usize;
                files::find(&self.root, &s("contains")?, limit)
            }
            "write_file" => {
                let p = self.root.for_writing(&s("path")?)?;
                files::write_file(&self.root, &p, &s("text")?)
            }
            "make_dir" => {
                let p = self.root.for_writing(&s("path")?)?;
                files::make_dir(&self.root, &p)
            }
            "move_file" => {
                let from = self.root.existing(&s("from")?)?;
                let to = self.root.for_writing(&s("to")?)?;
                files::move_file(&self.root, &from, &to)
            }
            "delete_file" => {
                let p = self.root.existing(&s("path")?)?;
                files::delete_file(&self.root, &p)
            }
            other => Err(Error::Refused(format!("no such capability: {other}"))),
        }
    }

    /// Everything gated goes in the log, including the refusals.
    ///
    /// A refusal is the interesting half. A log that only records what happened cannot answer
    /// what an agent tried to do and was stopped from doing, which is the question you ask
    /// when something has gone wrong.
    fn record(&mut self, tool: &str, tier: RiskTier, outcome: &str) {
        let Some(ledger) = self.ledger.as_mut() else {
            return;
        };
        let event = aos_core::Event::Capability {
            tool: tool.to_string(),
            tier,
            outcome: outcome.to_string(),
        };
        if let Err(e) = ledger.append(now(), self.agent.clone(), event) {
            // stderr, never stdout. Stdout is the protocol.
            eprintln!("could not record {tool}: {e}");
        }
    }
}

enum Gate {
    /// A plan was made and nothing was done.
    Planned(String),
    /// Go ahead, it was agreed to.
    GoAhead,
    Refused(String),
}

/// What a plan is made of, for this call.
///
/// The arguments are part of it, which is what makes a plan specific. A plan for deleting one
/// file must not commit the deletion of another.
fn spec_for(name: &str, args: &Value) -> AgentSpec {
    // An agent id is lowercase letters, digits and dashes, and a capability name has an
    // underscore in it. Translated rather than left to fail, because the id here is a label
    // on a plan and not something anybody types.
    let id = AgentId::new(format!("cap-{}", name.replace('_', "-")))
        .expect("capability names are letters and underscores");

    AgentSpec {
        id,
        program: name.to_string(),
        // The arguments are part of the plan, which is what makes a plan specific. A plan for
        // deleting one file must never commit the deletion of another.
        args: vec![canonical(args)],
        ceiling: RiskTier::Destructive,
    }
}

/// The arguments as one stable string, with `plan_id` taken out.
///
/// Sorted, because two JSON objects with the same contents in a different order are the same
/// request and must produce the same plan. Without sorting, committing would depend on how
/// the client happened to serialise the arguments.
fn canonical(args: &Value) -> String {
    let mut pairs: Vec<(String, String)> = args
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| k.as_str() != "plan_id")
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect()
        })
        .unwrap_or_default();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn describe(plan: &Plan, name: &str, args: &Value) -> String {
    format!(
        "This needs agreeing to first, because {name} is tier {}.\n\n\
         What would happen: {name} with {}\n\n\
         To carry it out, call {name} again with plan_id set to {}.\n\
         The plan is for exactly these arguments. Changing any of them needs a new plan.",
        plan.tier,
        canonical(args),
        plan.id
    )
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two clients serialising the same arguments in a different order are making the same
    /// request, so the plan has to match either way.
    #[test]
    fn argument_order_does_not_change_a_plan() {
        let a = json!({ "path": "x.txt", "text": "hi" });
        let b = json!({ "text": "hi", "path": "x.txt" });
        assert_eq!(canonical(&a), canonical(&b));
    }

    /// The plan id must not be part of what the plan is about, or a plan could never match
    /// its own commit.
    #[test]
    fn the_plan_id_is_not_part_of_the_request() {
        let without = json!({ "path": "x.txt" });
        let with = json!({ "path": "x.txt", "plan_id": "abc123" });
        assert_eq!(canonical(&without), canonical(&with));
    }

    /// A plan for deleting one file must never commit the deletion of another.
    #[test]
    fn different_arguments_are_different_requests() {
        let a = json!({ "path": "notes.txt" });
        let b = json!({ "path": "taxes.txt" });
        assert_ne!(canonical(&a), canonical(&b));
        assert_ne!(
            spec_for("delete_file", &a).args,
            spec_for("delete_file", &b).args
        );
    }

    /// And a plan for one capability must not commit a different one.
    #[test]
    fn different_capabilities_are_different_requests() {
        let args = json!({ "path": "x.txt" });
        assert_ne!(
            spec_for("read_file", &args).id,
            spec_for("delete_file", &args).id
        );
    }
}
