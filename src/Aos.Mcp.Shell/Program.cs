using Aos.Mcp.Shared;
using Aos.Mcp.Shell;
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
var runner = new CommandRunner(policy.Document.AllowedCommands);
var surface = new ShellSurface(guard, runner);

builder.Services.AddSingleton(AosPaths.BuildBroker(surface.All(), policy));

builder.Services
    .AddMcpServer()
    .WithStdioServerTransport()
    .WithTools<ShellTools>();

await builder.Build().RunAsync();
