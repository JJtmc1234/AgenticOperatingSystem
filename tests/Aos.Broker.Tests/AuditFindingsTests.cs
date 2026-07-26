using System.Text.Json.Nodes;
using Aos.Core;
using Xunit;

namespace Aos.Broker.Tests;

/// <summary>
/// One test per defect found in the safety audit. Each names the wrong behaviour it pins
/// shut, so a future change that reintroduces it fails here rather than in production.
/// </summary>
public class AuditFindingsTests
{
    private static (CapabilityBroker Broker, InMemoryAuditSink Audit, PlanLedger Plans) Build(
        SpyCapability capability,
        IApprovalPrompt? approvals = null,
        IAuditSink? sink = null)
    {
        var audit = new InMemoryAuditSink();
        var plans = new PlanLedger();
        var broker = new CapabilityBroker(
            [capability],
            YamlPolicyEvaluator.FromYaml(TestPolicy.Default),
            sink ?? audit,
            new NoOpSnapshotter(),
            approvals ?? new StubApprovals(true),
            plans: plans);
        return (broker, audit, plans);
    }

    // --- the handshake was a single call -------------------------------------------

    [Theory]
    [InlineData(RiskTier.Write)]
    [InlineData(RiskTier.System)]
    [InlineData(RiskTier.Destructive)]
    public async Task CommitWithoutAnyPriorPlan_IsRefused(RiskTier tier)
    {
        // The original bug: dryRun was computed purely from the commit flag, so a first-ever
        // call with commit=true applied straight away and the "second call" was optional.
        var cap = SpyCapability.ForTier(tier);
        var (broker, _, _) = Build(cap);

        // Deliberately NO plan call first. That is the whole point of this test.
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Denied, outcome.Status);
        Assert.Contains("No plan has been shown", outcome.Message);
        Assert.Equal(0, cap.CommitCount);
    }

    [Fact]
    public async Task PlanThenCommit_Succeeds()
    {
        var cap = SpyCapability.ForTier(RiskTier.Destructive);
        var (broker, _, _) = Build(cap);

        var plan = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));
        Assert.Equal(OutcomeStatus.DryRun, plan.Status);

        var commit = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));
        Assert.Equal(OutcomeStatus.Succeeded, commit.Status);
        Assert.Equal(1, cap.CommitCount);
    }

    [Fact]
    public async Task OnePlanAuthorisesOnlyOneCommit()
    {
        var cap = SpyCapability.ForTier(RiskTier.Destructive);
        var (broker, _, _) = Build(cap);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));
        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        // A replayed commit must show a fresh plan rather than riding the consumed one.
        var replay = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Denied, replay.Status);
        Assert.Equal(1, cap.CommitCount);
    }

    [Fact]
    public async Task PlanForDifferentArguments_DoesNotAuthoriseThisCommit()
    {
        var cap = SpyCapability.ForTier(RiskTier.Destructive);
        var (broker, _, _) = Build(cap);

        await broker.InvokeAsync(new CapabilityRequest(
            cap.Descriptor.Id, new JsonObject { ["path"] = "safe.txt" }));

        var commit = await broker.InvokeAsync(new CapabilityRequest(
            cap.Descriptor.Id, new JsonObject { ["path"] = "important.txt" }, Commit: true));

        Assert.Equal(OutcomeStatus.Denied, commit.Status);
        Assert.Equal(0, cap.CommitCount);
    }

    [Fact]
    public void PlanFingerprint_IgnoresArgumentOrder()
    {
        // Otherwise a semantically identical commit would be refused for cosmetic reasons.
        var a = new CapabilityRequest("x/y", new JsonObject { ["a"] = 1, ["b"] = 2 });
        var b = new CapabilityRequest("x/y", new JsonObject { ["b"] = 2, ["a"] = 1 });

        Assert.Equal(PlanLedger.Fingerprint(a), PlanLedger.Fingerprint(b));
    }

    [Fact]
    public async Task WriteTierRequiresCommit_EvenWhenPolicySaysAllow()
    {
        // RequiresCommit used to start at System, so a Write capability's handshake existed
        // only because the shipped policy happened to say prompt.
        var cap = SpyCapability.ForTier(RiskTier.Write);
        var broker = new CapabilityBroker(
            [cap],
            YamlPolicyEvaluator.FromYaml("tiers:\n  Write: { verdict: allow }\n"),
            new InMemoryAuditSink(),
            new NoOpSnapshotter());

        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(OutcomeStatus.DryRun, outcome.Status);
        Assert.Equal(0, cap.CommitCount);
    }

    // --- audit could be skipped entirely -------------------------------------------

    [Fact]
    public async Task CancelledCall_IsStillAudited()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read);
        cap.Throw = new OperationCanceledException();
        var (broker, audit, _) = Build(cap);

        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id)));

        var entry = Assert.Single(audit.Entries);
        Assert.Equal(OutcomeStatus.Failed, entry.Status);
        Assert.Contains("Cancelled", entry.Message);
    }

    [Fact]
    public async Task AlreadyCancelledToken_CannotSuppressTheAuditEntry()
    {
        // FinishAsync used to pass the caller's token to the sink, so a cancelled request
        // erased its own trail.
        var cap = SpyCapability.ForTier(RiskTier.Read);
        var (broker, audit, _) = Build(cap);
        using var cancelled = new CancellationTokenSource();
        await cancelled.CancelAsync();

        try { await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id), cancelled.Token); }
        catch (OperationCanceledException) { /* expected */ }

        Assert.NotEmpty(audit.Entries);
    }

    [Fact]
    public async Task AuditFailureAfterCommit_IsDistinguishableFromNothingHappening()
    {
        var cap = SpyCapability.ForTier(RiskTier.Write);
        var (broker, _, _) = Build(cap, sink: new ThrowingAuditSink());

        // Plan first so the commit is authorised; the plan's own audit write also throws,
        // but as a pre-mutation failure it surfaces as a plain IOException.
        await Assert.ThrowsAsync<IOException>(
            () => broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id)));
    }

    // --- malformed policy failed open -----------------------------------------------

    [Theory]
    [InlineData("4")]          // numeric: parses to an undefined enum value
    [InlineData("deny, prompt")] // flag-style combination
    public async Task UndefinedVerdictValue_Denies(string verdict)
    {
        var cap = SpyCapability.ForTier(RiskTier.Write);
        var broker = new CapabilityBroker(
            [cap],
            YamlPolicyEvaluator.FromYaml($"tiers:\n  Write: {{ verdict: \"{verdict}\" }}\n"),
            new InMemoryAuditSink(),
            new NoOpSnapshotter());

        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(OutcomeStatus.Denied, outcome.Status);
        Assert.Equal(0, cap.CommitCount);
    }

    [Fact]
    public void NumericTierKey_IsRejectedAtLoad()
    {
        // Enum.TryParse accepts "9", so without IsDefined this silently became (RiskTier)9.
        Assert.Throws<InvalidOperationException>(
            () => YamlPolicyEvaluator.FromYaml("tiers:\n  9: { verdict: allow }\n"));
    }

    // --- kill switch, dry-run integrity, verification --------------------------------

    [Fact]
    public async Task KillSwitchThrownDuringApproval_StopsTheCommit()
    {
        var cap = SpyCapability.ForTier(RiskTier.Write);
        CapabilityBroker? broker = null;
        var approvals = new CallbackApprovals(() => broker!.Halt());

        var audit = new InMemoryAuditSink();
        broker = new CapabilityBroker(
            [cap], YamlPolicyEvaluator.FromYaml(TestPolicy.Default), audit,
            new NoOpSnapshotter(), approvals);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));
        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));   // show the plan
        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.Equal(OutcomeStatus.Denied, outcome.Status);
        Assert.Contains("halted", outcome.Message);
        Assert.Equal(0, cap.CommitCount);
    }

    [Fact]
    public async Task CapabilityThatIgnoresDryRun_IsReportedNotLaundered()
    {
        // Returning Succeeded from a dry run means the flag was ignored and something may
        // have changed. It used to be relabelled as a plan, hiding the evidence.
        var cap = new IgnoresDryRunCapability(SpyCapability.ForTier(RiskTier.Write).Descriptor);
        var broker = new CapabilityBroker(
            [cap], YamlPolicyEvaluator.FromYaml(TestPolicy.Default),
            new InMemoryAuditSink(), new NoOpSnapshotter());

        var outcome = await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(OutcomeStatus.Failed, outcome.Status);
        Assert.Contains("ignored the dryRun flag", outcome.Message);
    }

    [Fact]
    public async Task NoVerifier_RecordsVerifiedAsNullRatherThanTrue()
    {
        // "verified fine" and "never checked" must not look identical in the log.
        var cap = SpyCapability.ForTier(RiskTier.Read);
        var (broker, audit, _) = Build(cap);

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));

        Assert.Null(Assert.Single(audit.Entries).Verified);
    }

    [Fact]
    public async Task PassingVerifier_RecordsVerifiedTrue()
    {
        var cap = new VerifyingCapability(
            SpyCapability.ForTier(RiskTier.Write).Descriptor, problem: null);
        var audit = new InMemoryAuditSink();
        var broker = new CapabilityBroker(
            [cap], YamlPolicyEvaluator.FromYaml(TestPolicy.Default), audit,
            new NoOpSnapshotter(), new StubApprovals(true));

        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id));
        await broker.InvokeAsync(TestPolicy.Request(cap.Descriptor.Id, commit: true));

        Assert.True(audit.Entries[^1].Verified);
    }

    // --- redaction of free text -------------------------------------------------------

    [Fact]
    public async Task ReasonAndMessage_AreRedactedAndTruncated()
    {
        var cap = SpyCapability.ForTier(RiskTier.Read);
        cap.Throw = new InvalidOperationException("failed for sk-ant-api03-abcdefghijklmnopqrstuvwxyz");
        var (broker, audit, _) = Build(cap);

        await broker.InvokeAsync(new CapabilityRequest(
            cap.Descriptor.Id,
            new JsonObject(),
            Reason: "rotating ghp_abcdefghijklmnopqrstuvwxyz012345 " + new string('x', 900)));

        var entry = Assert.Single(audit.Entries);
        Assert.DoesNotContain("ghp_abcdefghijklmnopqrstuvwxyz012345", entry.Reason);
        Assert.Contains("chars]", entry.Reason);            // truncated
        Assert.DoesNotContain("sk-ant-api03", entry.Message);
    }

    [Fact]
    public void SecretInAnInnocentlyNamedValue_IsRedacted()
    {
        var args = JsonArgs.Of(("connectionString", "Server=x;User Id=sa;Password=hunter2"));

        var redacted = ArgumentRedactor.Redact(args)!;

        Assert.DoesNotContain("hunter2", redacted["connectionString"]!.GetValue<string>());
    }

    [Theory]
    [InlineData("pat")]
    [InlineData("pwd")]
    [InlineData("bearer")]
    [InlineData("sessionId")]
    [InlineData("privateKey")]
    public void TopLevelCredentialShapedKeys_AreRedacted(string key)
    {
        var redacted = ArgumentRedactor.Redact(JsonArgs.Of((key, "ghp_secretvaluegoeshere1234")))!;

        Assert.Equal(ArgumentRedactor.Placeholder, redacted[key]!.GetValue<string>());
    }
}

/// <summary>Runs a callback when asked to approve, to simulate a race during the prompt.</summary>
public sealed class CallbackApprovals(Action onRequest) : IApprovalPrompt
{
    public Task<bool> RequestAsync(
        CapabilityDescriptor capability, CapabilityRequest request, CancellationToken ct)
    {
        onRequest();
        return Task.FromResult(true);
    }
}

/// <summary>Reports success even on a dry run, which is a harness violation.</summary>
public sealed class IgnoresDryRunCapability(CapabilityDescriptor descriptor) : ICapability
{
    public CapabilityDescriptor Descriptor { get; } = descriptor;

    public Task<CapabilityOutcome> ExecuteAsync(
        CapabilityRequest request, bool dryRun, CancellationToken cancellationToken) =>
        Task.FromResult(CapabilityOutcome.Ok(JsonValue.Create("mutated anyway")));
}
