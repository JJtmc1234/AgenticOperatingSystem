using System.Text.Json.Nodes;

namespace Aos.Core;

/// <summary>One attempted invocation, as it arrives from an MCP tool call.</summary>
/// <param name="CapabilityId">Must match a registered <see cref="CapabilityDescriptor.Id"/>.</param>
/// <param name="Arguments">Tool arguments, validated by the capability itself.</param>
/// <param name="Commit">
/// Explicit intent to apply changes. Ignored for capabilities where
/// <see cref="CapabilityDescriptor.RequiresCommit"/> is false; required otherwise.
/// </param>
/// <param name="CorrelationId">Ties a dry run and its subsequent commit together in the audit log.</param>
/// <param name="Reason">Why the agent wants this. Shown in approval prompts.</param>
public sealed record CapabilityRequest(
    string CapabilityId,
    JsonObject Arguments,
    bool Commit = false,
    string? CorrelationId = null,
    string? Reason = null);

public enum OutcomeStatus
{
    /// <summary>Applied for real.</summary>
    Succeeded = 0,

    /// <summary>Nothing changed; <see cref="CapabilityOutcome.Payload"/> describes what would have.</summary>
    DryRun = 1,

    /// <summary>Blocked by policy or a missing commit flag.</summary>
    Denied = 2,

    /// <summary>Reached the capability and threw.</summary>
    Failed = 3,

    /// <summary>
    /// The change was applied, but its post-condition check did not pass, so the harness
    /// cannot confirm the world looks the way the capability claimed it would.
    ///
    /// Deliberately distinct from <see cref="Failed"/>. Reporting a failure here would read
    /// as "nothing happened" and invite a retry that applies the change a second time.
    /// </summary>
    AppliedButUnverified = 4,
}

public sealed record CapabilityOutcome(
    OutcomeStatus Status,
    JsonNode? Payload = null,
    string? Message = null)
{
    public static CapabilityOutcome Ok(JsonNode? payload = null) => new(OutcomeStatus.Succeeded, payload);
    public static CapabilityOutcome Planned(JsonNode? plan, string message) => new(OutcomeStatus.DryRun, plan, message);
    public static CapabilityOutcome Denied(string message) => new(OutcomeStatus.Denied, null, message);
    public static CapabilityOutcome Failed(string message) => new(OutcomeStatus.Failed, null, message);

    public static CapabilityOutcome Unverified(JsonNode? payload, string message) =>
        new(OutcomeStatus.AppliedButUnverified, payload, message);
}

/// <summary>
/// A capability that can confirm its own effect after a committed change.
///
/// This is the provisioning runner's converge check applied to capabilities: it re-tests
/// the desired state after acting rather than trusting that acting worked. That check
/// caught a real bug in provisioning, which is the argument for having it here.
/// </summary>
public interface IVerifiableCapability
{
    /// <summary>
    /// Runs after a committed, successful execution. Return null when the post-condition
    /// holds, or a message explaining what does not look right.
    ///
    /// Must not mutate anything, and must not throw. A thrown exception is treated as a
    /// failed verification.
    /// </summary>
    Task<string?> VerifyAsync(
        CapabilityRequest request,
        CapabilityOutcome outcome,
        CancellationToken cancellationToken);
}

/// <summary>
/// A single brokered operation. Implementations must treat <paramref name="dryRun"/> as
/// absolute: when true, observe and describe, never mutate.
/// </summary>
public interface ICapability
{
    CapabilityDescriptor Descriptor { get; }

    Task<CapabilityOutcome> ExecuteAsync(
        CapabilityRequest request,
        bool dryRun,
        CancellationToken cancellationToken);
}
