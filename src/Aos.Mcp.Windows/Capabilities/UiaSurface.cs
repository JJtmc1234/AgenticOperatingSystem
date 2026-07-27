using System.Diagnostics;
using System.Text.Json.Nodes;
using System.Windows.Automation;
using Aos.Core;
using Aos.Mcp.Shared;
using Aos.Mcp.Windows.Native;

namespace Aos.Mcp.Windows.Capabilities;

/// <summary>
/// UIAutomation control-tree reading and interaction.
///
/// Elements are addressed by a "ref" -- a dot-separated path of child indices from the
/// window root, e.g. "0.3.1". Paths are deterministic for a given tree shape, which makes
/// them far more reliable to hand back to a model than matching on display names that
/// repeat, localize, or contain whitespace.
/// </summary>
internal static class UiaSurface
{
    private static readonly CapabilitySet Set = new("aos-windows");

    private const int DefaultMaxDepth = 6;
    private const int DefaultMaxNodes = 300;
    private const int DefaultBudgetMs = 15_000;

    public static IEnumerable<ICapability> All()
    {
        yield return Set.Read(
            "aos-windows/ui.tree",
            "Read the UIAutomation control tree of a window. Returns each element with a "
            + "'ref' path to pass to ui.invoke or ui.setText.",
            ReadTree);

        yield return Set.Mutating(
            "aos-windows/ui.invoke",
            RiskTier.Write,
            "Invoke, toggle or select a control identified by its ref path from ui.tree.",
            args => $"Invoke '{Describe(args)}' in window {args.RequireInt64("hwnd")}.",
            Invoke);

        yield return Set.Mutating(
            "aos-windows/ui.setText",
            RiskTier.Write,
            "Replace the text of an editable control identified by its ref path.",
            args => $"Set text of '{Describe(args)}' to \"{args.RequireString("text")}\".",
            SetText);
    }

    private static AutomationElement RootOf(JsonObject args)
    {
        var handle = WindowHandle.Require(args);

        return AutomationElement.FromHandle(handle)
            ?? throw new InvalidOperationException("Window exposes no UIAutomation root.");
    }

    private static JsonNode? ReadTree(JsonObject args)
    {
        var root = RootOf(args);
        var maxDepth = Math.Clamp(args.GetInt32("maxDepth", DefaultMaxDepth), 1, 20);
        var maxNodes = Math.Clamp(args.GetInt32("maxNodes", DefaultMaxNodes), 1, 5000);
        var budgetMs = Math.Clamp(args.GetInt32("timeoutMs", DefaultBudgetMs), 1_000, 60_000);
        var includeOffscreen = args.GetBool("includeOffscreen", false);

        var nodes = new JsonArray();
        // Every property read here is a cross-process RPC into the target app, so cost scales
        // with how cooperative that app is rather than with anything measurable up front. A
        // browser or game overlay ran past thirty seconds and returned nothing at all, which
        // to a client is indistinguishable from a hung server. A partial tree plus an honest
        // note is a usable answer; a hang is not.
        var deadline = Stopwatch.StartNew();
        var walk = new WalkLimits(maxDepth, maxNodes, includeOffscreen, deadline, budgetMs);

        var truncated = Walk(root, "0", 0, walk, nodes);
        var timedOut = deadline.ElapsedMilliseconds >= budgetMs;

        return new JsonObject
        {
            ["hwnd"] = args.RequireInt64("hwnd"),
            ["count"] = nodes.Count,
            ["truncated"] = truncated,
            ["elapsedMs"] = deadline.ElapsedMilliseconds,
            // Surfacing the cap explicitly: a silently trimmed tree reads as a complete one.
            ["note"] = Note(truncated, timedOut, maxNodes, maxDepth, budgetMs),
            ["elements"] = nodes,
        };
    }

    private static string? Note(bool truncated, bool timedOut, int maxNodes, int maxDepth, int budgetMs)
    {
        if (!truncated) { return null; }

        return timedOut
            ? $"Tree walk ran out of its {budgetMs} ms budget, so this is partial. This app answers "
              + "UIAutomation slowly. Narrow the walk with maxDepth, or target a child window, "
              + "rather than simply raising timeoutMs."
            : $"Tree was cut off at maxNodes={maxNodes} or maxDepth={maxDepth}. Raise the caps or "
              + "target a child window.";
    }

    private readonly record struct WalkLimits(
        int MaxDepth, int MaxNodes, bool IncludeOffscreen, Stopwatch Clock, int BudgetMs)
    {
        public bool OutOfTime => Clock.ElapsedMilliseconds >= BudgetMs;
    }

    /// <returns>True when the walk hit a limit and the tree is incomplete.</returns>
    private static bool Walk(
        AutomationElement element,
        string path,
        int depth,
        WalkLimits limits,
        JsonArray sink)
    {
        if (sink.Count >= limits.MaxNodes || limits.OutOfTime) { return true; }

        try
        {
            var info = element.Current;
            if (!limits.IncludeOffscreen && info.IsOffscreen) { return false; }

            sink.Add(Describe(element, info, path, depth));

            if (depth >= limits.MaxDepth)
            {
                // Only truncated if something was actually cut off. Reporting true for every
                // leaf that merely sits at maxDepth made any tree of exactly that depth
                // announce itself as incomplete, sending the model to re-query for nothing.
                return TreeWalker.ControlViewWalker.GetFirstChild(element) is not null;
            }

            var truncated = false;
            var walker = TreeWalker.ControlViewWalker;
            var child = walker.GetFirstChild(element);
            var index = 0;

            while (child is not null)
            {
                if (sink.Count >= limits.MaxNodes || limits.OutOfTime) { return true; }

                truncated |= Walk(child, $"{path}.{index}", depth + 1, limits, sink);

                child = walker.GetNextSibling(child);
                index++;
            }

            return truncated;
        }
        catch (ElementNotAvailableException)
        {
            // The UI changed mid-walk. Skipping the vanished subtree beats failing the call.
            return false;
        }
    }

    private static JsonObject Describe(
        AutomationElement element, AutomationElement.AutomationElementInformation info,
        string path, int depth)
    {
        var rect = info.BoundingRectangle;

        var actions = new JsonArray();
        if (Supports(element, InvokePattern.Pattern)) { actions.Add("invoke"); }
        if (Supports(element, TogglePattern.Pattern)) { actions.Add("toggle"); }
        if (Supports(element, SelectionItemPattern.Pattern)) { actions.Add("select"); }
        if (Supports(element, ValuePattern.Pattern)) { actions.Add("setText"); }
        if (Supports(element, ExpandCollapsePattern.Pattern)) { actions.Add("expand"); }

        var node = new JsonObject
        {
            ["ref"] = path,
            ["depth"] = depth,
            ["type"] = info.ControlType?.ProgrammaticName?.Replace("ControlType.", string.Empty),
            ["name"] = string.IsNullOrWhiteSpace(info.Name) ? null : info.Name,
            ["automationId"] = string.IsNullOrWhiteSpace(info.AutomationId) ? null : info.AutomationId,
            ["enabled"] = info.IsEnabled,
        };

        if (actions.Count > 0) { node["actions"] = actions; }

        if (!double.IsInfinity(rect.Width) && rect.Width > 0)
        {
            node["bounds"] = new JsonObject
            {
                ["x"] = (int)rect.X,
                ["y"] = (int)rect.Y,
                ["width"] = (int)rect.Width,
                ["height"] = (int)rect.Height,
            };
        }

        if (Supports(element, ValuePattern.Pattern))
        {
            try
            {
                node["value"] = element.GetCurrentPattern(ValuePattern.Pattern) is ValuePattern v
                    ? v.Current.Value
                    : null;
            }
            catch (Exception) { /* value is best-effort metadata */ }
        }

        return node;
    }

    private static bool Supports(AutomationElement element, AutomationPattern pattern)
    {
        try { return element.TryGetCurrentPattern(pattern, out _); }
        catch (ElementNotAvailableException) { return false; }
    }

    /// <summary>
    /// Resolves a ref, then refuses unless the element still matches what the caller expected.
    ///
    /// A ref is positional, and plan and commit are two separate broker calls that each
    /// re-resolve from scratch. Resolve only fails when a level has too FEW children, so a
    /// toast, a menu, or an async-loaded row appearing between the two calls shifts the
    /// indices and the same ref quietly lands on a different control. The plan would say
    /// Button 'Cancel' and the commit would press Button 'Delete'.
    ///
    /// Passing the expected name or automation id from ui.tree turns that silent misfire into
    /// a refusal. It is optional so a read-only caller need not supply it, but every mutating
    /// capability here does.
    /// </summary>
    private static AutomationElement ResolveExpected(JsonObject args)
    {
        var refPath = args.RequireString("ref");
        var element = Resolve(RootOf(args), refPath);

        var expectedName = args.GetString("expectName");
        var expectedId = args.GetString("expectAutomationId");
        if (expectedName is null && expectedId is null) { return element; }

        var info = element.Current;

        if (expectedId is not null && info.AutomationId != expectedId)
        {
            throw new InvalidOperationException(
                $"Ref '{refPath}' now resolves to automationId '{info.AutomationId}', not the "
                + $"expected '{expectedId}'. The UI changed since ui.tree was read. Re-read the "
                + "tree rather than acting on a stale ref.");
        }

        if (expectedName is not null && info.Name != expectedName)
        {
            throw new InvalidOperationException(
                $"Ref '{refPath}' now resolves to '{info.Name}', not the expected "
                + $"'{expectedName}'. The UI changed since ui.tree was read. Re-read the tree "
                + "rather than acting on a stale ref.");
        }

        return element;
    }

    /// <summary>Resolves a ref path like "0.3.1" against a window root.</summary>
    private static AutomationElement Resolve(AutomationElement root, string refPath)
    {
        var segments = refPath.Split('.', StringSplitOptions.RemoveEmptyEntries);
        if (segments.Length == 0 || segments[0] != "0")
        {
            throw new ArgumentException(
                $"Malformed ref '{refPath}'. Refs come from ui.tree and start at '0'.");
        }

        var current = root;
        var walker = TreeWalker.ControlViewWalker;

        for (var i = 1; i < segments.Length; i++)
        {
            if (!int.TryParse(segments[i], out var wanted))
            {
                throw new ArgumentException($"Malformed ref '{refPath}': '{segments[i]}' is not an index.");
            }

            var child = walker.GetFirstChild(current)
                ?? throw new InvalidOperationException(
                    $"Ref '{refPath}' is stale: element at depth {i - 1} has no children now.");

            for (var seen = 0; seen < wanted; seen++)
            {
                child = walker.GetNextSibling(child)
                    ?? throw new InvalidOperationException(
                        $"Ref '{refPath}' is stale: expected at least {wanted + 1} children at depth {i}. "
                        + "Re-read ui.tree; the UI has changed.");
            }

            current = child;
        }

        return current;
    }

    private static string Describe(JsonObject args)
    {
        // Used by the dry-run planner, so it must not mutate anything.
        try
        {
            var element = ResolveExpected(args);
            var info = element.Current;
            var name = string.IsNullOrWhiteSpace(info.Name) ? info.AutomationId : info.Name;
            var type = info.ControlType?.ProgrammaticName?.Replace("ControlType.", string.Empty);
            return $"{type} '{name}'";
        }
        catch (Exception ex)
        {
            return $"<unresolvable ref: {ex.Message}>";
        }
    }

    private static JsonNode? Invoke(JsonObject args)
    {
        var element = ResolveExpected(args);
        var info = element.Current;

        if (!info.IsEnabled)
        {
            throw new InvalidOperationException($"Control '{info.Name}' is disabled.");
        }

        string action;
        if (element.TryGetCurrentPattern(InvokePattern.Pattern, out var invoke))
        {
            ((InvokePattern)invoke).Invoke();
            action = "invoke";
        }
        else if (element.TryGetCurrentPattern(TogglePattern.Pattern, out var toggle))
        {
            ((TogglePattern)toggle).Toggle();
            action = "toggle";
        }
        else if (element.TryGetCurrentPattern(SelectionItemPattern.Pattern, out var select))
        {
            ((SelectionItemPattern)select).Select();
            action = "select";
        }
        else if (element.TryGetCurrentPattern(ExpandCollapsePattern.Pattern, out var expand))
        {
            var pattern = (ExpandCollapsePattern)expand;
            if (pattern.Current.ExpandCollapseState == ExpandCollapseState.Expanded)
            {
                pattern.Collapse();
                action = "collapse";
            }
            else
            {
                pattern.Expand();
                action = "expand";
            }
        }
        else
        {
            throw new InvalidOperationException(
                $"Control '{info.Name}' supports no invokable pattern. "
                + "Fall back to vision-driven clicking for this element.");
        }

        return new JsonObject
        {
            ["ref"] = args.RequireString("ref"),
            ["name"] = info.Name,
            ["action"] = action,
        };
    }

    private static JsonNode? SetText(JsonObject args)
    {
        var element = ResolveExpected(args);
        var text = args.RequireString("text");
        var info = element.Current;

        if (!element.TryGetCurrentPattern(ValuePattern.Pattern, out var raw))
        {
            throw new InvalidOperationException(
                $"Control '{info.Name}' does not expose ValuePattern, so its text cannot be set directly.");
        }

        var value = (ValuePattern)raw;
        if (value.Current.IsReadOnly)
        {
            throw new InvalidOperationException($"Control '{info.Name}' is read-only.");
        }

        var previous = value.Current.Value;
        value.SetValue(text);

        return new JsonObject
        {
            ["ref"] = args.RequireString("ref"),
            ["name"] = info.Name,
            ["previous"] = previous,
            ["current"] = text,
        };
    }
}
