using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Text.Json.Nodes;
using Aos.Core;
using Aos.Mcp.Shared;
using Aos.Mcp.Windows.Native;

namespace Aos.Mcp.Windows.Capabilities;

/// <summary>Windows, processes, and screen capture. No UIAutomation dependency.</summary>
internal static class ShellSurface
{
    private static readonly CapabilitySet Set = new("aos-windows");

    public static IEnumerable<ICapability> All(string screenshotDirectory)
    {
        yield return Set.Read(
            "aos-windows/window.list",
            "List top-level visible windows with handle, title, owning process and bounds.",
            ListWindows);

        yield return Set.Read(
            "aos-windows/process.list",
            "List running processes with pid, name, working set and start time.",
            ListProcesses);

        yield return Set.Read(
            "aos-windows/screen.capture",
            "Capture the virtual screen (or one window) to a PNG and return its path.",
            args => Capture(args, screenshotDirectory));

        yield return Set.Mutating(
            "aos-windows/window.focus",
            RiskTier.Write,
            "Restore and bring a window to the foreground.",
            args => $"Focus window {args.RequireInt64("hwnd")} ('{TitleOf(args.RequireInt64("hwnd"))}').",
            FocusWindow);

        yield return Set.Mutating(
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
            var (rect, restored) = User32.GetUsableBounds(hWnd);

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
                // Says so explicitly, because otherwise a minimized window's restored size
                // is indistinguishable from its current on-screen size.
                ["boundsAreRestoredPosition"] = restored ? JsonValue.Create(true) : null,
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
        // Disposed explicitly. This runs once per window inside the EnumWindows callback,
        // and window.list is auto-allowed, so an agent polling it leaked a native handle
        // per window per call in a long-lived server.
        try
        {
            using var process = Process.GetProcessById((int)pid);
            return process.ProcessName;
        }
        catch (ArgumentException) { return "<exited>"; }
        catch (InvalidOperationException) { return "<exited>"; }
    }

    private static JsonNode? ListProcesses(JsonObject args)
    {
        var nameFilter = args.GetString("nameContains");
        // Clamped like every sibling capability. Unclamped, a negative top produced
        // Take(-1), an empty list, and a count of zero, which reads as "no processes are
        // running" rather than as a bad argument.
        var top = Math.Clamp(args.GetInt32("top", 50), 1, 500);

        var rows = new List<JsonObject>();

        // Process.GetProcesses hands back live objects holding native handles. Each one is
        // disposed, including the ones filtered out, since reading Responding opens further
        // handles of its own.
        foreach (var process in Process.GetProcesses())
        {
            using (process)
            {
                try
                {
                    if (nameFilter is not null &&
                        !process.ProcessName.Contains(nameFilter, StringComparison.OrdinalIgnoreCase))
                    {
                        continue;
                    }

                    rows.Add(new JsonObject
                    {
                        ["pid"] = process.Id,
                        ["name"] = process.ProcessName,
                        ["workingSetMb"] = Math.Round(process.WorkingSet64 / 1024d / 1024d, 1),
                        ["responding"] = process.Responding,
                    });
                }
                catch (Exception)
                {
                    // Exited between enumeration and the property reads.
                }
            }
        }

        var array = new JsonArray();
        foreach (var row in rows
                     .OrderByDescending(o => o["workingSetMb"]!.GetValue<double>())
                     .Take(top))
        {
            array.Add(row);
        }

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

            // Refuse rather than capture. A minimized window's rectangle sits around
            // -32000 with a plausible width and height, so the size guard below never
            // fired: CopyFromScreen read off-screen coordinates and returned a black PNG
            // as a success. An agent would then feed that into a vision step and reason
            // over nothing at all.
            if (User32.IsIconic(handle))
            {
                throw new InvalidOperationException(
                    $"Window {handle.ToInt64()} is minimized, so there is nothing on screen to "
                    + "capture. Call window.focus first, or capture the whole screen.");
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
                $"Capture area has no size ({area.Width}x{area.Height}).");
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
