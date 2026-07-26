import { execFile } from 'node:child_process';

export interface RunResult {
  ok: boolean;
  stdout: string;
  stderr: string;
  code: number | null;
  /**
   * Set when the command did not simply exit non-zero: it was killed for exceeding the
   * timeout, or its output overflowed the buffer.
   *
   * Both used to collapse into `code: 1`, indistinguishable from a command that ran fine
   * and reported failure. That matters because the caller decides "unknown" versus "no",
   * and those are not the same answer.
   */
  failureKind?: 'timeout' | 'output-overflow' | 'spawn-error';
}

/**
 * Runs an executable with an argument list. No shell, for the same reason aos-shell uses
 * none: arguments stay inert data rather than becoming syntax.
 *
 * Never rejects. A routine that dies because one repo is in a odd state is useless, so
 * failures come back as data and the caller decides.
 */
export function run(
  command: string,
  args: string[],
  options: { cwd?: string; timeoutMs?: number } = {},
): Promise<RunResult> {
  return new Promise((resolve) => {
    execFile(
      command,
      args,
      {
        cwd: options.cwd,
        timeout: options.timeoutMs ?? 20_000,
        windowsHide: true,
        maxBuffer: 8 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        // Node reports an exit status as a numeric `code`, but uses a STRING code for its
        // own failures (ERR_CHILD_PROCESS_STDIO_MAXBUFFER), and signals a timeout kill via
        // `killed` with no useful code at all.
        const raw = error as (Error & { code?: unknown; killed?: boolean }) | null;
        const numericCode = typeof raw?.code === 'number' ? raw.code : null;

        let failureKind: RunResult['failureKind'];
        if (raw) {
          if (raw.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER') { failureKind = 'output-overflow'; }
          else if (raw.killed) { failureKind = 'timeout'; }
          else if (numericCode === null) { failureKind = 'spawn-error'; }
        }

        resolve({
          ok: !error,
          stdout: stdout?.toString() ?? '',
          stderr: stderr?.toString() ?? '',
          code: numericCode,
          failureKind,
        });
      },
    );
  });
}

/** Convenience for the common case of wanting trimmed stdout or a fallback. */
export async function runText(
  command: string,
  args: string[],
  options: { cwd?: string; fallback?: string } = {},
): Promise<string> {
  const result = await run(command, args, { cwd: options.cwd });
  return result.ok ? result.stdout.trim() : (options.fallback ?? '');
}
