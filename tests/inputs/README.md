# Real-data inputs

What this crate's real-data tests read, as tests/inputs/mod.rs declares them: the keys of the configuration file
`GAME_DATA` names (its layout is that of [primase-testdata](https://github.com/Kalmarv/primase-testdata), which the
tests use), the oracle sets and the files of each, and the debug keys `GAME_DATA_DEBUG` sets. A test whose input is not
configured skips with a line naming its key; the "gate" column marks the inputs that fail instead of skipping when the
file says `[run] require = 'gate'`. The tables below are generated from the declarations (the `real_data_inputs` test
fails when they differ); [the README](../../README.md#running-the-tests) says how to set the inputs up.

<!-- real-data inputs: made by primase_testdata::doc::tables from tests/inputs/mod.rs -->

#### Configuration keys

| key | gate | about | source |
| --- | --- | --- | --- |
| `installs.fh6` | - | Forza Horizon 6's install folder (the folder that holds `media`) | the user's own install |
| `keys.fh6` | - | the user's Forza Horizon 6 container keys: a text file in the layout `keys::Keys::parse` reads, with the usages `GameDB` and `General` and `gamedb.scramble` | the user's own copy of the keys; none is distributed here |
| `scratch.root` | - | where real-data tests write their outputs (the OS temporary folder when unset) | - |

#### Declared files

| key | status | gate | about | source |
| --- | --- | --- | --- | --- |
| `installs.fh6/media/Stripped/gamedbRC.slt` | unheld: Forza Horizon 6 is not installed on any machine that runs these tests yet | - | the game database: a large-block container, read by tests/real_data.rs to an SQLite image | the user's own install |
| `installs.fh6/media/NerdData/NerdData.json` | unheld: Forza Horizon 6 is not installed on any machine that runs these tests yet | - | a one-block small container, read by tests/real_data.rs with every MAC checked | the user's own install |

<!-- end of real-data inputs -->
