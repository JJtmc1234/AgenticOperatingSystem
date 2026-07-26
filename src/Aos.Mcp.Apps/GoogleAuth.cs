using System.Diagnostics;
using System.Net;
using System.Net.Http.Json;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Aos.Mcp.Apps;

/// <summary>OAuth client details, supplied once by the user from Google Cloud Console.</summary>
public sealed class GoogleClientSecrets
{
    public string ClientId { get; set; } = string.Empty;
    public string ClientSecret { get; set; } = string.Empty;
}

public sealed class StoredToken
{
    [JsonPropertyName("refresh_token")] public string RefreshToken { get; set; } = string.Empty;
    [JsonPropertyName("access_token")] public string AccessToken { get; set; } = string.Empty;
    [JsonPropertyName("expires_at")] public DateTimeOffset ExpiresAt { get; set; }
}

/// <summary>
/// Google OAuth for an installed application, plus token storage.
///
/// The refresh token is a long-lived credential to a person's mail and calendar, so it is
/// encrypted with DPAPI at the current-user scope. That ties the file to this Windows
/// account: copying it to another machine or another user yields ciphertext they cannot
/// read. A plaintext token file in %LOCALAPPDATA% would be readable by anything running
/// as this user, which is a poor trade for a few lines of code.
/// </summary>
public sealed class GoogleAuth(string dataDirectory)
{
    /// <summary>
    /// Requested scopes. Deliberately excludes full mail access: reading, drafting and
    /// sending is the whole job, and gmail.modify would also permit deleting mail.
    /// </summary>
    public static readonly string[] Scopes =
    [
        "https://www.googleapis.com/auth/gmail.readonly",
        "https://www.googleapis.com/auth/gmail.compose",
        "https://www.googleapis.com/auth/gmail.send",
        "https://www.googleapis.com/auth/calendar.events",
    ];

    private const string AuthEndpoint = "https://accounts.google.com/o/oauth2/v2/auth";
    private const string TokenEndpoint = "https://oauth2.googleapis.com/token";

    private readonly string _secretsPath = Path.Combine(dataDirectory, "google-client.json");
    private readonly string _tokenPath = Path.Combine(dataDirectory, "google-token.dat");
    private static readonly byte[] Entropy = Encoding.UTF8.GetBytes("AgenticOS.GoogleToken.v1");

    public string SecretsPath => _secretsPath;
    public string TokenPath => _tokenPath;

    public bool HasClientSecrets => File.Exists(_secretsPath);
    public bool HasToken => File.Exists(_tokenPath);

    public GoogleClientSecrets LoadClientSecrets()
    {
        if (!HasClientSecrets)
        {
            throw new FileNotFoundException(
                $"No Google OAuth client at '{_secretsPath}'. Create a Desktop app OAuth "
                + "client in Google Cloud Console, then save it there as "
                + "{\"ClientId\":\"...\",\"ClientSecret\":\"...\"}.");
        }

        return JsonSerializer.Deserialize<GoogleClientSecrets>(File.ReadAllText(_secretsPath))
            ?? throw new InvalidOperationException($"'{_secretsPath}' is not valid JSON.");
    }

    // --- token storage ---------------------------------------------------------------

    private void SaveToken(StoredToken token)
    {
        Directory.CreateDirectory(dataDirectory);
        var plaintext = JsonSerializer.SerializeToUtf8Bytes(token);
        var encrypted = ProtectedData.Protect(plaintext, Entropy, DataProtectionScope.CurrentUser);
        File.WriteAllBytes(_tokenPath, encrypted);
    }

    private StoredToken? LoadToken()
    {
        if (!HasToken) { return null; }

        try
        {
            var plaintext = ProtectedData.Unprotect(
                File.ReadAllBytes(_tokenPath), Entropy, DataProtectionScope.CurrentUser);
            return JsonSerializer.Deserialize<StoredToken>(plaintext);
        }
        catch (CryptographicException)
        {
            // Written by a different Windows account, or corrupted. Either way it is
            // unusable, and saying so beats a confusing failure at the first API call.
            throw new InvalidOperationException(
                $"'{_tokenPath}' cannot be decrypted by this Windows account. Delete it and "
                + "run the login again.");
        }
    }

    // --- interactive login -----------------------------------------------------------

    /// <summary>
    /// Runs the loopback authorization-code flow with PKCE. Opens the browser and listens
    /// on a loopback port for the redirect.
    /// </summary>
    public async Task<string> LoginAsync(CancellationToken cancellationToken)
    {
        var secrets = LoadClientSecrets();

        var verifier = Base64Url(RandomNumberGenerator.GetBytes(32));
        var challenge = Base64Url(SHA256.HashData(Encoding.ASCII.GetBytes(verifier)));
        var state = Base64Url(RandomNumberGenerator.GetBytes(16));

        // Port 0 lets the OS pick a free port, which avoids colliding with whatever else
        // is listening. Google permits any loopback port for Desktop clients.
        using var listener = new HttpListener();
        var port = FreeLoopbackPort();
        var redirectUri = $"http://127.0.0.1:{port}/";
        listener.Prefixes.Add(redirectUri);
        listener.Start();

        var authUrl =
            $"{AuthEndpoint}?client_id={Uri.EscapeDataString(secrets.ClientId)}"
            + $"&redirect_uri={Uri.EscapeDataString(redirectUri)}"
            + "&response_type=code"
            + $"&scope={Uri.EscapeDataString(string.Join(' ', Scopes))}"
            + $"&code_challenge={challenge}&code_challenge_method=S256"
            + $"&state={state}"
            + "&access_type=offline&prompt=consent";

        Console.Error.WriteLine("Opening your browser to authorize AgenticOS...");
        Console.Error.WriteLine($"If nothing opens, visit:\n{authUrl}\n");
        Process.Start(new ProcessStartInfo(authUrl) { UseShellExecute = true })?.Dispose();

        var context = await listener.GetContextAsync().WaitAsync(
            TimeSpan.FromMinutes(5), cancellationToken).ConfigureAwait(false);

        var query = context.Request.QueryString;
        var code = query["code"];
        var returnedState = query["state"];
        var error = query["error"];

        await RespondAsync(context, error is null && code is not null
            ? "AgenticOS is authorized. You can close this tab."
            : $"Authorization failed: {error ?? "no code returned"}.").ConfigureAwait(false);
        listener.Stop();

        if (error is not null) { throw new InvalidOperationException($"Google returned '{error}'."); }
        if (code is null) { throw new InvalidOperationException("Google returned no authorization code."); }

        // Guards against a forged redirect landing a foreign code in our listener.
        if (returnedState != state)
        {
            throw new InvalidOperationException("OAuth state mismatch; the redirect was not ours.");
        }

        using var http = new HttpClient();
        var response = await http.PostAsync(TokenEndpoint, new FormUrlEncodedContent(
            new Dictionary<string, string>
            {
                ["code"] = code,
                ["client_id"] = secrets.ClientId,
                ["client_secret"] = secrets.ClientSecret,
                ["redirect_uri"] = redirectUri,
                ["grant_type"] = "authorization_code",
                ["code_verifier"] = verifier,
            }), cancellationToken).ConfigureAwait(false);

        var payload = await ReadTokenResponseAsync(response, cancellationToken).ConfigureAwait(false);

        if (string.IsNullOrEmpty(payload.RefreshToken))
        {
            throw new InvalidOperationException(
                "Google did not return a refresh token. Revoke AgenticOS at "
                + "myaccount.google.com/permissions and log in again.");
        }

        SaveToken(new StoredToken
        {
            RefreshToken = payload.RefreshToken,
            AccessToken = payload.AccessToken ?? string.Empty,
            ExpiresAt = DateTimeOffset.UtcNow.AddSeconds(payload.ExpiresIn - 60),
        });

        return "Authorized. The refresh token is encrypted for this Windows account only.";
    }

    /// <summary>Returns a valid access token, refreshing it when it is close to expiry.</summary>
    public async Task<string> GetAccessTokenAsync(CancellationToken cancellationToken)
    {
        var token = LoadToken()
            ?? throw new InvalidOperationException(
                "Not authorized with Google yet. Run 'aos-mcp-apps.exe --login' once.");

        if (!string.IsNullOrEmpty(token.AccessToken) && token.ExpiresAt > DateTimeOffset.UtcNow)
        {
            return token.AccessToken;
        }

        var secrets = LoadClientSecrets();
        using var http = new HttpClient();
        var response = await http.PostAsync(TokenEndpoint, new FormUrlEncodedContent(
            new Dictionary<string, string>
            {
                ["client_id"] = secrets.ClientId,
                ["client_secret"] = secrets.ClientSecret,
                ["refresh_token"] = token.RefreshToken,
                ["grant_type"] = "refresh_token",
            }), cancellationToken).ConfigureAwait(false);

        var payload = await ReadTokenResponseAsync(response, cancellationToken).ConfigureAwait(false);

        token.AccessToken = payload.AccessToken
            ?? throw new InvalidOperationException("Refresh returned no access token.");
        token.ExpiresAt = DateTimeOffset.UtcNow.AddSeconds(payload.ExpiresIn - 60);
        // Google may rotate the refresh token; keep the new one when it does.
        if (!string.IsNullOrEmpty(payload.RefreshToken)) { token.RefreshToken = payload.RefreshToken; }
        SaveToken(token);

        return token.AccessToken;
    }

    private static async Task<TokenResponse> ReadTokenResponseAsync(
        HttpResponseMessage response, CancellationToken cancellationToken)
    {
        var body = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            // The body carries Google's error description; the status alone is not actionable.
            throw new InvalidOperationException(
                $"Google token endpoint returned {(int)response.StatusCode}: {body}");
        }

        return JsonSerializer.Deserialize<TokenResponse>(body)
            ?? throw new InvalidOperationException("Google token response was not valid JSON.");
    }

    private static async Task RespondAsync(HttpListenerContext context, string message)
    {
        var html = Encoding.UTF8.GetBytes(
            $"<!doctype html><meta charset=utf-8><title>AgenticOS</title>"
            + $"<body style='font:16px system-ui;padding:3rem'>{message}</body>");
        context.Response.ContentType = "text/html; charset=utf-8";
        context.Response.ContentLength64 = html.Length;
        await context.Response.OutputStream.WriteAsync(html).ConfigureAwait(false);
        context.Response.Close();
    }

    private static int FreeLoopbackPort()
    {
        var probe = new System.Net.Sockets.TcpListener(IPAddress.Loopback, 0);
        probe.Start();
        var port = ((IPEndPoint)probe.LocalEndpoint).Port;
        probe.Stop();
        return port;
    }

    private static string Base64Url(byte[] bytes) =>
        Convert.ToBase64String(bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_');

    private sealed class TokenResponse
    {
        [JsonPropertyName("access_token")] public string? AccessToken { get; set; }
        [JsonPropertyName("refresh_token")] public string? RefreshToken { get; set; }
        [JsonPropertyName("expires_in")] public int ExpiresIn { get; set; }
    }
}
