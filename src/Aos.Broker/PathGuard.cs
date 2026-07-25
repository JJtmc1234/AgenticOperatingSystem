namespace Aos.Broker;

/// <summary>
/// Enforces <c>denyPaths</c>. Capabilities that accept a path must run it through here
/// before touching the filesystem.
/// </summary>
public sealed class PathGuard
{
    private readonly string[] _deniedRoots;

    public PathGuard(IEnumerable<string> deniedPaths)
    {
        _deniedRoots = deniedPaths
            .Select(Expand)
            .Where(p => p.Length > 0)
            .ToArray();
    }

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

        foreach (var root in _deniedRoots)
        {
            if (full.Equals(root, StringComparison.OrdinalIgnoreCase)) { return true; }

            // Compare with a separator appended so "C:\ProgramData" is not treated as
            // being inside "C:\Program".
            if (full.StartsWith(root + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
        }

        return false;
    }

    /// <summary>Throws when the path is off limits. Use at the top of a file capability.</summary>
    public void EnsureAllowed(string candidate)
    {
        if (IsDenied(candidate))
        {
            throw new UnauthorizedAccessException(
                $"Policy denies access to '{candidate}'.");
        }
    }
}
