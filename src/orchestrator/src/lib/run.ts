import { execFile } from 'node:child_process';

export interface RunResult {
  ok: boolean;
  stdout: string;
  stderr: string;
  code: number | null;
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
        const code =
          error && typeof (error as { code?: unknown }).code === 'number'
            ? ((error as { code: number }).code)
            : error
              ? 1
              : 0;

        resolve({
          ok: !error,
          stdout: stdout?.toString() ?? '',
          stderr: stderr?.toString() ?? '',
          code,
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
