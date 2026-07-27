import { readdir, stat } from 'node:fs/promises';
import { join, extname } from 'node:path';
import { homedir } from 'node:os';
import { withServer, type CapabilityOutcome } from '../lib/mcp.js';

/**
 * Stages the Downloads folder: old installers and archives to trash, keepers filed by kind.
 *
 * The daily brief already reports this clutter and then asks a person to do something about
 * it, which is where every tidying tool dies. This one acts, through aos-files, so every
 * move and every trash lands in the audit log and stays reversible from staged trash.
 *
 * The rules below are deliberately dull and local rather than a model deciding per file.
 * "Which installer is older than sixty days" needs no judgment, and a wrong judgment here
 * moves someone's files around. A model is the right tool when the answer is unclear; this
 * answer is arithmetic.
 *
 * Nothing runs without --commit. Without it the routine prints exactly what it would do.
 */

const DOWNLOADS = join(homedir(), 'Downloads');

/**
 * The folder to tidy. Overridable so the commit path can be exercised end to end against a
 * scratch folder, because the alternative is proving it works by running it on someone's
 * actual Downloads and finding out afterwards. Must stay inside policy's allowedRoots.
 */
export function targetRoot(argv: string[] = []): string {
  const flag = argv.findIndex((arg) => arg === '--root');
  const value = flag >= 0 ? argv[flag + 1] : undefined;
  return value && value.length > 0 ? value : DOWNLOADS;
}

const FILED_FOLDER = '_filed';

/** Installers and archives older than this are almost certainly spent. */
const TRASH_AFTER_DAYS = 60;
/** Documents and media older than this are worth filing out of the way, never trashed. */
const FILE_AFTER_DAYS = 14;

/** Trashing anything this large deserves to be mentioned on its own line. */
const LARGE_MB = 500;

const INSTALLER_EXTENSIONS = new Set(['.exe', '.msi', '.msix', '.appx', '.dmg', '.pkg']);
const ARCHIVE_EXTENSIONS = new Set(['.zip', '.7z', '.rar', '.tar', '.gz', '.iso']);

const FILE_INTO: { folder: string; extensions: Set<string> }[] = [
  { folder: 'documents', extensions: new Set(['.pdf', '.docx', '.doc', '.txt', '.md', '.rtf', '.odt']) },
  { folder: 'sheets', extensions: new Set(['.xlsx', '.xls', '.csv', '.ods']) },
  { folder: 'slides', extensions: new Set(['.pptx', '.ppt', '.odp']) },
  { folder: 'images', extensions: new Set(['.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg', '.bmp', '.heic']) },
  { folder: 'media', extensions: new Set(['.mp4', '.mkv', '.mov', '.avi', '.mp3', '.wav', '.flac', '.webm']) },
];

export type Action = 'trash' | 'file';

export interface Candidate {
  name: string;
  path: string;
  sizeMb: number;
  ageDays: number;
  action: Action;
  /**
   * Full destination FILE path for a file action, absent for a trash action.
   *
   * The file name is included deliberately. files_move only moves an item *into* a
   * destination that is an existing folder; otherwise the destination is the new path of the
   * item itself. Passing the bare folder therefore worked only when the folder already
   * existed, and on the very first run it renamed report.pdf to a file called "documents"
   * with no extension, then reported success, because the broker's post-condition check asks
   * whether something arrived at the destination and something had. A full path is
   * unambiguous either way.
   */
  destination?: string;
  /** The folder the file lands in, for the report. */
  bucket?: string;
  why: string;
}

export interface TidyPlan {
  candidates: Candidate[];
  /** Files deliberately left alone, with the reason, so the report can be honest about scope. */
  skipped: { name: string; why: string }[];
  totalFiles: number;
}

/**
 * Classifies one entry. Exported because this is the whole decision, and a decision that
 * moves a person's files should be testable without touching a disk.
 */
export function classify(
  name: string,
  sizeMb: number,
  ageDays: number,
  root: string = DOWNLOADS,
): Candidate | { why: string } {
  const extension = extname(name).toLowerCase();

  // Partial downloads are live, not clutter, however old the timestamp looks.
  if (['.crdownload', '.part', '.partial', '.tmp', '.!ut'].includes(extension)) {
    return { why: 'looks like an in-progress download' };
  }

  const spent = INSTALLER_EXTENSIONS.has(extension) || ARCHIVE_EXTENSIONS.has(extension);

  if (spent && ageDays >= TRASH_AFTER_DAYS) {
    const kind = INSTALLER_EXTENSIONS.has(extension) ? 'installer' : 'archive';
    return {
      name,
      path: join(root, name),
      sizeMb,
      ageDays,
      action: 'trash',
      why: `${kind} last touched ${Math.round(ageDays)} days ago`,
    };
  }

  if (spent) {
    return { why: `installer or archive, but only ${Math.round(ageDays)} days old` };
  }

  const bucket = FILE_INTO.find((entry) => entry.extensions.has(extension));

  if (bucket && ageDays >= FILE_AFTER_DAYS) {
    return {
      name,
      path: join(root, name),
      sizeMb,
      ageDays,
      action: 'file',
      destination: join(root, FILED_FOLDER, bucket.folder, name),
      bucket: bucket.folder,
      why: `${bucket.folder}, ${Math.round(ageDays)} days old`,
    };
  }

  if (bucket) {
    return { why: `recent, only ${Math.round(ageDays)} days old` };
  }

  return { why: `unrecognised type '${extension || 'none'}'` };
}

export async function gather(root: string = DOWNLOADS, now = Date.now()): Promise<TidyPlan> {
  let entries: string[];
  try {
    entries = await readdir(root);
  } catch {
    return { candidates: [], skipped: [], totalFiles: 0 };
  }

  const candidates: Candidate[] = [];
  const skipped: { name: string; why: string }[] = [];
  let totalFiles = 0;

  for (const name of entries) {
    // The folder this routine files into must never be a candidate for its own tidying.
    if (name === FILED_FOLDER) { continue; }

    let info;
    try {
      info = await stat(join(root, name));
    } catch {
      skipped.push({ name, why: 'locked or vanished mid-scan' });
      continue;
    }

    // Folders are left alone entirely. Guessing whether a directory is spent is a much
    // worse bet than guessing about a single file, and getting it wrong moves more.
    if (!info.isFile()) {
      skipped.push({ name, why: 'a folder, and folders are never touched' });
      continue;
    }

    totalFiles++;

    const sizeMb = Math.round((info.size / 1_048_576) * 10) / 10;
    const ageDays = (now - info.mtimeMs) / 86_400_000;

    const verdict = classify(name, sizeMb, ageDays, root);
    if ('action' in verdict) {
      candidates.push(verdict);
    } else {
      skipped.push({ name, why: verdict.why });
    }
  }

  // Biggest first among trash, since that is where reclaiming space actually pays, and
  // trash before filing so the report leads with the consequential half.
  candidates.sort((a, b) => {
    if (a.action !== b.action) { return a.action === 'trash' ? -1 : 1; }
    return b.sizeMb - a.sizeMb;
  });

  return { candidates, skipped, totalFiles };
}

export function renderPlan(plan: TidyPlan, committed: boolean, root = DOWNLOADS): string {
  const trash = plan.candidates.filter((c) => c.action === 'trash');
  const file = plan.candidates.filter((c) => c.action === 'file');
  const reclaimable = Math.round(trash.reduce((sum, c) => sum + c.sizeMb, 0));

  const lines: string[] = ['# tidy downloads', '', `Folder: \`${root}\``, ''];

  if (plan.candidates.length === 0) {
    lines.push(`Nothing to do. ${plan.totalFiles} files scanned, all recent or in use.`, '');
    return lines.join('\n');
  }

  lines.push(
    committed
      ? `Acting on ${plan.candidates.length} of ${plan.totalFiles} files.`
      : `Would act on ${plan.candidates.length} of ${plan.totalFiles} files. Nothing has changed yet.`,
    '',
  );

  if (trash.length > 0) {
    lines.push(
      `## to staged trash, ${trash.length} files, ${reclaimable} MB`,
      '',
      'Recoverable with files_trash_restore. Nothing is permanently deleted.',
      '',
    );
    for (const item of trash) {
      const flag = item.sizeMb >= LARGE_MB ? ' **large**' : '';
      lines.push(`- ${item.name}, ${item.sizeMb} MB, ${item.why}${flag}`);
    }
    lines.push('');
  }

  if (file.length > 0) {
    lines.push(`## filed by kind, ${file.length} files`, '');
    for (const item of file) {
      lines.push(`- ${item.name} to \`${FILED_FOLDER}/${item.bucket}/\`, ${item.why}`);
    }
    lines.push('');
  }

  if (!committed) {
    lines.push('Re-run with --commit to apply.', '');
  }

  return lines.join('\n');
}

interface Applied {
  name: string;
  action: Action;
  status: string;
  message: string | null;
}

async function apply(plan: TidyPlan): Promise<Applied[]> {
  return withServer('aos-mcp-files.exe', async (files) => {
    const applied: Applied[] = [];

    for (const item of plan.candidates) {
      let outcome: CapabilityOutcome;

      if (item.action === 'trash') {
        const pair = await files.planThenCommit(
          'files_trash',
          { path: item.path },
          `tidy-downloads: ${item.why}`,
        );
        outcome = pair.commit;
      } else {
        const destination = item.destination ?? '';

        // Checked here rather than trusted. The broker's post-condition asks whether
        // something arrived at the destination, which cannot distinguish "report.pdf is now
        // in _filed/documents" from "report.pdf IS the file _filed/documents". Only the
        // caller knows the file was supposed to keep its name, so only the caller can verify
        // it. This is the guard for the first bug this routine ever had.
        if (!destination.endsWith(`\\${item.name}`) && !destination.endsWith(`/${item.name}`)) {
          applied.push({
            name: item.name,
            action: item.action,
            status: 'Failed',
            message:
              `Refused to move: destination '${destination}' does not end in the file name, so ` +
              'the move would rename it. This is a bug in the routine, not in the file.',
          });
          continue;
        }

        const pair = await files.planThenCommit(
          'files_move',
          { source: item.path, destination, createDirectories: true },
          `tidy-downloads: ${item.why}`,
        );
        outcome = pair.commit;
      }

      applied.push({
        name: item.name,
        action: item.action,
        status: outcome.status,
        message: outcome.message,
      });
    }

    return applied;
  });
}

function renderApplied(applied: Applied[]): string {
  const byStatus = new Map<string, Applied[]>();
  for (const entry of applied) {
    byStatus.set(entry.status, [...(byStatus.get(entry.status) ?? []), entry]);
  }

  const lines: string[] = ['## results', ''];

  // Succeeded first and briefly, then everything that did not, in full. The failures are
  // the part worth reading, so they get the space.
  const succeeded = byStatus.get('Succeeded') ?? [];
  if (succeeded.length > 0) {
    lines.push(`${succeeded.length} applied cleanly.`, '');
  }

  for (const [status, entries] of byStatus) {
    if (status === 'Succeeded') { continue; }
    lines.push(`### ${status}, ${entries.length}`, '');
    for (const entry of entries) {
      lines.push(`- ${entry.name} (${entry.action}): ${entry.message ?? 'no message'}`);
    }
    lines.push('');
  }

  return lines.join('\n');
}

export async function run(argv: string[] = []): Promise<string> {
  const commit = argv.includes('--commit');
  const root = targetRoot(argv);
  const plan = await gather(root);

  const report = renderPlan(plan, commit, root);
  if (!commit || plan.candidates.length === 0) { return report; }

  const applied = await apply(plan);
  return `${report}${renderApplied(applied)}`;
}
