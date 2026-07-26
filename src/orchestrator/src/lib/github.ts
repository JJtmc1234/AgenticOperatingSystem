import { run } from './run.js';

export interface IssueRef {
  repo: string;
  number: number;
  title: string;
  labels: string[];
  updatedAt: string;
  url: string;
}

interface RawIssue {
  number?: number;
  title?: string;
  updatedAt?: string;
  url?: string;
  labels?: { name?: string }[];
  repository?: { nameWithOwner?: string };
}

/** Open issues assigned to the authenticated user, across every repo. */
export async function assignedIssues(limit = 20): Promise<IssueRef[]> {
  const result = await run('gh', [
    'search', 'issues',
    '--assignee', '@me',
    '--state', 'open',
    '--limit', String(limit),
    '--json', 'number,title,updatedAt,url,labels,repository',
  ]);

  return result.ok ? parse(result.stdout) : [];
}

/** Open issues in one repo, used for repos the user owns and works in daily. */
export async function repoIssues(slug: string, limit = 10): Promise<IssueRef[]> {
  const result = await run('gh', [
    'issue', 'list',
    '--repo', slug,
    '--state', 'open',
    '--limit', String(limit),
    '--json', 'number,title,updatedAt,url,labels',
  ]);

  if (!result.ok) return [];
  return parse(result.stdout, slug);
}

function parse(json: string, fallbackRepo?: string): IssueRef[] {
  let raw: RawIssue[];
  try {
    raw = JSON.parse(json) as RawIssue[];
  } catch {
    return [];
  }

  return raw.map((issue) => ({
    repo: issue.repository?.nameWithOwner ?? fallbackRepo ?? 'unknown',
    number: issue.number ?? 0,
    title: issue.title ?? '(untitled)',
    labels: (issue.labels ?? []).map((l) => l.name ?? '').filter(Boolean),
    updatedAt: issue.updatedAt ?? '',
    url: issue.url ?? '',
  }));
}
