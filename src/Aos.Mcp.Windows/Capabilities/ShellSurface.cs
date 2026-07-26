using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Text.Json.Nodes;
using Aos.Core;
using Aos.Mcp.Windows.Native;

namespace Aos.Mcp.Windows.Capabilities;

/// <summary>Windows, processes, and screen capture. No UIAutomation dependency.</summary>
internal static class ShellSurface
{
    public static IEnumerable<ICapability> All(string screenshotDirectory)
    {
        yield return DelegateCapability.Read(
            "aos-windows/window.list",
            "List top-level visible windows with handle, title, owning process and bounds.",
            ListWindows);

        yield return DelegateCapability.Read(
            "aos-windows/process.list",
            "List running processes with pid, name, working set and start time.",
            ListProcesses);

        yield return DelegateCapability.Read(
            "aos-windows/screen.capture",
            "Capture the virtual screen (or one window) to a PNG and return its path.",
            args => Capture(args, screenshotDirectory));

        yield return DelegateCapability.Mutating(
            "aos-windows/window.focus",
            RiskTier.Write,
            "Restore and bring a window to the foreground.",
            args => $"Focus window {args.RequireInt64("hwnd")} ('{TitleOf(args.RequireInt64("hwnd"))}').",
            FocusWindow);

        yield return DelegateCapability.Mutating(
            "aos-windows/process.stop",
            RiskTier.System,
            "Terminate a process by pid. Unsaved work in that process is lost.",
            args => PlanStop(args),
            StopProcess,
            // A filesystem shadow copy cannot un-kill a process, so requiring one here would
            // block the capability outright for no safety gain. The commit handshake is the
            // real guard: System tier still needs an explicit second call.
            snapshot: false);
    }

    private static string TitleOf(long hwnd)
    {
        var handle = new IntPtr(hwnd);
        return User32.IsWindow(handle) ? User32.GetTitle(handle) : "<no such window>";
    }

    private static JsonNode? ListWindows(JsonObject args)
    {
        var titleFilter = args.GetString("titleContains");
        var includeUntitled = args.GetBool("includeUntitled", false);

        var windows = new JsonArray();

        User32.EnumWindows((hWnd, _) =>
        {
            if (!User32.IsWindowVisible(hWnd)) { return true; }

            var title = User32.GetTitle(hWnd);
            if (!includeUntitled && string.IsNullOrWhiteSpace(title)) { return true; }

            if (titleFilter is not null &&
                !title.Contains(titleFilter, StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }

            User32.GetWindowThreadProcessId(hWnd, out var pid);
            User32.GetWindowRect(hWnd, out var rect);

            windows.Add(new JsonObject
            {
                ["hwnd"] = hWnd.ToInt64(),
                ["title"] = title,
                ["pid"] = pid,
                ["process"] = ProcessName(pid),
                ["minimized"] = User32.IsIconic(hWnd),
                ["bounds"] = new JsonObject
                {
                    ["x"] = rect.Left,
                    ["y"] = rect.Top,
                    ["width"] = rect.Width,
                    ["height"] = rect.Height,
                },
            });

            return true;
        }, IntPtr.Zero);

        return new JsonObject
        {
            ["count"] = windows.Count,
            ["foreground"] = User32.GetForegroundWindow().ToInt64(),
            ["windows"] = windows,
        };
    }

    private static string ProcessName(uint pid)
    {
        try { return Process.GetProcessById((int)pid).ProcessName; }
        catch (ArgumentException) { return "<exited>"; }
        catch (InvalidOperationException) { return "<exited>"; }
    }

    private static JsonNode? ListProcesses(JsonObject args)
    {
        var nameFilter = args.GetString("nameContains");
        var top = args.GetInt32("top", 50);

        var rows = Process.GetProcesses()
            .Where(p => nameFilter is null ||
                        p.ProcessName.Contains(nameFilter, StringComparison.OrdinalIgnoreCase))
            .Select(p =>
            {
                // A process can exit between enumeration and property reads.
                try
                {
                    return new JsonObject
                    {
                        ["pid"] = p.Id,
                        ["name"] = p.ProcessName,
                        ["workingSetMb"] = Math.Round(p.WorkingSet64 / 1024d / 1024d, 1),
                        ["responding"] = p.Responding,
                    };
                }
                catch (Exception)
                {
                    return null;
                }
            })
            .Where(o => o is not null)
            .OrderByDescending(o => o!["workingSetMb"]!.GetValue<double>())
            .Take(top)
            .ToArray();

        var array = new JsonArray();
        foreach (var row in rows) { array.Add(row); }

        return new JsonObject { ["count"] = array.Count, ["processes"] = array };
    }

    private static JsonNode? Capture(JsonObject args, string directory)
    {
        Directory.CreateDirectory(directory);

        Rectangle area;
        var hwndArg = args["hwnd"];
        if (hwndArg is not null)
        {
            var handle = new IntPtr(args.RequireInt64("hwnd"));
            if (!User32.IsWindow(handle))
            {
                throw new ArgumentException($"No window with handle {handle.ToInt64()}.");
            }

            User32.GetWindowRect(handle, out var rect);
            area = new Rectangle(rect.Left, rect.Top, rect.Width, rect.Height);
        }
        else
        {
            area = System.Windows.Forms.SystemInformation.VirtualScreen;
        }

        if (area.Width <= 0 || area.Height <= 0)
        {
            throw new InvalidOperationException(
                "Capture area has zero size (the window may be minimized).");
        }

        var path = Path.Combine(
            directory, $"screen-{DateTimeOffset.UtcNow:yyyyMMdd-HHmmss-fff}.png");

        using var bitmap = new Bitmap(area.Width, area.Height, PixelFormat.Format32bppArgb);
        using (var graphics = Graphics.FromImage(bitmap))
        {
            graphics.CopyFromScreen(area.Location, Point.Empty, area.Size);
        }
        bitmap.Save(path, ImageFormat.Png);

        return new JsonObject
        {
            ["path"] = path,
            ["width"] = area.Width,
            ["height"] = area.Height,
        };
    }

    private static JsonNode? FocusWindow(JsonObject args)
    {
        var handle = new IntPtr(args.RequireInt64("hwnd"));
        if (!User32.IsWindow(handle))
        {
            throw new ArgumentException($"No window with handle {handle.ToInt64()}.");
        }

        if (User32.IsIconic(handle)) { User32.ShowWindow(handle, User32.SwRestore); }

        // SetForegroundWindow is advisory: Windows refuses it unless the calling process
        // already has foreground rights, so report what actually happened.
        User32.SetForegroundWindow(handle);
        var focused = User32.GetForegroundWindow() == handle;

        return new JsonObject
        {
            ["hwnd"] = handle.ToInt64(),
            ["title"] = User32.GetTitle(handle),
            ["focused"] = focused,
            ["note"] = focused
                ? null
                : "Windows declined the foreground change; the window was restored but not focused.",
        };
    }

    private static string PlanStop(JsonObject args)
    {
        var pid = args.RequireInt32("pid");
        try
        {
            var process = Process.GetProcessById(pid);
            return $"Terminate '{process.ProcessName}' (pid {pid}). Unsaved work is lost.";
        }
        catch (ArgumentException)
        {
            return $"No process with pid {pid} is running.";
        }
    }

    private static JsonNode? StopProcess(JsonObject args)
    {
        var pid = args.RequireInt32("pid");
        var process = Process.GetProcessById(pid);
        var name = process.ProcessName;

        process.Kill(entireProcessTree: args.GetBool("entireProcessTree", false));
        process.WaitForExit(5000);

        return new JsonObject
        {
            ["pid"] = pid,
            ["name"] = name,
            ["exited"] = process.HasExited,
        };
    }
}
