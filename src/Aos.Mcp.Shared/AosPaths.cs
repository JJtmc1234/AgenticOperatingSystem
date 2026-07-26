using Aos.Broker;
using Aos.Core;

namespace Aos.Mcp.Shared;

/// <summary>Shared locations and broker wiring for every capability server.</summary>
public static class AosPaths
{
    public static string Root { get; } = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "AgenticOS");

    public static string PolicyFile => Path.Combine(Root, "policy.yaml");
    public static string AuditDirectory => Path.Combine(Root, "audit");
    public static string TrashDirectory => Path.Combine(Root, "trash");
    public static string DataDirectory => Path.Combine(Root, "data");

    public static YamlPolicyEvaluator LoadPolicy()
    {
        if (!File.Exists(PolicyFile))
        {
            throw new FileNotFoundException(
                $"Policy not installed at '{PolicyFile}'. Run provisioning/Install-Aos.ps1 first.");
        }

        return YamlPolicyEvaluator.FromFile(PolicyFile);
    }

    /// <summary>Path guard built from the installed policy.</summary>
    public static PathGuard GuardFrom(YamlPolicyEvaluator policy) =>
        new(policy.Document.DenyPaths, policy.Document.AllowedRoots);

    public static CapabilityBroker BuildBroker(
        IEnumerable<ICapability> capabilities, YamlPolicyEvaluator policy) =>
        new(capabilities, policy, new JsonlAuditSink(AuditDirectory));
}
