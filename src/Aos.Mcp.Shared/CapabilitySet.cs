using System.Text.Json.Nodes;
using Aos.Core;

namespace Aos.Mcp.Shared;

/// <summary>
/// Builds capabilities for one MCP server. Capabilities are small enough that a class each
/// would be noise, and the descriptor is what carries the safety contract anyway.
/// </summary>
public sealed class CapabilitySet(string serverName)
{
    /// <summary>Read tier: never mutates, so it ignores the dry-run flag.</summary>
    public ICapability Read(
        string id, string description, Func<JsonObject, JsonNode?> handler) =>
        new DelegateCapability(
            new CapabilityDescriptor(id, serverName, RiskTier.Read, description),
            (args, _, _) => Task.FromResult(CapabilityOutcome.Ok(handler(args))));

    /// <summary>
    /// Mutating capability. <paramref name="plan"/> describes what would happen and must not
    /// change anything. <paramref name="apply"/> runs only on a committed call.
    /// </summary>
    /// <param name="snapshot">
    /// Override the tier default for requiring a restore point. Pass false only where a
    /// shadow copy could not undo the action anyway, and say why at the call site.
    /// </param>
    /// <param name="verify">
    /// Post-condition check, run only after a committed change succeeded. Return null when
    /// the world looks right, or a message describing what does not. Supply this wherever a
    /// wrong outcome costs the user something.
    /// </param>
    public ICapability Mutating(
        string id,
        RiskTier tier,
        string description,
        Func<JsonObject, string> plan,
        Func<JsonObject, JsonNode?> apply,
        bool? snapshot = null,
        Func<JsonObject, JsonNode?, string?>? verify = null) =>
        new DelegateCapability(
            new CapabilityDescriptor(id, serverName, tier, description, snapshot),
            (args, dryRun, _) => Task.FromResult(
                dryRun
                    ? CapabilityOutcome.Planned(JsonValue.Create(plan(args)),
                        "Dry run: nothing changed. Re-invoke with commit=true to apply.")
                    : CapabilityOutcome.Ok(apply(args))),
            verify);
}

internal sealed class DelegateCapability(
    CapabilityDescriptor descriptor,
    Func<JsonObject, bool, CancellationToken, Task<CapabilityOutcome>> handler,
    Func<JsonObject, JsonNode?, string?>? verify = null)
    : ICapability, IVerifiableCapability
{
    public CapabilityDescriptor Descriptor { get; } = descriptor;

    public Task<CapabilityOutcome> ExecuteAsync(
        CapabilityRequest request, bool dryRun, CancellationToken cancellationToken) =>
        handler(request.Arguments, dryRun, cancellationToken);

    public Task<string?> VerifyAsync(
        CapabilityRequest request, CapabilityOutcome outcome, CancellationToken cancellationToken) =>
        Task.FromResult(verify?.Invoke(request.Arguments, outcome.Payload));
}
