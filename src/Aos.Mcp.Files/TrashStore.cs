using System.Text.Json;
using System.Text.Json.Serialization;
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
public sealed class TrashStore(string root)
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

        var id = $"{DateTimeOffset.UtcNow:yyyyMMdd-HHmmss}-{Guid.NewGuid():n}"[..24];
        var slot = Path.Combine(Root, id);
        Directory.CreateDirectory(slot);

        var name = Path.GetFileName(sourcePath.TrimEnd(Path.DirectorySeparatorChar));
        var stored = Path.Combine(slot, name);

        var size = isDirectory ? DirectorySize(sourcePath) : new FileInfo(sourcePath).Length;

        // Move, not copy then delete: a move is atomic within a volume, so an interrupted
        // trash operation cannot leave the file in both places or neither.
        if (isDirectory) { Directory.Move(sourcePath, stored); }
        else { File.Move(sourcePath, stored); }

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

        if (Directory.Exists(entry.StoredPath)) { Directory.Move(entry.StoredPath, entry.OriginalPath); }
        else { File.Move(entry.StoredPath, entry.OriginalPath); }

        return entry;
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
                })
                .Sum(f => f.Length);
        }
        catch (Exception)
        {
            return 0;
        }
    }
}
