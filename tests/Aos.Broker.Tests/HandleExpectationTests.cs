using System.Reflection;
using Aos.Mcp.Windows;
using ModelContextProtocol.Server;
using Xunit;

namespace Aos.Broker.Tests;

/// <summary>
/// Structural guard on the aos-windows tool surface.
///
/// Window handles and process ids are both recycled by Windows, and refs from ui.tree are
/// positional. Plan and commit are two separate broker calls that each re-resolve the target
/// from scratch, so any of those three can silently point the committed action at something
/// other than what the plan described. The capabilities check an expectation when one is
/// supplied, but that guard is inert unless the tool signature actually offers the parameter.
///
/// That is exactly what went wrong once already: UiaSurface read expectName and
/// expectAutomationId, the comment explained why they mattered, and no tool method exposed
/// either, so the check could never fire. A test that reads the real signatures is the only
/// thing that catches the guard and its entry point drifting apart.
/// </summary>
public class HandleExpectationTests
{
    private static IEnumerable<MethodInfo> Tools() =>
        typeof(WindowsTools)
            .GetMethods(BindingFlags.Public | BindingFlags.Instance)
            .Where(m => m.GetCustomAttribute<McpServerToolAttribute>() is not null);

    private static bool Takes(MethodInfo method, string parameter) =>
        method.GetParameters().Any(p => p.Name == parameter);

    public static TheoryData<string> ToolsTakingHwnd()
    {
        var data = new TheoryData<string>();
        foreach (var tool in Tools().Where(m => Takes(m, "hwnd"))) { data.Add(tool.Name); }
        return data;
    }

    public static TheoryData<string> ToolsTakingPid()
    {
        var data = new TheoryData<string>();
        foreach (var tool in Tools().Where(m => Takes(m, "pid"))) { data.Add(tool.Name); }
        return data;
    }

    [Theory]
    [MemberData(nameof(ToolsTakingHwnd))]
    public void EveryToolTakingAnHwnd_AlsoOffersExpectTitle(string toolName)
    {
        var method = Tools().Single(m => m.Name == toolName);
        Assert.True(
            Takes(method, "expectTitle"),
            $"{toolName} takes an hwnd but offers no expectTitle, so a recycled handle cannot "
            + "be caught. Add the parameter and pass it through to the capability.");
    }

    [Theory]
    [MemberData(nameof(ToolsTakingPid))]
    public void EveryToolTakingAPid_AlsoOffersExpectName(string toolName)
    {
        var method = Tools().Single(m => m.Name == toolName);
        Assert.True(
            Takes(method, "expectName"),
            $"{toolName} takes a pid but offers no expectName, so a recycled pid cannot be "
            + "caught. Add the parameter and pass it through to the capability.");
    }

    [Theory]
    [InlineData("UiInvoke")]
    [InlineData("UiSetText")]
    public void EveryToolTakingARef_OffersBothElementExpectations(string toolName)
    {
        var method = Tools().Single(m => m.Name == toolName);

        Assert.True(Takes(method, "ref"), $"{toolName} was expected to address an element by ref.");
        Assert.True(Takes(method, "expectName"), $"{toolName} offers no expectName.");
        Assert.True(Takes(method, "expectAutomationId"), $"{toolName} offers no expectAutomationId.");
    }

    [Fact]
    public void TheExpectationParametersAreOptional()
    {
        // Mandatory would break read-only discovery, which has nothing to expect yet. The
        // point is that they exist and are documented, not that every call supplies them.
        foreach (var tool in Tools())
        {
            foreach (var parameter in tool.GetParameters()
                         .Where(p => p.Name?.StartsWith("expect", StringComparison.Ordinal) == true))
            {
                Assert.True(
                    parameter.IsOptional,
                    $"{tool.Name}.{parameter.Name} must be optional.");
            }
        }
    }

    [Fact]
    public void EveryMutatingToolTakesACommitFlagAndAReason()
    {
        // The plan-then-commit handshake and the audit trail both surface to the model
        // through these two parameters. A mutating tool missing either is a capability the
        // broker gates and the model cannot drive or explain.
        foreach (var tool in Tools().Where(m => Takes(m, "commit")))
        {
            Assert.True(
                Takes(tool, "reason"),
                $"{tool.Name} accepts commit but no reason, so its audit entry cannot say why.");
        }
    }
}
