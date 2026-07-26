using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace Aos.Broker;

/// <summary>
/// Strips secrets out of anything bound for the audit log. The log is long-lived plaintext,
/// so anything credential-shaped must never land in it.
///
/// Two mechanisms, because key names alone were not enough. Key matching misses a secret
/// embedded in an ordinary-looking value, such as a connection string carrying
/// Password=..., and it misses the free-text Reason and Message fields entirely since
/// those have no key at all.
/// </summary>
public static partial class ArgumentRedactor
{
    public const string Placeholder = "[redacted]";

    /// <summary>Values longer than this are truncated so one call cannot bloat the log.</summary>
    public const int MaxStringLength = 512;

    [GeneratedRegex(
        "token|secret|password|passwd|pwd|pass|apikey|api_key|credential|authorization|auth|"
        + "cookie|refresh|private|bearer|session|signature|mnemonic|passphrase|"
        + "\\bpat\\b|\\bkey\\b|\\botp\\b|\\bpin\\b|pem|pfx",
        RegexOptions.IgnoreCase)]
    private static partial Regex SensitiveKey();

    /// <summary>
    /// Values that are self-evidently credentials wherever they appear. Deliberately narrow:
    /// a false positive only costs readability in the log, but each pattern here has to be
    /// specific enough not to redact ordinary prose.
    /// </summary>
    [GeneratedRegex(
        "sk-[A-Za-z0-9_-]{16,}"                       // Anthropic / OpenAI style
        + "|gh[pousr]_[A-Za-z0-9]{16,}"               // GitHub tokens
        + "|xox[abposr]-[A-Za-z0-9-]{10,}"            // Slack
        + "|AIza[A-Za-z0-9_-]{20,}"                   // Google API keys
        + "|ya29\\.[A-Za-z0-9._-]{20,}"               // Google OAuth access tokens
        + "|eyJ[A-Za-z0-9_-]{10,}\\.[A-Za-z0-9_-]{10,}\\.[A-Za-z0-9_-]{10,}" // JWT
        + "|-----BEGIN [A-Z ]*PRIVATE KEY-----"
        + "|(?i:password|pwd)\\s*=\\s*[^;\\s\"']+",   // connection strings
        RegexOptions.None)]
    private static partial Regex SensitiveValue();

    public static JsonObject? Redact(JsonObject? arguments)
    {
        if (arguments is null) { return null; }
        return (JsonObject)RedactNode(arguments, inSensitiveKey: false)!;
    }

    /// <summary>
    /// Cleans a free-text field. Applied to Reason and Message, which are agent controlled
    /// and unbounded, and which previously went to the log verbatim, sidestepping both the
    /// redaction and the length cap.
    /// </summary>
    public static string? RedactText(string? text)
    {
        if (string.IsNullOrEmpty(text)) { return text; }

        var scrubbed = SensitiveValue().Replace(text, Placeholder);
        return Truncate(scrubbed);
    }

    private static string Truncate(string text) =>
        text.Length <= MaxStringLength
            ? text
            : text[..MaxStringLength] + $"...[+{text.Length - MaxStringLength} chars]";

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

                if (value.TryGetValue<string>(out var text))
                {
                    // A secret can sit inside an innocuously named field, so the value is
                    // scanned even when the key looks harmless.
                    var scrubbed = SensitiveValue().Replace(text, Placeholder);
                    return JsonValue.Create(Truncate(scrubbed));
                }

                return value.DeepClone();
            }

            default:
                return node?.DeepClone();
        }
    }
}
