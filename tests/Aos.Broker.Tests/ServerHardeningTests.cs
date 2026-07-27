using Aos.Mcp.Apps;
using Aos.Mcp.Files;
using Xunit;

namespace Aos.Broker.Tests;

/// <summary>
/// Guards for defects found in the capability servers themselves rather than in the broker.
///
/// Each of these shipped once. The test is the thing that stops it shipping twice.
/// </summary>
public class ServerHardeningTests
{
    // --- Google URL construction -------------------------------------------------------

    [Theory]
    [InlineData("../../settings/forwardingAddresses")]
    [InlineData("abc/../../../users/someone-else/messages")]
    [InlineData("abc?format=full&x=")]
    [InlineData("abc#fragment")]
    public void MessageId_CannotWalkOutOfItsPathSegment(string id)
    {
        // Ids arrive as tool arguments and can come from a model that read an untrusted
        // email, so an unescaped one repointed the request at an endpoint nobody named.
        var escaped = GoogleClient.Segment(id);

        Assert.DoesNotContain("/", escaped);
        Assert.DoesNotContain("?", escaped);
        Assert.DoesNotContain("#", escaped);

        var url = GoogleClient.GmailUrl($"/messages/{escaped}", ("format", "full"));
        Assert.StartsWith("https://gmail.googleapis.com/gmail/v1/users/me/messages/", url);
        Assert.DoesNotContain("/../", url);
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    public void BlankMessageId_IsRefused(string id)
    {
        // An empty segment silently addresses the collection instead of one item, which for
        // a delete-shaped endpoint is the difference between one message and all of them.
        Assert.Throws<ArgumentException>(() => GoogleClient.Segment(id));
    }

    [Fact]
    public void OrdinaryMessageId_IsUnchanged()
    {
        Assert.Equal("18f2a1b9c4d5e6f7", GoogleClient.Segment("18f2a1b9c4d5e6f7"));
    }

    // --- mail header injection ---------------------------------------------------------

    [Theory]
    [InlineData("someone@example.com\r\nBcc: attacker@evil.test")]
    [InlineData("someone@example.com\nBcc: attacker@evil.test")]
    [InlineData("someone@example.com\r\n\r\nEntirely different body")]
    public void RecipientWithHeaderBreak_IsRefused(string to)
    {
        Assert.Throws<ArgumentException>(
            () => GoogleClient.BuildRawMessage(to, "subject", "body", null));
    }

    // --- staged trash ------------------------------------------------------------------

    [Fact]
    public void TrashedDirectory_RestoresWithItsContents()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var project = sandbox.Path("project");
        Directory.CreateDirectory(Path.Combine(project, "nested"));
        File.WriteAllText(Path.Combine(project, "top.txt"), "top");
        File.WriteAllText(Path.Combine(project, "nested", "deep.txt"), "deep");

        var entry = store.Add(project, "test");

        Assert.False(Directory.Exists(project));
        Assert.True(entry.StillStored);

        store.Restore(entry.Id);

        Assert.Equal("top", File.ReadAllText(Path.Combine(project, "top.txt")));
        Assert.Equal("deep", File.ReadAllText(Path.Combine(project, "nested", "deep.txt")));
    }

    [Fact]
    public void RestoringOverSomethingThatCameBack_IsRefused()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("notes.txt");
        File.WriteAllText(file, "original");
        var entry = store.Add(file, null);

        File.WriteAllText(file, "something else wrote here since");

        // Overwriting would destroy the newer file, which is the exact outcome a staged
        // trash exists to make impossible.
        Assert.Throws<IOException>(() => store.Restore(entry.Id));
        Assert.Equal("something else wrote here since", File.ReadAllText(file));
    }

    [Fact]
    public void RestoreOutsideTheAllowedRoots_IsRefused()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("notes.txt");
        File.WriteAllText(file, "original");
        var entry = store.Add(file, null);

        // The manifest lives inside a writable root, so anything that can write a file could
        // append a line naming any destination it liked. The guard, not the manifest, decides.
        var forged = new TrashStore(
            sandbox.Path("trash"),
            new PathGuard(deniedPaths: [], allowedRoots: [sandbox.Path("elsewhere")]));

        Assert.ThrowsAny<Exception>(() => forged.Restore(entry.Id));
        Assert.False(File.Exists(file));
    }

    // --- purge, the one irreversible operation -----------------------------------------

    [Fact]
    public void PurgeLeavesAnythingYoungerThanTheStatedAge()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("recent.txt");
        File.WriteAllText(file, "trashed just now");
        var entry = store.Add(file, null);

        // The age floor is the entire safety story for purge. Anything that slips past it is
        // permanently gone, so the boundary is worth pinning rather than assuming.
        Assert.Empty(store.PurgeCandidates(30, DateTimeOffset.UtcNow));
        Assert.Empty(store.PurgeCandidates(1, DateTimeOffset.UtcNow));

        // Only when enough time has notionally passed does it become a candidate.
        var later = entry.DeletedAt.AddDays(31);
        var candidates = store.PurgeCandidates(30, later);

        Assert.Single(candidates);
        Assert.Equal(entry.Id, candidates[0].Entry.Id);
    }

    [Fact]
    public void PurgeActuallyReclaimsTheBytesAndTheEntryStopsBeingRestorable()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("old.bin");
        File.WriteAllBytes(file, new byte[4096]);
        var entry = store.Add(file, "an old thing");

        var stored = entry.StoredPath;
        Assert.True(File.Exists(stored));

        var candidate = store.PurgeCandidates(30, entry.DeletedAt.AddDays(31)).Single();
        Assert.Equal(4096, candidate.SizeBytes);

        store.Purge(candidate.Entry);

        // The bytes are the point. A purge that reports success while the file is still on
        // disk turns "reclaimed 6 GB" into a lie.
        Assert.False(File.Exists(stored));
        Assert.Empty(store.PurgeCandidates(30, entry.DeletedAt.AddDays(31)));
    }

    [Fact]
    public void RestoringAPurgedEntrySaysItWasPurgedRatherThanThatItIsMissing()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("gone.txt");
        File.WriteAllText(file, "x");
        var entry = store.Add(file, null);

        store.Purge(store.PurgeCandidates(0, entry.DeletedAt.AddDays(1)).Single().Entry);

        // "Purged on 3 July" and "the folder is unexpectedly empty" call for completely
        // different reactions from whoever is hunting for the file. The manifest line is kept
        // precisely so the first answer is available.
        var error = Assert.Throws<FileNotFoundException>(() => store.Restore(entry.Id));
        Assert.Contains("purged", error.Message, StringComparison.OrdinalIgnoreCase);
        Assert.Contains(entry.OriginalPath, error.Message);
    }

    [Fact]
    public void EveryEndStateIsDistinguishable()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        string trashOne(string name)
        {
            var path = sandbox.Path(name);
            File.WriteAllText(path, "x");
            return store.Add(path, null).Id;
        }

        var stays = trashOne("stays.txt");
        var comesBack = trashOne("comes-back.txt");
        var deleted = trashOne("deleted.txt");
        var vanishes = trashOne("vanishes.txt");

        store.Restore(comesBack);
        store.Purge(store.List().Last(e => e.Id == deleted));

        // Something outside this tool removing the bytes must not look like either of the two
        // deliberate outcomes. "We put it back", "we deleted it", and "it is unaccountably
        // gone" call for three different reactions.
        var orphan = store.List().Last(e => e.Id == vanishes);
        File.Delete(orphan.StoredPath);

        string state(string id) => store.List().Last(e => e.Id == id).State;

        Assert.Equal("trashed", state(stays));
        Assert.Equal("restored", state(comesBack));
        Assert.Equal("purged", state(deleted));
        Assert.Equal("missing", state(vanishes));
    }

    [Fact]
    public void OneIdWithSeveralManifestLinesCollapsesToItsLatestState()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("busy.txt");
        File.WriteAllText(file, "x");
        var id = store.Add(file, null).Id;
        store.Restore(id);

        // The manifest is append only, so a restore or a purge writes a further line for the
        // same id. Listing them raw showed one file several times with different restorable
        // flags, which makes "what can I get back" unanswerable.
        Assert.Equal(2, store.List().Count(e => e.Id == id));

        var latest = store.List().GroupBy(e => e.Id).Select(g => g.Last()).ToList();
        Assert.Single(latest);
        Assert.Equal("restored", latest[0].State);
        Assert.False(latest[0].StillStored);
    }

    [Fact]
    public void RestoringTwiceIsRefusedAndSaysWhen()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("once.txt");
        File.WriteAllText(file, "x");
        var id = store.Add(file, null).Id;
        store.Restore(id);

        var error = Assert.Throws<InvalidOperationException>(() => store.Restore(id));
        Assert.Contains("already restored", error.Message);
    }

    [Fact]
    public void ARestoredEntryIsNotAPurgeCandidate()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("back.txt");
        File.WriteAllText(file, "x");
        var entry = store.Add(file, null);
        store.Restore(entry.Id);

        // Purging a restored entry would try to delete a slot whose contents are now sitting
        // back at the original path, which is a live file the person is using.
        Assert.Empty(store.PurgeCandidates(0, entry.DeletedAt.AddDays(365)));
        Assert.True(File.Exists(file));
    }

    [Fact]
    public void APurgedEntryIsNoLongerCountedAsRestorable()
    {
        using var sandbox = new Sandbox();
        var store = new TrashStore(sandbox.Path("trash"), sandbox.Guard);

        var file = sandbox.Path("listed.txt");
        File.WriteAllText(file, "x");
        var entry = store.Add(file, null);

        Assert.True(store.List().Last(e => e.Id == entry.Id).StillStored);

        store.Purge(store.PurgeCandidates(0, entry.DeletedAt.AddDays(1)).Single().Entry);

        // trash.list reports restorability from this flag, so a purged entry that still read
        // as restorable would offer a restore that cannot possibly work.
        Assert.False(store.List().Last(e => e.Id == entry.Id).StillStored);
    }

    private sealed class Sandbox : IDisposable
    {
        private readonly string _root = Directory.CreateTempSubdirectory("aos-test-").FullName;

        public PathGuard Guard => new(deniedPaths: [], allowedRoots: [_root]);

        public string Path(string relative) => System.IO.Path.Combine(_root, relative);

        public void Dispose()
        {
            try { Directory.Delete(_root, recursive: true); } catch (Exception) { }
        }
    }
}
