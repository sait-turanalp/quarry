---
name: quarry
description: Use Quarry instead of grepping around a codebase. Read this before searching for code, locating a symbol, tracing who calls what, or changing anything whose blast radius is unknown — "where is X", "how does Y work", "find the code that", "who calls this", "what breaks if I change", "what does this module export", "what are this type's fields", and any point where the reflex is to run grep/rg/find across the repository or to open files hoping the right one is among them.
---

# Quarry

A local code intelligence engine, reachable over MCP. It knows where things are, what
calls what, and what a change would touch. Use it before reaching for grep.

Requires `quarry` on PATH and an index in the project (`/quarry-index` builds one).

## Which tool answers which question

| the question | tool |
|---|---|
| Where is the code that does *this*? | `semantic_search_chunks` |
| Which symbol is this, by what it does? | `semantic_search_docs` |
| Same, plus docs, callers and impact at once | `semantic_search_with_context` |
| Where is this exact name defined? | `find_symbol` |
| Show me its source | `get_source` |
| What calls this? | `find_callers` |
| What does this call? | `get_calls` |
| The whole downstream tree | `get_call_tree` |
| **What breaks if I change this?** | `analyze_impact` |
| This type's fields and methods | `get_type_fields` |
| What is public in this file? | `get_module_exports` |
| Architecture, stack, module relationships | `get_project_overview` |
| React hooks in a component | `get_state_graph` |
| The same search over markdown and docs | `search_documents` |

## How to use it well

**Ask in your own words.** The engine matches on what code *does*, not on how it is
spelled. "where do we decide a password is too common" works; guessing the identifier does
not have to.

**Read the snippet before opening the file.** Results carry line ranges and a body. Open
the file only when the snippet is genuinely not enough. This is the entire point: the
answer costs a hundred tokens instead of twenty thousand.

**Ask for more results rather than re-phrasing.** Recall climbs steeply with the limit:
the wanted file is in the first ten about 76% of the time and in the first twenty about
83%, at the same latency. A second search with different words usually costs more than
`limit=20` did.

**Before editing a symbol, call `analyze_impact`.** One call answers what a grep loop
answers in eight file reads, and it sees relationships that text search cannot: type
usage, trait implementations, composition.

**It is not always first.** The promise is "almost always in the list", so scan the
results rather than assuming the top hit is the answer.

## When grep is still right

Quarry ranks; grep is exhaustive. Use grep when you need **every** occurrence of an exact
string — renaming a literal, auditing a call signature, counting usages. For "where is the
thing that does X", Quarry is both more accurate and vastly cheaper.

## If there is no index

Searches come back empty when the project has never been indexed, and the tool says so
rather than pretending the query was bad. Run `/quarry-index`, or `quarry init && quarry
index .` in the project root.

**After indexing, wait a moment before searching.** The server holds the index in memory
and checks the disk every few seconds, so a search fired immediately after `quarry index`
can still answer from the empty snapshot. Give it five seconds and retry; nothing needs
restarting.

The index is a snapshot of the code at the time it was built. After a large change, run it
again, or start the server with `quarry serve --watch` to keep it current automatically.
