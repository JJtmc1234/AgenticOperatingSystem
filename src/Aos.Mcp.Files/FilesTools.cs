using System.ComponentModel;
using Aos.Broker;
using Aos.Core;
using Aos.Mcp.Shared;
using ModelContextProtocol.Server;

namespace Aos.Mcp.Files;

/// <summary>
/// MCP tool surface for files. Mutating tools take a commit flag: called without it they
/// return a plan and change nothing.
/// </summary>
[McpServerToolType]
public sealed class FilesTools(CapabilityBroker broker)
{
    [McpServerTool(Name = "files_roots")]
    [Description("List the folders these tools are allowed to work in. Call this first if a "
        + "path might be out of bounds.")]
    public Task<string> Roots(CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/roots.list", new(), cancellationToken: cancellationToken);

    [McpServerTool(Name = "files_find")]
    [Description("Find files by name pattern, extension, size or modification time. This is "
        + "the tool for requests like 'PDFs I touched this week' or 'big videos in Downloads'.")]
    public Task<string> Find(
        [Description("Folder to search. Omit to search every allowed root.")]
        string? root = null,
        [Description("Wildcard on the file name, for example *.pdf or invoice*.")]
        string? namePattern = null,
        [Description("Extensions to keep, for example [\"pdf\",\"docx\"]. Omit for all.")]
        string[]? extensions = null,
        [Description("Only files modified within this many days.")]
        int? modifiedWithinDays = null,
        [Description("Only files at least this many kilobytes.")]
        int minSizeKb = 0,
        [Description("Maximum files to return. Default 100.")]
        int limit = 100,
        [Description("Include node_modules, .git, obj, bin and similar. Usually noise.")]
        bool includeNoiseFolders = false,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/file.find",
            JsonArgs.Of(
                ("root", root), ("namePattern", namePattern),
                ("extensions", JsonArgs.ArrayOf(extensions)),
                ("modifiedWithinDays", modifiedWithinDays), ("minSizeKb", minSizeKb),
                ("limit", limit), ("includeNoiseFolders", includeNoiseFolders)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "files_grep")]
    [Description("Search inside text files for a string. Returns matching lines with paths "
        + "and line numbers. Skips binaries and files over 8 MB.")]
    public Task<string> Grep(
        [Description("Text to search for.")] string query,
        [Description("Folder to search. Omit to search every allowed root.")]
        string? root = null,
        [Description("Extensions to scan. Omit for the built-in text file list.")]
        string[]? extensions = null,
        [Description("Case insensitive. Default true.")] bool ignoreCase = true,
        [Description("Maximum matches to return. Default 100.")] int maxMatches = 100,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/file.grep",
            JsonArgs.Of(
                ("query", query), ("root", root),
                ("extensions", JsonArgs.ArrayOf(extensions)),
                ("ignoreCase", ignoreCase), ("maxMatches", maxMatches)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "files_read")]
    [Description("Read a text file, optionally from a starting line. Refuses files over 8 MB.")]
    public Task<string> Read(
        [Description("Full path to the file.")] string path,
        [Description("First line to return, 1 based.")] int startLine = 1,
        [Description("Maximum lines to return. Default 500.")] int maxLines = 500,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/file.read",
            JsonArgs.Of(("path", path), ("startLine", startLine), ("maxLines", maxLines)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "files_move")]
    [Description("Move or rename a file or folder. If the destination is an existing folder, "
        + "the item is moved into it. Never overwrites. Returns a plan unless commit is true.")]
    public Task<string> Move(
        [Description("Path to move.")] string source,
        [Description("Destination path, or an existing folder to move into.")] string destination,
        [Description("Create missing parent folders. Default true.")] bool createDirectories = true,
        [Description("Set true to actually move it.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/file.move",
            JsonArgs.Of(("source", source), ("destination", destination),
                        ("createDirectories", createDirectories)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "files_trash")]
    [Description("Move a file or folder to staged trash. Nothing is permanently deleted, so "
        + "this is reversible with files_trash_restore. Returns a plan unless commit is true.")]
    public Task<string> Trash(
        [Description("Path to move to staged trash.")] string path,
        [Description("Set true to actually move it to trash.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log and the trash manifest.")]
        string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/file.trash",
            JsonArgs.Of(("path", path), ("reason", reason)), commit, reason, cancellationToken);

    [McpServerTool(Name = "files_trash_list")]
    [Description("List everything moved to staged trash, newest first, with restore ids.")]
    public Task<string> TrashList(
        [Description("Maximum entries. Default 50.")] int limit = 50,
        [Description("Also show entries already restored or permanently purged. Default false, "
            + "since the usual question is what can still be recovered.")]
        bool includeClosed = false,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/trash.list",
            JsonArgs.Of(("limit", limit), ("includeClosed", includeClosed)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "files_trash_restore")]
    [Description("Restore a staged-trash entry to its original path, by id from "
        + "files_trash_list. Returns a plan unless commit is true.")]
    public Task<string> TrashRestore(
        [Description("Trash entry id.")] string id,
        [Description("Set true to actually restore it.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/trash.restore",
            JsonArgs.Of(("id", id)), commit, reason, cancellationToken);

    [McpServerTool(Name = "files_trash_purge")]
    [Description("PERMANENTLY delete staged-trash entries older than a given age, reclaiming "
        + "disk space. This is the only file operation that cannot be undone, so it is the one "
        + "to be most careful with: read the plan and check what it names before committing. "
        + "Staged trash sits on the same drive as the files it holds, so trashing alone frees "
        + "nothing; this is what actually reclaims the space.")]
    public Task<string> TrashPurge(
        [Description("Only purge entries trashed at least this many days ago. Default 30. "
            + "Lower it deliberately; the default is generous on purpose.")]
        int minimumAgeDays = 30,
        [Description("Set true to actually delete them permanently.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-files/trash.purge",
            JsonArgs.Of(("minimumAgeDays", minimumAgeDays)), commit, reason, cancellationToken);

    [McpServerTool(Name = "files_capabilities")]
    [Description("List every registered file capability with its risk tier and whether it "
        + "needs a commit handshake.")]
    public string Capabilities() => broker.DescribeCapabilities();
}
