using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace Aos.Broker;

/// <summary>
/// Strips secrets out of arguments before they reach the audit log. The log is long-lived
/// plaintext, so anything credential-shaped must never land in it.
/// </summary>
public static partial class ArgumentRedactor
{
    public const string Placeholder = "[redacted]";

    /// <summary>Values longer than this are truncated so one call cannot bloat the log.</summary>
    public const int MaxStringLength = 512;

    [GeneratedRegex(
        "token|secret|password|passwd|apikey|api_key|credential|authorization|cookie|refresh|private",
        RegexOptions.IgnoreCase)]
    private static partial Regex SensitiveKey();

    public static JsonObject? Redact(JsonObject? arguments)
    {
        if (arguments is null) { return null; }
        return (JsonObject)RedactNode(arguments, inSensitiveKey: false)!;
    }

    private static JsonNode? RedactNode(JsonNode? node, bool inSensitiveKey)
    {
        switch (node)
        {
            case JsonObject obj:
            {
                var result = new JsonObject();
                foreach (var (key, value) in obj)
                {
                    result[key] = RedactNode(value, inSensitiveKey || SensitiveKey().IsMatch(key));
                }
                return result;
            }

            case JsonArray array:
            {
                var result = new JsonArray();
                foreach (var item in array)
                {
                    result.Add(RedactNode(item, inSensitiveKey));
                }
                return result;
            }

            case JsonValue value:
            {
                if (inSensitiveKey) { return JsonValue.Create(Placeholder); }

                if (value.TryGetValue<string>(out var text) && text.Length > MaxStringLength)
                {
                    return JsonValue.Create(
                        text[..MaxStringLength] + $"...[+{text.Length - MaxStringLength} chars]");
                }

                return value.DeepClone();
            }

            default:
                return node?.DeepClone();
        }
    }
}
