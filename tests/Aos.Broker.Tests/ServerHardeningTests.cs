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
