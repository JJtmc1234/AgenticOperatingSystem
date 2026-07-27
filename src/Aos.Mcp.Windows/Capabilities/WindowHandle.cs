using System.Text.Json.Nodes;
using Aos.Core;
using Aos.Mcp.Windows.Native;

namespace Aos.Mcp.Windows.Capabilities;

/// <summary>
/// Resolves an hwnd argument, and optionally refuses unless the window is still the one the
/// caller meant.
///
/// Window handles are recycled. Windows hands out a handle from a table, and when a window
/// closes its slot is free for the next one, which on a busy desktop can be seconds later.
/// IsWindow answers "is some window here", never "is it the same window", and plan and commit
/// are two separate broker calls that each re-resolve from scratch. So the plan could read
/// "Set text of Edit 'Search' in Notepad" and the commit could type into whatever inherited
/// that handle in between.
///
/// The optional expectTitle turns that silent misfire into a refusal. It is not mandatory,
/// because a read-only caller discovering the desktop has nothing to expect yet, but every
/// mutating capability passes it, and window.list already returns the title to pass.
/// </summary>
internal static class WindowHandle
{
    /// <summary>Validates the hwnd argument and any expectation attached to it.</summary>
    internal static IntPtr Require(JsonObject args)
    {
        var raw = args.RequireInt64("hwnd");
        var handle = new IntPtr(raw);

        if (!User32.IsWindow(handle))
        {
            throw new ArgumentException($"No window with handle {raw}. Re-read window.list.");
        }

        var expectedTitle = args.GetString("expectTitle");
        if (expectedTitle is null) { return handle; }

        var title = User32.GetTitle(handle);
        if (!string.Equals(title, expectedTitle, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                $"Handle {raw} is now the window titled '{title}', not the expected "
                + $"'{expectedTitle}'. Either the title changed or the handle was recycled onto "
                + "a different window. Re-read window.list rather than acting on a stale handle.");
        }

        return handle;
    }

    /// <summary>Describes a handle for a plan line, without throwing when it is already gone.</summary>
    internal static string Describe(JsonObject args)
    {
        var raw = args.RequireInt64("hwnd");
        var handle = new IntPtr(raw);
        if (!User32.IsWindow(handle)) { return "<no such window>"; }

        var title = User32.GetTitle(handle);
        return string.IsNullOrWhiteSpace(title) ? "<untitled window>" : title;
    }
}
