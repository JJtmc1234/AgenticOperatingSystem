import { createInterface } from 'node:readline/promises';
import { stdin, stdout } from 'node:process';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { existsSync } from 'node:fs';
import {
  query,
  type CanUseTool,
  type McpServerConfig,
  type PermissionResult,
  type SDKUserMessage,
} from '@anthropic-ai/claude-agent-sdk';

/**
 * The AgenticOS agent app.
 *
 * This uses the Claude Agent SDK rather than the plain Messages API for one concrete
 * reason: our capability servers speak MCP over stdio, and the Messages API's mcp_servers
 * parameter only accepts remote url servers. The Agent SDK supplies the loop, context
 * management, and stdio MCP transport, so the harness we care about stays ours (the
 * broker) and the parts that are solved stay borrowed.
 */

const AOS_ROOT = join(
  process.env['LOCALAPPDATA'] ?? join(homedir(), 'AppData', 'Local'),
  'AgenticOS',
);
const BIN = join(AOS_ROOT, 'bin');

const MODEL = 'claude-opus-5';

const SERVERS: Record<string, string> = {
  'aos-windows': 'aos-mcp-windows.exe',
  'aos-files': 'aos-mcp-files.exe',
  'aos-shell': 'aos-mcp-shell.exe',
};

const SYSTEM_APPEND = `
You are running inside AgenticOS on this Windows machine, with the aos-windows,
aos-files and aos-shell capability servers available.

Those servers sit behind a safety broker, which matters for how you call them:

- Mutating tools take a commit flag. Called without it they return a plan and change
  nothing. Read the plan, confirm it is what the user asked for, then call again with
  commit set true. Do not skip the plan step on anything destructive.
- A result of AppliedButUnverified means the change landed but its post-condition check
  failed. Do not retry it blindly, since the change may already be in effect. Report it.
- Denied means policy refused. The message names the reason. Do not try to route around it.
- Prefer ui_tree over screenshots for reading a window: it is faster and exact.
- Files work is restricted to the allowed roots in policy. Call files_roots if unsure.
`.trim();

function buildServers(): Record<string, McpServerConfig> {
  const servers: Record<string, McpServerConfig> = {};
  const missing: string[] = [];

  for (const [name, exe] of Object.entries(SERVERS)) {
    const command = join(BIN, exe);
    if (existsSync(command)) {
      // alwaysLoad keeps these tools in the turn-1 prompt rather than deferring them
      // behind tool search. They are the whole point of this app, so paying the startup
      // connect cost is the right trade.
      servers[name] = { type: 'stdio', command, args: [], alwaysLoad: true };
    } else {
      missing.push(`${name} (${command})`);
    }
  }

  if (missing.length > 0) {
    console.error('These capability servers are not published:');
    for (const entry of missing) { console.error(`  ${entry}`); }
    console.error('Run provisioning/Install-Aos.ps1 to build and publish them.\n');
  }

  return servers;
}

/**
 * Our MCP tools are auto-approved because the broker already gates them: risk tiers,
 * allowed roots, a command allowlist, the plan-then-commit handshake, and an audit
 * entry per call. Prompting twice for the same action trains people to click through.
 *
 * Everything else, including the SDK's own Bash and Write, gets asked about, because
 * nothing of ours is standing behind those.
 */
function makePermissionHandler(rl: ReturnType<typeof createInterface>): CanUseTool {
  const alwaysAllow = new Set<string>();

  return async (toolName, input): Promise<PermissionResult> => {
    if (toolName.startsWith('mcp__aos-')) {
      return { behavior: 'allow', updatedInput: input };
    }

    if (alwaysAllow.has(toolName)) {
      return { behavior: 'allow', updatedInput: input };
    }

    const summary = JSON.stringify(input).slice(0, 200);
    console.log(`\n  permission needed: ${toolName}`);
    console.log(`  input: ${summary}`);
    const answer = (await rl.question('  allow? [y]es / [n]o / [a]lways: ')).trim().toLowerCase();

    if (answer === 'a' || answer === 'always') {
      alwaysAllow.add(toolName);
      return { behavior: 'allow', updatedInput: input };
    }

    return answer === 'y' || answer === 'yes'
      ? { behavior: 'allow', updatedInput: input }
      : { behavior: 'deny', message: 'The user declined this tool call.' };
  };
}

/** Turns a queue of typed lines into the async iterable the SDK consumes. */
class InputQueue {
  private readonly pending: SDKUserMessage[] = [];
  private wake: (() => void) | null = null;
  private closed = false;

  push(text: string): void {
    this.pending.push({
      type: 'user',
      message: { role: 'user', content: text },
      parent_tool_use_id: null,
      session_id: '',
    } as SDKUserMessage);
    this.wake?.();
  }

  close(): void {
    this.closed = true;
    this.wake?.();
  }

  async *stream(): AsyncGenerator<SDKUserMessage> {
    while (true) {
      while (this.pending.length > 0) {
        yield this.pending.shift()!;
      }
      if (this.closed) { return; }
      await new Promise<void>((resolve) => { this.wake = resolve; });
      this.wake = null;
    }
  }
}

function renderAssistant(content: unknown): void {
  if (typeof content === 'string') {
    process.stdout.write(content);
    return;
  }
  if (!Array.isArray(content)) { return; }

  for (const block of content as { type?: string; text?: string; name?: string }[]) {
    if (block.type === 'text' && block.text) {
      process.stdout.write(block.text);
    } else if (block.type === 'tool_use' && block.name) {
      // Strip the mcp__server__ prefix so the trace reads like the tool names in policy.
      const shortName = block.name.replace(/^mcp__[^_]+(?:-[^_]+)*__/, '');
      process.stdout.write(`\n  [${shortName}]`);
    }
  }
}

async function main(): Promise<number> {
  const servers = buildServers();
  const rl = createInterface({ input: stdin, output: stdout });
  const queue = new InputQueue();

  console.log('AgenticOS agent');
  console.log(`model: ${MODEL}`);
  console.log(`capabilities: ${Object.keys(servers).join(', ') || 'none'}`);
  console.log('Type a request, or /exit to quit.\n');

  const session = query({
    prompt: queue.stream(),
    options: {
      model: MODEL,
      mcpServers: servers,
      systemPrompt: { type: 'preset', preset: 'claude_code', append: SYSTEM_APPEND },
      permissionMode: 'default',
      canUseTool: makePermissionHandler(rl),
      cwd: process.cwd(),
    },
  });

  // The SDK drains the input iterable, so the reader and the renderer have to run
  // concurrently rather than one feeding the other.
  const reader = (async () => {
    while (true) {
      let line: string;
      try {
        line = (await rl.question('> ')).trim();
      } catch {
        // stdin closed. Happens whenever input is piped rather than typed, so it has to
        // end the loop; treating it as a transient error hangs the process forever.
        break;
      }
      if (line.length === 0) { continue; }
      if (line === '/exit' || line === '/quit') { break; }
      queue.push(line);
    }
    queue.close();
    await session.close();
  })();

  try {
    for await (const message of session) {
      if (message.type === 'assistant') {
        renderAssistant(message.message.content);
      } else if (message.type === 'result') {
        const cost = 'total_cost_usd' in message ? message.total_cost_usd : undefined;
        const suffix = typeof cost === 'number' ? `, $${cost.toFixed(4)}` : '';
        process.stdout.write(`\n  [done in ${Math.round(message.duration_ms / 100) / 10}s${suffix}]\n\n`);
      }
    }
  } catch (error) {
    console.error('\nSession ended:', error instanceof Error ? error.message : error);
  }

  await reader;
  rl.close();
  return 0;
}

main().then(
  (code) => process.exit(code),
  (error) => { console.error(error); process.exit(1); },
);
