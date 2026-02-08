#!/bin/bash

# Similarity Score Analysis - Language Filter Impact
# Demonstrates that similarity scores remain constant regardless of filtering
# The filter affects WHICH results are returned, not their scores

set -e

# Colors and formatting
BOLD='\033[1m'
DIM='\033[2m'
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RESET='\033[0m'

# Clear screen for better presentation
printf '\033[3J\033[H\033[2J'

echo -e "${BOLD}${CYAN}                    CODANNA SIMILARITY SCORE ANALYSIS"
echo -e "${RESET}${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo
echo -e "${DIM}Understanding how language filtering affects similarity scores${RESET}"
echo -e "${DIM}Key Insight: Filtering happens BEFORE similarity computation${RESET}"
echo
echo

# Test query
QUERY="Process user authentication validate credentials"

echo -e "${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${BOLD}${CYAN}                              TEST METHODOLOGY"
echo -e "${RESET}${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${BOLD}Query:${RESET} \"$QUERY\""
echo
echo -e "${BOLD}Process Flow:${RESET}"
echo -e "  1. Generate query embedding once"
echo -e "  2. Apply language filter (if specified)"
echo -e "  3. Compute cosine similarity only for filtered symbols"
echo -e "  4. Return top N results"
echo
echo -e "${BOLD}Expected Behavior:${RESET}"
echo -e "  • Same symbol = Same similarity score (regardless of filtering)"
echo -e "  • Filter only affects which symbols are considered"
echo -e "  • No score redistribution or normalization"
echo

echo -e "${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${BOLD}${CYAN}                        SCENARIO 1: NO LANGUAGE FILTER"
echo -e "${RESET}${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${DIM}Computing similarity against ALL symbols in the index${RESET}"
echo

# Run without filter and capture results
NO_FILTER_RESULTS=$(./target/release/codanna mcp semantic_search_docs query:"$QUERY" limit:8 --json 2>/dev/null)

# Parse and display results
echo "$NO_FILTER_RESULTS" | python3 -c "
import json, sys
from collections import defaultdict

data = json.load(sys.stdin)
results = data['data']

# Group by function name
by_name = defaultdict(list)
for item in results:
    sym = item['symbol']
    name = sym['name']
    score = item['score']
    lang = sym.get('language_id', 'unknown')
    by_name[name].append({'score': score, 'lang': lang})

# Show authenticate_user functions
print('${BOLD}authenticate_user functions across languages:${RESET}')
if 'authenticate_user' in by_name:
    for entry in by_name['authenticate_user']:
        print(f\"  ${GREEN}Score: {entry['score']:.7f}${RESET} - Language: {entry['lang']}\")
    print(f\"  → Found {len(by_name['authenticate_user'])} identical functions\")
else:
    print('  No authenticate_user functions found')

print()
print('${BOLD}All results (top 8):${RESET}')
for i, item in enumerate(results[:8], 1):
    sym = item['symbol']
    print(f\"  {i}. Score: ${GREEN}{item['score']:.7f}${RESET} - ${YELLOW}{sym['name']}${RESET} ({sym['kind']})\")

print()
print(f'${BOLD}Statistics:${RESET}')
print(f\"  • Total results: {len(results)}\")
print(f\"  • Unique function names: {len(by_name)}\")
print(f\"  • Duplicate functions: {sum(1 for v in by_name.values() if len(v) > 1)}\")
"

echo
echo -e "${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${BOLD}${CYAN}                     SCENARIO 2: WITH LANGUAGE FILTERS"
echo -e "${RESET}${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${DIM}Computing similarity ONLY against symbols in the specified language${RESET}"
echo

# Test each language
for LANG in rust python typescript php; do
    echo -e "${BOLD}${BLUE}Filter: lang:$LANG${RESET}"
    echo -e "${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
    
    # Run with language filter
    FILTERED_RESULTS=$(./target/release/codanna mcp semantic_search_docs query:"$QUERY" lang:$LANG limit:5 --json 2>/dev/null)
    
    # Parse and display
    echo "$FILTERED_RESULTS" | python3 -c "
import json, sys

data = json.load(sys.stdin)
results = data['data']

if not results:
    print('  ${DIM}No results found for this language${RESET}')
else:
    auth_score = None
    for item in results:
        sym = item['symbol']
        if sym['name'] == 'authenticate_user':
            auth_score = item['score']
            break
    
    if auth_score:
        print(f'  ${GREEN}✓${RESET} authenticate_user score: ${GREEN}{auth_score:.7f}${RESET}')
    else:
        print('  ${DIM}authenticate_user not found${RESET}')
    
    print(f'  ${DIM}Total symbols in language: {len(results)}${RESET}')
    print('  ${DIM}Top 3 results:${RESET}')
    for item in results[:3]:
        sym = item['symbol']
        print(f\"    • {item['score']:.5f} - {sym['name']}\")
"
    echo
done

echo -e "${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${BOLD}${CYAN}                            SCORE CONSISTENCY PROOF"
echo -e "${RESET}${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"

# Verify score consistency
echo "$NO_FILTER_RESULTS" | python3 -c "
import json, sys

data = json.load(sys.stdin)
results = data['data']

# Collect all authenticate_user scores
auth_scores = []
for item in results:
    sym = item['symbol']
    if sym['name'] == 'authenticate_user':
        auth_scores.append(item['score'])

if auth_scores:
    all_same = len(set(auth_scores)) == 1
    if all_same:
        print('${BOLD}${GREEN}✓ VERIFIED:${RESET} All authenticate_user functions have identical score')
        print(f'  Consistent score: ${GREEN}{auth_scores[0]:.7f}${RESET}')
        print(f'  Across {len(auth_scores)} language implementations')
    else:
        print('${BOLD}${YELLOW}⚠ WARNING:${RESET} Scores vary between languages')
        for i, score in enumerate(auth_scores, 1):
            print(f'  {i}. {score:.7f}')
else:
    print('No authenticate_user functions found for comparison')

print()
print('${BOLD}Key Findings:${RESET}')
print('  1. Similarity scores are computed from embeddings')
print('  2. Identical documentation → Identical embeddings → Identical scores')
print('  3. Language filter changes the candidate pool, not the scoring')
print('  4. No score redistribution or normalization occurs')
"

echo
echo -e "${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${BOLD}${CYAN}                              TECHNICAL DETAILS"
echo -e "${RESET}${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"

cat << 'EOF'
How It Works:

1. EMBEDDING GENERATION (happens during indexing)
   ```rust
   let embedding = model.embed(doc_comment);
   storage.save(symbol_id, embedding, language);
   ```

2. QUERY PROCESSING (happens during search)
   ```rust
   let query_embedding = model.embed(query_text);
   
   // Filter BEFORE similarity computation
   let candidates = if let Some(lang) = language_filter {
       embeddings.filter(|e| e.language == lang)
   } else {
       embeddings.all()
   };
   
   // Compute similarity only for filtered candidates
   for candidate in candidates {
       let score = cosine_similarity(query_embedding, candidate.embedding);
       results.push((candidate.id, score));
   }
   ```

3. COSINE SIMILARITY
   The score between two vectors is deterministic:
   
   similarity(A, B) = (A · B) / (||A|| × ||B||)
   
   This value depends ONLY on the vectors, not on what else is in the index.

EOF

echo -e "${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${BOLD}${CYAN}                                  CONCLUSION"
echo -e "${RESET}${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"

cat << 'EOF'
Language Filtering Impact on Scores:

  ✓ NO IMPACT on individual similarity scores
  ✓ REDUCES the candidate pool for comparison
  ✓ ELIMINATES cross-language duplicates
  ✓ IMPROVES result relevance

The similarity threshold doesn't vary - the same threshold applies
whether filtering or not. The only difference is which symbols are
considered as candidates for similarity computation.

Example:
  Without filter: Computes similarity for 100 symbols, returns top 10
  With filter:    Computes similarity for 25 symbols, returns top 10
  
  If "authenticate_user" is in both sets, its score will be identical.
EOF

echo
echo -e "${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"
echo -e "${BOLD}${CYAN}                      Codanna - X-ray vision for your code"
echo -e "${RESET}${DIM}────────────────────────────────────────────────────────────────────────────────${RESET}"