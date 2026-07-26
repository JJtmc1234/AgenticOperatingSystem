using System.Text.Json.Nodes;
using Aos.Core;
using Xunit;

namespace Aos.Broker.Tests;

/// <summary>
/// Properties that must hold for every capability that will ever be written, not just the
/// ones that exist today.
///
/// This is the difference between testing features and testing a harness. A per-capability
/// test proves one thing works. These prove the guarantees the whole system advertises
/// cannot be quietly dropped by the next capability someone adds.
/// </summary>
public class HarnessInvariantTests
{
    private static readonly RiskTier[] AllTiers = Enum.GetValues<RiskTier>();

    private static (CapabilityBroker Broker, InMemoryAuditSink Audit) Build(params ICapability[] caps) =>
        BuildWith(new StubApprovals(true), caps);

    private static (CapabilityBroker Broker, InMemoryAuditSink Audit) BuildWith(
        IApprovalPrompt approvals, params ICapability[] caps)
    {
        var audit = new InMemoryAuditSink();
        return (new CapabilityBroker(
            caps,
            YamlPolicyEvaluator.FromYaml(TestPolicy.Default),
            audit,
            new NoOpSnapshotter(),
            approvals), audit);
    }

    [Theory]
    [InlineData(RiskTier.Write)]
    [InlineData(RiskTier.System)]
    [InlineData(RiskTier.Destructive)]
    public async Task Invariant_DryRunNeverReachesTheCapabilityAsACommit(RiskTier tier)
    {
        var cap = SpyCapability.ForTier(tier);
        var (broker, _) = Build(cap);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(0, cap.CommitCount);
    }

    [Theory]
    [InlineData(RiskTier.System)]
    [InlineData(RiskTier.Destructive)]
    public void Invariant_HighTiersAlwaysRequireCommit(RiskTier tier)
    {
        // Derived from the tier rather than set per capability, so a new capability cannot
        // forget to opt in.
        Assert.True(SpyCapability.ForTier(tier).Descriptor.RequiresCommit);
    }

    [Fact]
    public void Invariant_ReadTierNeverRequiresCommitOrSnapshot()
    {
        var descriptor = SpyCapability.ForTier(RiskTier.Read).Descriptor;

        Assert.False(descriptor.RequiresCommit);
        Assert.False(descriptor.ShouldSnapshot);
    }

    [Theory]
    [InlineData(RiskTier.Read)]
    [InlineData(RiskTier.Write)]
    [InlineData(RiskTier.System)]
    [InlineData(RiskTier.Destructive)]
    public async Task Invariant_EveryCallWritesExactlyOneAuditEntry(RiskTier tier)
    {
        var cap = SpyCapability.ForTier(tier);
        var (broker, audit) = Build(cap);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));
        Assert.Single(audit.Entries);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));
        Assert.Equal(2, audit.Entries.Count);
    }

    [Theory]
    [InlineData(RiskTier.Read)]
    [InlineData(RiskTier.Write)]
    [InlineData(RiskTier.System)]
    [InlineData(RiskTier.Destructive)]
    public async Task Invariant_AThrowingCapabilityIsStillAudited(RiskTier tier)
    {
        var cap = SpyCapability.ForTier(tier);
        cap.Throw = new InvalidOperationException("boom");
        var (broker, audit) = Build(cap);

        // A throwing capability throws on the dry run too, so there is no plan to
        // commit against. The point of the test is that the failure is audited.
        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Single(audit.Entries);
        Assert.Equal(OutcomeStatus.Failed, audit.Entries[0].Status);
    }

    [Theory]
    [InlineData(RiskTier.Read)]
    [InlineData(RiskTier.Write)]
    [InlineData(RiskTier.System)]
    [InlineData(RiskTier.Destructive)]
    public async Task Invariant_KillSwitchStopsEveryTier(RiskTier tier)
    {
        var cap = SpyCapability.ForTier(tier);
        var (broker, _) = Build(cap);
        broker.Halt();

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));   // show the plan
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Denied, outcome.Status);
        Assert.False(cap.Executed);
    }

    [Fact]
    public void Invariant_PolicyPolicesEveryTierTheEnumDefines()
    {
        // A tier added to the enum without a policy rule would fall through to fail closed,
        // which is safe but silent. This makes it loud.
        var evaluator = YamlPolicyEvaluator.FromYaml(TestPolicy.Default);

        foreach (var tier in AllTiers)
        {
            Assert.True(
                evaluator.Document.Tiers.ContainsKey(tier.ToString()),
                $"Test policy has no rule for tier '{tier}'.");
        }
    }

    [Fact]
    public async Task Invariant_ArgumentsAreAlwaysRedactedBeforeAudit()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read);
        var (broker, audit) = Build(cap);

        await broker.InvokeAsync(new CapabilityRequest(
            cap.Descriptor.Id,
            new JsonObject { ["apiKey"] = "super-secret", ["path"] = "notes.md" }));

        var logged = audit.Entries[0].Arguments!;
        Assert.Equal(ArgumentRedactor.Placeholder, logged["apiKey"]!.GetValue<string>());
        Assert.Equal("notes.md", logged["path"]!.GetValue<string>());
    }

    [Fact]
    public async Task Invariant_FailedVerificationReportsAppliedNotFailed()
    {
        // Reporting Failed here would read as "nothing happened" and invite a retry that
        // applies the change twice. That distinction is the whole point of the status.
        var cap = new VerifyingCapability(
            SpyCapability.ForTier(RiskTier.Write).Descriptor,
            problem: "destination is empty");
        var (broker, audit) = Build(cap);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));   // show the plan
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.AppliedButUnverified, outcome.Status);
        Assert.Contains("destination is empty", outcome.Message);
        Assert.Contains("Do not retry blindly", outcome.Message);
        // Entry 0 is the plan; the commit is the most recent one.
        Assert.Equal(OutcomeStatus.AppliedButUnverified, audit.Entries[^1].Status);
    }

    [Fact]
    public async Task Invariant_PassingVerificationLeavesSuccessAlone()
    {
        var cap = new VerifyingCapability(
            SpyCapability.ForTier(RiskTier.Write).Descriptor, problem: null);
        var (broker, _) = Build(cap);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));   // show the plan
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Succeeded, outcome.Status);
    }

    [Fact]
    public async Task Invariant_VerificationIsNotRunOnADryRun()
    {
        // Verifying a plan would fail every time, since nothing was supposed to change.
        var cap = new VerifyingCapability(
            SpyCapability.ForTier(RiskTier.Write).Descriptor, problem: "should not be asked");
        var (broker, _) = Build(cap);

        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(OutcomeStatus.DryRun, outcome.Status);
        Assert.Equal(0, cap.VerifyCalls);
    }

    [Fact]
    public async Task Invariant_AThrowingVerifierDoesNotEscape()
    {
        var cap = new VerifyingCapability(
            SpyCapability.ForTier(RiskTier.Write).Descriptor,
            problem: null,
            verifyThrows: new InvalidOperationException("verifier bug"));
        var (broker, _) = Build(cap);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));   // show the plan
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.AppliedButUnverified, outcome.Status);
        Assert.Contains("verifier bug", outcome.Message);
    }
}

/// <summary>Capability whose post-condition check is scripted for the test.</summary>
public sealed class VerifyingCapability(
    CapabilityDescriptor descriptor,
    string? problem,
    Exception? verifyThrows = null) : ICapability, IVerifiableCapability
{
    public CapabilityDescriptor Descriptor { get; } = descriptor;
    public int VerifyCalls { get; private set; }

    public Task<CapabilityOutcome> ExecuteAsync(
        CapabilityRequest request, bool dryRun, CancellationToken cancellationToken) =>
        Task.FromResult(dryRun
            ? CapabilityOutcome.Planned(JsonValue.Create("would apply"), "Dry run.")
            : CapabilityOutcome.Ok(JsonValue.Create("applied")));

    public Task<string?> VerifyAsync(
        CapabilityRequest request, CapabilityOutcome outcome, CancellationToken cancellationToken)
    {
        VerifyCalls++;
        if (verifyThrows is not null) { throw verifyThrows; }
        return Task.FromResult(problem);
    }
}
