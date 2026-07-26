using System.Text.Json.Nodes;
using Aos.Core;
using Aos.Mcp.Shared;

namespace Aos.Mcp.Apps;

/// <summary>Gmail and Google Calendar, behind the broker like everything else.</summary>
internal sealed class AppsSurface(GoogleAuth auth, GoogleClient google)
{
    private static readonly CapabilitySet Set = new("aos-apps");

    public IEnumerable<ICapability> All()
    {
        yield return Set.Read(
            "aos-apps/auth.status",
            "Report whether Google credentials and an authorization token are present. "
            + "Call this first if any other aos-apps tool fails.",
            _ => new JsonObject
            {
                ["clientConfigured"] = auth.HasClientSecrets,
                ["authorized"] = auth.HasToken,
                ["clientPath"] = auth.SecretsPath,
                ["scopes"] = new JsonArray([.. GoogleAuth.Scopes.Select(s => JsonValue.Create(s))]),
                ["note"] = !auth.HasClientSecrets
                    ? "Create a Desktop app OAuth client in Google Cloud Console and save it to clientPath."
                    : !auth.HasToken
                        ? "Client is configured but not authorized. Run aos-mcp-apps.exe --login once."
                        : "Ready.",
            });

        yield return Set.ReadAsync(
            "aos-apps/mail.list",
            "List recent Gmail messages with sender, subject, date and snippet. Supports "
            + "Gmail search syntax, for example 'is:unread' or 'from:someone newer_than:7d'.",
            ListMailAsync);

        yield return Set.ReadAsync(
            "aos-apps/mail.read",
            "Read one Gmail message in full by its id from mail.list.",
            ReadMailAsync);

        yield return Set.ReadAsync(
            "aos-apps/calendar.list",
            "List upcoming Google Calendar events with start, end, title and attendees.",
            ListEventsAsync);

        yield return Set.MutatingAsync(
            "aos-apps/mail.draft",
            RiskTier.Write,
            "Create a Gmail draft. Nothing is sent, so this is the safe way to prepare a "
            + "reply for the user to review.",
            (args, _) => Task.FromResult(
                $"Create a draft to {args.RequireString("to")} with subject "
                + $"\"{args.RequireString("subject")}\". Nothing will be sent."),
            DraftMailAsync);

        yield return Set.MutatingAsync(
            "aos-apps/mail.send",
            // Destructive rather than Write: a sent email cannot be recalled, and it leaves
            // the machine. That is the strongest gate available, which is right for the only
            // capability here whose effect is visible to other people.
            RiskTier.Destructive,
            "Send an email immediately. Irreversible and visible to the recipient. Prefer "
            + "mail.draft unless the user explicitly asked for it to be sent.",
            (args, _) => Task.FromResult(
                $"SEND an email to {args.RequireString("to")} with subject "
                + $"\"{args.RequireString("subject")}\". This cannot be undone."),
            SendMailAsync,
            // A filesystem shadow copy cannot unsend an email, so demanding one would block
            // the capability on an unelevated host for no protection. The commit handshake
            // plus Destructive tier is the real guard.
            snapshot: false);

        yield return Set.MutatingAsync(
            "aos-apps/calendar.create",
            RiskTier.Write,
            "Create a Google Calendar event. Attendees are only invited when explicitly given.",
            (args, _) => Task.FromResult(
                $"Create event \"{args.RequireString("title")}\" from "
                + $"{args.RequireString("start")} to {args.RequireString("end")}."),
            CreateEventAsync);
    }

    // --- mail ------------------------------------------------------------------------

    private async Task<JsonNode?> ListMailAsync(JsonObject args, CancellationToken cancellationToken)
    {
        var query = args.GetString("query") ?? "in:inbox";
        var limit = Math.Clamp(args.GetInt32("limit", 10), 1, 50);

        var list = await google.GetAsync(
            GoogleClient.GmailUrl("/messages",
                ("q", query), ("maxResults", limit.ToString())),
            cancellationToken).ConfigureAwait(false);

        var messages = new JsonArray();

        if (list["messages"] is JsonArray ids)
        {
            foreach (var entry in ids)
            {
                var id = entry?["id"]?.GetValue<string>();
                if (id is null) { continue; }

                // metadata format keeps this cheap: headers and snippet, no bodies.
                var message = await google.GetAsync(
                    GoogleClient.GmailUrl($"/messages/{id}",
                        ("format", "metadata"),
                        ("metadataHeaders", "From"),
                        ("metadataHeaders", "Subject"),
                        ("metadataHeaders", "Date")),
                    cancellationToken).ConfigureAwait(false);

                messages.Add(new JsonObject
                {
                    ["id"] = id,
                    ["from"] = GoogleClient.Header(message, "From"),
                    ["subject"] = GoogleClient.Header(message, "Subject"),
                    ["date"] = GoogleClient.Header(message, "Date"),
                    ["unread"] = HasLabel(message, "UNREAD"),
                    ["snippet"] = message["snippet"]?.GetValue<string>(),
                });
            }
        }

        return new JsonObject
        {
            ["query"] = query,
            ["count"] = messages.Count,
            ["messages"] = messages,
        };
    }

    private static bool HasLabel(JsonNode? message, string label)
    {
        if (message?["labelIds"] is not JsonArray labels) { return false; }
        return labels.Any(l => l?.GetValue<string>() == label);
    }

    private async Task<JsonNode?> ReadMailAsync(JsonObject args, CancellationToken cancellationToken)
    {
        var id = args.RequireString("id");
        var message = await google.GetAsync(
            GoogleClient.GmailUrl($"/messages/{id}", ("format", "full")),
            cancellationToken).ConfigureAwait(false);

        var body = GoogleClient.ExtractPlainBody(message["payload"]) ?? string.Empty;
        var maxChars = Math.Clamp(args.GetInt32("maxChars", 20_000), 200, 200_000);
        var truncated = body.Length > maxChars;

        return new JsonObject
        {
            ["id"] = id,
            ["from"] = GoogleClient.Header(message, "From"),
            ["to"] = GoogleClient.Header(message, "To"),
            ["subject"] = GoogleClient.Header(message, "Subject"),
            ["date"] = GoogleClient.Header(message, "Date"),
            ["truncated"] = truncated,
            ["body"] = truncated ? body[..maxChars] : body,
        };
    }

    private async Task<JsonNode?> DraftMailAsync(JsonObject args, CancellationToken cancellationToken)
    {
        var raw = GoogleClient.BuildRawMessage(
            args.RequireString("to"),
            args.RequireString("subject"),
            args.RequireString("body"),
            args.GetString("cc"));

        var created = await google.PostAsync(
            GoogleClient.GmailUrl("/drafts"),
            new JsonObject { ["message"] = new JsonObject { ["raw"] = raw } },
            cancellationToken).ConfigureAwait(false);

        return new JsonObject
        {
            ["draftId"] = created["id"]?.GetValue<string>(),
            ["to"] = args.RequireString("to"),
            ["subject"] = args.RequireString("subject"),
            ["note"] = "Saved as a draft. Nothing was sent.",
        };
    }

    private async Task<JsonNode?> SendMailAsync(JsonObject args, CancellationToken cancellationToken)
    {
        var raw = GoogleClient.BuildRawMessage(
            args.RequireString("to"),
            args.RequireString("subject"),
            args.RequireString("body"),
            args.GetString("cc"));

        var sent = await google.PostAsync(
            GoogleClient.GmailUrl("/messages/send"),
            new JsonObject { ["raw"] = raw },
            cancellationToken).ConfigureAwait(false);

        return new JsonObject
        {
            ["messageId"] = sent["id"]?.GetValue<string>(),
            ["to"] = args.RequireString("to"),
            ["subject"] = args.RequireString("subject"),
        };
    }

    // --- calendar --------------------------------------------------------------------

    private async Task<JsonNode?> ListEventsAsync(JsonObject args, CancellationToken cancellationToken)
    {
        var days = Math.Clamp(args.GetInt32("days", 7), 1, 90);
        var limit = Math.Clamp(args.GetInt32("limit", 20), 1, 100);
        var calendarId = args.GetString("calendarId") ?? "primary";

        var now = DateTimeOffset.UtcNow;
        var result = await google.GetAsync(
            GoogleClient.CalendarUrl($"/calendars/{Uri.EscapeDataString(calendarId)}/events",
                ("timeMin", now.ToString("o")),
                ("timeMax", now.AddDays(days).ToString("o")),
                ("singleEvents", "true"),
                ("orderBy", "startTime"),
                ("maxResults", limit.ToString())),
            cancellationToken).ConfigureAwait(false);

        var events = new JsonArray();

        if (result["items"] is JsonArray items)
        {
            foreach (var item in items)
            {
                events.Add(new JsonObject
                {
                    ["id"] = item?["id"]?.GetValue<string>(),
                    ["title"] = item?["summary"]?.GetValue<string>(),
                    // All-day events carry "date"; timed events carry "dateTime".
                    ["start"] = item?["start"]?["dateTime"]?.GetValue<string>()
                        ?? item?["start"]?["date"]?.GetValue<string>(),
                    ["end"] = item?["end"]?["dateTime"]?.GetValue<string>()
                        ?? item?["end"]?["date"]?.GetValue<string>(),
                    ["location"] = item?["location"]?.GetValue<string>(),
                    ["attendees"] = item?["attendees"] is JsonArray attendees
                        ? new JsonArray([.. attendees.Select(a =>
                            JsonValue.Create(a?["email"]?.GetValue<string>()))])
                        : null,
                });
            }
        }

        return new JsonObject
        {
            ["calendarId"] = calendarId,
            ["windowDays"] = days,
            ["count"] = events.Count,
            ["events"] = events,
        };
    }

    private async Task<JsonNode?> CreateEventAsync(JsonObject args, CancellationToken cancellationToken)
    {
        var calendarId = args.GetString("calendarId") ?? "primary";

        var body = new JsonObject
        {
            ["summary"] = args.RequireString("title"),
            ["start"] = new JsonObject { ["dateTime"] = args.RequireString("start") },
            ["end"] = new JsonObject { ["dateTime"] = args.RequireString("end") },
        };

        var description = args.GetString("description");
        if (description is not null) { body["description"] = description; }

        var location = args.GetString("location");
        if (location is not null) { body["location"] = location; }

        if (args["attendees"] is JsonArray attendees && attendees.Count > 0)
        {
            body["attendees"] = new JsonArray([.. attendees
                .Select(a => a?.GetValue<string>())
                .Where(email => !string.IsNullOrWhiteSpace(email))
                .Select(email => (JsonNode)new JsonObject { ["email"] = email })]);
        }

        var created = await google.PostAsync(
            GoogleClient.CalendarUrl($"/calendars/{Uri.EscapeDataString(calendarId)}/events"),
            body, cancellationToken).ConfigureAwait(false);

        return new JsonObject
        {
            ["eventId"] = created["id"]?.GetValue<string>(),
            ["title"] = args.RequireString("title"),
            ["start"] = args.RequireString("start"),
            ["link"] = created["htmlLink"]?.GetValue<string>(),
        };
    }
}
