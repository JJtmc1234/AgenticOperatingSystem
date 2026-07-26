import * as dailyBrief from './routines/daily-brief.js';

/**
 * Routine runner. Routines are deliberately deterministic data gatherers rather than agent
 * loops: a brief that reports facts is useful every single morning, whereas one that asks a
 * model to go find things is slower, costs tokens, and can be wrong about whether your work
 * is committed. Reasoning belongs on top of this output, not inside it.
 */
const ROUTINES: Record<string, { describe: string; run: () => Promise<string> }> = {
  'daily-brief': {
    describe: 'Repo state, open issues and Downloads clutter, saved as markdown.',
    run: dailyBrief.run,
  },
};

async function main(): Promise<number> {
  const name = process.argv[2];

  if (!name || name === '--help' || name === 'list') {
    console.log('Usage: node dist/index.js <routine>\n\nRoutines:');
    for (const [key, routine] of Object.entries(ROUTINES)) {
      console.log(`  ${key.padEnd(14)} ${routine.describe}`);
    }
    return name ? 0 : 1;
  }

  const routine = ROUTINES[name];
  if (!routine) {
    console.error(`Unknown routine '${name}'. Known: ${Object.keys(ROUTINES).join(', ')}`);
    return 1;
  }

  try {
    process.stdout.write(await routine.run());
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
