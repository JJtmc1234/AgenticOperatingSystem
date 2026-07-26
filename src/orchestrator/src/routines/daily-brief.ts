import { readdir, stat, mkdir, writeFile } from 'node:fs/promises';
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
}

export async function gather(): Promise<Brief> {
  const repoPaths = await findRepos(PROJECTS_ROOT);
  const repos = await Promise.all(repoPaths.map(readRepo));

  const attention = repos
    .map((repo) => ({ repo, reasons: needsAttention(repo) }))
    .filter((entry) => entry.reasons.length > 0)
    // Most uncommitted work first, since that is what is most at risk of being lost.
    .sort((a, b) => b.repo.dirtyFiles - a.repo.dirtyFiles);

  const issues = await collectIssues(repos);
  const downloads = await scanDownloads();

  return {
    generatedAt: new Date(),
    repos,
    attention,
    issues,
    staleDownloads: downloads.stale,
    downloadsTotal: downloads.total,
  };
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

  add(await assignedIssues());

  // Issues in your own active repos matter even when nobody assigned them to you, which is
  // the normal case for a solo project.
  const activeSlugs = [
    ...new Set(
      repos
        .filter((r) => r.slug && r.lastCommitAgeDays < 30)
        .map((r) => r.slug as string),
    ),
  ].slice(0, 6);

  for (const slug of activeSlugs) {
    add(await repoIssues(slug));
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
    lines.push('Every repo is clean and pushed. Nothing at risk.', '');
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
    lines.push('', 'Ask to file or trash these. Nothing is deleted, only staged.', '');
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
