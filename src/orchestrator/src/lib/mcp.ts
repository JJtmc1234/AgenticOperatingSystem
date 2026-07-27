import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { existsSync } from 'node:fs';

/**
 * A minimal MCP stdio client, for routines that call capabilities directly.
 *
 * Why this exists alongside agent.ts. That app hands the servers to the Claude Agent SDK
 * because a person is typing English at it and something has to decide what to call. A
 * routine has already decided: "trash these eleven files". Putting a model in that path buys
 * nothing and costs tokens, latency, and determinism, and a scheduled 07:30 job is exactly
 * where nondeterminism is least welcome.
 *
 * What it does not do is bypass the broker. Every call still goes through the same
 * capability server, so risk tiers, allowed roots, the plan-then-commit handshake, staged
 * trash, and the audit entry all apply. The routine is a different caller, not a shortcut.
 */

const AOS_ROOT = join(
  process.env['LOCALAPPDATA'] ?? join(homedir(), 'AppData', 'Local'),
  'AgenticOS',
);
const BIN = join(AOS_ROOT, 'bin');

export interface CapabilityOutcome {
  /** Succeeded, DryRun, Denied, Failed, or AppliedButUnverified. */
  status: string;
  message: string | null;
  result: unknown;
}

interface JsonRpcResponse {
  id?: number;
  result?: { content?: { type?: string; text?: string }[]; isError?: boolean };
  error?: { code?: number; message?: string };
}

export class McpServerError extends Error {}

export class McpClient {
  private readonly child: ChildProcessWithoutNullStreams;
  private nextId = 1;
  private buffer = '';
  private readonly waiting = new Map<number, (response: JsonRpcResponse) => void>();
  private exited: string | null = null;

  private constructor(child: ChildProcessWithoutNullStreams) {
    this.child = child;

    this.child.stdout.setEncoding('utf8');
    this.child.stdout.on('data', (chunk: string) => this.consume(chunk));

    // Drained and discarded. stderr is where these servers log, and an undrained pipe
    // eventually fills and blocks the server mid-write, which presents as a hang with no
    // explanation. Kept as a tail so a crash can say something useful.
    this.child.stderr.setEncoding('utf8');
    this.child.stderr.on('data', (chunk: string) => {
      this.stderrTail = (this.stderrTail + chunk).slice(-4000);
    });

    // Anything still waiting must be failed rather than left pending. A server that dies
    // mid-call would otherwise hang the routine forever on a promise nobody can resolve.
    this.child.on('exit', (code, signal) => {
      this.exited = `server exited with code ${code ?? 'null'}${signal ? `, signal ${signal}` : ''}`;
      this.failAllWaiting();
    });
    this.child.on('error', (error) => {
      this.exited = `server could not be started: ${error.message}`;
      this.failAllWaiting();
    });
  }

  private stderrTail = '';

  /** Starts a published server by exe name and completes the MCP handshake. */
  static async start(exeName: string): Promise<McpClient> {
    const command = join(BIN, exeName);
    if (!existsSync(command)) {
      throw new McpServerError(
        `${exeName} is not published at ${command}. Run provisioning/Install-Aos.ps1.`,
      );
    }

    const child = spawn(command, [], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    const client = new McpClient(child as ChildProcessWithoutNullStreams);

    await client.request('initialize', {
      protocolVersion: '2024-11-05',
      capabilities: {},
      clientInfo: { name: 'aos-orchestrator', version: '1' },
    });
    client.notify('notifications/initialized');

    return client;
  }

  private failAllWaiting(): void {
    const reason = this.exited ?? 'server closed';
    for (const [id, resolve] of this.waiting) {
      resolve({ id, error: { message: `${reason}. Last stderr:\n${this.stderrTail.trim()}` } });
    }
    this.waiting.clear();
  }

  private consume(chunk: string): void {
    this.buffer += chunk;

    // Framing is one JSON object per line. Splitting on every newline and keeping the tail
    // matters because a single read can deliver a partial line or several whole ones.
    let newline = this.buffer.indexOf('\n');
    while (newline >= 0) {
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      newline = this.buffer.indexOf('\n');

      if (line.length === 0) { continue; }

      let parsed: JsonRpcResponse;
      try {
        parsed = JSON.parse(line) as JsonRpcResponse;
      } catch {
        // A log line that escaped onto stdout. Not fatal, and guessing at it would be worse.
        continue;
      }

      if (typeof parsed.id !== 'number') { continue; }
      const resolve = this.waiting.get(parsed.id);
      if (resolve) {
        this.waiting.delete(parsed.id);
        resolve(parsed);
      }
    }
  }

  private notify(method: string, params?: unknown): void {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  private request(method: string, params: unknown, timeoutMs = 60_000): Promise<JsonRpcResponse> {
    if (this.exited) { return Promise.reject(new McpServerError(this.exited)); }

    const id = this.nextId++;

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting.delete(id);
        reject(new McpServerError(`${method} did not answer within ${timeoutMs} ms.`));
      }, timeoutMs);

      this.waiting.set(id, (response) => {
        clearTimeout(timer);
        if (response.error) {
          reject(new McpServerError(response.error.message ?? 'unknown JSON-RPC error'));
          return;
        }
        resolve(response);
      });

      this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    });
  }

  /**
   * Calls one tool and parses the broker's outcome envelope.
   *
   * The status is returned rather than thrown on, deliberately. Denied and
   * AppliedButUnverified are answers a routine has to reason about, not exceptions: a tidy
   * pass over forty files should report the two the policy refused and still file the other
   * thirty eight.
   */
  async call(tool: string, args: Record<string, unknown> = {}): Promise<CapabilityOutcome> {
    const response = await this.request('tools/call', { name: tool, arguments: args });

    const text = (response.result?.content ?? [])
      .filter((block) => block.type === 'text')
      .map((block) => block.text ?? '')
      .join('');

    if (text.length === 0) {
      return { status: 'Failed', message: `${tool} returned no text content.`, result: null };
    }

    try {
      const parsed = JSON.parse(text) as Partial<CapabilityOutcome>;
      return {
        status: parsed.status ?? 'Failed',
        message: parsed.message ?? null,
        result: parsed.result ?? null,
      };
    } catch {
      // A tool that is not broker-gated (files_roots, files_capabilities) answers with plain
      // text rather than an outcome envelope.
      return { status: 'Succeeded', message: null, result: text };
    }
  }

  /**
   * Plans a mutating call, then commits it, over this one connection.
   *
   * The plan ledger lives in the server process, so the two calls must share a connection or
   * the commit is refused for having no plan to redeem. The arguments must also match
   * exactly, which is why the caller passes them once and this adds the flag.
   */
  async planThenCommit(
    tool: string,
    args: Record<string, unknown>,
    reason: string,
  ): Promise<{ plan: CapabilityOutcome; commit: CapabilityOutcome }> {
    const shared = { ...args, reason };
    const plan = await this.call(tool, shared);

    // Committing after a refused plan would either be denied by the ledger or, worse, act on
    // something the plan never described.
    if (plan.status !== 'DryRun') {
      return { plan, commit: plan };
    }

    const commit = await this.call(tool, { ...shared, commit: true });
    return { plan, commit };
  }

  async close(): Promise<void> {
    if (this.exited) { return; }
    this.child.stdin.end();

    await new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        // Kill the tree. aos-shell can have children of its own, and orphans hold the
        // published DLLs, which then blocks the next provisioning publish.
        this.child.kill();
        resolve();
      }, 5_000);

      this.child.once('exit', () => { clearTimeout(timer); resolve(); });
    });
  }
}

/** Runs a body against a server and always closes it, even when the body throws. */
export async function withServer<T>(
  exeName: string,
  body: (client: McpClient) => Promise<T>,
): Promise<T> {
  const client = await McpClient.start(exeName);
  try {
    return await body(client);
  } finally {
    await client.close();
  }
}
