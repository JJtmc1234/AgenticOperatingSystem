using Aos.Core;
using YamlDotNet.Serialization;
using YamlDotNet.Serialization.NamingConventions;

namespace Aos.Broker;

/// <summary>
/// Evaluates <c>policy.yaml</c>. Resolution order: capability-specific rule, then tier
/// rule, then fail closed.
/// </summary>
public sealed class YamlPolicyEvaluator : IPolicyEvaluator
{
    private readonly Dictionary<RiskTier, PolicyRule> _tiers = new();
    private readonly Dictionary<string, PolicyRule> _capabilities;

    public PolicyDocument Document { get; }

    public YamlPolicyEvaluator(PolicyDocument document)
    {
        Document = document;

        foreach (var (name, rule) in document.Tiers)
        {
            // Enum.TryParse also accepts numeric text, so "9" would parse to (RiskTier)9 and
            // slip past this check as a tier that is never consulted. IsDefined is what makes
            // the load-time rejection actually cover unknown tiers.
            if (Enum.TryParse<RiskTier>(name, ignoreCase: true, out var tier)
                && Enum.IsDefined(tier))
            {
                _tiers[tier] = rule;
            }
            else
            {
                throw new InvalidOperationException(
                    $"policy.yaml declares an unknown tier '{name}'. Expected one of: " +
                    string.Join(", ", Enum.GetNames<RiskTier>()));
            }
        }

        _capabilities = new Dictionary<string, PolicyRule>(
            document.Capabilities, StringComparer.OrdinalIgnoreCase);
    }

    public static YamlPolicyEvaluator FromYaml(string yaml)
    {
        var deserializer = new DeserializerBuilder()
            .WithNamingConvention(CamelCaseNamingConvention.Instance)
            .IgnoreUnmatchedProperties()
            .Build();

        var doc = deserializer.Deserialize<PolicyDocument>(yaml)
            ?? throw new InvalidOperationException("policy.yaml is empty.");

        return new YamlPolicyEvaluator(doc);
    }

    public static YamlPolicyEvaluator FromFile(string path) =>
        FromYaml(File.ReadAllText(path));

    public PolicyDecision Evaluate(CapabilityDescriptor capability, CapabilityRequest request)
    {
        if (_capabilities.TryGetValue(capability.Id, out var specific))
        {
            return Materialize(specific, capability, $"capability rule for {capability.Id}");
        }

        if (_tiers.TryGetValue(capability.Tier, out var tierRule))
        {
            return Materialize(tierRule, capability, $"tier rule for {capability.Tier}");
        }

        // Fail closed: an undeclared tier is a policy authoring gap, not permission.
        return PolicyDecision.Denied(
            $"No policy rule for {capability.Id} (tier {capability.Tier}); denying by default.");
    }

    private static PolicyDecision Materialize(
        PolicyRule rule, CapabilityDescriptor capability, string source)
    {
        var reason = rule.Reason ?? source;

        // IsDefined matters as much as TryParse. Enum.TryParse happily accepts numeric and
        // comma-separated flag text, so 'verdict: 4' parsed to (PolicyVerdict)4 and
        // 'verdict: "deny, prompt"' to 3. Neither equals Deny or Prompt, so both used to be
        // treated as Allow: a malformed verdict failed open, which is the opposite of the
        // guarantee this method exists to provide.
        if (!Enum.TryParse<PolicyVerdict>(rule.Verdict, ignoreCase: true, out var verdict)
            || !Enum.IsDefined(verdict))
        {
            return PolicyDecision.Denied(
                $"Malformed verdict '{rule.Verdict}' in {source}; denying by default.");
        }

        if (verdict == PolicyVerdict.Deny)
        {
            return PolicyDecision.Denied(reason);
        }

        // A tier that requires a commit handshake cannot be downgraded by omitting
        // dryRunOnly in the policy file.
        var dryRunOnly = (rule.DryRunOnly ?? false) || capability.RequiresCommit;

        return new PolicyDecision(verdict, reason, dryRunOnly);
    }
}
