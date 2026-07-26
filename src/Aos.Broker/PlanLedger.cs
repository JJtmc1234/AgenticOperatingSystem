using System.Security.Cryptography;
using System.Text;
using System.Text.Json.Nodes;
using Aos.Core;

namespace Aos.Broker;

/// <summary>
/// Records which plans have actually been shown, so a commit can be checked against one.
///
/// Without this the handshake was a single call: the broker computed
/// dryRun = needsHandshake &amp;&amp; !request.Commit and nothing anywhere remembered whether a
/// plan had ever been produced, so a first-ever call with commit set true applied straight
/// away. The default approver made it circular by answering "approved" whenever
/// request.Commit was true, which is the flag the caller controls.
///
/// A commit now has to name a plan this process issued for the same capability and the same
/// arguments. That makes the two-call sequence a property of the harness rather than an
/// instruction the agent may follow.
/// </summary>
public sealed class PlanLedger(TimeProvider? clock = null)
{
    /// <summary>
    /// How long a shown plan stays committable. Long enough for a human to read an approval
    /// prompt, short enough that a stale plan cannot be committed against changed state.
    /// </summary>
    public static readonly TimeSpan Lifetime = TimeSpan.FromMinutes(10);

    private readonly TimeProvider _clock = clock ?? TimeProvider.System;
    private readonly Dictionary<string, DateTimeOffset> _plans = new(StringComparer.Ordinal);
    private readonly Lock _gate = new();

    /// <summary>Notes that a dry-run plan was produced for this exact request.</summary>
    public void RecordPlan(CapabilityRequest request)
    {
        var key = Fingerprint(request);
        lock (_gate)
        {
            Prune();
            _plans[key] = _clock.GetUtcNow();
        }
    }

    /// <summary>
    /// True when a live plan exists for this request. Consumes it, so one plan authorises
    /// one commit and a replayed commit has to show a fresh plan.
    /// </summary>
    public bool TryConsumePlan(CapabilityRequest request)
    {
        var key = Fingerprint(request);
        lock (_gate)
        {
            Prune();
            if (!_plans.TryGetValue(key, out var shownAt)) { return false; }
            if (_clock.GetUtcNow() - shownAt > Lifetime) { _plans.Remove(key); return false; }
            _plans.Remove(key);
            return true;
        }
    }

    private void Prune()
    {
        var cutoff = _clock.GetUtcNow() - Lifetime;
        foreach (var key in _plans.Where(p => p.Value < cutoff).Select(p => p.Key).ToArray())
        {
            _plans.Remove(key);
        }
    }

    /// <summary>
    /// Identity of a request for handshake purposes: the capability plus its arguments,
    /// excluding the commit flag and the free-text reason. Argument order must not matter,
    /// or a reordered but identical commit would be refused.
    /// </summary>
    public static string Fingerprint(CapabilityRequest request)
    {
        var canonical = new StringBuilder(request.CapabilityId).Append('');
        AppendCanonical(canonical, request.Arguments);

        return Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes(canonical.ToString())));
    }

    private static void AppendCanonical(StringBuilder builder, JsonNode? node)
    {
        switch (node)
        {
            case JsonObject obj:
                builder.Append('{');
                foreach (var (key, value) in obj.OrderBy(p => p.Key, StringComparer.Ordinal))
                {
                    builder.Append(key).Append(':');
                    AppendCanonical(builder, value);
                    builder.Append(',');
                }
                builder.Append('}');
                break;

            case JsonArray array:
                builder.Append('[');
                foreach (var item in array)
                {
                    AppendCanonical(builder, item);
                    builder.Append(',');
                }
                builder.Append(']');
                break;

            case null:
                builder.Append("null");
                break;

            default:
                builder.Append(node.ToJsonString());
                break;
        }
    }
}
