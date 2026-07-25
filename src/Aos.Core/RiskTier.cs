namespace Aos.Core;

/// <summary>
/// Escalating blast radius of a capability. Policy is expressed per tier, so every
/// capability must declare one honestly — this is the primary safety control.
/// </summary>
public enum RiskTier
{
    /// <summary>Observes only. Cannot change machine state.</summary>
    Read = 0,

    /// <summary>Mutates user data in recoverable ways (create/edit a file, send a draft).</summary>
    Write = 1,

    /// <summary>Mutates machine or OS state (registry, services, processes, network config).</summary>
    System = 2,

    /// <summary>Loses data or is hard to reverse (delete, overwrite, format, uninstall).</summary>
    Destructive = 3,
}
