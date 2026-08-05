# Token cost

Recall says how often the right file is found. It says nothing about the price, and price
is the whole argument for putting a retrieval engine in front of an agent. An agent working
from grep pays for every file it opens looking for the one it needed. An agent working from
Quarry pays for the snippets it was handed.

This measures that difference. It is the same discipline as `../retrieval/`: the baseline is
written down before the run, the losses are reported next to the wins, and the raw
per-query numbers are committed so anyone can disagree with the summary.

## What is compared

Three arms answer the same 1376 queries on the same four repositories, and are scored
identically: **tokens spent by the time the wanted file is in hand.**

| arm | what it pays for |
|---|---|
| **Quarry** | the snippets one `search` call returns |
| **grep+read** | whole files, opened in rank order |
| **grep+context** | 20 lines either side of every match in those files |

`grep+read` is the obvious baseline. `grep+context` exists because a careful agent does not
read whole files, and a comparison that ignores that is one a reader is right to distrust.
It is the harder opponent and it is the number to quote.

## The baseline, pre-registered

Written here before the first run, so it cannot be quietly weakened afterwards:

1. Content words are extracted from the query; stop-words and changelog verbs are dropped,
   camelCase and snake_case are split, and the six longest identifiers are kept.
2. Those terms are searched with `rg -i`, one pass per term.
3. Files are ranked by how many *distinct* terms they contain, then by total hits, then by
   path length.
4. Files are read in that order until the wanted file has been read, or a 100,000 token
   budget is exhausted.

The Quarry arm gets one call, `limit=20`, and pays for every snippet up to and including
the one that contains the answer.

## Counting

Tokens are counted with **tiktoken `o200k_base`**, not estimated. A `chars/4` figure is
recorded alongside every measurement because other projects publish that way and the two
should be comparable; both are in the JSON.

## Reading the results

- **Median and p90**, not the mean. A handful of very large files drag a mean around and
  make the result look better than it is.
- **Paired only.** A query counts toward the ratio only when *both* arms found the file at
  that depth. Queries where grep never found it are excluded, which removes the cases where
  grep's cost would be unbounded and the comparison most flattering to us.
- **Per repository**, never pooled into one number.
- `paired_n` against the total is itself a result: it says how often the two methods are
  even comparable.

## Running it

```bash
python3 search_tokens.py <eval.jsonl> <repo> <quarry-bin> <label> [n] [out.json]
```

The eval sets come from `../retrieval/suite.py prepare`, which builds them from git history:
a commit message is the query, the files that commit changed are the answer, and the index
is built from the parent commit so the code has not yet been touched by the change being
asked about.

Raw results are in `results/`.

## What this does not measure

- **Structural questions.** "What breaks if I change this" is where the gap should be
  largest, because grep has to read files to answer it at all and `analyze_impact` does not.
  Measuring it honestly needs an independent oracle (rust-analyzer, jedi, tsserver) rather
  than Quarry's own index, or the benchmark grades itself. Not done yet.
- **Whether the agent then answers correctly.** This measures the cost of getting the file
  in front of the model, not what the model does with it.
