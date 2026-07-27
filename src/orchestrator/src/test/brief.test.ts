import { test } from 'node:test';
import assert from 'node:assert/strict';
import { render, type Brief } from '../routines/daily-brief.js';
import type { RepoState } from '../lib/repos.js';

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
    remote: null,
    slug: null,
    ...overrides,
  };
}

function brief(overrides: Partial<Brief> = {}): Brief {
  return {
    generatedAt: new Date('2026-07-27T09:00:00Z'),
    repos: [],
    attention: [],
    issues: [],
    staleDownloads: [],
    downloadsTotal: 0,
    trash: { sizeMb: 0, entries: 0, oldestDays: 0 },
    ...overrides,
  };
}

test('staged trash is reported as pending, never as reclaimed', () => {
  // The trash sits on the same volume as the files it holds, so tidy-downloads moving 6 GB
  // into it freed exactly nothing. Reporting that as dealt with would be the same category
  // of false reassurance as the all-clear over unchecked repos.
  const young = render(brief({ trash: { sizeMb: 6007, entries: 42, oldestDays: 1 } }));
  assert.match(young, /42 entries, 6007 MB still on disk/);
  assert.match(young, /not reclaimed yet/);
  assert.match(young, /nothing is purged under 30 days/);
  assert.doesNotMatch(young, /ask to purge/);

  const old = render(brief({ trash: { sizeMb: 6007, entries: 42, oldestDays: 95 } }));
  assert.match(old, /oldest is 95 days old/);
  assert.match(old, /ask to purge staged trash older than 30 days/);
});

test('an empty trash gets no section at all', () => {
  const rendered = render(brief());
  assert.doesNotMatch(rendered, /staged trash/);
});

test('the all clear is only printed when every repo actually answered', () => {
  const clean = render(brief({ repos: [repo(), repo()] }));
  assert.match(clean, /Every repo is clean and pushed/);

  const partial = render(brief({ repos: [repo(), repo({ dirtyFiles: null })] }));
  assert.doesNotMatch(partial, /Nothing at risk/);
  assert.match(partial, /1 repo\(s\) could not be checked/);
});

test('an unchecked repo sorts above a repo with a known dirty count', () => {
  // Unknown outranks a known small number, because it is the one that still needs a human.
  // Subtracting nulls-as-Infinity would have produced NaN with two unchecked repos, which
  // makes the comparator incoherent rather than merely wrong.
  const unchecked = repo({ name: 'unchecked', dirtyFiles: null });
  const dirty = repo({ name: 'dirty', dirtyFiles: 40 });
  const alsoUnchecked = repo({ name: 'also-unchecked', dirtyFiles: null });

  const rendered = render(
    brief({
      repos: [dirty, unchecked, alsoUnchecked],
      attention: [
        { repo: dirty, reasons: ['40 uncommitted file(s)'] },
        { repo: unchecked, reasons: ['git status did not answer, so this repo is unchecked'] },
        { repo: alsoUnchecked, reasons: ['git status did not answer, so this repo is unchecked'] },
      ].sort((a, b) => rank(b.repo) - rank(a.repo)),
    }),
  );

  assert.ok(rendered.indexOf('**unchecked**') < rendered.indexOf('**dirty**'));
});

function rank(r: RepoState): number {
  return r.dirtyFiles === null ? Number.MAX_SAFE_INTEGER : r.dirtyFiles;
}

test('homework issues are listed separately from the rest', () => {
  const rendered = render(
    brief({
      issues: [
        { repo: 'o/r', number: 5, title: 'Design pattern', labels: ['homework'], updatedAt: '', url: '' },
        { repo: 'o/r', number: 9, title: 'A bug', labels: ['bug'], updatedAt: '', url: '' },
      ],
    }),
  );

  const homeworkAt = rendered.indexOf('## homework');
  const othersAt = rendered.indexOf('## other open issues');

  assert.ok(homeworkAt >= 0 && othersAt >= 0);
  assert.ok(homeworkAt < othersAt);
  assert.match(rendered, /homework, 1 open/);
  assert.match(rendered, /other open issues, 1/);
});

test('the downloads section states what could be reclaimed and promises no deletion', () => {
  const rendered = render(
    brief({
      downloadsTotal: 135,
      staleDownloads: [{ name: 'big.zip', ageDays: 46, sizeMb: 9393.9 }],
    }),
  );

  assert.match(rendered, /135 files/);
  assert.match(rendered, /9394 MB in old clutter/);
  // The no-deletion promise has to survive rewording. A brief that reports clutter without
  // saying the remedy is reversible is one a person will not act on.
  assert.match(rendered, /nothing is ever permanently deleted/i);
  // And it must name the routine that acts, or the report is a dead end.
  assert.match(rendered, /aos tidy-downloads/);
});
