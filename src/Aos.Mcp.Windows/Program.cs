using Aos.Mcp.Windows;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

// stdio is the MCP transport, so anything on stdout corrupts the protocol stream.
// All logging goes to stderr.
var builder = Host.CreateApplicationBuilder(args);

builder.Logging.ClearProviders();
builder.Logging.AddConsole(options => options.LogToStandardErrorThreshold = LogLevel.Trace);

builder.Services.AddSingleton(AosHost.BuildBroker());

builder.Services
    .AddMcpServer()
    .WithStdioServerTransport()
    .WithTools<WindowsTools>();

await builder.Build().RunAsync();
