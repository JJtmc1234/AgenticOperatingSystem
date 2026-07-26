using System.Text.Json.Nodes;
using Aos.Broker;
using Aos.Core;

namespace Aos.Mcp.Shared;

/// <summary>
/// Turns a brokered outcome into the JSON string an MCP tool returns. Denials and dry-run
/// plans come back as data rather than exceptions, so the model can read the reason and
/// decide whether to re-invoke with commit set true.
/// </summary>
public static class McpBridge
{
    public static async Task<string> CallAsync(
        this CapabilityBroker broker,
        string capabilityId,
        JsonObject arguments,
        bool commit = false,
        string? reason = null,
        CancellationToken cancellationToken = default)
    {
        var outcome = await broker
            .InvokeAsync(new CapabilityRequest(capabilityId, arguments, commit, null, reason), cancellationToken)
            .ConfigureAwait(false);

        var envelope = new JsonObject
        {
            ["status"] = outcome.Status.ToString(),
            ["message"] = outcome.Message,
            ["result"] = outcome.Payload?.DeepClone(),
        };

        return envelope.ToJsonString(AosJson.Options);
    }

    /// <summary>Descriptor listing, shared by every server's introspection tool.</summary>
    public static string DescribeCapabilities(this CapabilityBroker broker)
    {
        var rows = broker.Descriptors
            .OrderBy(d => d.Id, StringComparer.Ordinal)
            .Select(d => new
            {
                id = d.Id,
                tier = d.Tier.ToString(),
                requiresCommit = d.RequiresCommit,
                snapshotBeforeExecute = d.ShouldSnapshot,
                description = d.Description,
            });

        return System.Text.Json.JsonSerializer.Serialize(
            new { halted = broker.IsHalted, capabilities = rows }, AosJson.Options);
    }
}
