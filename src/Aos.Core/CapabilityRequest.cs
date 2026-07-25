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
