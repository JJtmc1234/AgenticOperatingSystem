using System.Text.Json;
using System.Text.Json.Serialization;
using Aos.Broker;
using Aos.Core;

namespace Aos.Mcp.Files;

public sealed record TrashEntry
{
    public required string Id { get; init; }
    public required string OriginalPath { get; init; }
    public required string StoredPath { get; init; }
    public required DateTimeOffset DeletedAt { get; init; }
    public required long SizeBytes { get; init; }
    public string? Reason { get; init; }

    /// <summary>
    /// Set on the manifest line written when an entry is permanently deleted.
    ///
    /// The line is kept rather than the record removed, so a later restore attempt can say
    /// "this was purged on such a date" instead of "no such entry". Someone hunting for a
    /// file deserves the first answer.
    /// </summary>
    public DateTimeOffset? PurgedAt { get; init; }

    /// <summary>
    /// Set on the manifest line written when an entry is restored to its original path.
    ///
    /// Recorded for the same reason as <see cref="PurgedAt"/>. Without it, a restored entry
    /// was indistinguishable from one whose bytes had gone missing, so the tooling reported
    /// "its contents are gone, something else removed it" about a file it had itself put back.
    /// </summary>
    public DateTimeOffset? RestoredAt { get; init; }

    /// <summary>The three end states an entry can be in, for reporting.</summary>
    [JsonIgnore]
    public string State =>
        PurgedAt is not null ? "purged"
        : RestoredAt is not null ? "restored"
        : StillStored ? "trashed"
        : "missing";

    [JsonIgnore]
    public bool StillStored =>
        PurgedAt is null && RestoredAt is null &&
        (File.Exists(StoredPath) || Directory.Exists(StoredPath));
}

/// <summary>
/// Staged deletes. Nothing is ever really deleted by a capability, it is moved here with a
/// manifest recording where it came from, so any mistake is one restore call away.
///
/// Each entry gets its own folder named by id, which removes the whole class of bugs where
/// two files with the same name collide in the trash.
/// </summary>
public sealed class TrashStore(string root, PathGuard guard)
{
    private readonly string _manifestPath = Path.Combine(root, "manifest.jsonl");

    public string Root { get; } = root;

    public TrashEntry Add(string sourcePath, string? reason)
    {
        var isDirectory = Directory.Exists(sourcePath);
        if (!isDirectory && !File.Exists(sourcePath))
        {
            throw new FileNotFoundException($"Nothing to trash at '{sourcePath}'.");
        }

        var id = $"{DateTimeOffset.UtcNow:yyyyMMdd-HHmmss}-{Guid.NewGuid():n}";
        var slot = Path.Combine(Root, id);
        Directory.CreateDirectory(slot);

        var name = Path.GetFileName(sourcePath.TrimEnd(Path.DirectorySeparatorChar));
        var stored = Path.Combine(slot, name);

        var size = isDirectory ? DirectorySize(sourcePath) : new FileInfo(sourcePath).Length;

        Relocate(sourcePath, stored, isDirectory);

        var entry = new TrashEntry
        {
            Id = id,
            OriginalPath = sourcePath,
            StoredPath = stored,
            DeletedAt = DateTimeOffset.UtcNow,
            SizeBytes = size,
            Reason = reason,
        };

        File.AppendAllLines(_manifestPath, [JsonSerializer.Serialize(entry, AosJson.Options)]);
        return entry;
    }

    public IReadOnlyList<TrashEntry> List()
    {
        if (!File.Exists(_manifestPath)) { return []; }

        var entries = new List<TrashEntry>();
        foreach (var line in File.ReadLines(_manifestPath))
        {
            if (string.IsNullOrWhiteSpace(line)) { continue; }
            try
            {
                var entry = JsonSerializer.Deserialize<TrashEntry>(line, AosJson.Options);
                if (entry is not null) { entries.Add(entry); }
            }
            catch (JsonException)
            {
                // A torn final line beats losing the whole manifest.
            }
        }

        return entries;
    }

    public TrashEntry Restore(string id)
    {
        var entry = List().LastOrDefault(e => e.Id == id)
            ?? throw new FileNotFoundException($"No trash entry with id '{id}'.");

        // The manifest is data, not authority. It lives inside an allowed root, so anything
        // that can write a file could append a line naming any OriginalPath it liked and turn
        // this restore into "place a file wherever I want" -- laundered through a tool whose
        // description promises it only puts things back. Re-checking both paths against the
        // guard is what stops that, and it also catches the honest case where allowedRoots
        // was narrowed after the item was trashed.
        guard.EnsureAllowed(entry.OriginalPath);
        guard.EnsureAllowed(entry.StoredPath);

        // Says which of the two it is. "Purged on 3 July" and "the folder is unexpectedly
        // empty" call for completely different reactions from whoever is looking for the file.
        if (entry.PurgedAt is { } purgedAt)
        {
            throw new FileNotFoundException(
                $"Trash entry '{id}' was permanently purged on {purgedAt:yyyy-MM-dd} and cannot "
                + "be restored. Its original path was '" + entry.OriginalPath + "'.");
        }

        if (entry.RestoredAt is { } restoredAt)
        {
            throw new InvalidOperationException(
                $"Trash entry '{id}' was already restored on {restoredAt:yyyy-MM-dd} to "
                + $"'{entry.OriginalPath}'. There is nothing left in trash for it.");
        }

        if (!entry.StillStored)
        {
            throw new FileNotFoundException(
                $"Trash entry '{id}' is recorded but its contents are gone from '{entry.StoredPath}'. "
                + "It was neither restored nor purged through this tool, so something else "
                + "removed it.");
        }

        if (File.Exists(entry.OriginalPath) || Directory.Exists(entry.OriginalPath))
        {
            throw new IOException(
                $"Cannot restore: '{entry.OriginalPath}' exists again. Move it aside first.");
        }

        var parent = Path.GetDirectoryName(entry.OriginalPath);
        if (!string.IsNullOrEmpty(parent)) { Directory.CreateDirectory(parent); }

        Relocate(entry.StoredPath, entry.OriginalPath, Directory.Exists(entry.StoredPath));

        // Recorded, symmetric with Purge. The manifest is the only durable account of what
        // happened to an item, and a restore that left no trace made a restored entry read as
        // one whose contents had mysteriously vanished.
        var restored = entry with { RestoredAt = DateTimeOffset.UtcNow };
        File.AppendAllLines(_manifestPath, [JsonSerializer.Serialize(restored, AosJson.Options)]);

        return restored;
    }

    /// <summary>
    /// Moves a file or directory, falling back to copy then delete when the two paths sit on
    /// different volumes.
    ///
    /// The trash lives under LOCALAPPDATA on C:, and plenty of real files do not. Directory.Move
    /// throws a flat "source and destination must have the same root" IOException across
    /// volumes, so trashing or restoring a folder from a second drive failed outright. File.Move
    /// does handle it, but by copying, so the atomicity the original comment here claimed was
    /// only ever true within one volume.
    ///
    /// Ordering is what keeps a cross-volume move safe: the source is deleted only after the
    /// copy has fully succeeded, and a failed copy takes its own partial output with it. An
    /// interruption can therefore leave the item in both places, never in neither.
    /// </summary>
    private static void Relocate(string source, string destination, bool isDirectory)
    {
        if (!isDirectory)
        {
            File.Move(source, destination);
            return;
        }

        if (string.Equals(
                Path.GetPathRoot(Path.GetFullPath(source)),
                Path.GetPathRoot(Path.GetFullPath(destination)),
                StringComparison.OrdinalIgnoreCase))
        {
            Directory.Move(source, destination);
            return;
        }

        try
        {
            CopyTree(source, destination);
        }
        catch (Exception)
        {
            // Leave no half-copied folder behind pretending to be a complete one.
            try { Directory.Delete(destination, recursive: true); } catch (Exception) { }
            throw;
        }

        Directory.Delete(source, recursive: true);
    }

    private static void CopyTree(string source, string destination)
    {
        Directory.CreateDirectory(destination);

        foreach (var file in Directory.EnumerateFiles(source))
        {
            File.Copy(file, Path.Combine(destination, Path.GetFileName(file)), overwrite: false);
        }

        foreach (var directory in Directory.EnumerateDirectories(source))
        {
            // Refused rather than skipped. Copying through a junction would duplicate its
            // target instead of the link, and a self-referential one would never terminate,
            // but skipping it silently is worse: the recursive delete below would then remove
            // the link from the source, and the trash could no longer restore what it took.
            if (new DirectoryInfo(directory).Attributes.HasFlag(FileAttributes.ReparsePoint))
            {
                throw new IOException(
                    $"'{directory}' is a junction or symlink, and this move crosses volumes, so "
                    + "it cannot be relocated without either following the link or losing it. "
                    + "Move the folder within its own drive, or remove the link first.");
            }

            CopyTree(directory, Path.Combine(destination, Path.GetFileName(directory)));
        }
    }

    /// <summary>An entry old enough to purge, with its size, for reporting a plan.</summary>
    public sealed record PurgeCandidate(TrashEntry Entry, long SizeBytes, int AgeDays);

    /// <summary>
    /// Entries older than the given age that are still on disk.
    ///
    /// Sizes are re-measured rather than read from the manifest, because the manifest records
    /// what the item was when it was trashed and the number being reported here is how much
    /// space purging would actually reclaim.
    /// </summary>
    public IReadOnlyList<PurgeCandidate> PurgeCandidates(int minimumAgeDays, DateTimeOffset now)
    {
        var cutoff = now.AddDays(-minimumAgeDays);
        var candidates = new List<PurgeCandidate>();

        // Latest entry per id. The manifest is append only, and a restore then re-trash
        // writes a second line for the same id, so grouping keeps this from purging against
        // a stale record.
        foreach (var entry in List().GroupBy(e => e.Id).Select(g => g.Last()))
        {
            if (entry.DeletedAt > cutoff) { continue; }
            if (!entry.StillStored) { continue; }

            guard.EnsureAllowed(entry.StoredPath);

            var size = Directory.Exists(entry.StoredPath)
                ? DirectorySize(entry.StoredPath)
                : new FileInfo(entry.StoredPath).Length;

            candidates.Add(new PurgeCandidate(entry, size, (int)(now - entry.DeletedAt).TotalDays));
        }

        // Biggest first, since the whole point of purging is reclaiming space.
        return [.. candidates.OrderByDescending(c => c.SizeBytes)];
    }

    /// <summary>
    /// Permanently deletes one staged entry and records the purge in the manifest.
    ///
    /// This is the only operation in the whole system that destroys data with no undo, which
    /// is why it is separate from everything else, gated at Destructive tier, and refuses
    /// anything younger than the caller's stated age. The trash existing does not make
    /// deleting from it safe; it makes deleting from it deliberate.
    ///
    /// The append-only manifest keeps the record. An entry whose bytes are gone but whose
    /// line remains is how a restore can say "this was purged on such a date" rather than
    /// "no such entry", which is a much worse answer to give someone looking for a file.
    /// </summary>
    public void Purge(TrashEntry entry)
    {
        // Re-checked at the moment of deletion, not merely when the plan was built. This is
        // the last line of defence before an irreversible delete, and the path came from a
        // manifest that anything able to write a file could have appended to.
        guard.EnsureAllowed(entry.StoredPath);

        var slot = Path.GetDirectoryName(entry.StoredPath);
        if (slot is null || !IsOwnSlot(slot))
        {
            throw new InvalidOperationException(
                $"'{entry.StoredPath}' does not sit in a slot of this trash store, so it will "
                + "not be purged. The manifest entry does not match the store layout.");
        }

        if (Directory.Exists(entry.StoredPath)) { Directory.Delete(entry.StoredPath, recursive: true); }
        else if (File.Exists(entry.StoredPath)) { File.Delete(entry.StoredPath); }

        // The slot folder goes too, but only when empty, so a surprise sibling file is never
        // taken along silently.
        try
        {
            if (Directory.Exists(slot) && Directory.EnumerateFileSystemEntries(slot).Any() == false)
            {
                Directory.Delete(slot);
            }
        }
        catch (IOException) { /* a lingering slot folder is harmless */ }

        File.AppendAllLines(_manifestPath, [JsonSerializer.Serialize(
            entry with { PurgedAt = DateTimeOffset.UtcNow }, AosJson.Options)]);
    }

    /// <summary>True when the path is a direct child slot of this store's root.</summary>
    private bool IsOwnSlot(string slot)
    {
        var parent = Path.GetDirectoryName(Path.GetFullPath(slot));
        return parent is not null &&
               string.Equals(
                   Path.GetFullPath(parent).TrimEnd(Path.DirectorySeparatorChar),
                   Path.GetFullPath(Root).TrimEnd(Path.DirectorySeparatorChar),
                   StringComparison.OrdinalIgnoreCase);
    }

    private static long DirectorySize(string path)
    {
        try
        {
            return new DirectoryInfo(path)
                .EnumerateFiles("*", new EnumerationOptions
                {
                    RecurseSubdirectories = true,
                    IgnoreInaccessible = true,
                    // Junctions carry neither Hidden nor System, so the default skip list
                    // lets the walk descend into them, and .NET has no loop detection. A
                    // trashed folder containing a self-junction would spin here.
                    AttributesToSkip = FileAttributes.System | FileAttributes.ReparsePoint,
                })
                .Sum(f => f.Length);
        }
        catch (Exception)
        {
            return 0;
        }
    }
}
