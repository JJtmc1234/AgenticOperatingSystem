# Factorio

Paused as of 2026 08 28. Kept because the facts were expensive and are easy to lose.

Two instances. The client is `~/.factorio`, the dedicated server is `~/factorio-server`.
Version 2.1. `claude-companion` is JJ's own mod and is not for the mod portal.

## Hard won API facts

- **`find_entities_filtered{radius, limit=1}` returns an arbitrary entity, not the nearest.**
  This caused what looked like agents wandering. Measure the distance yourself.
- **`mine_entity` returns false when it mines a unit without depleting the tile.** Count the
  inventory instead of trusting the return value.
- A character is a `force='player'` entity, so an unfiltered position lookup matches the agent
  itself. This is why `insert` and `take` kept acting on the wrong thing.
- `walking_state` ticks for a playerless character on 2.1. `mining_state` does not.
  `request_path` returns a request id and delivers by event, which console Lua cannot reach.
- Craft item triggers do not fire for a playerless character. Furnace production does raise the
  event, hand crafting by a characterless character does not.
- `defines.inventory.furnace_source` does not exist in 2.1. Fuel is 1, source is 2, result is 3.
- Entities snap to half tile centres. An offshore pump connects at its own position in the
  direction it faces.
- Factorio rewrites `mod-list.json` on startup and enables every zip it finds.

## Status codes

Decode before guessing. `1` working, `12` full output, `17` no power, `18` no ingredients,
`19` no fuel, `21` no minable resources, `27` missing required fluid, `32` waiting for source
items, `34` waiting for space in destination.

## Standing instructions from JJ

- Never cheat entities in.
- Do not touch the storehouse near 0,0.
- Take materials from belts. The chests are walled in and the ground is mined out.
- Teleport directly across surfaces. Space Exploration's `teleport_to_zone` silently does
  nothing.
- Do not run the cleanup script. It destroyed his character once.
