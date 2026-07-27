using System.Text.Json.Nodes;
using Aos.Broker;
using Aos.Core;
using Aos.Mcp.Shared;

namespace Aos.Mcp.Files;

/// <summary>
/// File finding, content search, and safe reorganisation. Every path is checked against the
/// allowed roots and denied paths from policy before anything touches the disk.
/// </summary>
internal sealed class FileSurface(PathGuard guard, TrashStore trash)
{
    private static readonly CapabilitySet Set = new("aos-files");

    /// <summary>Directories that are almost never what the user means.</summary>
    private static readonly string[] NoiseDirectories =
        ["node_modules", ".git", "obj", "bin", ".vs", "__pycache__", ".venv", "dist"];

    /// <summary>Extensions worth reading as text for content search.</summary>
    private static readonly string[] TextExtensions =
    [
        ".txt", ".md", ".json", ".yaml", ".yml", ".xml", ".csv", ".log", ".ini", ".cfg",
        ".cs", ".ts", ".js", ".jsx", ".tsx", ".py", ".ps1", ".psm1", ".lua", ".sql",
        ".html", ".css", ".sh", ".bat", ".toml", ".gitignore", ".env",
    ];

    private const long MaxGrepFileBytes = 8L * 1024 * 1024;

    public IEnumerable<ICapability> All()
    {
        yield return Set.Read(
            "aos-files/roots.list",
            "List the folders file capabilities are allowed to work in. Call this first if "
            + "you are unsure whether a path is reachable.",
            _ => new JsonObject
            {
                ["allowedRoots"] = new JsonArray([.. guard.AllowedRoots.Select(r => JsonValue.Create(r))]),
                ["note"] = guard.AllowedRoots.Count == 0
                    ? "No roots configured, so the whole filesystem is reachable except denied paths."
                    : "Paths outside these roots are refused before the risk tier is considered.",
            });

        yield return Set.Read(
            "aos-files/file.find",
            "Find files by name, extension, size or modification time. Use this for requests "
            + "like 'PDFs I touched this week'.",
            Find);

        yield return Set.Read(
            "aos-files/file.grep",
            "Search inside text files for a string. Returns matching lines with line numbers.",
            Grep);

        yield return Set.Read(
            "aos-files/file.read",
            "Read a text file, optionally a line range. Refuses binary and very large files.",
            ReadFile);

        yield return Set.Read(
            "aos-files/trash.list",
            "List everything the agent has moved to staged trash, newest first.",
            ListTrash);

        yield return Set.Mutating(
            "aos-files/file.move",
            RiskTier.Write,
            "Move or rename a file or folder. Both source and destination must be inside an "
            + "allowed root. Never overwrites an existing destination.",
            PlanMove,
            Move,
            verify: VerifyMove);

        yield return Set.Mutating(
            "aos-files/file.trash",
            RiskTier.Destructive,
            "Move a file or folder to staged trash. Nothing is permanently deleted, so this "
            + "is reversible with trash.restore.",
            PlanTrash,
            Trash,
            // A VSS shadow copy adds nothing here: the trash store is itself the undo
            // mechanism, and the original is preserved rather than destroyed.
            snapshot: false,
            verify: VerifyTrash);

        yield return Set.Mutating(
            "aos-files/trash.restore",
            RiskTier.Write,
            "Restore a staged-trash entry to where it came from, by its id from trash.list.",
            PlanRestore,
            RestoreTrash,
            verify: VerifyRestore);

        yield return Set.Mutating(
            "aos-files/trash.purge",
            RiskTier.Destructive,
            "PERMANENTLY delete staged-trash entries older than a given age. This is the only "
            + "operation here that cannot be undone. Nothing younger than minimumAgeDays is "
            + "touched, and the default age is deliberately generous.",
            PlanPurge,
            Purge,
            // Nothing to snapshot onto. A shadow copy of the trash folder would be a copy of
            // the very bytes being reclaimed, which defeats the entire purpose of the call.
            snapshot: false,
            verify: VerifyPurge);
    }

    // --- post-condition checks -------------------------------------------------------
    // Each confirms the world actually looks the way the mutation claimed. A returned
    // message becomes AppliedButUnverified rather than Failed, since the change did land.

    private static string? VerifyMove(JsonObject args, JsonNode? result)
    {
        var destination = result?["destination"]?.GetValue<string>();
        if (result is null || destination is null) { return "Result carried no destination path."; }

        var source = result["source"]?.GetValue<string>();
        if (source is null) { return "Result carried no source path."; }

        var arrived = File.Exists(destination) || Directory.Exists(destination);
        var departed = !File.Exists(source) && !Directory.Exists(source);

        if (!arrived) { return $"Nothing is present at the destination '{destination}'."; }
        if (!departed) { return $"The source '{source}' still exists, so this was a copy."; }

        return null;
    }

    private string? VerifyTrash(JsonObject args, JsonNode? result)
    {
        var id = result?["id"]?.GetValue<string>();
        if (result is null || id is null) { return "Result carried no trash id."; }

        var original = result["originalPath"]?.GetValue<string>();
        if (original is null) { return "Result carried no original path."; }

        if (File.Exists(original) || Directory.Exists(original))
        {
            return $"'{original}' still exists, so it was not moved out of the way.";
        }

        // The point of staged trash is that the item is recoverable. If the manifest entry
        // or the stored copy is missing then it was effectively a real delete, which is the
        // one thing this capability promises never to do.
        var entry = trash.List().LastOrDefault(e => e.Id == id);
        if (entry is null) { return $"No manifest entry was written for id '{id}'."; }
        if (!entry.StillStored)
        {
            return $"Manifest entry '{id}' exists but the stored copy is missing, so the "
                + "item is not recoverable.";
        }

        return null;
    }

    private static string? VerifyRestore(JsonObject args, JsonNode? result)
    {
        var restoredTo = result?["restoredTo"]?.GetValue<string>();
        if (restoredTo is null) { return "Result carried no restored path."; }

        return File.Exists(restoredTo) || Directory.Exists(restoredTo)
            ? null
            : $"Nothing is present at '{restoredTo}' after the restore.";
    }

    // --- reads -----------------------------------------------------------------------

    private IReadOnlyList<string> ResolveRoots(JsonObject args)
    {
        var requested = args.GetString("root");

        if (!string.IsNullOrWhiteSpace(requested))
        {
            guard.EnsureAllowed(requested);
            if (!Directory.Exists(requested))
            {
                throw new DirectoryNotFoundException($"No such folder: '{requested}'.");
            }
            return [requested];
        }

        // Default to every allowed root that actually exists. OneDrive redirection means
        // some of the configured roots are absent on any given machine.
        var roots = guard.AllowedRoots.Where(Directory.Exists).ToArray();
        return roots.Length > 0
            ? roots
            : throw new DirectoryNotFoundException(
                "No allowed root exists on this machine. Check allowedRoots in policy.yaml.");
    }

    private static EnumerationOptions Walk() => new()
    {
        RecurseSubdirectories = true,
        IgnoreInaccessible = true,
        AttributesToSkip = FileAttributes.System | FileAttributes.ReparsePoint,
    };

    private static bool IsNoise(string path) =>
        path.Split(Path.DirectorySeparatorChar)
            .Any(segment => NoiseDirectories.Contains(segment, StringComparer.OrdinalIgnoreCase));

    private JsonNode? Find(JsonObject args)
    {
        var roots = ResolveRoots(args);
        var pattern = args.GetString("namePattern") ?? "*";
        var limit = Math.Clamp(args.GetInt32("limit", 100), 1, 2000);
        var includeNoise = args.GetBool("includeNoiseFolders", false);
        var minSizeKb = args.GetInt32("minSizeKb", 0);

        var extensions = (args["extensions"] as JsonArray)?
            .Select(n => n!.GetValue<string>())
            .Select(e => e.StartsWith('.') ? e : "." + e)
            .ToArray();

        DateTimeOffset? modifiedAfter = null;
        if (args.TryGetInt64("modifiedWithinDays", out var days) && days > 0)
        {
            modifiedAfter = DateTimeOffset.UtcNow.AddDays(-days);
        }

        var hits = new List<FileInfo>();
        var truncated = false;

        foreach (var root in roots)
        {
            foreach (var file in new DirectoryInfo(root).EnumerateFiles(pattern, Walk()))
            {
                if (hits.Count >= limit) { truncated = true; break; }

                if (!includeNoise && IsNoise(file.FullName)) { continue; }
                if (guard.IsDenied(file.FullName)) { continue; }
                if (extensions is not null &&
                    !extensions.Contains(file.Extension, StringComparer.OrdinalIgnoreCase)) { continue; }
                if (minSizeKb > 0 && file.Length < minSizeKb * 1024L) { continue; }
                if (modifiedAfter is not null && file.LastWriteTimeUtc < modifiedAfter) { continue; }

                hits.Add(file);
            }

            if (truncated) { break; }
        }

        var files = new JsonArray();
        foreach (var file in hits.OrderByDescending(f => f.LastWriteTimeUtc))
        {
            files.Add(new JsonObject
            {
                ["path"] = file.FullName,
                ["name"] = file.Name,
                ["extension"] = file.Extension,
                ["sizeKb"] = Math.Round(file.Length / 1024d, 1),
                ["modified"] = file.LastWriteTimeUtc.ToString("o"),
            });
        }

        return new JsonObject
        {
            ["count"] = files.Count,
            ["truncated"] = truncated,
            // Never let a capped result read as a complete one.
            ["note"] = truncated
                ? $"Stopped at limit={limit}. Narrow the search or raise the limit."
                : null,
            ["rootsSearched"] = new JsonArray([.. roots.Select(r => JsonValue.Create(r))]),
            ["files"] = files,
        };
    }

    private JsonNode? Grep(JsonObject args)
    {
        var roots = ResolveRoots(args);
        var query = args.RequireString("query");
        var ignoreCase = args.GetBool("ignoreCase", true);
        var maxMatches = Math.Clamp(args.GetInt32("maxMatches", 100), 1, 1000);
        var comparison = ignoreCase
            ? StringComparison.OrdinalIgnoreCase
            : StringComparison.Ordinal;

        var extensions = (args["extensions"] as JsonArray)?
            .Select(n => n!.GetValue<string>())
            .Select(e => e.StartsWith('.') ? e : "." + e)
            .ToArray() ?? TextExtensions;

        var matches = new JsonArray();
        var filesScanned = 0;
        var truncated = false;

        foreach (var root in roots)
        {
            foreach (var file in new DirectoryInfo(root).EnumerateFiles("*", Walk()))
            {
                if (matches.Count >= maxMatches) { truncated = true; break; }

                if (IsNoise(file.FullName)) { continue; }
                if (guard.IsDenied(file.FullName)) { continue; }
                if (!extensions.Contains(file.Extension, StringComparer.OrdinalIgnoreCase)) { continue; }
                if (file.Length > MaxGrepFileBytes) { continue; }

                filesScanned++;

                try
                {
                    var lineNumber = 0;
                    foreach (var line in File.ReadLines(file.FullName))
                    {
                        lineNumber++;
                        if (!line.Contains(query, comparison)) { continue; }

                        matches.Add(new JsonObject
                        {
                            ["path"] = file.FullName,
                            ["line"] = lineNumber,
                            ["text"] = line.Length > 300 ? line[..300] + "..." : line.Trim(),
                        });

                        if (matches.Count >= maxMatches) { truncated = true; break; }
                    }
                }
                catch (Exception)
                {
                    // Locked or undecodable file. Skipping beats failing the whole search.
                }
            }

            if (truncated) { break; }
        }

        return new JsonObject
        {
            ["count"] = matches.Count,
            ["filesScanned"] = filesScanned,
            ["truncated"] = truncated,
            ["note"] = truncated ? $"Stopped at maxMatches={maxMatches}." : null,
            ["matches"] = matches,
        };
    }

    private JsonNode? ReadFile(JsonObject args)
    {
        var path = args.RequireString("path");
        guard.EnsureAllowed(path);

        if (!File.Exists(path)) { throw new FileNotFoundException($"No such file: '{path}'."); }

        var info = new FileInfo(path);
        if (info.Length > MaxGrepFileBytes)
        {
            throw new InvalidOperationException(
                $"File is {Math.Round(info.Length / 1024d / 1024d, 1)} MB, over the 8 MB read limit.");
        }

        var startLine = Math.Max(args.GetInt32("startLine", 1), 1);
        var maxLines = Math.Clamp(args.GetInt32("maxLines", 500), 1, 5000);

        var lines = File.ReadLines(path).Skip(startLine - 1).Take(maxLines).ToArray();

        return new JsonObject
        {
            ["path"] = path,
            ["startLine"] = startLine,
            ["lineCount"] = lines.Length,
            ["text"] = string.Join("\n", lines),
        };
    }

    private JsonNode? ListTrash(JsonObject args)
    {
        var limit = Math.Clamp(args.GetInt32("limit", 50), 1, 500);

        var includeClosed = args.GetBool("includeClosed", false);

        // One row per id, from its latest line. The manifest is append only, so a purge or a
        // restore writes a further line for the same id, and listing them raw showed one file
        // three times with three different restorable flags. A list of trash is meant to
        // answer "what can I get back", and duplicates make that unreadable.
        var latest = trash.List()
            .GroupBy(e => e.Id)
            .Select(g => g.Last())
            .Where(e => includeClosed || e.State == "trashed")
            .OrderByDescending(e => e.DeletedAt)
            .Take(limit);

        var entries = new JsonArray();
        var closed = 0;

        foreach (var entry in latest)
        {
            if (entry.State != "trashed") { closed++; }

            entries.Add(new JsonObject
            {
                ["id"] = entry.Id,
                ["originalPath"] = entry.OriginalPath,
                ["deletedAt"] = entry.DeletedAt.ToString("o"),
                ["sizeKb"] = Math.Round(entry.SizeBytes / 1024d, 1),
                ["reason"] = entry.Reason,
                // trashed, restored, purged, or missing. The flag alone could not distinguish
                // "we put this back" from "the bytes vanished", which are very different news.
                ["state"] = entry.State,
                ["restorable"] = entry.StillStored,
                ["restoredAt"] = entry.RestoredAt?.ToString("o"),
                ["purgedAt"] = entry.PurgedAt?.ToString("o"),
            });
        }

        return new JsonObject
        {
            ["count"] = entries.Count,
            ["trashRoot"] = trash.Root,
            ["closedShown"] = includeClosed ? closed : null,
            ["note"] = includeClosed
                ? "Includes entries already restored or permanently purged."
                : "Only entries still in trash. Pass includeClosed for restored and purged ones.",
            ["entries"] = entries,
        };
    }

    // --- mutations -------------------------------------------------------------------

    private (string Source, string Destination) MovePaths(JsonObject args)
    {
        var source = args.RequireString("source");
        var destination = args.RequireString("destination");

        guard.EnsureAllowed(source);
        guard.EnsureAllowed(destination);

        // A destination that is an existing folder means "move into it", which is what a
        // person means by "file these into that folder".
        if (Directory.Exists(destination))
        {
            var name = Path.GetFileName(source.TrimEnd(Path.DirectorySeparatorChar));
            destination = Path.Combine(destination, name);
            guard.EnsureAllowed(destination);
        }

        return (source, destination);
    }

    private string PlanMove(JsonObject args)
    {
        var (source, destination) = MovePaths(args);

        if (!File.Exists(source) && !Directory.Exists(source))
        {
            return $"Nothing exists at '{source}', so the move would fail.";
        }

        if (File.Exists(destination) || Directory.Exists(destination))
        {
            return $"'{destination}' already exists, so the move would be refused.";
        }

        return $"Move '{source}' to '{destination}'.";
    }

    private JsonNode? Move(JsonObject args)
    {
        var (source, destination) = MovePaths(args);
        var isDirectory = Directory.Exists(source);

        if (!isDirectory && !File.Exists(source))
        {
            throw new FileNotFoundException($"Nothing to move at '{source}'.");
        }

        if (File.Exists(destination) || Directory.Exists(destination))
        {
            throw new IOException(
                $"Refusing to overwrite '{destination}'. Trash or rename it first.");
        }

        var parent = Path.GetDirectoryName(destination);
        if (!string.IsNullOrEmpty(parent) && args.GetBool("createDirectories", true))
        {
            Directory.CreateDirectory(parent);
        }

        if (isDirectory) { Directory.Move(source, destination); }
        else { File.Move(source, destination); }

        return new JsonObject
        {
            ["source"] = source,
            ["destination"] = destination,
            ["kind"] = isDirectory ? "directory" : "file",
        };
    }

    private string PlanTrash(JsonObject args)
    {
        var path = args.RequireString("path");
        guard.EnsureAllowed(path);

        if (Directory.Exists(path))
        {
            var count = 0;
            try
            {
                count = Directory.EnumerateFiles(path, "*", Walk()).Count();
            }
            catch (Exception) { /* best effort count for the plan */ }

            return $"Move folder '{path}' and its {count} file(s) to staged trash. Reversible.";
        }

        if (File.Exists(path))
        {
            var size = Math.Round(new FileInfo(path).Length / 1024d, 1);
            return $"Move file '{path}' ({size} KB) to staged trash. Reversible.";
        }

        return $"Nothing exists at '{path}', so there is nothing to trash.";
    }

    private JsonNode? Trash(JsonObject args)
    {
        var path = args.RequireString("path");
        guard.EnsureAllowed(path);

        var entry = trash.Add(path, args.GetString("reason"));

        return new JsonObject
        {
            ["id"] = entry.Id,
            ["originalPath"] = entry.OriginalPath,
            ["sizeKb"] = Math.Round(entry.SizeBytes / 1024d, 1),
            ["restoreWith"] = $"trash.restore id={entry.Id}",
        };
    }

    /// <summary>
    /// Resolves the entry so the plan describes what will actually happen.
    ///
    /// This used to be a fixed sentence built from the id alone, which meant the plan for
    /// restoring a purged entry read "Restore trash entry X to its original path" and the
    /// commit then failed. A plan that confidently describes an impossible action is worse
    /// than no plan, because it is the thing a person reads before approving.
    /// </summary>
    private string PlanRestore(JsonObject args)
    {
        var id = args.RequireString("id");
        var entry = trash.List().LastOrDefault(e => e.Id == id);

        if (entry is null) { return $"No trash entry with id '{id}'. Nothing would be restored."; }

        if (entry.PurgedAt is { } purgedAt)
        {
            return $"Cannot restore: entry '{id}' ('{entry.OriginalPath}') was permanently purged "
                + $"on {purgedAt:yyyy-MM-dd}.";
        }

        if (entry.RestoredAt is { } restoredAt)
        {
            return $"Cannot restore: entry '{id}' was already restored on {restoredAt:yyyy-MM-dd} "
                + $"to '{entry.OriginalPath}'.";
        }

        if (!entry.StillStored)
        {
            return $"Cannot restore: entry '{id}' is recorded but its contents are gone from "
                + $"'{entry.StoredPath}'.";
        }

        if (File.Exists(entry.OriginalPath) || Directory.Exists(entry.OriginalPath))
        {
            return $"Cannot restore: '{entry.OriginalPath}' exists again, and this never "
                + "overwrites. Move that aside first.";
        }

        return $"Restore '{entry.OriginalPath}' from staged trash, trashed "
            + $"{entry.DeletedAt:yyyy-MM-dd}.";
    }

    private JsonNode? RestoreTrash(JsonObject args)
    {
        var entry = trash.Restore(args.RequireString("id"));

        return new JsonObject
        {
            ["id"] = entry.Id,
            ["restoredTo"] = entry.OriginalPath,
        };
    }

    // --- purge -----------------------------------------------------------------------

    /// <summary>
    /// Thirty days. Long enough that anything still sitting there has demonstrably not been
    /// missed, and the caller has to say a smaller number out loud to go below it.
    /// </summary>
    private const int DefaultPurgeAgeDays = 30;

    private static int PurgeAge(JsonObject args) =>
        Math.Clamp(args.GetInt32("minimumAgeDays", DefaultPurgeAgeDays), 1, 3650);

    private string PlanPurge(JsonObject args)
    {
        var age = PurgeAge(args);
        var candidates = trash.PurgeCandidates(age, DateTimeOffset.UtcNow);

        if (candidates.Count == 0)
        {
            return $"Nothing in staged trash is older than {age} days, so there is nothing to purge.";
        }

        var totalMb = Math.Round(candidates.Sum(c => c.SizeBytes) / 1024d / 1024d, 1);

        // Names the largest few rather than all of them. The plan is what a person reads
        // before authorising an irreversible delete, so it has to be readable, and the big
        // items are the ones worth a second look.
        var named = string.Join(", ", candidates
            .Take(5)
            .Select(c => $"{Path.GetFileName(c.Entry.OriginalPath)} ({Math.Round(c.SizeBytes / 1024d / 1024d)} MB, {c.AgeDays} days)"));

        var more = candidates.Count > 5 ? $", and {candidates.Count - 5} more" : string.Empty;

        return $"PERMANENTLY delete {candidates.Count} staged-trash entries older than {age} days, "
            + $"reclaiming {totalMb} MB. This cannot be undone. Largest: {named}{more}.";
    }

    private JsonNode? Purge(JsonObject args)
    {
        var age = PurgeAge(args);
        var candidates = trash.PurgeCandidates(age, DateTimeOffset.UtcNow);

        var purged = new JsonArray();
        var failed = new JsonArray();
        var reclaimed = 0L;

        foreach (var candidate in candidates)
        {
            try
            {
                trash.Purge(candidate.Entry);
                reclaimed += candidate.SizeBytes;
                purged.Add(new JsonObject
                {
                    ["id"] = candidate.Entry.Id,
                    ["originalPath"] = candidate.Entry.OriginalPath,
                    ["sizeMb"] = Math.Round(candidate.SizeBytes / 1024d / 1024d, 1),
                });
            }
            catch (Exception ex)
            {
                // One locked file must not abandon the rest. Reported individually, because
                // "purged 40 of 42" is a true answer and throwing here would make the whole
                // call look like it did nothing while having already deleted forty.
                failed.Add(new JsonObject
                {
                    ["id"] = candidate.Entry.Id,
                    ["originalPath"] = candidate.Entry.OriginalPath,
                    ["error"] = ex.Message,
                });
            }
        }

        return new JsonObject
        {
            ["purged"] = purged.Count,
            ["failed"] = failed.Count,
            ["reclaimedMb"] = Math.Round(reclaimed / 1024d / 1024d, 1),
            ["minimumAgeDays"] = age,
            ["entries"] = purged,
            ["errors"] = failed.Count > 0 ? failed : null,
        };
    }

    private string? VerifyPurge(JsonObject args, JsonNode? result)
    {
        var claimed = result?["purged"]?.GetValue<int>() ?? 0;
        if (claimed == 0) { return null; }

        // Asks the store again rather than trusting the loop above. A purge that reported
        // success while the bytes are still on disk is the one failure mode that turns
        // "reclaimed 6 GB" into a lie, and it is the whole reason this call exists.
        var age = PurgeAge(args);
        var remaining = trash.PurgeCandidates(age, DateTimeOffset.UtcNow);

        return remaining.Count == 0
            ? null
            : $"{remaining.Count} entries older than {age} days are still stored after the purge "
              + "reported success, so the space was not actually reclaimed.";
    }
}
