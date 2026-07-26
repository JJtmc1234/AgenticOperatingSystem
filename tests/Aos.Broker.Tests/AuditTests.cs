using System.Text.Json;
using System.Text.Json.Nodes;
using Aos.Core;
using Xunit;

namespace Aos.Broker.Tests;

public class ArgumentRedactorTests
{
    [Theory]
    [InlineData("apiKey")]
    [InlineData("api_key")]
    [InlineData("password")]
    [InlineData("Authorization")]
    [InlineData("refreshToken")]
    [InlineData("clientSecret")]
    [InlineData("privateKey")]
    public void RedactsSensitiveKeys(string key)
    {
        var args = new JsonObject { [key] = "hunter2" };

        var redacted = ArgumentRedactor.Redact(args)!;

        Assert.Equal(ArgumentRedactor.Placeholder, redacted[key]!.GetValue<string>());
    }

    [Fact]
    public void KeepsHarmlessValues()
    {
        var args = new JsonObject { ["path"] = @"C:\notes.md", ["count"] = 3 };

        var redacted = ArgumentRedactor.Redact(args)!;

        Assert.Equal(@"C:\notes.md", redacted["path"]!.GetValue<string>());
        Assert.Equal(3, redacted["count"]!.GetValue<int>());
    }

    [Fact]
    public void RedactsNestedValuesUnderSensitiveKey()
    {
        // A whole object under a sensitive key must go, not just scalar leaves.
        var args = new JsonObject
        {
            ["credentials"] = new JsonObject { ["user"] = "testuser", ["pat"] = "abc123" },
        };

        var redacted = ArgumentRedactor.Redact(args)!;
        var creds = redacted["credentials"]!.AsObject();

        Assert.Equal(ArgumentRedactor.Placeholder, creds["user"]!.GetValue<string>());
        Assert.Equal(ArgumentRedactor.Placeholder, creds["pat"]!.GetValue<string>());
    }

    [Fact]
    public void RedactsInsideArrays()
    {
        var args = new JsonObject
        {
            ["tokens"] = new JsonArray("a", "b"),
        };

        var redacted = ArgumentRedactor.Redact(args)!;

        Assert.All(redacted["tokens"]!.AsArray(),
            n => Assert.Equal(ArgumentRedactor.Placeholder, n!.GetValue<string>()));
    }

    [Fact]
    public void TruncatesOversizedStrings()
    {
        var big = new string('x', ArgumentRedactor.MaxStringLength + 100);

        var redacted = ArgumentRedactor.Redact(new JsonObject { ["body"] = big })!;
        var value = redacted["body"]!.GetValue<string>();

        Assert.Contains("+100 chars", value);
        Assert.True(value.Length < big.Length);
    }

    [Fact]
    public void DoesNotMutateInput()
    {
        var args = new JsonObject { ["password"] = "hunter2" };

        ArgumentRedactor.Redact(args);

        Assert.Equal("hunter2", args["password"]!.GetValue<string>());
    }
}

public class JsonlAuditSinkTests
{
    [Fact]
    public async Task AppendsOneJsonLinePerEntry_AndRoundTrips()
    {
        var dir = Path.Combine(Path.GetTempPath(), "aos-audit-" + Guid.NewGuid().ToString("n"));
        try
        {
            using var sink = new JsonlAuditSink(dir);

            for (var i = 0; i < 3; i++)
            {
                await sink.WriteAsync(Entry($"test/op{i}"), CancellationToken.None);
            }

            var lines = await File.ReadAllLinesAsync(sink.CurrentFile);
            Assert.Equal(3, lines.Length);

            var first = JsonSerializer.Deserialize<AuditEntry>(lines[0], AosJson.Options)!;
            Assert.Equal("test/op0", first.CapabilityId);
            Assert.Equal(RiskTier.Destructive, first.Tier);
            Assert.Equal(OutcomeStatus.Denied, first.Status);
        }
        finally
        {
            Directory.Delete(dir, recursive: true);
        }
    }

    [Fact]
    public async Task ConcurrentWrites_DoNotInterleaveOrDropLines()
    {
        var dir = Path.Combine(Path.GetTempPath(), "aos-audit-" + Guid.NewGuid().ToString("n"));
        try
        {
            using var sink = new JsonlAuditSink(dir);

            await Task.WhenAll(Enumerable.Range(0, 50).Select(i =>
                sink.WriteAsync(Entry($"test/op{i}"), CancellationToken.None)));

            var lines = await File.ReadAllLinesAsync(sink.CurrentFile);
            Assert.Equal(50, lines.Length);
            Assert.All(lines, l =>
                Assert.NotNull(JsonSerializer.Deserialize<AuditEntry>(l, AosJson.Options)));
        }
        finally
        {
            Directory.Delete(dir, recursive: true);
        }
    }

    private static AuditEntry Entry(string capabilityId) => new()
    {
        Timestamp = DateTimeOffset.UtcNow,
        CorrelationId = "c1",
        CapabilityId = capabilityId,
        Tier = RiskTier.Destructive,
        Verdict = PolicyVerdict.Deny,
        Status = OutcomeStatus.Denied,
        DryRun = false,
        DurationMs = 1,
    };
}
