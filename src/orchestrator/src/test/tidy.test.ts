import { test } from 'node:test';
import assert from 'node:assert/strict';
import { basename, dirname } from 'node:path';
import { classify, renderPlan, type Candidate, type TidyPlan } from '../routines/tidy-downloads.js';

const ROOT = 'C:\\scratch';

function candidate(name: string, sizeMb: number, ageDays: number): Candidate {
  const verdict = classify(name, sizeMb, ageDays, ROOT);
  assert.ok('action' in verdict, `${name} at ${ageDays} days was expected to be actionable`);
  return verdict;
}

function skipReason(name: string, sizeMb: number, ageDays: number): string {
  const verdict = classify(name, sizeMb, ageDays, ROOT);
  assert.ok(!('action' in verdict), `${name} at ${ageDays} days was expected to be left alone`);
  return verdict.why;
}

test('a filed destination is a full path that keeps the original file name', () => {
  // The first bug this routine had, and the one worth a test above all others. files_move
  // moves an item INTO a destination only when that destination is an existing folder;
  // otherwise the destination becomes the item's new path. Passing the bare bucket folder
  // therefore turned report.pdf into a file called "documents" with no extension, on a
  // first run where the folder did not exist yet, and reported success.
  const filed = candidate('report.pdf', 0.05, 200);

  assert.equal(filed.action, 'file');
  assert.equal(basename(filed.destination!), 'report.pdf');
  assert.equal(dirname(filed.destination!), 'C:\\scratch\\_filed\\documents');
});

test('every bucket files into its own folder, name intact', () => {
  const cases: [string, string][] = [
    ['report.pdf', 'documents'],
    ['budget.xlsx', 'sheets'],
    ['deck.pptx', 'slides'],
    ['photo.png', 'images'],
    ['clip.mp4', 'media'],
  ];

  for (const [name, folder] of cases) {
    const filed = candidate(name, 1, 200);
    assert.equal(filed.bucket, folder);
    assert.equal(filed.destination, `C:\\scratch\\_filed\\${folder}\\${name}`);
  }
});

test('old installers and archives are trashed, not filed', () => {
  for (const name of ['Setup.exe', 'thing.msi', 'bundle.zip', 'image.iso']) {
    const item = candidate(name, 100, 200);
    assert.equal(item.action, 'trash');
    assert.equal(item.destination, undefined);
  }
});

test('a recent installer is left alone', () => {
  // The threshold is the whole safety story for trashing. An installer someone downloaded
  // last week is very likely still needed.
  assert.match(skipReason('FreshSetup.exe', 100, 3), /only 3 days old/);
  assert.match(skipReason('FreshSetup.exe', 100, 59), /only 59 days old/);
  assert.equal(candidate('FreshSetup.exe', 100, 60).action, 'trash');
});

test('a recent document is left alone', () => {
  assert.match(skipReason('notes.txt', 0.01, 3), /only 3 days old/);
  assert.equal(candidate('notes.txt', 0.01, 14).action, 'file');
});

test('in-progress downloads are never touched however old the timestamp looks', () => {
  // A stalled browser download can carry a timestamp from whenever it began. Trashing one
  // mid-transfer is the sort of thing that makes a tidy tool untrustworthy for good.
  for (const name of ['big.crdownload', 'big.part', 'big.partial', 'scratch.tmp']) {
    assert.match(skipReason(name, 900, 400), /in-progress/);
  }
});

test('unrecognised types are reported rather than guessed at', () => {
  assert.match(skipReason('mystery.xyz', 1, 400), /unrecognised type '\.xyz'/);
  assert.match(skipReason('LICENSE', 1, 400), /unrecognised type 'none'/);
});

test('the dry-run report says nothing has changed and how to change that', () => {
  const plan: TidyPlan = {
    candidates: [candidate('Setup.exe', 600, 200), candidate('report.pdf', 0.05, 200)],
    skipped: [],
    totalFiles: 8,
  };

  const dry = renderPlan(plan, false, ROOT);
  assert.match(dry, /Nothing has changed yet/);
  assert.match(dry, /Re-run with --commit/);
  assert.match(dry, /Recoverable with files_trash_restore/);

  const done = renderPlan(plan, true, ROOT);
  assert.doesNotMatch(done, /Nothing has changed yet/);
  assert.doesNotMatch(done, /Re-run with --commit/);
});

test('trash is listed before filing, and biggest first within trash', () => {
  // Trash is the consequential half, and the largest items are where reclaiming space pays,
  // so a person skimming the top of the report sees the decisions that matter.
  const plan: TidyPlan = {
    candidates: [
      candidate('small.exe', 5, 200),
      candidate('report.pdf', 0.05, 200),
      candidate('huge.zip', 900, 200),
    ].sort((a, b) => {
      if (a.action !== b.action) { return a.action === 'trash' ? -1 : 1; }
      return b.sizeMb - a.sizeMb;
    }),
    skipped: [],
    totalFiles: 3,
  };

  const rendered = renderPlan(plan, false, ROOT);
  assert.ok(rendered.indexOf('huge.zip') < rendered.indexOf('small.exe'));
  assert.ok(rendered.indexOf('small.exe') < rendered.indexOf('report.pdf'));
});

test('an empty plan says so instead of printing empty sections', () => {
  const rendered = renderPlan({ candidates: [], skipped: [], totalFiles: 42 }, false, ROOT);
  assert.match(rendered, /Nothing to do\. 42 files scanned/);
  assert.doesNotMatch(rendered, /staged trash/);
});
