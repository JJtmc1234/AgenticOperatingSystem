// System.IO is not part of the implicit using set for WindowsDesktop SDK projects.
using System.IO;
using System.Text.Json.Nodes;
using Aos.Broker;
using Aos.Core;
using Aos.Mcp.Windows.Capabilities;

namespace Aos.Mcp.Windows;

internal static class AosHost
{
    public static string Root { get; } = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "AgenticOS");

    public static CapabilityBroker BuildBroker()
    {
        var policyPath = Path.Combine(Root, "policy.yaml");
        if (!File.Exists(policyPath))
        {
            throw new FileNotFoundException(
                $"Policy not installed at '{policyPath}'. Run provisioning/Install-Aos.ps1 first.");
        }

        var capabilities = ShellSurface
            .All(Path.Combine(Root, "data", "screens"))
            .Concat(UiaSurface.All());

        return new CapabilityBroker(
            capabilities,
            YamlPolicyEvaluator.FromFile(policyPath),
            new JsonlAuditSink(Path.Combine(Root, "audit")));
    }
}

/// <summary>
/// Turns a brokered outcome into the JSON string an MCP tool returns. Denials and dry-run
/// plans come back as data rather than exceptions, so the model can read the reason and
/// decide whether to re-invoke with commit=true.
/// </summary>
internal static class McpBridge
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
}
