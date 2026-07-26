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

    [JsonIgnore]
    public bool StillStored => File.Exists(StoredPath) || Directory.Exists(StoredPath);
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

        if (!entry.StillStored)
        {
            throw new FileNotFoundException(
                $"Trash entry '{id}' is recorded but its contents are gone from '{entry.StoredPath}'.");
        }

        if (File.Exists(entry.OriginalPath) || Directory.Exists(entry.OriginalPath))
        {
            throw new IOException(
                $"Cannot restore: '{entry.OriginalPath}' exists again. Move it aside first.");
        }

        var parent = Path.GetDirectoryName(entry.OriginalPath);
        if (!string.IsNullOrEmpty(parent)) { Directory.CreateDirectory(parent); }

        Relocate(entry.StoredPath, entry.OriginalPath, Directory.Exists(entry.StoredPath));

        return entry;
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
