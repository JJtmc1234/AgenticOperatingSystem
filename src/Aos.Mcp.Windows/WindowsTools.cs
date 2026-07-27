using System.ComponentModel;
using Aos.Broker;
using Aos.Core;
using Aos.Mcp.Shared;
using ModelContextProtocol.Server;

namespace Aos.Mcp.Windows;

/// <summary>
/// The MCP tool surface. One tool per capability so the client gets a real schema for each,
/// rather than a single opaque "invoke" tool.
///
/// Mutating tools take a <c>commit</c> flag. Called without it they return a plan and change
/// nothing; that is the broker's plan-then-commit handshake surfacing to the model.
/// </summary>
[McpServerToolType]
public sealed class WindowsTools(CapabilityBroker broker)
{
    // Handles and refs are both positional, and plan and commit are separate calls that each
    // re-resolve from scratch. These optional expectations are what stop a recycled handle or
    // a shifted index from quietly redirecting an approved action at something else.
    private const string ExpectTitleHelp =
        "The window title you expect this handle to have, from window_list. Window handles are "
        + "recycled, so without this a window closing between the plan and the commit can hand "
        + "its handle to a different window.";

    private const string ExpectNameHelp =
        "The element name you expect at this ref, from ui_tree. Refs are positional, so a toast "
        + "or a loaded row appearing in between shifts them onto a different control.";

    private const string ExpectIdHelp =
        "The automationId you expect at this ref, from ui_tree. More stable than the name where "
        + "the app provides one.";

    [McpServerTool(Name = "window_list")]
    [Description("List top-level visible windows with their handle (hwnd), title, owning "
        + "process and screen bounds. Start here to find a window to act on.")]
    public Task<string> WindowList(
        [Description("Only return windows whose title contains this text (case-insensitive).")]
        string? titleContains = null,
        [Description("Include windows with an empty title. Usually noise.")]
        bool includeUntitled = false,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-windows/window.list",
            JsonArgs.Of(("titleContains", titleContains), ("includeUntitled", includeUntitled)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "process_list")]
    [Description("List running processes with pid, name and memory use, heaviest first.")]
    public Task<string> ProcessList(
        [Description("Only return processes whose name contains this text.")]
        string? nameContains = null,
        [Description("Maximum rows to return. Default 50.")]
        int top = 50,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-windows/process.list",
            JsonArgs.Of(("nameContains", nameContains), ("top", top)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "ui_tree")]
    [Description("Read the UIAutomation control tree of a window. Each element comes back "
        + "with a 'ref' path and its supported 'actions'. Pass that ref to ui_invoke or "
        + "ui_set_text. Prefer this over screenshots: it is faster and exact.")]
    public Task<string> UiTree(
        [Description("Window handle from window_list.")] long hwnd,
        [Description("How deep to walk. Default 6.")] int maxDepth = 6,
        [Description("Cap on elements returned. Default 300.")] int maxNodes = 300,
        [Description("Include elements currently scrolled or hidden off screen.")]
        bool includeOffscreen = false,
        [Description("Wall-clock budget for the walk in milliseconds. Default 15000. Apps that "
            + "answer UIAutomation slowly return a partial tree rather than hanging.")]
        int timeoutMs = 15_000,
        [Description(ExpectTitleHelp)] string? expectTitle = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-windows/ui.tree",
            JsonArgs.Of(("hwnd", hwnd), ("maxDepth", maxDepth), ("maxNodes", maxNodes),
                    ("includeOffscreen", includeOffscreen), ("timeoutMs", timeoutMs),
                    ("expectTitle", expectTitle)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "screen_capture")]
    [Description("Capture the whole virtual screen, or one window, to a PNG file and return "
        + "its path. Use ui_tree first for reading UI; this is for visual confirmation.")]
    public Task<string> ScreenCapture(
        [Description("Window handle to capture. Omit to capture all monitors.")]
        long? hwnd = null,
        [Description(ExpectTitleHelp)] string? expectTitle = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-windows/screen.capture",
            JsonArgs.Of(("hwnd", hwnd), ("expectTitle", expectTitle)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "window_focus")]
    [Description("Restore and bring a window to the foreground. Returns a plan unless "
        + "commit is true.")]
    public Task<string> WindowFocus(
        [Description("Window handle from window_list.")] long hwnd,
        [Description(ExpectTitleHelp)] string? expectTitle = null,
        [Description("Set true to actually focus the window.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-windows/window.focus",
            JsonArgs.Of(("hwnd", hwnd), ("expectTitle", expectTitle)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "ui_invoke")]
    [Description("Click, toggle, select or expand a control by its ref path from ui_tree. "
        + "Returns a plan unless commit is true.")]
    public Task<string> UiInvoke(
        [Description("Window handle from window_list.")] long hwnd,
        [Description("Element ref path from ui_tree, e.g. \"0.3.1\".")] string @ref,
        [Description(ExpectNameHelp)] string? expectName = null,
        [Description(ExpectIdHelp)] string? expectAutomationId = null,
        [Description(ExpectTitleHelp)] string? expectTitle = null,
        [Description("Set true to actually perform the action.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-windows/ui.invoke",
            JsonArgs.Of(("hwnd", hwnd), ("ref", @ref), ("expectName", expectName),
                    ("expectAutomationId", expectAutomationId), ("expectTitle", expectTitle)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "ui_set_text")]
    [Description("Replace the text of an editable control by its ref path from ui_tree. "
        + "Returns a plan unless commit is true.")]
    public Task<string> UiSetText(
        [Description("Window handle from window_list.")] long hwnd,
        [Description("Element ref path from ui_tree, e.g. \"0.3.1\".")] string @ref,
        [Description("Text to set. Replaces the existing value entirely.")] string text,
        [Description(ExpectNameHelp)] string? expectName = null,
        [Description(ExpectIdHelp)] string? expectAutomationId = null,
        [Description(ExpectTitleHelp)] string? expectTitle = null,
        [Description("Set true to actually set the text.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-windows/ui.setText",
            JsonArgs.Of(("hwnd", hwnd), ("ref", @ref), ("text", text), ("expectName", expectName),
                    ("expectAutomationId", expectAutomationId), ("expectTitle", expectTitle)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "process_stop")]
    [Description("Terminate a process by pid. Unsaved work in that process is lost. "
        + "Returns a plan unless commit is true.")]
    public Task<string> ProcessStop(
        [Description("Process id from process_list.")] int pid,
        [Description("The process name you expect this pid to be, from process_list. Process "
            + "ids are recycled, so without this a pid that exited between the plan and the "
            + "commit can point the kill at something else entirely.")]
        string? expectName = null,
        [Description("Also kill child processes.")] bool entireProcessTree = false,
        [Description("Set true to actually terminate the process.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-windows/process.stop",
            JsonArgs.Of(("pid", pid), ("expectName", expectName),
                    ("entireProcessTree", entireProcessTree)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "aos_capabilities")]
    [Description("List every registered capability with its risk tier and whether it needs "
        + "a commit handshake or a restore point. Useful for understanding what is gated.")]
    public string Capabilities() => broker.DescribeCapabilities();
}
