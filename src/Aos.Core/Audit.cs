using System.Text.Json.Nodes;
using System.Text.Json.Serialization;

namespace Aos.Core;

/// <summary>
/// One line of the append-only audit log. Every brokered call produces exactly one entry,
/// including denials and failures — a gap in this log is a bug.
/// </summary>
public sealed record AuditEntry
{
    public required DateTimeOffset Timestamp { get; init; }
    public required string CorrelationId { get; init; }
    public required string CapabilityId { get; init; }
    public required RiskTier Tier { get; init; }
    public required PolicyVerdict Verdict { get; init; }
    public required OutcomeStatus Status { get; init; }

    /// <summary>True when policy allowed the call but forbade committing.</summary>
    public required bool DryRun { get; init; }

    public required long DurationMs { get; init; }

    /// <summary>Arguments after redaction. Never write raw secrets here.</summary>
    public JsonObject? Arguments { get; init; }

    public string? Reason { get; init; }
    public string? Message { get; init; }

    /// <summary>VSS shadow copy id, when one was taken before committing.</summary>
    public string? SnapshotId { get; init; }

    public string User { get; init; } = Environment.UserName;
    public string Machine { get; init; } = Environment.MachineName;
    public int ProcessId { get; init; } = Environment.ProcessId;
}

/// <summary>
/// Append-only audit destination. Implemented in Aos.Broker as JSONL on disk.
/// Writes must not be silently droppable: if auditing fails, the call fails.
/// </summary>
public interface IAuditSink
{
    Task WriteAsync(AuditEntry entry, CancellationToken cancellationToken);
}

/// <summary>Serializer settings shared by the audit log and MCP payloads.</summary>
public static class AosJson
{
    public static readonly System.Text.Json.JsonSerializerOptions Options = new()
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        Converters = { new JsonStringEnumConverter() },
        WriteIndented = false,
    };
}
