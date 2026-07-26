import { readdir, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { run, runText } from './run.js';

export interface RepoState {
  name: string;
  path: string;
  branch: string;
  /**
   * Null when git could not answer, which is different from zero.
   *
   * This used to collapse any failure to 0, so a timeout, a locked index or a buffer
   * overflow rendered as a clean repo and the brief announced "nothing at risk". A tool
   * whose entire job is surfacing unpushed work must not report the reassuring answer when
   * it does not know.
   */
  dirtyFiles: number | null;
  ahead: number;
  behind: number;
  /** Null when the upstream query itself failed, rather than there being no upstream. */
  hasUpstream: boolean | null;
  lastCommitRelative: string;
  lastCommitSubject: string;
  lastCommitAgeDays: number;
  remote: string | null;
  /** owner/name when the remote is GitHub, otherwise null. */
  slug: string | null;
}

export async function findRepos(root: string): Promise<string[]> {
  let entries: string[] = [];
  try {
    entries = await readdir(root);
  } catch {
    return [];
  }

  const repos: string[] = [];
  for (const entry of entries) {
    const path = join(root, entry);
    try {
      const info = await stat(path);
      if (!info.isDirectory()) continue;
      const gitDir = await stat(join(path, '.git')).catch(() => null);
      if (gitDir) repos.push(path);
    } catch {
      // Unreadable directory. Skip rather than fail the whole scan.
    }
  }
  return repos;
}

export async function readRepo(path: string): Promise<RepoState> {
  const name = path.split(/[\\/]/).pop() ?? path;

  const branch = await runText('git', ['rev-parse', '--abbrev-ref', 'HEAD'], {
    cwd: path,
    fallback: 'unknown',
  });

  const status = await run('git', ['status', '--porcelain'], { cwd: path });
  const dirtyFiles = status.ok
    ? status.stdout.split('\n').filter((line) => line.trim().length > 0).length
    : null;

  // rev-list fails both when there is genuinely no upstream and when git could not run at
  // all. Those mean opposite things to a reader, so they are distinguished: exit code 128
  // with an upstream complaint is the real "no upstream", anything else is unknown.
  const counts = await run('git', ['rev-list', '--left-right', '--count', '@{u}...HEAD'], {
    cwd: path,
  });
  let ahead = 0;
  let behind = 0;
  let hasUpstream: boolean | null;

  if (counts.ok) {
    hasUpstream = true;
    const [behindRaw, aheadRaw] = counts.stdout.trim().split(/\s+/);
    behind = Number(behindRaw ?? 0) || 0;
    ahead = Number(aheadRaw ?? 0) || 0;
  } else if (/no upstream|unknown revision|ambiguous argument/i.test(counts.stderr)) {
    hasUpstream = false;
  } else {
    hasUpstream = null;
  }

  // %x1f is a literal unit separator. A commit subject can contain anything printable,
  // so splitting on a common character would corrupt the parse.
  const lastCommit = await runText(
    'git',
    ['log', '-1', '--format=%cr%x1f%s%x1f%ct'],
    { cwd: path, fallback: '' },
  );
  // An empty string means git log failed, which is what a repo with no commits looks like.
  // Destructuring defaults do not cover it, because '' is defined.
  const parts = lastCommit.length > 0 ? lastCommit.split('\u{001f}') : [];
  const relative = parts[0]?.trim() || 'never';
  // Some commits carry a leading byte order mark, which renders as a stray glyph.
  const subject = parts[1]?.replace(/^﻿/, '').trim() || 'no commits yet';
  const epoch = parts[2]?.trim() || '0';
  const ageDays = epoch === '0' ? Number.POSITIVE_INFINITY : (Date.now() / 1000 - Number(epoch)) / 86_400;

  const remote = (await runText('git', ['remote', 'get-url', 'origin'], { cwd: path })) || null;
  const slug = remote ? parseGitHubSlug(remote) : null;

  return {
    name,
    path,
    branch,
    dirtyFiles,
    ahead,
    behind,
    hasUpstream,
    lastCommitRelative: relative,
    lastCommitSubject: subject,
    lastCommitAgeDays: ageDays,
    remote,
    slug,
  };
}

export function parseGitHubSlug(remote: string): string | null {
  const match = remote.match(/github\.com[/:]([^/]+)\/(.+?)(?:\.git)?$/i);
  return match ? `${match[1]}/${match[2]}` : null;
}

/**
 * Things worth putting in front of a person, in rough order of urgency.
 *
 * An unanswerable query is itself worth reporting. Saying nothing would let a repo whose
 * state is unknown be counted among the clean ones.
 */
export function needsAttention(repo: RepoState): string[] {
  const reasons: string[] = [];

  if (repo.dirtyFiles === null) {
    reasons.push('git status did not answer, so this repo is unchecked');
  } else if (repo.dirtyFiles > 0) {
    reasons.push(`${repo.dirtyFiles} uncommitted file(s)`);
  }

  if (repo.ahead > 0) reasons.push(`${repo.ahead} commit(s) not pushed`);
  if (repo.behind > 0) reasons.push(`${repo.behind} commit(s) behind origin`);

  if (repo.hasUpstream === null) {
    reasons.push('could not determine whether it has an upstream');
  } else if (!repo.hasUpstream && repo.lastCommitSubject !== 'no commits yet') {
    reasons.push('no upstream branch, so nothing is backed up');
  }

  return reasons;
}
