using System.ComponentModel;
using Aos.Broker;
using Aos.Core;
using Aos.Mcp.Shared;
using ModelContextProtocol.Server;

namespace Aos.Mcp.Apps;

[McpServerToolType]
public sealed class AppsTools(CapabilityBroker broker)
{
    [McpServerTool(Name = "google_status")]
    [Description("Check whether Google credentials and authorization are in place. Call this "
        + "first if any other Google tool fails.")]
    public Task<string> Status(CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-apps/auth.status", new(), cancellationToken: cancellationToken);

    [McpServerTool(Name = "mail_list")]
    [Description("List recent Gmail messages with sender, subject, date and snippet. Accepts "
        + "Gmail search syntax such as 'is:unread', 'from:alice newer_than:7d', 'has:attachment'.")]
    public Task<string> MailList(
        [Description("Gmail search query. Defaults to in:inbox.")] string? query = null,
        [Description("Maximum messages to return. Default 10, max 50.")] int limit = 10,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-apps/mail.list",
            JsonArgs.Of(("query", query), ("limit", limit)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "mail_read")]
    [Description("Read one Gmail message in full, including its plain text body.")]
    public Task<string> MailRead(
        [Description("Message id from mail_list.")] string id,
        [Description("Truncate the body at this many characters. Default 20000.")] int maxChars = 20_000,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-apps/mail.read",
            JsonArgs.Of(("id", id), ("maxChars", maxChars)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "mail_draft")]
    [Description("Create a Gmail draft without sending it. This is the safe way to prepare a "
        + "reply for review. Returns a plan unless commit is true.")]
    public Task<string> MailDraft(
        [Description("Recipient address.")] string to,
        [Description("Subject line.")] string subject,
        [Description("Plain text body.")] string body,
        [Description("Optional Cc address.")] string? cc = null,
        [Description("Set true to actually create the draft.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-apps/mail.draft",
            JsonArgs.Of(("to", to), ("subject", subject), ("body", body), ("cc", cc)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "mail_send")]
    [Description("Send an email immediately. Irreversible and visible to the recipient, so "
        + "prefer mail_draft unless the user explicitly asked to send. Returns a plan unless "
        + "commit is true.")]
    public Task<string> MailSend(
        [Description("Recipient address.")] string to,
        [Description("Subject line.")] string subject,
        [Description("Plain text body.")] string body,
        [Description("Optional Cc address.")] string? cc = null,
        [Description("Set true to actually send it. This cannot be undone.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-apps/mail.send",
            JsonArgs.Of(("to", to), ("subject", subject), ("body", body), ("cc", cc)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "calendar_list")]
    [Description("List upcoming Google Calendar events with start, end, title and attendees.")]
    public Task<string> CalendarList(
        [Description("How many days ahead to look. Default 7, max 90.")] int days = 7,
        [Description("Maximum events to return. Default 20.")] int limit = 20,
        [Description("Calendar id. Defaults to the primary calendar.")] string? calendarId = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-apps/calendar.list",
            JsonArgs.Of(("days", days), ("limit", limit), ("calendarId", calendarId)),
            cancellationToken: cancellationToken);

    [McpServerTool(Name = "calendar_create")]
    [Description("Create a Google Calendar event. Times are RFC3339, for example "
        + "2026-07-27T14:00:00-04:00. Returns a plan unless commit is true.")]
    public Task<string> CalendarCreate(
        [Description("Event title.")] string title,
        [Description("Start time, RFC3339 with offset.")] string start,
        [Description("End time, RFC3339 with offset.")] string end,
        [Description("Optional description.")] string? description = null,
        [Description("Optional location.")] string? location = null,
        [Description("Attendee email addresses. Only these people are invited.")]
        string[]? attendees = null,
        [Description("Calendar id. Defaults to the primary calendar.")] string? calendarId = null,
        [Description("Set true to actually create the event.")] bool commit = false,
        [Description("Why this is being done. Recorded in the audit log.")] string? reason = null,
        CancellationToken cancellationToken = default) =>
        broker.CallAsync("aos-apps/calendar.create",
            JsonArgs.Of(
                ("title", title), ("start", start), ("end", end),
                ("description", description), ("location", location),
                ("attendees", JsonArgs.ArrayOf(attendees)), ("calendarId", calendarId)),
            commit, reason, cancellationToken);

    [McpServerTool(Name = "apps_capabilities")]
    [Description("List registered Google capabilities with their risk tiers.")]
    public string Capabilities() => broker.DescribeCapabilities();
}
