using System.IO;
using Aos.Mcp.Shared;
using Aos.Mcp.Windows;
using Aos.Mcp.Windows.Capabilities;
using Aos.Mcp.Windows.Native;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

// First statement in the process, deliberately. The DPI awareness mode is latched by the
// first window or GDI call, so this has to beat the host builder to it.
User32.EnableDpiAwareness();

// stdio is the MCP transport, so anything on stdout corrupts the protocol stream.
// All logging goes to stderr.
var builder = Host.CreateApplicationBuilder(args);

builder.Logging.ClearProviders();
builder.Logging.AddConsole(options => options.LogToStandardErrorThreshold = LogLevel.Trace);

var policy = AosPaths.LoadPolicy();
var capabilities = ShellSurface
    .All(Path.Combine(AosPaths.DataDirectory, "screens"))
    .Concat(UiaSurface.All());

builder.Services.AddSingleton(AosPaths.BuildBroker(capabilities, policy));

builder.Services
    .AddMcpServer()
    .WithStdioServerTransport()
    .WithTools<WindowsTools>();

await builder.Build().RunAsync();
