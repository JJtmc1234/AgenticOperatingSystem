//! Evan repairs issues only through the governed runtime.

pub(super) fn brief(name: &str) -> &'static str {
    match name {
        "adrian" => {
            "For fixing Iris issues, delegate with carl handoff --from adrian --to evan. \
            Preserve the repository and draft intent. Return Evan's actual evidence and PR links."
        }
        "evan" => {
            "Evan workflow instructions: Run carl evan run --repo owner/name through Bash \
            for an assigned repository. Preserve --draft. Use carl evan status for status requests. \
            Never alter repository policies, test commands, publication settings or model budgets. \
            The runtime owns selection, isolated tests, edits, commits and PR creation. \
            Only the bounded tool-free workers managed by that runtime are an exception to the \
            helper restriction. Never launch arbitrary helpers. Report actual blockers to Adrian. \
            Never merge a PR or close an issue without JJ's explicit authorization."
        }
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use crate::army::{chain, org};

    #[test]
    fn evan_routes_fixes_through_runtime_without_changing_the_chain() {
        let worker = chain::brief_for(org::find("evan").unwrap());
        assert!(worker.contains("carl evan run --repo"));
        assert!(worker.contains("carl evan status"));
        assert!(worker.contains("Never alter repository policies"));
        assert!(
            chain::brief_for(org::find("adrian").unwrap())
                .contains("carl handoff --from adrian --to evan")
        );
        assert!(org::may_delegate("adrian", "evan"));
        assert!(!org::may_delegate("carl", "evan"));
    }
}
