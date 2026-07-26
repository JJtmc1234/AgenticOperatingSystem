using Aos.Mcp.Files;
using Aos.Mcp.Shared;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

// stdio is the MCP transport, so anything on stdout corrupts the protocol stream.
// All logging goes to stderr.
var builder = Host.CreateApplicationBuilder(args);

builder.Logging.ClearProviders();
builder.Logging.AddConsole(options => options.LogToStandardErrorThreshold = LogLevel.Trace);

var policy = AosPaths.LoadPolicy();
var guard = AosPaths.GuardFrom(policy);
Directory.CreateDirectory(AosPaths.TrashDirectory);
var surface = new FileSurface(guard, new TrashStore(AosPaths.TrashDirectory));

builder.Services.AddSingleton(AosPaths.BuildBroker(surface.All(), policy));

builder.Services
    .AddMcpServer()
    .WithStdioServerTransport()
    .WithTools<FilesTools>();

await builder.Build().RunAsync();
