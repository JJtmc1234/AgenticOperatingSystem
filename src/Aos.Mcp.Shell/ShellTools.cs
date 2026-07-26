using System.ComponentModel;
using Aos.Broker;
using Aos.Core;
using Aos.Mcp.Shared;
using ModelContextProtocol.Server;

namespace Aos.Mcp.Shell;

[McpServerToolType]
public sealed class ShellTools(CapabilityBroker broker)
{
    [McpServerTool(Name = "shell_commands")]
    [Description("List which executables may be run and in which folders. Call this before "
        + "guessing at a command.")]
    public Task<string> Commands(CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-shell/commands.list", new(), cancellationToken: cancellationToken);

    [McpServerTool(Name = "shell_run")]
    [Description("Run an allowlisted executable with an argument list. There is no shell, so "
        + "pipes, redirection, && and quoting tricks do not work. Pass each argument as a "
        + "separate list item. Returns a plan unless commit is true.")]
    public Task<string> Run(
        [Description("Bare executable name, for example git or dotnet. Not a path.")]
        string command,
        [Description("Arguments as separate items, for example [\"status\",\"--short\"].")]
        string[]? arguments = null,
        [Description("Folder to run in. Must be inside an allowed root. Defaults to the first one.")]
        string? workingDirectory = null,
        [Description("Kill the command after this many seconds. Default 60.")]
        int timeoutSeconds = 60,
        [Description("Set true to actually run it.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-shell/command.run",
            JsonArgs.Of(
                ("command", command),
                ("arguments", JsonArgs.ArrayOf(arguments)),
                ("workingDirectory", workingDirectory),
                ("timeoutSeconds", timeoutSeconds)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "shell_capabilities")]
    [Description("List registered shell capabilities with risk tiers.")]
    public string Capabilities() => broker.DescribeCapabilities();
}
