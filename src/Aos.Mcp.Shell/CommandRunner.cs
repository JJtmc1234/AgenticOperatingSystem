using System.Diagnostics;
using System.Text;

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
/// This is the whole safety argument for the server. A gated "run this PowerShell string"
/// capability is close to impossible to make safe, because the shell itself provides
/// pipes, redirection, command chaining and expression evaluation, so any allowlist of
/// commands can be walked straight around. Passing an argument list to CreateProcess with
/// no shell means the arguments are inert data. There is nothing for "; rm -rf" to be
/// interpreted by.
/// </summary>
public sealed class CommandRunner(IReadOnlyCollection<string> allowedCommands)
{
    private const int MaxOutputChars = 20_000;

    public IReadOnlyCollection<string> AllowedCommands { get; } = allowedCommands;

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
                $"'{command}' is allowed by policy but was not found on PATH.");

        return resolved;
    }

    private static string? FindOnPath(string command)
    {
        var extensions = (Environment.GetEnvironmentVariable("PATHEXT") ?? ".EXE;.CMD;.BAT")
            .Split(';', StringSplitOptions.RemoveEmptyEntries);

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
