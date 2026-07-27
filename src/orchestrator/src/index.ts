import * as dailyBrief from './routines/daily-brief.js';
import * as tidyDownloads from './routines/tidy-downloads.js';

/**
 * Routine runner. Routines are deliberately deterministic rather than agent loops: a brief
 * that reports facts is useful every single morning, whereas one that asks a model to go find
 * things is slower, costs tokens, and can be wrong about whether your work is committed.
 * Reasoning belongs on top of this output, not inside it.
 *
 * The same argument applies to the ones that act. Whether an installer is older than sixty
 * days is arithmetic, and a model deciding it would be slower, dearer, and occasionally wrong
 * about which of your files to move.
 */
const ROUTINES: Record<string, {
  describe: string;
  run: (argv: string[]) => Promise<string>;
}> = {
  'daily-brief': {
    describe: 'Repo state, open issues and Downloads clutter, saved as markdown.',
    run: () => dailyBrief.run(),
  },
  'tidy-downloads': {
    describe: 'Stage old installers to trash and file keepers by kind. Add --commit to apply.',
    run: tidyDownloads.run,
  },
};

async function main(): Promise<number> {
  const name = process.argv[2];
  // Everything after the routine name belongs to the routine, so --commit cannot be
  // mistaken for a flag of the runner's own.
  const argv = process.argv.slice(3);

  if (!name || name === '--help' || name === 'list') {
    // Says aos, because that is what people type. aos.cmd is the documented entry point and
    // printing the internal node invocation just sends the reader down a longer path.
    console.log('Usage: aos <routine> [options]\n\nRoutines:');
    for (const [key, routine] of Object.entries(ROUTINES)) {
      console.log(`  ${key.padEnd(16)} ${routine.describe}`);
    }
    console.log('\nRun `aos` with no routine for the interactive agent.');
    return name ? 0 : 1;
  }

  const routine = ROUTINES[name];
  if (!routine) {
    console.error(`Unknown routine '${name}'. Known: ${Object.keys(ROUTINES).join(', ')}`);
    return 1;
  }

  try {
    process.stdout.write(await routine.run(argv));
    return 0;
  } catch (error) {
    console.error(`Routine '${name}' failed:`, error instanceof Error ? error.message : error);
    return 1;
  }
}

main().then(
  (code) => process.exit(code),
  (error) => {
    console.error(error);
    process.exit(1);
  },
);
