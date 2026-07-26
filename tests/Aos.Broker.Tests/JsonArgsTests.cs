using System.Text.Json;
using System.Text.Json.Nodes;
using Aos.Core;
using Xunit;

namespace Aos.Broker.Tests;

public class JsonArgsTests
{
    /// <summary>
    /// Regression: JsonValue.GetValue&lt;long&gt;() throws on a node built from an int, so an
    /// int-typed tool parameter used to fail arg parsing before ever reaching the capability.
    /// </summary>
    [Fact]
    public void RequireInt64_ReadsValueBuiltFromInt()
    {
        var args = JsonArgs.Of(("pid", 4242));

        Assert.Equal(4242L, args.RequireInt64("pid"));
        Assert.Equal(4242, args.RequireInt32("pid"));
    }

    [Fact]
    public void RequireInt64_ReadsValueBuiltFromLong()
    {
        var args = JsonArgs.Of(("hwnd", 2492904L));

        Assert.Equal(2492904L, args.RequireInt64("hwnd"));
    }

    [Fact]
    public void RequireInt64_ReadsWireArguments()
    {
        // Arguments arriving from a client are JsonElement-backed, not constructed in-process.
        var args = JsonNode.Parse("""{"pid": 4242, "hwnd": 2492904}""")!.AsObject();

        Assert.Equal(4242L, args.RequireInt64("pid"));
        Assert.Equal(2492904L, args.RequireInt64("hwnd"));
    }

    [Theory]
    [InlineData("""{"pid": "4242"}""")]  // number as string
    [InlineData("""{"pid": 4242.0}""")]  // integral double
    public void RequireInt64_CoercesTolerantly(string json)
    {
        var args = JsonNode.Parse(json)!.AsObject();

        Assert.Equal(4242L, args.RequireInt64("pid"));
    }

    [Fact]
    public void RequireInt64_RejectsNonIntegralAndMissing()
    {
        var args = JsonNode.Parse("""{"pid": "abc", "frac": 1.5}""")!.AsObject();

        Assert.Throws<ArgumentException>(() => args.RequireInt64("pid"));
        Assert.Throws<ArgumentException>(() => args.RequireInt64("frac"));

        var missing = Assert.Throws<ArgumentException>(() => args.RequireInt64("nope"));
        Assert.Contains("nope", missing.Message);
    }

    [Fact]
    public void RequireInt32_RejectsOutOfRange()
    {
        var args = JsonArgs.Of(("big", long.MaxValue));

        Assert.Throws<ArgumentException>(() => args.RequireInt32("big"));
    }

    [Fact]
    public void Of_OmitsNullsButKeepsFalseAndZero()
    {
        var args = JsonArgs.Of(
            ("present", "value"),
            ("absent", null),
            ("flag", false),
            ("zero", 0));

        Assert.False(args.ContainsKey("absent"));
        Assert.True(args.ContainsKey("flag"));
        Assert.False(args.GetBool("flag", true));
        Assert.Equal(0, args.GetInt32("zero", 99));
    }

    [Fact]
    public void GetInt32_FallsBackWhenMissingOrUnparseable()
    {
        var args = JsonNode.Parse("""{"bad": "xyz"}""")!.AsObject();

        Assert.Equal(7, args.GetInt32("bad", 7));
        Assert.Equal(7, args.GetInt32("missing", 7));
    }

    [Fact]
    public void RequireString_RejectsEmptyAndWhitespace()
    {
        var args = JsonArgs.Of(("blank", "   "), ("ok", "text"));

        Assert.Throws<ArgumentException>(() => args.RequireString("blank"));
        Assert.Equal("text", args.RequireString("ok"));
    }

    [Fact]
    public void GetBool_AcceptsWireStringsAndBooleans()
    {
        var args = JsonNode.Parse("""{"a": true, "b": "true", "c": "nonsense"}""")!.AsObject();

        Assert.True(args.GetBool("a", false));
        Assert.True(args.GetBool("b", false));
        Assert.False(args.GetBool("c", false));
        Assert.True(args.GetBool("missing", true));
    }
}
