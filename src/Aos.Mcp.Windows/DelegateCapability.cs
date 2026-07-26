using System.Text.Json.Nodes;
using Aos.Core;

namespace Aos.Mcp.Windows;

/// <summary>
/// Wraps a handler as an <see cref="ICapability"/>. Capabilities here are small enough that
/// a class each would be noise; the descriptor is what carries the safety contract.
/// </summary>
internal sealed class DelegateCapability(
    CapabilityDescriptor descriptor,
    Func<JsonObject, bool, CancellationToken, Task<CapabilityOutcome>> handler) : ICapability
{
    public CapabilityDescriptor Descriptor { get; } = descriptor;

    public Task<CapabilityOutcome> ExecuteAsync(
        CapabilityRequest request, bool dryRun, CancellationToken cancellationToken) =>
        handler(request.Arguments, dryRun, cancellationToken);

    /// <summary>Read-tier capability: never mutates, so it ignores the dry-run flag.</summary>
    public static DelegateCapability Read(
        string id, string description, Func<JsonObject, JsonNode?> handler) =>
        new(new CapabilityDescriptor(id, ServerName, RiskTier.Read, description),
            (args, _, _) => Task.FromResult(CapabilityOutcome.Ok(handler(args))));

    /// <summary>
    /// Mutating capability. <paramref name="plan"/> describes what would happen and must not
    /// change anything; <paramref name="apply"/> runs only on a committed call.
    /// </summary>
    public static DelegateCapability Mutating(
        string id,
        RiskTier tier,
        string description,
        Func<JsonObject, string> plan,
        Func<JsonObject, JsonNode?> apply,
        bool? snapshot = null) =>
        new(new CapabilityDescriptor(id, ServerName, tier, description, snapshot),
            (args, dryRun, _) => Task.FromResult(
                dryRun
                    ? CapabilityOutcome.Planned(JsonValue.Create(plan(args)),
                        "Dry run: nothing changed. Re-invoke with commit=true to apply.")
                    : CapabilityOutcome.Ok(apply(args))));

    public const string ServerName = "aos-windows";
}
