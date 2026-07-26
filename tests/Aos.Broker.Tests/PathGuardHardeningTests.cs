using Xunit;

namespace Aos.Broker.Tests;

/// <summary>
/// Path canonicalisation defects found in the safety audit.
///
/// The audit verified empirically that Path.GetFullPath leaves extended-length paths
/// untouched, so "\\?\C:\Windows\System32\..\System32" kept its .. segment and walked past a
/// prefix comparison. Win32 accepts that form in every file API.
/// </summary>
public class PathGuardHardeningTests
{
    private static readonly PathGuard DenyOnly = new([@"C:\Windows\System32"]);

    [Theory]
    [InlineData(@"\\?\C:\Windows\System32\cmd.exe")]
    [InlineData(@"\\?\C:\Windows\System32\..\System32\cmd.exe")]
    [InlineData(@"\\?\GLOBALROOT\Device\HarddiskVolume1\Windows\System32")]
    [InlineData(@"\\localhost\C$\Windows\System32\cmd.exe")]
    [InlineData(@"\\server\share\file.txt")]
    [InlineData(@"//server/share/file.txt")]
    public void ExtendedLengthAndUncPaths_AreDenied(string path)
    {
        // Refused rather than normalised: these are forms the canonicaliser will not
        // canonicalise, so no comparison against them can be trusted.
        Assert.True(DenyOnly.IsDenied(path));
        Assert.False(DenyOnly.IsAllowed(path));
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    public void BlankPaths_AreDenied(string path)
    {
        // Previously false, which made IsAllowed("   ") true when no roots were configured.
        Assert.True(DenyOnly.IsDenied(path));
    }

    [Fact]
    public void OrdinaryPathsStillWork()
    {
        Assert.False(DenyOnly.IsDenied(@"C:\Users\testuser\Documents\notes.md"));
        Assert.True(DenyOnly.IsDenied(@"C:\Windows\System32\drivers\etc\hosts"));
    }

    [Fact]
    public void JunctionInsideAnAllowedRoot_CannotReachADeniedPath()
    {
        // An unprivileged directory junction used to satisfy both checks while pointing
        // somewhere else entirely, because GetFullPath does not resolve reparse points.
        var sandbox = Path.Combine(Path.GetTempPath(), "aos-guard-" + Guid.NewGuid().ToString("n"));
        var allowed = Path.Combine(sandbox, "allowed");
        var secret = Path.Combine(sandbox, "secret");
        Directory.CreateDirectory(allowed);
        Directory.CreateDirectory(secret);

        var link = Path.Combine(allowed, "link");

        try
        {
            try
            {
                Directory.CreateSymbolicLink(link, secret);
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                // Symlink creation needs Developer Mode or elevation. The guard is still
                // exercised by the other tests; skip rather than fail on policy.
                return;
            }

            var guard = new PathGuard(deniedPaths: [secret], allowedRoots: [allowed]);
            var target = Path.Combine(link, "id_rsa");

            Assert.True(guard.IsDenied(target));
            Assert.False(guard.IsAllowed(target));
        }
        finally
        {
            try { Directory.Delete(sandbox, recursive: true); } catch (IOException) { }
        }
    }
}
