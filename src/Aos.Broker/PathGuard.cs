namespace Aos.Broker;

/// <summary>
/// Enforces <c>denyPaths</c>. Capabilities that accept a path must run it through here
/// before touching the filesystem.
/// </summary>
public sealed class PathGuard
{
    private readonly string[] _deniedRoots;
    private readonly string[] _allowedRoots;

    /// <param name="deniedPaths">Paths that are off limits even inside an allowed root.</param>
    /// <param name="allowedRoots">
    /// If non-empty, a path must sit inside one of these to be touchable at all. This is the
    /// difference between "the agent cannot delete System32" and "the agent only ever works
    /// in the handful of folders I named", which is the boundary worth having.
    /// An empty list means the whole filesystem except the denied paths.
    /// </param>
    public PathGuard(IEnumerable<string> deniedPaths, IEnumerable<string>? allowedRoots = null)
    {
        _deniedRoots = deniedPaths
            .Select(Expand)
            .Where(p => p.Length > 0)
            .ToArray();

        _allowedRoots = (allowedRoots ?? [])
            .Select(Expand)
            .Where(p => p.Length > 0)
            .ToArray();
    }

    public IReadOnlyList<string> AllowedRoots => _allowedRoots;

    /// <summary>
    /// Canonicalizes a path for comparison, or returns empty when it cannot be trusted.
    /// Empty always means "treat as denied" at the call sites.
    /// </summary>
    private static string Expand(string path)
    {
        var expanded = Environment.ExpandEnvironmentVariables(path);
        if (string.IsNullOrWhiteSpace(expanded)) { return string.Empty; }

        // Extended-length and UNC prefixes are refused rather than normalized. Win32 accepts
        // \\?\C:\... in every file API, but Path.GetFullPath deliberately leaves such paths
        // untouched, so "\\?\C:\Windows\System32\..\System32" kept its .. segment and walked
        // straight past the comparison below. There is no legitimate need for either form
        // here, and refusing is the only safe handling of a path the normalizer will not
        // canonicalize.
        if (expanded.StartsWith(@"\\", StringComparison.Ordinal)
            || expanded.StartsWith("//", StringComparison.Ordinal))
        {
            return string.Empty;
        }

        try
        {
            // Canonicalize so "C:\Windows\System32\..\System32", trailing separators and
            // 8.3 short names cannot slip past a plain string comparison.
            return ResolveLinks(Path.GetFullPath(expanded));
        }
        catch (Exception e) when (e is ArgumentException or NotSupportedException or PathTooLongException)
        {
            // Alternate data stream syntax throws NotSupportedException, which used to escape
            // as an unhandled failure rather than a denial.
            return string.Empty;
        }
    }

    /// <summary>
    /// Follows reparse points to their final target.
    ///
    /// GetFullPath does not resolve junctions or symlinks, so an unprivileged
    /// "mklink /J" inside an allowed root produced a path that satisfied both the allowed
    /// root check and the deny check while actually pointing somewhere else entirely.
    /// The nearest existing ancestor is resolved too, because the leaf of a move or write
    /// target usually does not exist yet.
    /// </summary>
    private static string ResolveLinks(string full)
    {
        try
        {
            // Walk down to the deepest ancestor that exists, resolve that, then reattach the
            // segments that do not exist yet. A move or write target usually has a
            // non-existent leaf, and a junction one level above it must still be seen.
            var unresolved = new List<string>();
            var current = full;

            while (true)
            {
                // A drive root cannot be a reparse point, and asking it to resolve one throws.
                // Stop here rather than treating that exception as "path unverifiable", which
                // denied every path whose ancestors did not already exist.
                if (current.Equals(Path.GetPathRoot(current), StringComparison.OrdinalIgnoreCase))
                {
                    break;
                }

                if (Directory.Exists(current))
                {
                    var target = new DirectoryInfo(current).ResolveLinkTarget(returnFinalTarget: true);
                    if (target is not null) { current = Path.GetFullPath(target.FullName); }
                    break;
                }

                if (File.Exists(current))
                {
                    var target = new FileInfo(current).ResolveLinkTarget(returnFinalTarget: true);
                    if (target is not null) { current = Path.GetFullPath(target.FullName); }
                    break;
                }

                var parent = Path.GetDirectoryName(current);
                // Reached the root with nothing existing along the way: nothing to resolve.
                if (string.IsNullOrEmpty(parent)) { return TrimTrailing(full); }

                unresolved.Add(Path.GetFileName(current));
                current = parent;
            }

            if (unresolved.Count == 0) { return TrimTrailing(current); }

            unresolved.Reverse();
            return TrimTrailing(Path.Combine([current, .. unresolved]));
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or ArgumentException)
        {
            // Cannot inspect it, so cannot vouch for it.
            return string.Empty;
        }
    }

    /// <summary>
    /// Strips a trailing separator without mangling a drive root.
    ///
    /// TrimEndingDirectorySeparator turns "C:\" into "C:", which is a relative path. That
    /// broke path rebuilding here, and it also meant a denyPaths entry of a drive root denied
    /// nothing, because the comparison then ran against "C:\\".
    /// </summary>
    private static string TrimTrailing(string path)
    {
        var root = Path.GetPathRoot(path);
        return !string.IsNullOrEmpty(root) && path.Equals(root, StringComparison.OrdinalIgnoreCase)
            ? path
            : Path.TrimEndingDirectorySeparator(path);
    }

    /// <summary>True when <paramref name="candidate"/> is a denied root or sits under one.</summary>
    public bool IsDenied(string candidate)
    {
        // Blank is denied rather than allowed. It previously returned false, so
        // IsAllowed("   ") was true whenever no allowed roots were configured, and the
        // guard's verdict depended on the OS throwing later.
        if (string.IsNullOrWhiteSpace(candidate)) { return true; }

        var full = Expand(candidate);
        if (full.Length == 0) { return true; }

        return _deniedRoots.Any(root => IsSelfOrUnder(full, root));
    }

    /// <summary>True when no allowed roots are configured, or the path sits inside one.</summary>
    public bool IsInAllowedRoot(string candidate)
    {
        if (_allowedRoots.Length == 0) { return true; }

        var full = Expand(candidate);
        if (full.Length == 0) { return false; }

        return _allowedRoots.Any(root => IsSelfOrUnder(full, root));
    }

    public bool IsAllowed(string candidate) =>
        IsInAllowedRoot(candidate) && !IsDenied(candidate);

    private static bool IsSelfOrUnder(string full, string root)
    {
        if (full.Equals(root, StringComparison.OrdinalIgnoreCase)) { return true; }

        // Compare with a separator appended so "C:\ProgramData" is not treated as being
        // inside "C:\Program". A drive root already ends with one, and doubling it would make
        // the comparison never match.
        var prefix = root.EndsWith(Path.DirectorySeparatorChar)
            ? root
            : root + Path.DirectorySeparatorChar;

        return full.StartsWith(prefix, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>Throws when the path is off limits. Use at the top of a file capability.</summary>
    public void EnsureAllowed(string candidate)
    {
        if (!IsInAllowedRoot(candidate))
        {
            throw new UnauthorizedAccessException(
                $"'{candidate}' is outside every allowed root. Allowed: "
                + string.Join(", ", _allowedRoots));
        }

        if (IsDenied(candidate))
        {
            throw new UnauthorizedAccessException($"Policy denies access to '{candidate}'.");
        }
    }
}
