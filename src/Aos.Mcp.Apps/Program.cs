using Aos.Mcp.Apps;
using Aos.Mcp.Shared;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

var dataDirectory = AosPaths.DataDirectory;
Directory.CreateDirectory(dataDirectory);
var auth = new GoogleAuth(dataDirectory);

// --login runs the interactive browser flow once and exits. Kept out of the MCP tool surface
// on purpose: it opens a browser and blocks on a human, which is not something an agent
// should be able to trigger.
if (args.Contains("--login", StringComparer.OrdinalIgnoreCase))
{
    try
    {
        Console.WriteLine(await auth.LoginAsync(CancellationToken.None));
        return 0;
    }
    catch (Exception ex)
    {
        Console.Error.WriteLine($"Login failed: {ex.Message}");
        return 1;
    }
}

if (args.Contains("--status", StringComparer.OrdinalIgnoreCase))
{
    Console.WriteLine($"client configured: {auth.HasClientSecrets}  ({auth.SecretsPath})");
    Console.WriteLine($"authorized:        {auth.HasToken}");
    return 0;
}

// stdio is the MCP transport, so anything on stdout corrupts the protocol stream.
// All logging goes to stderr.
var builder = Host.CreateApplicationBuilder(args);

builder.Logging.ClearProviders();
builder.Logging.AddConsole(options => options.LogToStandardErrorThreshold = LogLevel.Trace);

var policy = AosPaths.LoadPolicy();
var surface = new AppsSurface(auth, new GoogleClient(auth));

builder.Services.AddSingleton(AosPaths.BuildBroker(surface.All(), policy));

builder.Services
    .AddMcpServer()
    .WithStdioServerTransport()
    .WithTools<AppsTools>();

await builder.Build().RunAsync();
return 0;
