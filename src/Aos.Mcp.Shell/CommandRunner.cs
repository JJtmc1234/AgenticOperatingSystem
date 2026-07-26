using System.Diagnostics;
using System.Text;
using System.Text.RegularExpressions;

namespace Aos.Mcp.Shell;

public sealed record CommandResult(
    string Executable,
    IReadOnlyList<string> Arguments,
    string WorkingDirectory,
    int ExitCode,
    string StandardOutput,
    string StandardError,
    bool TimedOut,
    long DurationMs);

/// <summary>
/// Launches an allowlisted executable directly, never through a shell.
///
/// A gated "run this PowerShell string" capability is close to impossible to make safe: the
/// shell supplies pipes, redirection, chaining and expression evaluation, so any command
/// allowlist can be walked straight around. Passing an argument list to CreateProcess with
/// UseShellExecute false avoids that.
///
/// Two limits on that guarantee, both learned the hard way and both now enforced rather than
/// assumed. This class previously claimed arguments were simply "inert data", which was
/// wrong on both counts:
///
/// 1. It only holds for real executables. CreateProcess rewrites a .bat or .cmd target into
///    "cmd.exe /c ...", and cmd then re-parses the argument text, so an argument of
///    "&amp;whoami" executes. Batch targets are therefore refused outright, and PATHEXT is
///    ignored in favour of a fixed .exe/.com list.
/// 2. It bounds which binary starts, not what that binary then does. Any interpreter takes
///    code on its own argument vector (node -e, python -c, git -c alias.x='!...'), so
///    listing one grants arbitrary code execution as the user. Interpreters are out of the
///    default allowlist, and per-command argument patterns block the remaining escape
///    hatches on the tools that stay.
/// </summary>
public sealed class CommandRunner
{
    private const int MaxOutputChars = 20_000;

    private readonly Dictionary<string, Regex[]> _deniedArguments;

    public CommandRunner(
        IReadOnlyCollection<string> allowedCommands,
        IReadOnlyDictionary<string, List<string>>? deniedArguments = null)
    {
        AllowedCommands = allowedCommands;

        _deniedArguments = (deniedArguments ?? new Dictionary<string, List<string>>())
            .ToDictionary(
                entry => entry.Key,
                entry => entry.Value
                    .Select(pattern => new Regex(pattern, RegexOptions.IgnoreCase | RegexOptions.CultureInvariant))
                    .ToArray(),
                StringComparer.OrdinalIgnoreCase);
    }

    public IReadOnlyCollection<string> AllowedCommands { get; }

    /// <summary>
    /// Rejects arguments that would turn an allowlisted tool into an arbitrary-code runner,
    /// such as <c>git -c alias.x='!sh -c ...'</c>. The allowlist bounds which binary starts;
    /// this bounds what it is asked to do.
    /// </summary>
    public void EnsureArgumentsAllowed(string command, IReadOnlyList<string> arguments)
    {
        if (!_deniedArguments.TryGetValue(command, out var patterns)) { return; }

        foreach (var argument in arguments)
        {
            var blocked = patterns.FirstOrDefault(p => p.IsMatch(argument));
            if (blocked is not null)
            {
                throw new ArgumentException(
                    $"Argument '{argument}' is refused for '{command}' by policy "
                    + $"(deniedArguments pattern '{blocked}'). It would let the command run "
                    + "code of its own choosing, which the allowlist cannot bound.");
            }
        }
    }

    /// <summary>Resolves a bare command name to a real executable, or explains why not.</summary>
    public string Resolve(string command)
    {
        if (command.Contains(Path.DirectorySeparatorChar) ||
            command.Contains(Path.AltDirectorySeparatorChar) ||
            Path.IsPathRooted(command))
        {
            // Only bare names, so the allowlist cannot be bypassed by pointing at a copy
            // of a banned binary sitting somewhere else on disk.
            throw new ArgumentException(
                $"Use a bare command name, not a path. Got '{command}'.");
        }

        if (!AllowedCommands.Contains(command, StringComparer.OrdinalIgnoreCase))
        {
            throw new ArgumentException(
                $"'{command}' is not in allowedCommands. Allowed: "
                + string.Join(", ", AllowedCommands.Order(StringComparer.OrdinalIgnoreCase)));
        }

        var resolved = FindOnPath(command)
            ?? throw new FileNotFoundException(
                $"'{command}' is allowed by policy but no .exe or .com for it was found on "
                + "PATH. Batch shims (.cmd, .bat) are refused on purpose: Windows runs them "
                + "through cmd.exe, which re-parses the arguments and would make injection "
                + "possible. Point the allowlist at a real executable instead.");

        // Belt and braces. Even if the search list were widened later, a batch target must
        // never reach CreateProcess through this path.
        var extension = Path.GetExtension(resolved);
        if (extension.Equals(".cmd", StringComparison.OrdinalIgnoreCase)
            || extension.Equals(".bat", StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                $"Refusing to run '{resolved}': batch files are interpreted by cmd.exe, which "
                + "would re-parse the argument list.");
        }

        return resolved;
    }

    /// <summary>
    /// Executable extensions we are willing to launch.
    ///
    /// Deliberately excludes .bat and .cmd, and does NOT honour PATHEXT. CreateProcess
    /// silently rewrites a batch target into "cmd.exe /c &lt;command line&gt;", so cmd re-parses
    /// the argument text and the whole no-shell guarantee collapses: an argument of
    /// "&amp;whoami" is appended unquoted by .NET's argument encoder, cmd sees the ampersand as
    /// a command separator, and it runs. Verified on this machine, and it is the
    /// CVE-2024-24576 class of bug, which .NET addressed with documentation rather than a
    /// behaviour change.
    ///
    /// The practical cost is that .cmd shims are unreachable, which on Windows means npm and
    /// npx. That is the correct trade: an allowlist that can be walked around is worse than a
    /// smaller one that holds.
    /// </summary>
    private static readonly string[] LaunchableExtensions = [".EXE", ".COM"];

    private static string? FindOnPath(string command)
    {
        var extensions = LaunchableExtensions;

        foreach (var directory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                 .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
        {
            foreach (var extension in extensions)
            {
                try
                {
                    var candidate = Path.Combine(directory.Trim(), command + extension);
                    if (File.Exists(candidate)) { return candidate; }
                }
                catch (ArgumentException)
                {
                    // Malformed PATH entry. Keep looking.
                }
            }
        }

        return null;
    }

    public CommandResult Run(
        string command,
        IReadOnlyList<string> arguments,
        string workingDirectory,
        int timeoutSeconds)
    {
        var executable = Resolve(command);
        EnsureArgumentsAllowed(command, arguments);

        var info = new ProcessStartInfo
        {
            FileName = executable,
            WorkingDirectory = workingDirectory,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            // Must be redirected. Otherwise the child inherits this process's stdin, which
            // is the MCP protocol pipe, and a command that reads stdin will consume the
            // JSON-RPC stream or block waiting on it. It is closed immediately after start
            // so children see EOF instead.
            RedirectStandardInput = true,
            // The critical flag. False means CreateProcess is used directly rather than
            // handing the string to cmd.exe for interpretation.
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        // ArgumentList escapes each entry, so spaces and quotes stay literal.
        foreach (var argument in arguments) { info.ArgumentList.Add(argument); }

        using var process = new Process { StartInfo = info };
        var stdout = new StringBuilder();
        var stderr = new StringBuilder();

        process.OutputDataReceived += (_, e) => Append(stdout, e.Data);
        process.ErrorDataReceived += (_, e) => Append(stderr, e.Data);

        var stopwatch = Stopwatch.StartNew();
        process.Start();

        // Give the child EOF on stdin rather than the MCP protocol pipe.
        try { process.StandardInput.Close(); }
        catch (Exception) { /* child may already have gone */ }

        process.BeginOutputReadLine();
        process.BeginErrorReadLine();

        var exited = process.WaitForExit(timeoutSeconds * 1000);
        if (!exited)
        {
            try { process.Kill(entireProcessTree: true); }
            catch (Exception) { /* already gone */ }
        }

        // Bounded drain. The parameterless WaitForExit waits for the output pipes to reach
        // EOF, which never happens if a surviving grandchild inherited the write handle, so
        // an unbounded call there can hang past the timeout the caller asked for. Slightly
        // truncated output beats a stuck server.
        process.WaitForExit(2000);

        stopwatch.Stop();

        return new CommandResult(
            Executable: executable,
            Arguments: arguments,
            WorkingDirectory: workingDirectory,
            ExitCode: exited ? process.ExitCode : -1,
            StandardOutput: stdout.ToString(),
            StandardError: stderr.ToString(),
            TimedOut: !exited,
            DurationMs: stopwatch.ElapsedMilliseconds);
    }

    private static void Append(StringBuilder sink, string? line)
    {
        if (line is null) { return; }
        if (sink.Length >= MaxOutputChars) { return; }

        sink.AppendLine(line);

        if (sink.Length >= MaxOutputChars)
        {
            sink.AppendLine($"...[output truncated at {MaxOutputChars} characters]");
        }
    }
}
