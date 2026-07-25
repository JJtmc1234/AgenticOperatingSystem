using System.Text.Json.Nodes;
using Aos.Core;

namespace Aos.Broker.Tests;

/// <summary>Records whether it ran and with which dryRun flag.</summary>
public sealed class SpyCapability(CapabilityDescriptor descriptor) : ICapability
{
    public CapabilityDescriptor Descriptor { get; } = descriptor;

    public int CommitCount { get; private set; }
    public int DryRunCount { get; private set; }
    public bool Executed => CommitCount + DryRunCount > 0;

    /// <summary>When set, ExecuteAsync throws this to exercise failure auditing.</summary>
    public Exception? Throw { get; set; }

    public Task<CapabilityOutcome> ExecuteAsync(
        CapabilityRequest request, bool dryRun, CancellationToken cancellationToken)
    {
        if (dryRun) { DryRunCount++; } else { CommitCount++; }
        if (Throw is not null) { throw Throw; }

        return Task.FromResult(CapabilityOutcome.Ok(JsonValue.Create("done")));
    }

    public static SpyCapability ForTier(RiskTier tier, string? id = null) =>
        new(new CapabilityDescriptor(
            id ?? $"test/{tier.ToString().ToLowerInvariant()}.op",
            "test",
            tier,
            $"{tier} test capability"));
}

public sealed class InMemoryAuditSink : IAuditSink
{
    public List<AuditEntry> Entries { get; } = new();

    public Task WriteAsync(AuditEntry entry, CancellationToken cancellationToken)
    {
        Entries.Add(entry);
        return Task.CompletedTask;
    }
}

public sealed class ThrowingAuditSink : IAuditSink
{
    public Task WriteAsync(AuditEntry entry, CancellationToken cancellationToken) =>
        throw new IOException("audit volume full");
}

public sealed class StubApprovals(bool approve) : IApprovalPrompt
{
    public int Calls { get; private set; }

    public Task<bool> RequestAsync(
        CapabilityDescriptor capability, CapabilityRequest request, CancellationToken ct)
    {
        Calls++;
        return Task.FromResult(approve);
    }
}

public static class TestPolicy
{
    /// <summary>Mirrors the shape of policy/default.yaml.</summary>
    public const string Default = """
        tiers:
          Read:        { verdict: allow }
          Write:       { verdict: prompt }
          System:      { verdict: prompt, dryRunOnly: true }
          Destructive: { verdict: prompt, dryRunOnly: true }
        capabilities:
          test/blocked.op: { verdict: deny, reason: "blocked for tests" }
        denyPaths:
          - "C:\\Windows\\System32"
          - "C:\\Program Files"
        """;

    public static CapabilityRequest Request(string capabilityId, bool commit = false) =>
        new(capabilityId, new JsonObject(), Commit: commit);
}
