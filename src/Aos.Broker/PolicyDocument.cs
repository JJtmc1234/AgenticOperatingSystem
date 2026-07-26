namespace Aos.Broker;

/// <summary>One rule from policy.yaml. All members optional so rules can partially override.</summary>
public sealed class PolicyRule
{
    public string? Verdict { get; set; }
    public bool? DryRunOnly { get; set; }
    public string? Reason { get; set; }
}

/// <summary>Deserialized shape of <c>policy/default.yaml</c>.</summary>
public sealed class PolicyDocument
{
    /// <summary>Keyed by <see cref="Aos.Core.RiskTier"/> name.</summary>
    public Dictionary<string, PolicyRule> Tiers { get; set; } = new();

    /// <summary>Keyed by capability id. Overrides the tier rule.</summary>
    public Dictionary<string, PolicyRule> Capabilities { get; set; } = new();

    /// <summary>
    /// Folders the file capabilities may work inside. Empty means the whole filesystem,
    /// minus <see cref="DenyPaths"/>.
    /// </summary>
    public List<string> AllowedRoots { get; set; } = new();

    /// <summary>Paths no capability may touch, whatever the tier says.</summary>
    public List<string> DenyPaths { get; set; } = new();

    /// <summary>
    /// Executables <c>aos-shell</c> may launch, by bare name. Empty means none, so the shell
    /// server is inert until the list is populated deliberately.
    /// </summary>
    public List<string> AllowedCommands { get; set; } = new();

    /// <summary>
    /// Argument patterns refused per command, keyed by bare command name. Regexes, matched
    /// case-insensitively against each individual argument.
    ///
    /// Needed because an allowlist only bounds which binary starts, not what it will do once
    /// started. Several otherwise reasonable tools take a "now run this" argument, so the
    /// command name alone is not a sufficient boundary.
    /// </summary>
    public Dictionary<string, List<string>> DeniedArguments { get; set; } = new();

    public string? TrashPath { get; set; }
    public string? AuditPath { get; set; }
}
