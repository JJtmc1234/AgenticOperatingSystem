using System.Runtime.InteropServices;
using System.Text;

namespace Aos.Mcp.Windows.Native;

[StructLayout(LayoutKind.Sequential)]
internal struct Rect
{
    public int Left, Top, Right, Bottom;

    public int Width => Right - Left;
    public int Height => Bottom - Top;
}

[StructLayout(LayoutKind.Sequential)]
internal struct WindowPlacement
{
    public int Length;
    public int Flags;
    public int ShowCmd;
    public int MinPositionX, MinPositionY;
    public int MaxPositionX, MaxPositionY;
    public Rect NormalPosition;
}

internal static class User32
{
    internal delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    internal static extern bool GetWindowPlacement(IntPtr hWnd, ref WindowPlacement placement);

    [DllImport("user32.dll")]
    internal static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    internal static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    internal static extern bool IsIconic(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    internal static extern int GetWindowTextLength(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    internal static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    internal static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    internal static extern bool GetWindowRect(IntPtr hWnd, out Rect rect);

    [DllImport("user32.dll")]
    internal static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    internal static extern bool ShowWindow(IntPtr hWnd, int command);

    [DllImport("user32.dll")]
    internal static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    internal static extern bool IsWindow(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool SetProcessDpiAwarenessContext(IntPtr value);

    internal const int SwRestore = 9;

    private static readonly IntPtr PerMonitorAwareV2 = new(-4);

    /// <summary>
    /// Opts this process into per-monitor DPI awareness. Must run before any window, GDI
    /// surface, or UIAutomation call, because the first one latches the awareness mode.
    ///
    /// A DPI-unaware process gets lied to by Windows: on a scaled display GetWindowRect and
    /// VirtualScreen come back in virtualized logical pixels, while UIAutomation's
    /// BoundingRectangle is always physical. So window.list and ui.tree reported different
    /// rectangles for the same control, and a screenshot captured at 100% while the desktop
    /// ran at 150% meant every coordinate a vision step derived from that image pointed
    /// somewhere other than the thing in it. Declaring awareness makes all four agree, and
    /// the capture comes back at full resolution instead of upscaled and soft.
    /// </summary>
    internal static void EnableDpiAwareness()
    {
        // Per-monitor v2 needs Windows 10 1703 or newer. Older builds throw EntryPointNotFound
        // rather than returning false, and DPI-unaware is a degraded result, not a broken one.
        try { SetProcessDpiAwarenessContext(PerMonitorAwareV2); }
        catch (EntryPointNotFoundException) { }
        catch (DllNotFoundException) { }
    }

    /// <summary>
    /// Bounds a caller can act on. A minimized window reports a stub rectangle far off
    /// screen (around -32000), so returning GetWindowRect for one is misleading. In that
    /// case the restored position from GetWindowPlacement is what the user actually means.
    /// </summary>
    internal static (Rect Bounds, bool Restored) GetUsableBounds(IntPtr hWnd)
    {
        if (IsIconic(hWnd))
        {
            var placement = new WindowPlacement();
            placement.Length = Marshal.SizeOf<WindowPlacement>();
            if (GetWindowPlacement(hWnd, ref placement))
            {
                return (placement.NormalPosition, true);
            }
        }

        GetWindowRect(hWnd, out var rect);
        return (rect, false);
    }

    internal static string GetTitle(IntPtr hWnd)
    {
        var length = GetWindowTextLength(hWnd);
        if (length <= 0) { return string.Empty; }

        var buffer = new StringBuilder(length + 1);
        GetWindowText(hWnd, buffer, buffer.Capacity);
        return buffer.ToString();
    }
}
