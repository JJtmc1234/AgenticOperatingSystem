using Aos.Core;
using Xunit;

namespace Aos.Broker.Tests;

public class PolicyEvaluatorTests
{
    private static PolicyDecision Evaluate(RiskTier tier, string? id = null)
    {
        var cap = SpyCapability.ForTier(tier, id);
        return YamlPolicyEvaluator.FromYaml(TestPolicy.Default)
            .Evaluate(cap.Descriptor, TestPolicy.Request(cap.Descriptor.Id));
    }

    [Theory]
    [InlineData(RiskTier.Read, PolicyVerdict.Allow, false)]
    [InlineData(RiskTier.Write, PolicyVerdict.Prompt, false)]
    [InlineData(RiskTier.System, PolicyVerdict.Prompt, true)]
    [InlineData(RiskTier.Destructive, PolicyVerdict.Prompt, true)]
    public void TierRules_MapToExpectedVerdicts(
        RiskTier tier, PolicyVerdict expected, bool expectedDryRunOnly)
    {
        var decision = Evaluate(tier);

        Assert.Equal(expected, decision.Verdict);
        Assert.Equal(expectedDryRunOnly, decision.DryRunOnly);
    }

    [Fact]
    public void CapabilityRule_OverridesTierRule()
    {
        // Read tier would be 'allow', but the capability-specific rule denies it.
        var decision = Evaluate(RiskTier.Read, "test/blocked.op");

        Assert.Equal(PolicyVerdict.Deny, decision.Verdict);
        Assert.Contains("blocked for tests", decision.Reason);
    }

    [Fact]
    public void MissingTierRule_FailsClosed()
    {
        var evaluator = YamlPolicyEvaluator.FromYaml("tiers:\n  Read: { verdict: allow }\n");
        var cap = SpyCapability.ForTier(RiskTier.Destructive);

        var decision = evaluator.Evaluate(cap.Descriptor, TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(PolicyVerdict.Deny, decision.Verdict);
    }

    [Fact]
    public void MalformedVerdict_FailsClosed()
    {
        var evaluator = YamlPolicyEvaluator.FromYaml("tiers:\n  Read: { verdict: yolo }\n");
        var cap = SpyCapability.ForTier(RiskTier.Read);

        var decision = evaluator.Evaluate(cap.Descriptor, TestPolicy.Request(cap.Descriptor.Id));

        Assert.Equal(PolicyVerdict.Deny, decision.Verdict);
    }

    [Fact]
    public void UnknownTierName_IsRejectedLoudly()
    {
        // A typo in policy.yaml must not silently leave a tier unpoliced.
        Assert.Throws<InvalidOperationException>(
            () => YamlPolicyEvaluator.FromYaml("tiers:\n  Reed: { verdict: allow }\n"));
    }

    [Fact]
    public void PolicyCannotDowngradeCommitHandshake()
    {
        // dryRunOnly omitted for a System tier: RequiresCommit must still force it.
        var evaluator = YamlPolicyEvaluator.FromYaml("tiers:\n  System: { verdict: allow }\n");
        var cap = SpyCapability.ForTier(RiskTier.System);

        var decision = evaluator.Evaluate(cap.Descriptor, TestPolicy.Request(cap.Descriptor.Id));

        Assert.True(decision.DryRunOnly);
    }

    [Fact]
    public void ShippedPolicyFile_Parses()
    {
        var path = Path.Combine(RepoRoot(), "policy", "default.yaml");
        var evaluator = YamlPolicyEvaluator.FromFile(path);

        // The real file must police every tier the enum defines, or a capability at an
        // unmapped tier silently falls through to the fail-closed default.
        foreach (var tier in Enum.GetValues<RiskTier>())
        {
            Assert.True(
                evaluator.Document.Tiers.ContainsKey(tier.ToString()),
                $"policy/default.yaml has no rule for tier '{tier}'.");
        }

        // Read is the only tier permitted to auto-allow.
        var readOnly = SpyCapability.ForTier(RiskTier.Read, "unmapped/read.op");
        Assert.Equal(
            PolicyVerdict.Allow,
            evaluator.Evaluate(readOnly.Descriptor, TestPolicy.Request(readOnly.Descriptor.Id)).Verdict);

        foreach (var tier in new[] { RiskTier.Write, RiskTier.System, RiskTier.Destructive })
        {
            var cap = SpyCapability.ForTier(tier, $"unmapped/{tier}.op");
            var decision = evaluator.Evaluate(cap.Descriptor, TestPolicy.Request(cap.Descriptor.Id));
            Assert.NotEqual(PolicyVerdict.Allow, decision.Verdict);
        }

        Assert.Contains("aos-shell/exec.raw", evaluator.Document.Capabilities.Keys);
        Assert.Equal("deny", evaluator.Document.Capabilities["aos-shell/exec.raw"].Verdict);
    }

    private static string RepoRoot()
    {
        var dir = AppContext.BaseDirectory;
        while (dir is not null && !File.Exists(Path.Combine(dir, "aos.sln")))
        {
            dir = Path.GetDirectoryName(dir);
        }
        return dir ?? throw new InvalidOperationException("aos.sln not found above test output.");
    }
}

public class PathGuardTests
{
    private static readonly PathGuard Guard = new([
        @"C:\Windows\System32",
        @"C:\Program Files",
        @"%USERPROFILE%\.ssh",
    ]);

    [Theory]
    [InlineData(@"C:\Windows\System32")]
    [InlineData(@"C:\Windows\System32\drivers\etc\hosts")]
    [InlineData(@"c:\windows\system32\cmd.exe")]          // case-insensitive
    [InlineData(@"C:\Windows\System32\")]                  // trailing separator
    [InlineData(@"C:\Windows\System32\..\System32\cmd.exe")] // traversal that lands inside
    [InlineData(@"C:\Program Files\app\thing.dll")]
    public void DeniesProtectedPaths(string path) => Assert.True(Guard.IsDenied(path));

    [Theory]
    [InlineData(@"C:\Users\pmarc\Documents\notes.md")]
    [InlineData(@"C:\ProgramData\thing")]      // must not match "C:\Program Files"
    [InlineData(@"C:\Program Filesx\thing")]   // prefix, not a child
    [InlineData(@"C:\Windows\Temp\scratch")]   // sibling of System32
    public void AllowsEverythingElse(string path) => Assert.False(Guard.IsDenied(path));

    [Fact]
    public void ExpandsEnvironmentVariables()
    {
        var ssh = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".ssh", "id_rsa");
        Assert.True(Guard.IsDenied(ssh));
    }

    [Fact]
    public void EnsureAllowed_ThrowsOnDeniedPath() =>
        Assert.Throws<UnauthorizedAccessException>(
            () => Guard.EnsureAllowed(@"C:\Windows\System32\cmd.exe"));
}
