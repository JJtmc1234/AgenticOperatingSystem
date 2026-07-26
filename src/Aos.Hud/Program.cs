using System.Diagnostics;

namespace Aos.Hud;

internal static class Program
{
    private const string InstanceName = @"Local\AgenticOS.Hud.SingleInstance";

    [STAThread]
    private static void Main()
    {
        // One resident session is the entire point. A second instance would register a
        // hotkey that silently fails, hold a second agent, and double the cost.
        using var single = new Mutex(initiallyOwned: true, InstanceName, out var isFirst);
        if (!isFirst)
        {
            MessageBox.Show(
                "AgenticOS is already running. Look for it in the system tray, or press Ctrl+Alt+Space.",
                "AgenticOS", MessageBoxButtons.OK, MessageBoxIcon.Information);
            return;
        }

        ApplicationConfiguration.Initialize();

        var (agentScript, repoRoot) = Locate();
        if (agentScript is null)
        {
            MessageBox.Show(
                "Could not find the built agent (src\\orchestrator\\dist\\agent.js).\n\n"
                + "Run provisioning\\Install-Aos.ps1 first, then start AgenticOS again.",
                "AgenticOS", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return;
        }

        Application.Run(new HudForm(agentScript, repoRoot!));
    }

    /// <summary>
    /// Finds the repo by walking up from the executable looking for aos.sln. The HUD may be
    /// launched from its build output, from a shortcut, or from a published folder, so a
    /// path relative to the current directory is not reliable.
    /// </summary>
    private static (string? AgentScript, string? RepoRoot) Locate()
    {
        var directory = Path.GetDirectoryName(Environment.ProcessPath
            ?? Process.GetCurrentProcess().MainModule?.FileName
            ?? AppContext.BaseDirectory);

        while (!string.IsNullOrEmpty(directory))
        {
            if (File.Exists(Path.Combine(directory, "aos.sln")))
            {
                var script = Path.Combine(directory, "src", "orchestrator", "dist", "agent.js");
                return File.Exists(script) ? (script, directory) : (null, directory);
            }

            directory = Path.GetDirectoryName(directory);
        }

        return (null, null);
    }
}
