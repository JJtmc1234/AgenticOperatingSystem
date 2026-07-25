namespace Aos.Core;

public enum PolicyVerdict
{
    /// <summary>Execute without asking.</summary>
    Allow = 0,

    /// <summary>Execute only after the human approves this specific call.</summary>
    Prompt = 1,

    /// <summary>Refuse. Never reaches the capability.</summary>
    Deny = 2,
}

/// <summary>Result of evaluating policy for one request. Always audited, including denials.</summary>
public sealed record PolicyDecision(
    PolicyVerdict Verdict,
    string Reason,
    bool DryRunOnly)
{
    public static PolicyDecision Allowed(string reason) => new(PolicyVerdict.Allow, reason, false);
    public static PolicyDecision Denied(string reason) => new(PolicyVerdict.Deny, reason, false);

    /// <summary>
    /// Allowed to run, but must not commit — the caller sees the plan and has to re-request.
    /// This is the default posture for <see cref="RiskTier.System"/> and above.
    /// </summary>
    public static PolicyDecision DryRun(string reason) => new(PolicyVerdict.Allow, reason, true);
}

/// <summary>
/// Maps tiers to verdicts. Implemented in Aos.Broker over <c>policy/default.yaml</c>;
/// kept as an interface here so capability projects never depend on the broker.
/// </summary>
public interface IPolicyEvaluator
{
    PolicyDecision Evaluate(CapabilityDescriptor capability, CapabilityRequest request);
}
