//! `quarry search` - ask in plain English, get ripgrep-shaped output.
//!
//! The engine has always been reachable through `quarry mcp semantic_search_chunks
//! query:"..."`, which is the right shape for an agent and the wrong shape for a person.
//! Nobody replaces a reflex they have to look up the syntax for, so this is the same
//! search wearing the interface the reflex expects: a query, a list of paths and line
//! numbers, colour when a terminal is watching and none when it is not.

use owo_colors::OwoColorize;
use std::io::IsTerminal;

use crate::indexing::IndexFacade;

/// How many lines of a chunk to show before it stops being a preview.
const PREVIEW_LINES: usize = 3;

pub fn run(
    indexer: &IndexFacade,
    query: &str,
    limit: usize,
    lang: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let outcome = indexer
        .hybrid_chunk_search_detailed(query, limit, lang)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if json {
        let payload: Vec<_> = outcome
            .results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "path": r.filepath,
                    "line_start": r.line_start,
                    "line_end": r.line_end,
                    "scope": r.parent_scope,
                    "language": r.language,
                    "score": r.score,
                    "snippet": r.snippet,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if outcome.results.is_empty() {
        eprintln!("no matches for {query:?}");
        eprintln!("Suggestion: name a symbol, module or behaviour rather than a whole sentence.");
        return Ok(());
    }

    let colour = std::io::stdout().is_terminal();
    for r in &outcome.results {
        let location = format!("{}:{}-{}", r.filepath, r.line_start, r.line_end);
        let scope = r.parent_scope.as_deref().unwrap_or_default();

        if colour {
            print!("{}", location.magenta().bold());
            if !scope.is_empty() {
                print!("  {}", scope.dimmed());
            }
            println!();
        } else {
            println!(
                "{location}{}{scope}",
                if scope.is_empty() { "" } else { "  " }
            );
        }

        for (offset, line) in r.snippet.lines().take(PREVIEW_LINES).enumerate() {
            let number = r.line_start as usize + offset;
            if colour {
                println!("{:>6} {}", number.green(), line);
            } else {
                println!("{number:>6} {line}");
            }
        }
        println!();
    }

    // The count is the useful part of the footer: it tells the reader whether the answer
    // being absent means "not indexed" or "look further down".
    eprintln!(
        "{} results in {} ms",
        outcome.results.len(),
        outcome.timing.total
    );
    Ok(())
}
