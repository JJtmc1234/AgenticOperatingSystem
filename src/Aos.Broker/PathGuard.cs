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

    private static string Expand(string path)
    {
        var expanded = Environment.ExpandEnvironmentVariables(path);
        if (string.IsNullOrWhiteSpace(expanded)) { return string.Empty; }

        try
        {
            // Canonicalize so "C:\Windows\System32\..\System32" and trailing separators
            // cannot slip past a plain string comparison.
            return Path.TrimEndingDirectorySeparator(Path.GetFullPath(expanded));
        }
        catch (ArgumentException)
        {
            return string.Empty;
        }
    }

    /// <summary>True when <paramref name="candidate"/> is a denied root or sits under one.</summary>
    public bool IsDenied(string candidate)
    {
        if (string.IsNullOrWhiteSpace(candidate)) { return false; }

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
        // inside "C:\Program".
        return full.StartsWith(root + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase);
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
