# Build the legs before the fleet

The most expensive mistake made on this project, twice.

An army of ten agents ran overnight on Factorio. In the morning there were 31 furnaces and zero
plates. Nothing had ever worked. JJ said he expected more than that, and he was right.

The orchestration was fine. The primitives underneath it were broken in three ways that a
single worker walking to a single ore patch would have exposed in two minutes.

- `nearest()` returned an arbitrary entity rather than the closest one.
- `mine_entity` returned false on success, so the mining loop broke every time it worked.
- `insert` and `take` matched the agent's own character instead of the machine.

## The rule

Prove one worker can do the whole loop before you start ten. JJ wrote the order himself and it
is the right one.

1. One worker: walk, find nearest coal, mine, refuel, mine iron and copper, smelt both, verify
   the inventory, clear the intent.
2. Two workers: confirm task locking, lease expiry, and no duplicated construction.
3. The full army for ten supervised minutes.
4. Only then an unattended run, and only after stable electricity and automated plates.

No deploying the fleet first and discovering the legs do not work later.

## The second half of the lesson

JJ pushed back on the first postmortem for being too generous. It called the failure a process
failure. His correction:

> The broken primitives were the primary cause, but a healthy coordinator should have noticed
> zero plates and 31 unusable furnaces and halted the run.

Both are true. A supervisor that cannot recognise an absurd state is not supervising. Anything
that runs unattended needs a check that asks whether the world makes sense, not only whether
the last step returned success.
