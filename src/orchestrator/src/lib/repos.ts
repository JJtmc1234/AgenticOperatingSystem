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

  // These four are independent queries against the same repo, so they go out together.
  // Sequentially they cost four process spawns of latency per repo, which on a Projects
  // folder of eleven repos was most of the brief's git time.
  const [branch, status, lastCommit, remoteUrl] = await Promise.all([
    runText('git', ['rev-parse', '--abbrev-ref', 'HEAD'], { cwd: path, fallback: 'unknown' }),
    run('git', ['status', '--porcelain'], { cwd: path }),
    runText('git', ['log', '-1', '--format=%cr%x1f%s%x1f%ct'], { cwd: path, fallback: '' }),
    runText('git', ['remote', 'get-url', 'origin'], { cwd: path }),
  ]);

  const dirtyFiles = status.ok
    ? status.stdout.split('\n').filter((line) => line.trim().length > 0).length
    : null;

  // %x1f is a literal unit separator. A commit subject can contain anything printable,
  // so splitting on a common character would corrupt the parse.
  //
  // An empty string means git log failed, which is what a repo with no commits looks like.
  // Destructuring defaults do not cover it, because '' is defined.
  const parts = lastCommit.length > 0 ? lastCommit.split('\u{001f}') : [];
  const hasCommits = parts.length > 0;
  const relative = parts[0]?.trim() || 'never';
  // Some commits carry a leading byte order mark, which renders as a stray glyph.
  const subject = parts[1]?.replace(/^﻿/, '').trim() || 'no commits yet';
  const epoch = parts[2]?.trim() || '0';
  const ageDays = epoch === '0' ? Number.POSITIVE_INFINITY : (Date.now() / 1000 - Number(epoch)) / 86_400;

  const { ahead, behind, hasUpstream } = await readUpstream(path, hasCommits);

  const remote = remoteUrl || null;
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

async function readUpstream(
  path: string,
  hasCommits: boolean,
): Promise<{ ahead: number; behind: number; hasUpstream: boolean | null }> {
  // A repo with no commits has no HEAD to compare, so rev-list fails with a message that
  // matches none of the patterns below and the state came back as "could not determine
  // whether it has an upstream". That is a query that was never meaningful being reported
  // as a query that went wrong. There is nothing to push, so nothing to say.
  if (!hasCommits) {
    return { ahead: 0, behind: 0, hasUpstream: false };
  }

  // rev-list fails both when there is genuinely no upstream and when git could not run at
  // all. Those mean opposite things to a reader, so they are distinguished: an upstream
  // complaint is the real "no upstream", anything else is unknown.
  const counts = await run('git', ['rev-list', '--left-right', '--count', '@{u}...HEAD'], {
    cwd: path,
  });

  if (counts.ok) {
    const [behindRaw, aheadRaw] = counts.stdout.trim().split(/\s+/);
    return {
      behind: Number(behindRaw ?? 0) || 0,
      ahead: Number(aheadRaw ?? 0) || 0,
      hasUpstream: true,
    };
  }

  const noUpstream = /no upstream|unknown revision|ambiguous argument/i.test(counts.stderr);
  return { ahead: 0, behind: 0, hasUpstream: noUpstream ? false : null };
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

  const hasCommits = repo.lastCommitRelative !== 'never';

  if (repo.hasUpstream === null) {
    reasons.push('could not determine whether it has an upstream');
  } else if (!repo.hasUpstream && hasCommits) {
    reasons.push('no upstream branch, so nothing is backed up');
  } else if (!hasCommits && (repo.dirtyFiles ?? 0) > 0) {
    // Worth saying plainly. A folder of real work with no commit at all is the least backed
    // up state there is, and the upstream line above would not mention it.
    reasons.push('no commits yet, so none of this is in git history');
  }

  return reasons;
}
