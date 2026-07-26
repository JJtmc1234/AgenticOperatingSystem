using System.Text.Json.Nodes;
using Aos.Broker;
using Aos.Core;
using Aos.Mcp.Shared;

namespace Aos.Mcp.Shell;

internal sealed class ShellSurface(PathGuard guard, CommandRunner runner)
{
    private static readonly CapabilitySet Set = new("aos-shell");

    private const int DefaultTimeoutSeconds = 60;

    public IEnumerable<ICapability> All()
    {
        yield return Set.Read(
            "aos-shell/commands.list",
            "List the executables that may be run and the folders they may run in.",
            _ => new JsonObject
            {
                ["allowedCommands"] = new JsonArray(
                    [.. runner.AllowedCommands
                        .Order(StringComparer.OrdinalIgnoreCase)
                        .Select(c => JsonValue.Create(c))]),
                ["allowedWorkingDirectories"] = new JsonArray(
                    [.. guard.AllowedRoots.Select(r => JsonValue.Create(r))]),
                ["note"] = "Arguments are passed as a list and never through a shell, so "
                    + "pipes, redirection and command chaining are not available.",
            });

        yield return Set.Mutating(
            "aos-shell/command.run",
            RiskTier.System,
            "Run an allowlisted executable with an argument list in a folder inside an "
            + "allowed root. No shell is involved, so pipes and chaining do not work.",
            Plan,
            Execute,
            // A shadow copy is the wrong tool here. What a command does is bounded by the
            // allowlist and its working directory, and the plan step shows the exact
            // command line before anything runs. Requiring VSS would block the capability
            // entirely on an unelevated host for no real gain.
            snapshot: false);
    }

    private (string Command, List<string> Arguments, string WorkingDirectory, int Timeout)
        Parse(JsonObject args)
    {
        var command = args.RequireString("command");

        var arguments = (args["arguments"] as JsonArray)?
            .Where(n => n is not null)
            .Select(n => n!.GetValue<string>())
            .ToList() ?? [];

        var workingDirectory = args.GetString("workingDirectory");
        if (string.IsNullOrWhiteSpace(workingDirectory))
        {
            workingDirectory = guard.AllowedRoots.FirstOrDefault(Directory.Exists)
                ?? throw new ArgumentException(
                    "No workingDirectory given and no allowed root exists to default to.");
        }

        guard.EnsureAllowed(workingDirectory);

        if (!Directory.Exists(workingDirectory))
        {
            throw new DirectoryNotFoundException($"No such folder: '{workingDirectory}'.");
        }

        var timeout = Math.Clamp(args.GetInt32("timeoutSeconds", DefaultTimeoutSeconds), 1, 600);

        return (command, arguments, workingDirectory, timeout);
    }

    private string Plan(JsonObject args)
    {
        var (command, arguments, workingDirectory, timeout) = Parse(args);

        string resolved;
        try
        {
            resolved = runner.Resolve(command);
        }
        catch (Exception ex)
        {
            return $"Would be refused: {ex.Message}";
        }

        var line = arguments.Count == 0
            ? command
            : $"{command} {string.Join(' ', arguments.Select(a => a.Contains(' ') ? $"\"{a}\"" : a))}";

        return $"Run `{line}` in '{workingDirectory}', timeout {timeout}s. Resolved to '{resolved}'.";
    }

    private JsonNode? Execute(JsonObject args)
    {
        var (command, arguments, workingDirectory, timeout) = Parse(args);
        var result = runner.Run(command, arguments, workingDirectory, timeout);

        return new JsonObject
        {
            ["command"] = command,
            ["executable"] = result.Executable,
            ["workingDirectory"] = result.WorkingDirectory,
            ["exitCode"] = result.ExitCode,
            ["timedOut"] = result.TimedOut,
            ["durationMs"] = result.DurationMs,
            ["stdout"] = result.StandardOutput,
            ["stderr"] = result.StandardError,
            ["note"] = result.TimedOut
                ? $"Killed after {timeout}s. Output is whatever arrived before that."
                : null,
        };
    }
}
