import { readdir, stat, mkdir, writeFile, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { assignedIssues, repoIssues, type IssueRef } from '../lib/github.js';
import { findRepos, needsAttention, readRepo, type RepoState } from '../lib/repos.js';

const AOS_ROOT = join(process.env['LOCALAPPDATA'] ?? join(homedir(), 'AppData', 'Local'), 'AgenticOS');
const PROJECTS_ROOT = join(homedir(), 'OneDrive', 'Desktop', 'Projects');
const DOWNLOADS = join(homedir(), 'Downloads');

/** Files sitting in Downloads longer than this are clutter rather than in-flight. */
const STALE_DOWNLOAD_DAYS = 7;

export interface Brief {
  generatedAt: Date;
  repos: RepoState[];
  attention: { repo: RepoState; reasons: string[] }[];
  issues: IssueRef[];
  staleDownloads: { name: string; ageDays: number; sizeMb: number }[];
  downloadsTotal: number;
  /** Staged trash still occupying disk, and the oldest entry's age in days. */
  trash: { sizeMb: number; entries: number; oldestDays: number };
}

export async function gather(): Promise<Brief> {
  const repoPaths = await findRepos(PROJECTS_ROOT);
  const repos = await Promise.all(repoPaths.map(readRepo));

  const attention = repos
    .map((repo) => ({ repo, reasons: needsAttention(repo) }))
    .filter((entry) => entry.reasons.length > 0)
    // Most uncommitted work first, since that is what is most at risk of being lost. A repo
    // whose status never answered sorts to the top: unknown outranks a known count, because
    // it is the one that still needs a human to look. Subtracting nulls-as-Infinity would
    // give NaN when two are unchecked, so the ranks are compared explicitly.
    .sort((a, b) => dirtyRank(b.repo) - dirtyRank(a.repo));

  const issues = await collectIssues(repos);
  const downloads = await scanDownloads();
  const trash = await measureTrash();

  return {
    generatedAt: new Date(),
    repos,
    attention,
    issues,
    staleDownloads: downloads.stale,
    downloadsTotal: downloads.total,
    trash,
  };
}

/**
 * Measures staged trash, because trashing something does not free any space.
 *
 * The trash lives on the same volume as the files it holds, so tidy-downloads moving 6 GB
 * into it reclaimed exactly nothing. Reporting the clutter as dealt with while it still
 * occupies the disk would be the same category of lie as the false all-clear above.
 */
async function measureTrash(): Promise<Brief['trash']> {
  const root = join(AOS_ROOT, 'trash');
  let sizeMb = 0;
  let entries = 0;

  const walk = async (directory: string): Promise<void> => {
    let names: string[];
    try {
      names = await readdir(directory);
    } catch {
      return;
    }

    for (const name of names) {
      const path = join(directory, name);
      try {
        const info = await stat(path);
        if (info.isDirectory()) {
          await walk(path);
        } else {
          sizeMb += info.size / 1_048_576;
        }
      } catch {
        // Locked or vanished mid-scan.
      }
    }
  };

  try {
    // One slot folder per entry, so the top level count is the entry count.
    entries = (await readdir(root)).filter((name) => name !== 'manifest.jsonl').length;
  } catch {
    return { sizeMb: 0, entries: 0, oldestDays: 0 };
  }

  await walk(root);

  return { sizeMb: Math.round(sizeMb), entries, oldestDays: await oldestTrashedDays(root) };
}

/**
 * How long the longest-sitting entry has been IN TRASH, from the manifest.
 *
 * Not from file modification times, which was the first attempt and was wrong in a way that
 * mattered: a file last edited 588 days ago and trashed this morning kept its old mtime, so
 * the brief announced "the oldest is 588 days old" and advised purging entries older than 30
 * days when nothing qualified. Purge keys off deletedAt, so this has to as well or the advice
 * describes an operation that would do nothing.
 */
async function oldestTrashedDays(root: string): Promise<number> {
  let text: string;
  try {
    text = await readFile(join(root, 'manifest.jsonl'), 'utf8');
  } catch {
    return 0;
  }

  // Latest line per id wins, so restored and purged entries drop out rather than counting
  // toward an age nobody can act on.
  const latest = new Map<string, { deletedAt?: string; purgedAt?: string; restoredAt?: string }>();

  for (const line of text.split('\n')) {
    if (line.trim().length === 0) continue;
    try {
      const entry = JSON.parse(line) as { id?: string; deletedAt?: string; purgedAt?: string; restoredAt?: string };
      if (entry.id) latest.set(entry.id, entry);
    } catch {
      // A torn final line beats losing the whole measurement.
    }
  }

  let oldestMs = Number.POSITIVE_INFINITY;
  for (const entry of latest.values()) {
    if (entry.purgedAt || entry.restoredAt || !entry.deletedAt) continue;
    const when = Date.parse(entry.deletedAt);
    if (!Number.isNaN(when)) oldestMs = Math.min(oldestMs, when);
  }

  return Number.isFinite(oldestMs) ? Math.round((Date.now() - oldestMs) / 86_400_000) : 0;
}

/** Unchecked repos outrank every known count; among the known, more dirty files wins. */
function dirtyRank(repo: RepoState): number {
  return repo.dirtyFiles === null ? Number.MAX_SAFE_INTEGER : repo.dirtyFiles;
}

async function collectIssues(repos: RepoState[]): Promise<IssueRef[]> {
  const seen = new Set<string>();
  const all: IssueRef[] = [];

  const add = (issues: IssueRef[]) => {
    for (const issue of issues) {
      const key = `${issue.repo}#${issue.number}`;
      if (seen.has(key)) continue;
      seen.add(key);
      all.push(issue);
    }
  };

  // Issues in your own active repos matter even when nobody assigned them to you, which is
  // the normal case for a solo project.
  const activeSlugs = [
    ...new Set(
      repos
        .filter((r) => r.slug && r.lastCommitAgeDays < 30)
        .map((r) => r.slug as string),
    ),
  ].slice(0, 6);

  // Issued together rather than one after another. Each gh call is a network round trip of
  // roughly two seconds, and seven of them in sequence was most of the brief's runtime. They
  // are independent queries, so waiting for each before starting the next bought nothing.
  //
  // Order still matters for dedup: the assigned list wins, since being assigned an issue is
  // a stronger signal than it merely living in a repo you touched. So the results are
  // collected in parallel and then added in a fixed order.
  const [assigned, ...perRepo] = await Promise.all([
    assignedIssues(),
    ...activeSlugs.map((slug) => repoIssues(slug)),
  ]);

  add(assigned);
  for (const issues of perRepo) {
    add(issues);
  }

  return all;
}

async function scanDownloads(): Promise<{
  stale: { name: string; ageDays: number; sizeMb: number }[];
  total: number;
}> {
  let entries: string[];
  try {
    entries = await readdir(DOWNLOADS);
  } catch {
    return { stale: [], total: 0 };
  }

  const stale: { name: string; ageDays: number; sizeMb: number }[] = [];
  let total = 0;

  for (const entry of entries) {
    try {
      const info = await stat(join(DOWNLOADS, entry));
      if (!info.isFile()) continue;
      total++;

      const ageDays = (Date.now() - info.mtimeMs) / 86_400_000;
      if (ageDays > STALE_DOWNLOAD_DAYS) {
        stale.push({
          name: entry,
          ageDays: Math.round(ageDays),
          sizeMb: Math.round((info.size / 1_048_576) * 10) / 10,
        });
      }
    } catch {
      // Locked or vanished mid-scan.
    }
  }

  // Biggest first, because that is where reclaiming space actually pays.
  stale.sort((a, b) => b.sizeMb - a.sizeMb);
  return { stale: stale.slice(0, 10), total };
}

export function render(brief: Brief): string {
  const date = brief.generatedAt.toISOString().slice(0, 10);
  const lines: string[] = [`# daily brief, ${date}`, ''];

  if (brief.attention.length === 0) {
    // Only claim the all-clear when every repo actually answered. Anything else would be
    // reassurance the data does not support.
    const unchecked = brief.repos.filter((r) => r.dirtyFiles === null).length;
    lines.push(
      unchecked === 0
        ? 'Every repo is clean and pushed. Nothing at risk.'
        : `No problems found, but ${unchecked} repo(s) could not be checked.`,
      '',
    );
  } else {
    lines.push(`## ${brief.attention.length} repo(s) need attention`, '');
    for (const { repo, reasons } of brief.attention) {
      lines.push(`- **${repo.name}** on \`${repo.branch}\`, ${reasons.join(', ')}`);
      lines.push(`  last commit ${repo.lastCommitRelative}, ${repo.lastCommitSubject}`);
    }
    lines.push('');
  }

  const homework = brief.issues.filter((i) =>
    i.labels.some((l) => l.toLowerCase() === 'homework'),
  );
  const others = brief.issues.filter((i) => !homework.includes(i));

  if (homework.length > 0) {
    lines.push(`## homework, ${homework.length} open`, '');
    for (const issue of homework) {
      lines.push(`- ${issue.repo}#${issue.number} ${issue.title}`);
    }
    lines.push('');
  }

  if (others.length > 0) {
    lines.push(`## other open issues, ${others.length}`, '');
    for (const issue of others.slice(0, 12)) {
      const tags = issue.labels.length > 0 ? ` [${issue.labels.join(', ')}]` : '';
      lines.push(`- ${issue.repo}#${issue.number} ${issue.title}${tags}`);
    }
    lines.push('');
  }

  if (brief.staleDownloads.length > 0) {
    const reclaimable = brief.staleDownloads.reduce((sum, f) => sum + f.sizeMb, 0);
    lines.push(
      `## downloads, ${brief.downloadsTotal} files, ` +
        `${Math.round(reclaimable)} MB in old clutter`,
      '',
    );
    for (const file of brief.staleDownloads) {
      lines.push(`- ${file.name}, ${file.sizeMb} MB, ${file.ageDays} days old`);
    }
    // Names the routine that acts on this, because a report that identifies a problem and
    // then leaves you to work out the remedy is how tidying tools get ignored.
    lines.push(
      '',
      'Run `aos tidy-downloads` to see exactly what would move, then add `--commit`.',
      'Old installers and archives go to staged trash, keepers are filed by kind, and',
      'nothing is ever permanently deleted.',
      '',
    );
  }

  if (brief.trash.sizeMb > 0) {
    // Says pending, not reclaimed. The trash sits on the same drive, so this space is still
    // spent until it is purged, and the purge floor is thirty days by design.
    lines.push(
      `## staged trash, ${brief.trash.entries} entries, ${brief.trash.sizeMb} MB still on disk`,
      '',
      'This space is not reclaimed yet. Trashing moves files within the same drive, so they',
      'keep occupying it until purged, which is what makes every trash reversible.',
      '',
      brief.trash.oldestDays >= 30
        ? `The oldest is ${brief.trash.oldestDays} days old. Run \`aos\` and ask to purge staged ` +
          'trash older than 30 days to reclaim it permanently.'
        : `The oldest is ${brief.trash.oldestDays} days old, and nothing is purged under 30 days.`,
      '',
    );
  }

  lines.push('---', `${brief.repos.length} repos scanned.`);
  return lines.join('\n');
}

export async function run(): Promise<string> {
  const brief = await gather();
  const markdown = render(brief);

  const directory = join(AOS_ROOT, 'data', 'briefs');
  await mkdir(directory, { recursive: true });
  const file = join(directory, `${brief.generatedAt.toISOString().slice(0, 10)}.md`);
  await writeFile(file, markdown, 'utf8');

  return `${markdown}\n\nSaved to ${file}\n`;
}
