using System.Runtime.InteropServices;

namespace Aos.Hud.Native;

/// <summary>
/// Global hotkey registration. A real listener, unlike the Start Menu shortcut hotkey it
/// replaces: dispatch is immediate rather than going through Explorer, and a failed
/// registration is reported instead of silently doing nothing.
/// </summary>
internal static class HotKey
{
    internal const int WmHotKey = 0x0312;

    [Flags]
    internal enum Modifiers : uint
    {
        Alt = 0x0001,
        Control = 0x0002,
        Shift = 0x0004,
        /// <summary>Stops the hotkey from auto-repeating while the keys are held down.</summary>
        NoRepeat = 0x4000,
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint modifiers, uint virtualKey);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool UnregisterHotKey(IntPtr hWnd, int id);

    [DllImport("user32.dll")]
    internal static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    internal static extern bool ShowWindow(IntPtr hWnd, int command);

    internal const int SwRestore = 9;

    /// <summary>
    /// Registers a hotkey, returning false when the combination is already owned by
    /// another process. Windows gives no way to discover who holds it, so the caller can
    /// only report the collision and suggest a different key.
    /// </summary>
    internal static bool TryRegister(IntPtr window, int id, Modifiers modifiers, Keys key) =>
        RegisterHotKey(window, id, (uint)(modifiers | Modifiers.NoRepeat), (uint)key);

    internal static void Unregister(IntPtr window, int id) => UnregisterHotKey(window, id);
}
