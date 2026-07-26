using System.Text.Json.Nodes;

namespace Aos.Core;

/// <summary>
/// Typed reads over capability arguments, shared by every MCP server.
///
/// Numeric access is deliberately tolerant. <see cref="JsonValue.GetValue{T}"/> requires an
/// exact type match when the node was constructed in-process, so a value built from an
/// <c>int</c> throws when read as <c>long</c> -- even though arguments arriving over the wire
/// are JsonElement-backed and convert freely. Coercing here keeps a capability's behaviour
/// identical whether it was called by a real MCP client or constructed in a test.
/// </summary>
public static class JsonArgs
{
    /// <summary>Builds an argument object, omitting keys whose value is null.</summary>
    public static JsonObject Of(params (string Key, JsonNode? Value)[] pairs)
    {
        var obj = new JsonObject();
        foreach (var (key, value) in pairs)
        {
            if (value is not null) { obj[key] = value; }
        }
        return obj;
    }

    public static bool TryGetInt64(this JsonObject args, string name, out long value)
    {
        value = 0;
        if (args[name] is not JsonValue node) { return false; }

        if (node.TryGetValue<long>(out var asLong)) { value = asLong; return true; }
        if (node.TryGetValue<int>(out var asInt)) { value = asInt; return true; }

        if (node.TryGetValue<double>(out var asDouble) &&
            asDouble >= long.MinValue && asDouble <= long.MaxValue &&
            Math.Abs(asDouble % 1) < double.Epsilon)
        {
            value = (long)asDouble;
            return true;
        }

        // Some clients send numbers as strings.
        if (node.TryGetValue<string>(out var asText) &&
            long.TryParse(asText, out var parsed))
        {
            value = parsed;
            return true;
        }

        return false;
    }

    public static long RequireInt64(this JsonObject args, string name)
    {
        if (args[name] is null)
        {
            throw new ArgumentException($"Missing required argument '{name}'.");
        }

        return args.TryGetInt64(name, out var value)
            ? value
            : throw new ArgumentException($"Argument '{name}' must be an integer.");
    }

    public static int RequireInt32(this JsonObject args, string name)
    {
        var value = args.RequireInt64(name);
        return value is >= int.MinValue and <= int.MaxValue
            ? (int)value
            : throw new ArgumentException($"Argument '{name}' is out of range for a 32-bit integer.");
    }

    public static int GetInt32(this JsonObject args, string name, int fallback) =>
        args.TryGetInt64(name, out var value) && value is >= int.MinValue and <= int.MaxValue
            ? (int)value
            : fallback;

    public static string RequireString(this JsonObject args, string name)
    {
        if (args[name] is not JsonValue node)
        {
            throw new ArgumentException($"Missing required argument '{name}'.");
        }

        if (!node.TryGetValue<string>(out var text) || string.IsNullOrWhiteSpace(text))
        {
            throw new ArgumentException($"Argument '{name}' must be a non-empty string.");
        }

        return text;
    }

    public static string? GetString(this JsonObject args, string name) =>
        args[name] is JsonValue node && node.TryGetValue<string>(out var text) ? text : null;

    public static bool GetBool(this JsonObject args, string name, bool fallback)
    {
        if (args[name] is not JsonValue node) { return fallback; }

        if (node.TryGetValue<bool>(out var flag)) { return flag; }
        if (node.TryGetValue<string>(out var text) && bool.TryParse(text, out var parsed)) { return parsed; }

        return fallback;
    }
}
