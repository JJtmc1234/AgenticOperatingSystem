# Evan repository policies

These files record exact operator scopes used on Nexus. Copy a policy into the
`repositories` mapping in `~/.carl/evan/config.json`. They are not loaded automatically.

`holoprojector.json` covers confirmed Iris issue 2 in `JJtmc1234/Holoprojector`.
Only `src/holo/app/input_map.py` may be repaired. Only
`tests/test_input_map_evan.py` may be created or changed as the regression.

The first command checks six existing keyboard behaviors with Python's standard
library. The second runs the assigned regression tests. Before the new regression
exists, that command discovers zero tests. The six baseline checks still run.
This is targeted keyboard validation, not Holoprojector's full pytest suite or a
physical display test. Both commands run in Evan's isolated environment.

Publication is disabled on Nexus. Prepared fixes remain local for review. The
workflow must still confirm the real issue against Iris's publication journal.
Deliberate validation issues are excluded.
