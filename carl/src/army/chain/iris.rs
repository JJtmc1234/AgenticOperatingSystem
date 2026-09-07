//! Iris has one governed route for repository investigation.

pub(super) fn brief(name: &str) -> &'static str {
    if name != "iris" {
        return "";
    }
    "Iris workflow instructions: For an issue investigation request from Adrian, run \
     `carl iris run --request '<the assigned work>'` through Bash, quoting the request as \
     one literal argument. This controlled workflow owns repository selection, duplicate \
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
