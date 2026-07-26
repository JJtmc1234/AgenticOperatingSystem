using System.Net.Http.Headers;
using System.Text;
using System.Text.Json.Nodes;

namespace Aos.Mcp.Apps;

/// <summary>
/// Thin REST client for Gmail and Calendar.
///
/// Deliberately HttpClient plus JsonNode rather than the Google .NET SDK: the four
/// endpoints used here are simple, and the SDK would add a large dependency surface plus
/// its own auth and retry model on top of the one the broker already provides.
/// </summary>
public sealed class GoogleClient(GoogleAuth auth)
{
    private const string Gmail = "https://gmail.googleapis.com/gmail/v1/users/me";
    private const string Calendar = "https://www.googleapis.com/calendar/v3";

    private static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(30) };

    private async Task<HttpRequestMessage> AuthorizeAsync(
        HttpMethod method, string url, CancellationToken cancellationToken)
    {
        var token = await auth.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        var request = new HttpRequestMessage(method, url);
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", token);
        return request;
    }

    private static async Task<JsonNode> ReadAsync(
        HttpResponseMessage response, CancellationToken cancellationToken)
    {
        var body = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);

        if (!response.IsSuccessStatusCode)
        {
            // Google's error body names the actual problem (bad scope, quota, malformed
            // query). Surfacing only the status code would hide all of that.
            throw new InvalidOperationException(
                $"Google API returned {(int)response.StatusCode}: {Trim(body)}");
        }

        return JsonNode.Parse(body)
            ?? throw new InvalidOperationException("Google API returned an empty body.");
    }

    private static string Trim(string text) =>
        text.Length > 600 ? text[..600] + "..." : text;

    public async Task<JsonNode> GetAsync(string url, CancellationToken cancellationToken)
    {
        using var request = await AuthorizeAsync(HttpMethod.Get, url, cancellationToken).ConfigureAwait(false);
        using var response = await Http.SendAsync(request, cancellationToken).ConfigureAwait(false);
        return await ReadAsync(response, cancellationToken).ConfigureAwait(false);
    }

    public async Task<JsonNode> PostAsync(string url, JsonNode body, CancellationToken cancellationToken)
    {
        using var request = await AuthorizeAsync(HttpMethod.Post, url, cancellationToken).ConfigureAwait(false);
        request.Content = new StringContent(body.ToJsonString(), Encoding.UTF8, "application/json");
        using var response = await Http.SendAsync(request, cancellationToken).ConfigureAwait(false);
        return await ReadAsync(response, cancellationToken).ConfigureAwait(false);
    }

    public static string GmailUrl(string path, params (string Key, string? Value)[] query) =>
        BuildUrl($"{Gmail}{path}", query);

    public static string CalendarUrl(string path, params (string Key, string? Value)[] query) =>
        BuildUrl($"{Calendar}{path}", query);

    /// <summary>
    /// Escapes an id for use as a single URL path segment.
    ///
    /// Gmail ids are hex today, but they arrive as a tool argument and a model can be talked
    /// into passing anything. Interpolated raw, an id of "x/../../settings/forwarding" walks
    /// the API path to an endpoint the caller never named and the plan never described. The
    /// escape keeps a bad id a 404 instead.
    /// </summary>
    public static string Segment(string value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new ArgumentException("An empty id cannot address a Google resource.");
        }

        return Uri.EscapeDataString(value);
    }

    private static string BuildUrl(string baseUrl, (string Key, string? Value)[] query)
    {
        var parts = query
            .Where(q => !string.IsNullOrWhiteSpace(q.Value))
            .Select(q => $"{q.Key}={Uri.EscapeDataString(q.Value!)}")
            .ToArray();

        return parts.Length == 0 ? baseUrl : $"{baseUrl}?{string.Join('&', parts)}";
    }

    /// <summary>
    /// Builds an RFC 2822 message and base64url encodes it, which is what Gmail's
    /// drafts and send endpoints accept.
    /// </summary>
    public static string BuildRawMessage(string to, string subject, string body, string? cc)
    {
        // A CRLF in an address field would end the header line and let the caller add
        // headers of its own -- a Bcc to somewhere else, or a blank line that terminates the
        // header block and replaces the entire body. Either makes the plan the human approved
        // stop describing what is actually sent, and the address can arrive from an untrusted
        // email that mail.read pulled in, so this is a prompt-injection path rather than a
        // theoretical one. Subject is safe already because it is RFC 2047 encoded, and body
        // sits after the separator, so addresses are the only vector.
        var recipient = RequireHeaderSafe(to, nameof(to));
        var copyTo = string.IsNullOrWhiteSpace(cc) ? null : RequireHeaderSafe(cc, nameof(cc));

        var builder = new StringBuilder();
        builder.Append("To: ").Append(recipient).Append("\r\n");
        if (copyTo is not null) { builder.Append("Cc: ").Append(copyTo).Append("\r\n"); }
        // Encoded-word form, so a non-ASCII subject does not corrupt the header.
        builder.Append("Subject: =?utf-8?B?")
            .Append(Convert.ToBase64String(Encoding.UTF8.GetBytes(subject)))
            .Append("?=\r\n");
        builder.Append("MIME-Version: 1.0\r\n");
        builder.Append("Content-Type: text/plain; charset=utf-8\r\n\r\n");
        builder.Append(body);

        return Convert.ToBase64String(Encoding.UTF8.GetBytes(builder.ToString()))
            .TrimEnd('=').Replace('+', '-').Replace('/', '_');
    }

    /// <summary>
    /// Rejects anything that could break out of a single header line. Control characters are
    /// refused wholesale rather than stripped, because silently rewriting an address changes
    /// who receives the mail without telling anyone.
    /// </summary>
    private static string RequireHeaderSafe(string value, string field)
    {
        if (value.Any(c => c is '\r' or '\n' or '\0' || char.IsControl(c)))
        {
            throw new ArgumentException(
                $"'{field}' contains a control character, so it cannot be used as an email "
                + "header. This is how a message body or an extra recipient gets smuggled in.");
        }

        return value.Trim();
    }

    /// <summary>Pulls a header value out of a Gmail message payload, case-insensitively.</summary>
    public static string? Header(JsonNode? message, string name)
    {
        if (message?["payload"]?["headers"] is not JsonArray headers) { return null; }

        foreach (var header in headers)
        {
            if (string.Equals(header?["name"]?.GetValue<string>(), name, StringComparison.OrdinalIgnoreCase))
            {
                return header?["value"]?.GetValue<string>();
            }
        }

        return null;
    }

    /// <summary>
    /// Walks a Gmail payload for the first text/plain part and decodes it. Gmail nests
    /// parts arbitrarily for multipart mail, so a single-level read misses most bodies.
    /// </summary>
    public static string? ExtractPlainBody(JsonNode? payload)
    {
        if (payload is null) { return null; }

        var mimeType = payload["mimeType"]?.GetValue<string>();
        var data = payload["body"]?["data"]?.GetValue<string>();

        if (mimeType == "text/plain" && !string.IsNullOrEmpty(data)) { return DecodeBase64Url(data); }

        if (payload["parts"] is JsonArray parts)
        {
            foreach (var part in parts)
            {
                var found = ExtractPlainBody(part);
                if (found is not null) { return found; }
            }
        }

        // Fall back to whatever body exists (often text/html) rather than returning nothing.
        return string.IsNullOrEmpty(data) ? null : DecodeBase64Url(data);
    }

    private static string DecodeBase64Url(string value)
    {
        var padded = value.Replace('-', '+').Replace('_', '/');
        padded += new string('=', (4 - padded.Length % 4) % 4);
        try { return Encoding.UTF8.GetString(Convert.FromBase64String(padded)); }
        catch (FormatException) { return "<undecodable body>"; }
    }
}
