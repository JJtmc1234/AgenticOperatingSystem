import { test } from 'node:test';
import assert from 'node:assert/strict';
import { needsAttention, parseGitHubSlug, type RepoState } from '../lib/repos.js';

/**
 * Guards for the reporting defects on the TypeScript side.
 *
 * Every one of these shipped, and the brief is the one output a person reads and trusts
 * without checking, so a wrong reassurance here is worse than a crash.
 *
 * Uses node:test, which ships with Node 22. A test runner that needs no dependency is a
 * test runner that cannot go stale or drift out of the provisioning module.
 */

function repo(overrides: Partial<RepoState> = {}): RepoState {
  return {
    name: 'example',
    path: 'C:\\repos\\example',
    branch: 'main',
    dirtyFiles: 0,
    ahead: 0,
    behind: 0,
    hasUpstream: true,
    lastCommitRelative: '2 hours ago',
    lastCommitSubject: 'a commit',
    lastCommitAgeDays: 0.1,
    remote: 'https://github.com/owner/example.git',
    slug: 'owner/example',
    ...overrides,
  };
}

test('a clean, pushed, tracked repo raises nothing', () => {
  assert.deepEqual(needsAttention(repo()), []);
});

test('a git status that never answered is reported, not treated as clean', () => {
  // This is the defect that mattered most. Collapsing a failure to zero made a timeout, a
  // locked index, or a buffer overflow render as a clean repo, and the brief then printed
  // "Every repo is clean and pushed. Nothing at risk." over work it had never looked at.
  const reasons = needsAttention(repo({ dirtyFiles: null }));

  assert.equal(reasons.length, 1);
  assert.match(reasons[0]!, /unchecked/);
});

test('a repo with no commits and real work says so plainly', () => {
  // It used to say "could not determine whether it has an upstream", which describes a
  // query that went wrong rather than the actual situation. rev-list cannot compare a
  // HEAD that does not exist, so that question was never meaningful here.
  const reasons = needsAttention(
    repo({
      dirtyFiles: 2,
      hasUpstream: false,
      lastCommitRelative: 'never',
      lastCommitSubject: 'no commits yet',
    }),
  );

  assert.ok(reasons.some((r) => /no commits yet/.test(r)));
  assert.ok(!reasons.some((r) => /could not determine/.test(r)));
  assert.ok(!reasons.some((r) => /no upstream branch/.test(r)));
});

test('an empty repo with no work at all is not nagged about', () => {
  const reasons = needsAttention(
    repo({ dirtyFiles: 0, hasUpstream: false, lastCommitRelative: 'never' }),
  );

  assert.deepEqual(reasons, []);
});

test('an unanswerable upstream query is distinguished from having no upstream', () => {
  assert.ok(
    needsAttention(repo({ hasUpstream: null })).some((r) => /could not determine/.test(r)),
  );
  assert.ok(
    needsAttention(repo({ hasUpstream: false })).some((r) => /nothing is backed up/.test(r)),
  );
});

test('unpushed and behind counts are both surfaced', () => {
  const reasons = needsAttention(repo({ ahead: 3, behind: 1 }));

  assert.ok(reasons.some((r) => /3 commit\(s\) not pushed/.test(r)));
  assert.ok(reasons.some((r) => /1 commit\(s\) behind/.test(r)));
});

test('github slugs parse from both https and ssh remotes', () => {
  assert.equal(parseGitHubSlug('https://github.com/owner/repo.git'), 'owner/repo');
  assert.equal(parseGitHubSlug('https://github.com/owner/repo'), 'owner/repo');
  assert.equal(parseGitHubSlug('git@github.com:owner/repo.git'), 'owner/repo');
  assert.equal(parseGitHubSlug('https://gitlab.com/owner/repo.git'), null);
});
