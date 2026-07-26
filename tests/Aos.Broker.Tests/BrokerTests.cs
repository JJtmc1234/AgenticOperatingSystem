using Aos.Core;
using Xunit;

namespace Aos.Broker.Tests;

public class BrokerTests
{
    private static (CapabilityBroker Broker, InMemoryAuditSink Audit) Build(
        SpyCapability capability,
        IStateSnapshotter? snapshotter = null,
        IApprovalPrompt? approvals = null)
    {
        var audit = new InMemoryAuditSink();
        var broker = new CapabilityBroker(
            [capability],
            YamlPolicyEvaluator.FromYaml(TestPolicy.Default),
            audit,
            snapshotter ?? new NoOpSnapshotter(),
            approvals);
        return (broker, audit);
    }

    [Fact]
    public async Task ReadTier_IsAllowed_AndCommitsWithoutHandshake()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read);
        var (broker, audit) = Build(cap);

        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(OutcomeStatus.Succeeded, outcome.Status);
        Assert.Equal(1, cap.CommitCount);
        Assert.False(audit.Entries.Single().DryRun);
    }

    [Theory]
    [InlineData(RiskTier.Write)]
    [InlineData(RiskTier.System)]
    [InlineData(RiskTier.Destructive)]
    public async Task NonAllowTiers_DryRunUntilCommitted(RiskTier tier)
    {
        var cap = SpyCapability.ForTier(tier);
        var (broker, _) = Build(cap, approvals: new StubApprovals(true));

        var first = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(OutcomeStatus.DryRun, first.Status);
        Assert.Equal(0, cap.CommitCount);
        Assert.Equal(1, cap.DryRunCount);

        var second = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Succeeded, second.Status);
        Assert.Equal(1, cap.CommitCount);
    }

    [Fact]
    public async Task DenyVerdict_NeverReachesCapability()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read, "test/blocked.op");
        var (broker, audit) = Build(cap);

        // A deny verdict is evaluated before the handshake, so no plan call is needed.
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Denied, outcome.Status);
        Assert.False(cap.Executed);
        Assert.Equal(PolicyVerdict.Deny, audit.Entries.Single().Verdict);
    }

    [Fact]
    public async Task UnknownCapability_IsDeniedAndAudited()
    {
        var (broker, audit) = Build(SpyCapability.ForTier(RiskTier.Read));

        var outcome = await broker.InvokeAsync(TestPolicy.Request("test/does-not-exist"));

        Assert.Equal(OutcomeStatus.Denied, outcome.Status);
        Assert.Equal("test/does-not-exist", audit.Entries.Single().CapabilityId);
    }

    [Fact]
    public async Task SystemTier_RefusesCommit_WhenNoSnapshotAvailable()
    {
        var cap = SpyCapability.ForTier(RiskTier.System);
        var (broker, _) = Build(cap, UnavailableSnapshotter.Instance, new StubApprovals(true));

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));   // show the plan
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Denied, outcome.Status);
        Assert.Contains("restore point", outcome.Message);
        Assert.Equal(0, cap.CommitCount);
    }

    [Fact]
    public async Task ReadTier_DoesNotRequireSnapshot()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read);
        var (broker, _) = Build(cap, UnavailableSnapshotter.Instance);

        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(OutcomeStatus.Succeeded, outcome.Status);
    }

    [Fact]
    public async Task DeclinedApproval_BlocksCommit()
    {
        var cap = SpyCapability.ForTier(RiskTier.Write);
        var approvals = new StubApprovals(approve: false);
        var (broker, _) = Build(cap, approvals: approvals);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));   // show the plan
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Denied, outcome.Status);
        Assert.Equal(1, approvals.Calls);
        Assert.Equal(0, cap.CommitCount);
    }

    [Fact]
    public async Task Approval_NotRequestedForDryRun()
    {
        var cap = SpyCapability.ForTier(RiskTier.Destructive);
        var approvals = new StubApprovals(approve: true);
        var (broker, _) = Build(cap, approvals: approvals);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(0, approvals.Calls);
    }

    [Fact]
    public async Task KillSwitch_DeniesEverything_UntilResumed()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read);
        var (broker, _) = Build(cap);

        broker.Halt();
        var halted = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));
        Assert.Equal(OutcomeStatus.Denied, halted.Status);
        Assert.False(cap.Executed);

        broker.Resume();
        var resumed = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));
        Assert.Equal(OutcomeStatus.Succeeded, resumed.Status);
    }

    [Fact]
    public async Task CapabilityException_IsCaughtAndAudited()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read);
        cap.Throw = new InvalidOperationException("boom");
        var (broker, audit) = Build(cap);

        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(OutcomeStatus.Failed, outcome.Status);
        Assert.Contains("boom", outcome.Message);
        Assert.Equal(OutcomeStatus.Failed, audit.Entries.Single().Status);
    }

    [Fact]
    public async Task AuditFailure_FailsTheCall()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read);
        var broker = new CapabilityBroker(
            [cap], YamlPolicyEvaluator.FromYaml(TestPolicy.Default), new ThrowingAuditSink());

        await Assert.ThrowsAsync<IOException>(
            () => broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id)));
    }

    [Fact]
    public void DuplicateCapabilityIds_AreRejectedAtConstruction()
    {
        var a = SpyCapability.ForTier(RiskTier.Read, "test/same.op");
        var b = SpyCapability.ForTier(RiskTier.Write, "test/same.op");

        Assert.Throws<InvalidOperationException>(() => new CapabilityBroker(
            [a, b], YamlPolicyEvaluator.FromYaml(TestPolicy.Default), new InMemoryAuditSink()));
    }

    [Fact]
    public async Task EveryOutcome_ProducesExactlyOneAuditEntry()
    {
        var cap = SpyCapability.ForTier(RiskTier.Destructive);
        var (broker, audit) = Build(cap, approvals: new StubApprovals(true));

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));                 // dry run
        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));   // commit
        await broker.InvokeAsync(TestPolicy.Request("test/nope"));                       // unknown

        Assert.Equal(3, audit.Entries.Count);
        Assert.Collection(audit.Entries,
            e => Assert.True(e.DryRun),
            e => Assert.False(e.DryRun),
            e => Assert.Equal(OutcomeStatus.Denied, e.Status));
    }
}
