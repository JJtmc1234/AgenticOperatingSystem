using System.Text;
using System.Text.Json;
using Aos.Core;

namespace Aos.Broker;

/// <summary>
/// Append-only JSONL audit log, one file per UTC day. Writes are serialized and flushed
/// per entry: a crash mid-session must not lose the record of what was already done.
/// </summary>
public sealed class JsonlAuditSink : IAuditSink, IDisposable
{
    private readonly string _directory;
    private readonly TimeProvider _clock;
    private readonly SemaphoreSlim _gate = new(1, 1);

    public JsonlAuditSink(string directory, TimeProvider? clock = null)
    {
        _directory = directory;
        _clock = clock ?? TimeProvider.System;
        Directory.CreateDirectory(_directory);
    }

    public string CurrentFile =>
        Path.Combine(_directory, $"audit-{_clock.GetUtcNow():yyyy-MM-dd}.jsonl");

    public async Task WriteAsync(AuditEntry entry, CancellationToken cancellationToken)
    {
        var line = JsonSerializer.Serialize(entry, AosJson.Options);

        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            // FileShare.Read so the HUD audit viewer can tail this while we write.
            await using var stream = new FileStream(
                CurrentFile, FileMode.Append, FileAccess.Write, FileShare.Read);
            await using var writer = new StreamWriter(stream, new UTF8Encoding(false));
            await writer.WriteLineAsync(line.AsMemory(), cancellationToken).ConfigureAwait(false);
            await writer.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
    }

    public void Dispose() => _gate.Dispose();
}
