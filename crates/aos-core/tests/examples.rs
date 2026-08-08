//! The files the repo ships as examples have to actually work.
//!
//! Bug 6. `examples/policy.toml` did not parse, and had not since it was written, because a
//! bare key sitting after `[agents]` belongs to `[agents]`. So `plan_ttl_secs = 120` at the
//! bottom of the file was read as an agent called `plan_ttl_secs`, and the whole file was
//! refused.
//!
//! Nothing caught it because every test builds its policy in Rust. The example was only ever
//! read by people, and the first person to copy it would have got a daemon that refuses to
//! start, which is the correct behaviour for a bad policy and a terrible first five minutes.

use std::path::PathBuf;

use aos_core::{Policy, RiskTier};

fn repo() -> PathBuf {
    // CARGO_MANIFEST_DIR is the crate, and the examples live at the workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the crate should be two levels under the workspace root")
        .to_path_buf()
}

#[test]
fn the_example_policy_parses() {
    let path = repo().join("examples/policy.toml");
    assert!(path.exists(), "{} is missing", path.display());

    let policy = Policy::load(&path)
        .unwrap_or_else(|e| panic!("the example policy the repo ships does not load: {e}"));

    // Every tier has to be covered, which is what the loader is strictest about and the most
    // likely way a hand edited example goes quietly wrong.
    for tier in RiskTier::ALL {
        assert!(
            policy.tiers.contains_key(&tier),
            "the example policy says nothing about {tier}"
        );
    }
}

/// The exact shape of bug 6, checked on the parsed result rather than on the text.
///
/// Reading the file and complaining about bare keys under headers does not work, because
/// `read = "allow"` under `[tiers]` is exactly that and is correct. What was wrong was the
/// key landing in the wrong table, and the only way to see that is to parse it and look.
#[test]
fn the_plan_lifetime_is_read_rather_than_defaulted() {
    let policy = Policy::load(repo().join("examples/policy.toml")).unwrap();

    assert_eq!(
        policy.plan_ttl_secs, 120,
        "the example sets 120, so anything else means the key was not read from the root"
    );
    assert!(
        !policy.agents.keys().any(|a| a.as_str().contains("ttl")),
        "plan_ttl_secs ended up inside [agents], which is bug 6 again"
    );
}
