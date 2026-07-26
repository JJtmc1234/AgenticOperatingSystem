namespace Aos.Core;

/// <summary>
/// Static declaration of one capability. Registered at startup so the broker can make
/// policy decisions without invoking anything.
/// </summary>
/// <param name="Id">Stable dotted id, namespaced by server: <c>aos-files/file.move</c>.</param>
/// <param name="Server">Owning MCP server name, e.g. <c>aos-files</c>.</param>
/// <param name="Tier">Blast radius. Drives policy lookup.</param>
/// <param name="Description">One line, surfaced to the model and in approval prompts.</param>
/// <param name="SnapshotBeforeExecute">
/// Take a VSS shadow copy before committing. Defaults on for <see cref="RiskTier.System"/>
/// and above so system-state changes stay reversible.
/// </param>
public sealed record CapabilityDescriptor(
    string Id,
    string Server,
    RiskTier Tier,
    string Description,
    bool? SnapshotBeforeExecute = null)
{
    /// <summary>
    /// Whether a caller must pass <see cref="CapabilityRequest.Commit"/> to move past a
    /// dry run. Anything that mutates requires the second call.
    ///
    /// Covers <see cref="RiskTier.Write"/> deliberately. When this started at
    /// <see cref="RiskTier.System"/>, a Write capability's handshake existed only because
    /// the shipped policy happened to say prompt, so a one-line policy edit removed it
    /// entirely. Deriving it from the tier is what makes the guarantee structural rather
    /// than a property of the current config file.
    /// </summary>
    public bool RequiresCommit => Tier >= RiskTier.Write;

    public bool ShouldSnapshot => SnapshotBeforeExecute ?? Tier >= RiskTier.System;
}
