//! Iris has one governed route for repository investigation.

pub(super) fn brief(name: &str) -> &'static str {
    if name == "adrian" {
        return "For GitHub issue reviews, specific issue requests and Iris status, delegate through \
                Bash with carl handoff --from adrian --to iris and the complete assignment. \
                Preserve the repository, file paths and draft or publication intent. Return Iris's \
                actual report and links to Carl. Do not replace Iris with your own review.";
    }
    if name != "iris" {
        return "";
    }
    "Iris workflow instructions: For an issue investigation request from Adrian, run \
     `carl iris run --request '<the assigned work>'` through Bash, quoting the request as \
     one literal argument. Always add --repo owner/name when the assignment names one repository. \
     Add repeated --path arguments for exact committed files when supplied. For a specific \
     issue use carl iris issue --repo owner/name --request 'the requested outcome'. Preserve \
     --draft when JJ requests a draft or verification, and never change publication configuration. \
     For progress, results or budget questions run carl iris status, without starting another scan. \
     Return the report to Adrian so Carl can display it in the same conversation. \
     A missing repository is a question for Adrian, not permission to scan every repository. \
     This controlled workflow owns repository selection, duplicate \
     checks, evidence gathering, publication policy and the durable report. Report its \
     actual outcome and issue links to Adrian. Never claim an issue was published unless \
     the workflow confirms it. If the workflow fails, return the actual diagnostic. \
     The only exception to the helper restriction above is the scoped investigators \
     managed by this Iris workflow runtime. Do not launch Agent, Task or arbitrary helper \
     processes yourself. These investigators do not change the named chain of command."
}

#[cfg(test)]
mod tests {
    use crate::army::{chain, org};

    #[test]
    fn adrian_routes_iris_requests_and_preserves_the_result() {
        let lead = chain::brief_for(org::find("adrian").unwrap());
        assert!(lead.contains("carl handoff --from adrian --to iris"));
        assert!(lead.contains("draft or publication intent"));
        let worker = chain::brief_for(org::find("iris").unwrap());
        assert!(worker.contains("carl iris issue --repo"));
        assert!(worker.contains("carl iris status"));
        assert!(worker.contains("not permission to scan every repository"));
    }

    #[test]
    fn iris_brief_routes_investigation_through_controlled_workflow_only() {
        let brief = chain::brief_for(org::find("iris").unwrap());
        assert!(brief.contains("carl iris run --request"));
        assert!(brief.contains("scoped investigators"));
        assert!(brief.contains("You report to adrian"));
        assert!(brief.contains("Never spawn a helper"));
        for name in ["carl", "adrian", "evan", "miles"] {
            assert!(
                !chain::brief_for(org::find(name).unwrap()).contains("Iris workflow instructions")
            );
        }
        assert!(org::may_delegate("carl", "adrian"));
        assert!(org::may_delegate("adrian", "iris"));
        assert!(!org::may_delegate("carl", "iris"));
    }
}
