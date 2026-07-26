using System.Diagnostics;
using System.Text;

namespace Aos.Hud;

/// <summary>
/// A single long-lived agent process, driven over stdio.
///
/// This is the point of the whole tray app. Launching the agent per request paid Node
/// startup, the SDK subprocess spawn, and three MCP server connections every time. Holding
/// one session open pays that once and makes every later request immediate.
///
/// The agent is the existing terminal REPL, unmodified: it reads request lines on stdin and
/// writes output on stdout, which is exactly what a pipe provides. No separate protocol was
/// needed, and the same binary still works standalone in a terminal.
/// </summary>
internal sealed class AgentSession : IDisposable
{
    private readonly string _agentScript;
    private readonly string _workingDirectory;
    private Process? _process;

    /// <summary>Raised for each line the agent writes. Always marshalled to the UI thread by the form.</summary>
    public event Action<string>? OutputReceived;

    /// <summary>Raised when the agent exits, expectedly or not.</summary>
    public event Action<int>? Exited;

    public AgentSession(string agentScript, string workingDirectory)
    {
        _agentScript = agentScript;
        _workingDirectory = workingDirectory;
    }

    public bool IsRunning => _process is { HasExited: false };

    public void Start()
    {
        if (IsRunning) { return; }

        if (!File.Exists(_agentScript))
        {
            throw new FileNotFoundException(
                $"Agent build not found at '{_agentScript}'. Run provisioning/Install-Aos.ps1.");
        }

        var info = new ProcessStartInfo
        {
            FileName = "node",
            WorkingDirectory = _workingDirectory,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            // The agent prints tool names and prompts; keep them readable regardless of the
            // console code page this process inherited.
            StandardOutputEncoding = Encoding.UTF8,
            StandardErrorEncoding = Encoding.UTF8,
        };
        info.ArgumentList.Add(_agentScript);

        var process = new Process { StartInfo = info, EnableRaisingEvents = true };

        process.OutputDataReceived += (_, e) => { if (e.Data is not null) { OutputReceived?.Invoke(e.Data); } };
        // Agent and SDK diagnostics go to stderr. Surfacing them is what makes a failed MCP
        // connection visible instead of looking like an unresponsive agent.
        process.ErrorDataReceived += (_, e) =>
        {
            if (!string.IsNullOrWhiteSpace(e.Data)) { OutputReceived?.Invoke($"[stderr] {e.Data}"); }
        };
        process.Exited += (_, _) => Exited?.Invoke(SafeExitCode(process));

        process.Start();
        process.BeginOutputReadLine();
        process.BeginErrorReadLine();

        _process = process;
    }

    private static int SafeExitCode(Process process)
    {
        try { return process.ExitCode; }
        catch (InvalidOperationException) { return -1; }
    }

    public void Send(string request)
    {
        if (!IsRunning)
        {
            throw new InvalidOperationException("The agent is not running.");
        }

        _process!.StandardInput.WriteLine(request);
        _process.StandardInput.Flush();
    }

    /// <summary>Asks the agent to exit, then kills it if it does not. Safe to call repeatedly.</summary>
    public void Stop()
    {
        var process = _process;
        _process = null;
        if (process is null) { return; }

        try
        {
            if (!process.HasExited)
            {
                // The REPL treats /exit as a clean shutdown, which lets the SDK close its
                // MCP servers rather than orphaning them.
                try { process.StandardInput.WriteLine("/exit"); process.StandardInput.Flush(); }
                catch (IOException) { /* stdin already gone */ }

                if (!process.WaitForExit(4000))
                {
                    process.Kill(entireProcessTree: true);
                    process.WaitForExit(2000);
                }
            }
        }
        catch (InvalidOperationException)
        {
            // Already gone between the check and the call.
        }
        finally
        {
            process.Dispose();
        }
    }

    public void Dispose() => Stop();
}
