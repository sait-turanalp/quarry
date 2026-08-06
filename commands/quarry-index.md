---
description: Build or refresh the Quarry index for the current project
---

Set up Quarry for this project so its search tools have something to search.

1. Check the binary is there: run `quarry --version`. If it is missing, tell the user to
   install it (`brew install sait-turanalp/quarry/quarry`, or from the releases page at
   https://github.com/sait-turanalp/quarry/releases) and stop.

2. From the project root, run `quarry init` unless `.quarry/settings.toml` already exists.

3. Run `quarry index .` — add `--force` only if the user asked for a rebuild or the index
   is known to be stale.

4. Report what was indexed: the symbol count and file count from the output. If it indexed
   far fewer files than the project has, say so — tests are excluded by default, which is
   often not what someone searching their own codebase wants, and
   `QUARRY_INDEXING__INCLUDE_TESTS=true` includes them.

Then confirm it works with one real query against this codebase, phrased in plain English
rather than as an identifier, and show the top result.

$ARGUMENTS
