---
description: Update Quarry and refresh this project's index
---

Bring Quarry up to date and make sure this project's index matches the current code.

1. Note the current version: `quarry --version`.

2. Update the binary, whichever way it was installed:
   - Homebrew: `brew update && brew upgrade quarry`
   - From source: `cargo install --git https://github.com/sait-turanalp/quarry --force`
   - Prebuilt binary: point the user at https://github.com/sait-turanalp/quarry/releases
     and stop; downloading and replacing a binary is theirs to do, not yours.

3. Report the version afterwards. If it did not change, say so plainly rather than implying
   an update happened.

4. Re-index this project: `quarry index .`. Add `--force` if the version changed, because a
   new index format or a changed embedding model makes the old index stale in ways nothing
   will warn you about.

5. Wait about five seconds before querying. A running MCP server re-reads the index from
   disk on a short interval, so a search issued the instant indexing finishes can still come
   back empty.

If the version changed, restart the MCP server so the new binary is the one serving: in
Claude Code that is `/mcp` and reconnecting, or restarting the session.

$ARGUMENTS
