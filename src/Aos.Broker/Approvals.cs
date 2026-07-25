using Aos.Core;

namespace Aos.Broker;

/// <summary>
/// Asks the human to approve one specific call. Supplied by the HUD in Phase 3.
/// </summary>
public interface IApprovalPrompt
{
    Task<bool> RequestAsync(
        CapabilityDescriptor capability,
        CapabilityRequest request,
        CancellationToken cancellationToken);
}

/// <summary>
/// Used when no interactive approver is wired up, which is the Phase 1 situation: the MCP
/// server is driven by a client (Claude Code, Claude Desktop) that already shows the human
/// every tool call before it runs.
///
/// Rather than auto-approving, a <see cref="PolicyVerdict.Prompt"/> call degrades to a
/// plan-then-commit handshake in <see cref="CapabilityBroker"/>: the first call returns a
/// dry-run plan, and only an explicit second call with <c>commit: true</c> applies it. The
/// human approving that second tool call in their client *is* the approval.
/// </summary>
public sealed class HandshakeOnlyApprovals : IApprovalPrompt
{
    public static readonly HandshakeOnlyApprovals Instance = new();

    public Task<bool> RequestAsync(
        CapabilityDescriptor capability,
        CapabilityRequest request,
        CancellationToken cancellationToken) => Task.FromResult(request.Commit);
}
