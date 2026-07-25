using Aos.Core;

namespace Aos.Broker;

public sealed record SnapshotResult(bool Available, string? SnapshotId, string? Message);

/// <summary>
/// Takes a restore point before a commit that changes system state. Implementations must
/// report unavailability rather than silently succeeding -- the broker refuses to commit
/// <see cref="RiskTier.System"/> and above without one.
/// </summary>
public interface IStateSnapshotter
{
    Task<SnapshotResult> SnapshotAsync(
        CapabilityDescriptor capability,
        CancellationToken cancellationToken);
}

/// <summary>
/// Declares snapshots unavailable. This is the correct default for an unelevated process:
/// VSS shadow copy creation requires administrator rights, so an unelevated broker cannot
/// honour the reversibility promise and must refuse System+ commits instead of pretending.
/// The real VSS implementation arrives with the elevated Windows service in Phase 4.
/// </summary>
public sealed class UnavailableSnapshotter : IStateSnapshotter
{
    public static readonly UnavailableSnapshotter Instance = new();

    public Task<SnapshotResult> SnapshotAsync(
        CapabilityDescriptor capability, CancellationToken cancellationToken) =>
        Task.FromResult(new SnapshotResult(
            Available: false,
            SnapshotId: null,
            Message: "No snapshot provider (VSS requires an elevated host)."));
}

/// <summary>Records snapshot requests without taking one. Tests and dry-run hosts only.</summary>
public sealed class NoOpSnapshotter : IStateSnapshotter
{
    private int _counter;

    public Task<SnapshotResult> SnapshotAsync(
        CapabilityDescriptor capability, CancellationToken cancellationToken) =>
        Task.FromResult(new SnapshotResult(true, $"noop-{Interlocked.Increment(ref _counter)}", null));
}
