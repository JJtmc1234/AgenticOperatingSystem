using System.Diagnostics;
using Aos.Core;

namespace Aos.Broker;

/// <summary>
/// The single gate every capability invocation passes through. Responsibilities, in order:
/// resolve the capability, evaluate policy, enforce the plan-then-commit handshake against
/// plans actually shown, take a restore point when committing system state, execute, verify,
/// and audit. Always audit, including denials, cancellations and failures.
/// </summary>
public sealed class CapabilityBroker
{
    private readonly Dictionary<string, ICapability> _capabilities;
    private readonly IPolicyEvaluator _policy;
    private readonly IAuditSink _audit;
    private readonly IStateSnapshotter _snapshotter;
    private readonly IApprovalPrompt _approvals;
    private readonly PlanLedger _plans;
    private readonly TimeProvider _clock;

    private volatile bool _halted;

    public CapabilityBroker(
        IEnumerable<ICapability> capabilities,
        IPolicyEvaluator policy,
        IAuditSink audit,
        IStateSnapshotter? snapshotter = null,
        IApprovalPrompt? approvals = null,
        TimeProvider? clock = null,
        PlanLedger? plans = null)
    {
        _capabilities = new Dictionary<string, ICapability>(StringComparer.OrdinalIgnoreCase);
        foreach (var capability in capabilities)
        {
            var id = capability.Descriptor.Id;
            if (!_capabilities.TryAdd(id, capability))
            {
                throw new InvalidOperationException($"Duplicate capability id '{id}'.");
            }
        }

        _policy = policy;
        _audit = audit;
        _snapshotter = snapshotter ?? UnavailableSnapshotter.Instance;
        _approvals = approvals ?? HandshakeOnlyApprovals.Instance;
        _clock = clock ?? TimeProvider.System;
        _plans = plans ?? new PlanLedger(_clock);
    }

    public IReadOnlyCollection<CapabilityDescriptor> Descriptors =>
        _capabilities.Values.Select(c => c.Descriptor).ToArray();

    /// <summary>
    /// Kill switch. Once halted, every request is denied until <see cref="Resume"/>.
    /// Wired to the tray menu and the global hotkey listener.
    /// </summary>
    public void Halt() => _halted = true;

    public void Resume() => _halted = false;

    public bool IsHalted => _halted;

    public async Task<CapabilityOutcome> InvokeAsync(
        CapabilityRequest request, CancellationToken cancellationToken = default)
    {
        var correlationId = request.CorrelationId ?? Guid.NewGuid().ToString("n");
        var stopwatch = Stopwatch.StartNew();

        if (!_capabilities.TryGetValue(request.CapabilityId, out var capability))
        {
            // Unknown ids are audited against a synthetic descriptor: an agent probing for
            // capabilities that do not exist is worth seeing in the log.
            var unknown = new CapabilityDescriptor(
                request.CapabilityId, "unknown", RiskTier.Destructive, "Unregistered capability.");

            return await FinishAsync(
                unknown, request, correlationId, PolicyVerdict.Deny, dryRun: false,
                CapabilityOutcome.Denied($"No capability registered as '{request.CapabilityId}'."),
                snapshotId: null, verified: null, stopwatch).ConfigureAwait(false);
        }

        var descriptor = capability.Descriptor;

        if (_halted)
        {
            return await FinishAsync(
                descriptor, request, correlationId, PolicyVerdict.Deny, dryRun: false,
                CapabilityOutcome.Denied("Broker is halted (kill switch engaged)."),
                snapshotId: null, verified: null, stopwatch).ConfigureAwait(false);
        }

        var decision = _policy.Evaluate(descriptor, request);

        // Only Allow and Prompt are recognised. Anything else, including a verdict that
        // parsed out of malformed policy into an undefined enum value, denies. Testing only
        // for Deny here is what let such a value fall through as if it were Allow.
        if (decision.Verdict is not (PolicyVerdict.Allow or PolicyVerdict.Prompt))
        {
            return await FinishAsync(
                descriptor, request, correlationId, PolicyVerdict.Deny, dryRun: false,
                CapabilityOutcome.Denied($"Denied by policy: {decision.Reason}"),
                snapshotId: null, verified: null, stopwatch).ConfigureAwait(false);
        }

        // Plan-then-commit: anything not plainly allowed needs an explicit second call.
        var needsHandshake = decision.DryRunOnly
            || descriptor.RequiresCommit
            || decision.Verdict == PolicyVerdict.Prompt;

        var dryRun = needsHandshake && !request.Commit;

        // A commit has to redeem a plan this process actually produced for these exact
        // arguments. Without the ledger the handshake was a single call, since nothing
        // remembered whether a plan had ever been shown.
        if (needsHandshake && request.Commit && !_plans.TryConsumePlan(request))
        {
            return await FinishAsync(
                descriptor, request, correlationId, decision.Verdict, dryRun: false,
                CapabilityOutcome.Denied(
                    "No plan has been shown for this exact call, or the plan expired after "
                    + $"{PlanLedger.Lifetime.TotalMinutes:0} minutes. Invoke it without commit "
                    + "first, read the plan, then repeat the call with commit=true and "
                    + "identical arguments."),
                snapshotId: null, verified: null, stopwatch).ConfigureAwait(false);
        }

        if (!dryRun && decision.Verdict == PolicyVerdict.Prompt)
        {
            var approved = await _approvals
                .RequestAsync(descriptor, request, cancellationToken).ConfigureAwait(false);

            if (!approved)
            {
                return await FinishAsync(
                    descriptor, request, correlationId, decision.Verdict, dryRun: false,
                    CapabilityOutcome.Denied("Human approval was not granted."),
                    snapshotId: null, verified: null, stopwatch).ConfigureAwait(false);
            }
        }

        string? snapshotId = null;
        if (!dryRun && descriptor.ShouldSnapshot)
        {
            var snapshot = await _snapshotter
                .SnapshotAsync(descriptor, cancellationToken).ConfigureAwait(false);

            if (!snapshot.Available)
            {
                // Refuse rather than commit an irreversible change with no restore point.
                return await FinishAsync(
                    descriptor, request, correlationId, decision.Verdict, dryRun: false,
                    CapabilityOutcome.Denied(
                        $"Refusing to commit {descriptor.Tier} capability without a restore point. "
                        + snapshot.Message),
                    snapshotId: null, verified: null, stopwatch).ConfigureAwait(false);
            }

            snapshotId = snapshot.SnapshotId;
        }

        // Re-checked here on purpose. The approval prompt and a VSS snapshot are both long
        // awaits, and a kill switch thrown during them has to stop the change rather than
        // being ignored because the flag was read minutes earlier.
        if (_halted)
        {
            return await FinishAsync(
                descriptor, request, correlationId, PolicyVerdict.Deny, dryRun: false,
                CapabilityOutcome.Denied("Broker was halted before the change was applied."),
                snapshotId, verified: null, stopwatch).ConfigureAwait(false);
        }

        CapabilityOutcome outcome;
        try
        {
            outcome = await capability
                .ExecuteAsync(request, dryRun, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            // Audited before rethrowing. A cancelled commit is the most dangerous state
            // there is: the change may well have been applied, and it used to leave no log
            // line at all because this path skipped the audit entirely. A capability call
            // that times out on the wire raises TaskCanceledException, so this is a routine
            // path rather than a theoretical one.
            await FinishAsync(
                descriptor, request, correlationId, decision.Verdict, dryRun,
                CapabilityOutcome.Failed(
                    dryRun
                        ? "Cancelled during a dry run; nothing was applied."
                        : "Cancelled mid-commit. The change may or may not have been applied. "
                          + "Verify before retrying."),
                snapshotId, verified: null, stopwatch).ConfigureAwait(false);

            throw;
        }
        catch (Exception ex)
        {
            outcome = CapabilityOutcome.Failed($"{ex.GetType().Name}: {ex.Message}");
        }

        if (dryRun)
        {
            outcome = HandleDryRunOutcome(request, outcome);
        }

        bool? verified = null;
        if (!dryRun
            && outcome.Status == OutcomeStatus.Succeeded
            && capability is IVerifiableCapability verifiable)
        {
            string? problem;
            try
            {
                problem = await verifiable
                    .VerifyAsync(request, outcome, cancellationToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                problem = "Verification was cancelled.";
            }
            catch (Exception ex)
            {
                problem = $"Verification threw {ex.GetType().Name}: {ex.Message}";
            }

            verified = problem is null;

            if (problem is not null)
            {
                outcome = CapabilityOutcome.Unverified(
                    outcome.Payload,
                    $"The change was applied but could not be verified: {problem} "
                    + "Do not retry blindly, since the change may already be in effect.");
            }
        }

        return await FinishAsync(
            descriptor, request, correlationId, decision.Verdict, dryRun,
            outcome, snapshotId, verified, stopwatch).ConfigureAwait(false);
    }

    /// <summary>
    /// Normalises a dry-run result and records the plan.
    ///
    /// A capability that returns Succeeded from a dry run has ignored the flag and may have
    /// mutated. That used to be silently relabelled as a plan, which hid the evidence and
    /// handed the real payload back as if it were a preview. It is now reported as a harness
    /// violation, and no plan is recorded, so nothing can be committed against it.
    /// </summary>
    private CapabilityOutcome HandleDryRunOutcome(CapabilityRequest request, CapabilityOutcome outcome)
    {
        switch (outcome.Status)
        {
            case OutcomeStatus.Succeeded:
                return CapabilityOutcome.Failed(
                    "Harness violation: the capability reported success during a dry run, so it "
                    + "ignored the dryRun flag and may have changed something. Refusing to treat "
                    + "this as a plan.");

            case OutcomeStatus.DryRun:
                _plans.RecordPlan(request);
                return outcome;

            default:
                // Denied, Failed or AppliedButUnverified from a dry run: pass it through
                // unchanged and record no plan. AppliedButUnverified in particular must never
                // be laundered into looking like a preview.
                return outcome;
        }
    }

    private async Task<CapabilityOutcome> FinishAsync(
        CapabilityDescriptor descriptor,
        CapabilityRequest request,
        string correlationId,
        PolicyVerdict verdict,
        bool dryRun,
        CapabilityOutcome outcome,
        string? snapshotId,
        bool? verified,
        Stopwatch stopwatch)
    {
        stopwatch.Stop();

        var entry = new AuditEntry
        {
            Timestamp = _clock.GetUtcNow(),
            CorrelationId = correlationId,
            CapabilityId = descriptor.Id,
            Tier = descriptor.Tier,
            Verdict = verdict,
            Status = outcome.Status,
            DryRun = dryRun,
            DurationMs = stopwatch.ElapsedMilliseconds,
            Arguments = ArgumentRedactor.Redact(request.Arguments),
            // Both are agent or capability controlled free text and used to reach the log
            // verbatim, which sidestepped redaction and the length cap entirely.
            Reason = ArgumentRedactor.RedactText(request.Reason),
            Message = ArgumentRedactor.RedactText(outcome.Message),
            SnapshotId = snapshotId,
            Verified = verified,
        };

        // Deliberately not swallowed: an unauditable system is not one to trust with
        // system-level capabilities, so a failed write fails the call.
        //
        // CancellationToken.None on purpose. Passing the caller's token let an
        // already-cancelled request suppress its own audit trail, which is precisely the
        // record you most want when a call is abandoned mid-flight.
        try
        {
            await _audit.WriteAsync(entry, CancellationToken.None).ConfigureAwait(false);
        }
        // Only when something actually changed. A Read whose audit write fails has mutated
        // nothing, so reporting it as an unaudited mutation would be alarming and wrong.
        catch (Exception ex) when (!dryRun
                                   && descriptor.Tier >= RiskTier.Write
                                   && outcome.Status is OutcomeStatus.Succeeded
                                       or OutcomeStatus.AppliedButUnverified)
        {
            // The change already happened. A bare rethrow here is indistinguishable from the
            // pre-execute denial paths, which read as "nothing happened" and invite a retry
            // that applies the change twice.
            throw new UnauditedMutationException(descriptor.Id, ex);
        }

        return outcome;
    }
}

/// <summary>
/// Thrown when a change was applied but could not be recorded in the audit log. Distinct
/// from a plain audit failure so a caller can tell "nothing happened" from "it happened and
/// we have no record", and therefore knows not to retry.
/// </summary>
public sealed class UnauditedMutationException(string capabilityId, Exception inner)
    : Exception(
        $"'{capabilityId}' was applied but the audit write failed, so there is no record of it. "
        + "Do not retry: the change is already in effect.", inner)
{
    public string CapabilityId { get; } = capabilityId;
}
