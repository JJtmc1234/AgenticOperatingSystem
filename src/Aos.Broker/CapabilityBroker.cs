using System.Diagnostics;
using Aos.Core;

namespace Aos.Broker;

/// <summary>
/// The single gate every capability invocation passes through. Responsibilities, in order:
/// resolve the capability, evaluate policy, apply the plan-then-commit handshake, take a
/// restore point when committing system state, execute, and audit -- always audit, including
/// denials and failures.
/// </summary>
public sealed class CapabilityBroker
{
    private readonly Dictionary<string, ICapability> _capabilities;
    private readonly IPolicyEvaluator _policy;
    private readonly IAuditSink _audit;
    private readonly IStateSnapshotter _snapshotter;
    private readonly IApprovalPrompt _approvals;
    private readonly TimeProvider _clock;

    private volatile bool _halted;

    public CapabilityBroker(
        IEnumerable<ICapability> capabilities,
        IPolicyEvaluator policy,
        IAuditSink audit,
        IStateSnapshotter? snapshotter = null,
        IApprovalPrompt? approvals = null,
        TimeProvider? clock = null)
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
    }

    public IReadOnlyCollection<CapabilityDescriptor> Descriptors =>
        _capabilities.Values.Select(c => c.Descriptor).ToArray();

    /// <summary>
    /// Kill switch. Once halted, every request is denied until <see cref="Resume"/>.
    /// Wired to the HUD tray item and global hotkey in Phase 3.
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
                snapshotId: null, stopwatch, cancellationToken).ConfigureAwait(false);
        }

        var descriptor = capability.Descriptor;

        if (_halted)
        {
            return await FinishAsync(
                descriptor, request, correlationId, PolicyVerdict.Deny, dryRun: false,
                CapabilityOutcome.Denied("Broker is halted (kill switch engaged)."),
                snapshotId: null, stopwatch, cancellationToken).ConfigureAwait(false);
        }

        var decision = _policy.Evaluate(descriptor, request);

        if (decision.Verdict == PolicyVerdict.Deny)
        {
            return await FinishAsync(
                descriptor, request, correlationId, decision.Verdict, dryRun: false,
                CapabilityOutcome.Denied($"Denied by policy: {decision.Reason}"),
                snapshotId: null, stopwatch, cancellationToken).ConfigureAwait(false);
        }

        // Plan-then-commit: anything that is not plainly allowed needs an explicit second
        // call. This is what makes a destructive mistake take two deliberate steps.
        var needsHandshake = decision.DryRunOnly
            || descriptor.RequiresCommit
            || decision.Verdict == PolicyVerdict.Prompt;

        var dryRun = needsHandshake && !request.Commit;

        if (!dryRun && decision.Verdict == PolicyVerdict.Prompt)
        {
            var approved = await _approvals
                .RequestAsync(descriptor, request, cancellationToken).ConfigureAwait(false);

            if (!approved)
            {
                return await FinishAsync(
                    descriptor, request, correlationId, decision.Verdict, dryRun: false,
                    CapabilityOutcome.Denied("Human approval was not granted."),
                    snapshotId: null, stopwatch, cancellationToken).ConfigureAwait(false);
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
                        $"Refusing to commit {descriptor.Tier} capability without a restore point. " +
                        snapshot.Message),
                    snapshotId: null, stopwatch, cancellationToken).ConfigureAwait(false);
            }

            snapshotId = snapshot.SnapshotId;
        }

        CapabilityOutcome outcome;
        try
        {
            outcome = await capability
                .ExecuteAsync(request, dryRun, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (Exception ex)
        {
            outcome = CapabilityOutcome.Failed($"{ex.GetType().Name}: {ex.Message}");
        }

        // A dry run that reports success would let the agent believe work happened.
        if (dryRun && outcome.Status == OutcomeStatus.Succeeded)
        {
            outcome = CapabilityOutcome.Planned(
                outcome.Payload,
                outcome.Message ?? "Dry run: nothing changed. Re-invoke with commit=true to apply.");
        }

        // Post-condition check. Only meaningful after a committed change actually succeeded.
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
                throw;
            }
            catch (Exception ex)
            {
                problem = $"Verification threw {ex.GetType().Name}: {ex.Message}";
            }

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
            outcome, snapshotId, stopwatch, cancellationToken).ConfigureAwait(false);
    }

    private async Task<CapabilityOutcome> FinishAsync(
        CapabilityDescriptor descriptor,
        CapabilityRequest request,
        string correlationId,
        PolicyVerdict verdict,
        bool dryRun,
        CapabilityOutcome outcome,
        string? snapshotId,
        Stopwatch stopwatch,
        CancellationToken cancellationToken)
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
            Reason = request.Reason,
            Message = outcome.Message,
            SnapshotId = snapshotId,
        };

        // Deliberately not swallowed: an unauditable system is not one to trust with
        // system-level capabilities, so a failed write fails the call.
        await _audit.WriteAsync(entry, cancellationToken).ConfigureAwait(false);

        return outcome;
    }
}
