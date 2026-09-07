# Issue 45 verification

The original page and an isolated comparison were tested with the same delayed-response Playwright test.

| Version | Run | Stored messages | Displayed messages |
|---|---:|---:|---:|
| original | 1 | 1 | 2 |
| original | 2 | 1 | 2 |
| original | 3 | 1 | 2 |
| control | 1 | 1 | 1 |
| control | 2 | 1 | 1 |
| control | 3 | 1 | 1 |

The original test page matches the current unmodified portal source byte for byte.
SHA256: `cb06afa1807a64423ff85db4f717289feeb8689ff6e090ecb182a8a4defecf3c`.

The comparison changes only the display loop to skip message IDs already seen.
It is an isolated diagnostic change, not a deployed application fix.
The test database and accounts are synthetic. Response delay is deliberately introduced.
These results do not establish how often the issue occurs on the deployed room.
