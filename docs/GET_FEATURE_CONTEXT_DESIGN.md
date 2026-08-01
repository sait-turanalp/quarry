# get_feature_context Tool - Design Document

**Date:** 2026-02-06
**Status:** APPROVED - Ready for Implementation
**Decision:** Smart Defaults (All True)

---

## Problem Statement

Current workflow requires AI assistants to make **6 separate MCP tool calls** to gather complete context:

```
find_symbol → get_source → find_callers → get_calls → get_type_fields → analyze_impact
= ~800ms + 6 responses + AI must orchestrate
```

**Goal:** Single tool that provides comprehensive feature context in one call.

---

## Infrastructure Analysis

### Available Facade Methods (%100 Ready)

✅ `get_symbol(symbol_id)` → Symbol
✅ `get_symbol_context(symbol_id, ContextIncludes)` → SymbolContext with relationships
✅ `get_calling_functions_with_metadata(symbol_id)` → Vec<(Symbol, RelationshipMetadata)>
✅ `get_called_functions_with_metadata(symbol_id)` → Vec<(Symbol, RelationshipMetadata)>
✅ `get_impact_radius(symbol_id, max_depth)` → Vec<SymbolId>
✅ `get_file_path(file_id)` → Option<String>

### Existing Patterns

✅ **get_source pattern:** File reading + line range extraction + formatting
✅ **SymbolContext:** Already has format_full() for comprehensive output
✅ **RelationshipMetadata:** Contains call site line numbers for snippets

**Conclusion:** All infrastructure exists. Only need to compose existing methods.

---

## Design Decision: Smart Defaults vs Optional

### Option 1: Optional Parameters (AI Decides)

```rust
pub struct GetFeatureContextRequest {
    pub symbol_id: u32,
    #[serde(default)]  // default: false
    pub include_source: bool,
    #[serde(default)]  // default: false
    pub include_impact: bool,
    #[serde(default)]  // default: false
    pub show_call_examples: bool,
    // + 5 more limit parameters
}
```

**Usage:**
```json
{"symbol_id": 123, "include_source": true, "include_impact": true, "show_call_examples": true, ...}
```

**Cons:**
- AI must decide 8 parameters every call
- Risk of wrong toggle → incomplete context → poor response
- Defeats purpose of "one comprehensive call"

---

### Option 2: Smart Defaults (CHOSEN) ⭐

```rust
pub struct GetFeatureContextRequest {
    pub symbol_id: u32,

    // ALL default to TRUE
    #[serde(default = "default_true")]
    pub include_source: bool,
    #[serde(default = "default_true")]
    pub include_impact: bool,
    #[serde(default = "default_true")]
    pub show_call_examples: bool,

    // Sensible limits
    #[serde(default = "default_max_callers")]     // 10
    pub max_callers: u32,
    #[serde(default = "default_max_calls")]       // 10
    pub max_calls: u32,
    #[serde(default = "default_max_impact")]      // 20
    pub max_impact: u32,
    #[serde(default = "default_impact_depth")]    // 2
    pub impact_depth: u32,
    #[serde(default = "default_max_examples")]    // 3
    pub max_examples: u32,
    #[serde(default = "default_context_lines")]   // 5
    pub context_lines: u32,
}
```

**Usage:**
```json
{"symbol_id": 123}  // ← Gets EVERYTHING with sensible limits
```

**Pros:**
- AI writes minimal JSON, gets full context
- Consistent behavior across all calls
- Still flexible: AI can disable features if needed
- Performance acceptable: ~220ms avg, ~4K tokens

**Rationale:**
- Tool purpose is "comprehensive context in one call"
- 220ms is fast enough for full context
- Token cost (4K) is cheaper than 6 separate calls (6× overhead)
- Large projects protected by smart limits (max 200 lines output)

---

## Parameter Specification

### Required
- `symbol_id: u32` — Target symbol ID

### Optional (Smart Defaults)

| Parameter | Default | Purpose | Notes |
|-----------|---------|---------|-------|
| `include_source` | `true` | Show source code | Core feature |
| `include_impact` | `true` | Show impact analysis | Critical for refactoring |
| `show_call_examples` | `true` | Show call site snippets | High value |
| `max_callers` | `10` | Limit caller list | Prevents explosion |
| `max_calls` | `10` | Limit calls list | Prevents explosion |
| `max_impact` | `20` | Limit impact results | Prevents explosion |
| `impact_depth` | `2` | BFS depth | 2 is sweet spot |
| `max_examples` | `3` | Call site examples | Enough to understand usage |
| `context_lines` | `5` | Source context lines | Balances readability |

---

## Output Format

### Small Function (5 callers, 20 impact)
**~100 lines, ~2.5K tokens, ~180ms**

### Medium Component (15 callers, 50 impact)
**~150 lines, ~4K tokens, ~220ms**

### Large Utility (150+ callers, 400+ impact)
**~200 lines (truncated), ~4K tokens, ~250ms**

Truncation message example:
```
Called by 156 function(s): (showing first 10)
  ... (10 items)
  ... and 146 more (use max_callers:20 to see more)
```

### Output Structure

```
[Header]
- Symbol name, kind, location, signature
- Module, visibility, documentation

[Relationships]
- Called by N function(s): (list with limits)
- Calls N function(s): (list with limits)
- Uses N type(s): (list)
- Implements/Extends: (if applicable)

[Source Code]
```language
line | code
line | code
```

[Impact Analysis]
- Changing X will affect N symbol(s)
- Direct Dependents (N): (list with limits)
- Indirect Dependents - Level 2 (N): (list with limits)
- WARNING if high impact (>100)

[Call Site Examples]
Example 1/N: Called from Y
Location: path:line
```language
context before
CALL SITE ← marker
context after
```

[Guidance]
💡 TIP: contextual suggestions
```

---

## Performance Characteristics

| Component | Time | Scaling |
|-----------|------|---------|
| get_symbol_context | ~50ms | O(relationships) |
| Source file read | ~20ms | O(1) |
| Impact BFS (depth=2) | ~100ms | O(dependents × depth) |
| Call site snippets (3×) | ~50ms | O(examples) |
| **Total** | **~220ms** | **Linear with limits** |

**Worst case (large project, all limits hit):** ~250ms

**Token usage:**
- Small: ~1K tokens
- Medium: ~2.5K tokens
- Large (truncated): ~4K tokens

**Comparison to 6-tool approach:**
- 6 tools: ~800ms + 6× parsing overhead + 6× AI reasoning
- 1 tool: ~220ms + 1× parsing + 0× AI reasoning
- **Speedup: ~3.6×**

---

## Implementation Phases

### Phase 1: V1 - Basic Context (1-2 hours)
**Features:**
- Symbol metadata
- Source code
- Callers
- Calls
- Type relationships

**Files:**
- `src/mcp/mod.rs`: Request struct + tool method (~60 lines)
- `src/cli/commands/mcp.rs`: CLI routing (~10 lines)

---

### Phase 2: V3 - Impact Preview (+30-45 min)
**Features:**
- BFS impact analysis
- Grouped by depth (direct vs indirect)
- High-impact warnings

**Changes:**
- Add `format_impact_graph()` helper (~30 lines)
- Integrate with get_impact_radius()

---

### Phase 3: V5 - Call Examples (+45-60 min)
**Features:**
- Call site source snippets
- 3-line context around call
- Location markers

**Changes:**
- Add `read_call_site_snippet()` helper (~20 lines)
- Add `format_call_snippet()` helper (~15 lines)

---

### Total Implementation Time
**2.5-3.5 hours** for V1+V3+V5

---

## Future Considerations

### V6: Data Flow Analysis (NOT IMPLEMENTED)
**Estimated effort:** 2-3 days
**Reason for deferral:**
- Only 20% infrastructure ready (React hooks exist, general data flow does not)
- Requires control flow analysis, inter-procedural tracking
- High complexity, uncertain ROI
- Can be added later if demand exists

**If implemented later, would need:**
- Variable assignment tracking across languages
- Control flow graph construction
- Def-use chain analysis
- Cross-function data flow

---

### Parameter Flexibility (Future Option)

If users request more control, can expose parameters without changing defaults:

**Conservative users:**
```json
{"symbol_id": 123, "max_impact": 5, "max_examples": 1}
```

**Power users:**
```json
{"symbol_id": 123, "impact_depth": 4, "max_callers": 50, "context_lines": 10}
```

**Performance-focused:**
```json
{"symbol_id": 123, "include_impact": false, "show_call_examples": false}
```

Current design supports all these without code changes.

---

## Testing Strategy

### Test Cases

1. **Small utility function** (1-3 callers)
   - Verify all sections present
   - Check output ~50-80 lines

2. **Medium component** (10-20 callers)
   - Verify truncation messages
   - Check output ~100-150 lines

3. **Large core function** (100+ callers)
   - Verify limits applied correctly
   - Check "... and X more" messages
   - Verify high-impact warning
   - Check output capped at ~200 lines

4. **Symbol with no relationships**
   - Verify graceful empty sections
   - No errors on missing data

5. **Custom parameters**
   - Test each toggle (source off, impact off, etc.)
   - Test limit increases/decreases
   - Verify respect of user settings

### Integration Test

Run on lut-app project symbols:
- `loadLUT` (medium complexity)
- `useDualLUT` (hook with state)
- `parseCubeFile` (utility)
- `ExportModal` (component)

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-02-06 | Smart defaults (all true) | Matches "comprehensive context" goal, AI-friendly |
| 2026-02-06 | V1+V3+V5 scope | High ROI, reasonable effort (2.5-3.5h) |
| 2026-02-06 | Defer V6 (data flow) | Low infrastructure readiness (20%), high effort |
| 2026-02-06 | Max limits: 10/10/20/2/3/5 | Balances detail with token efficiency |

---

## Approval

**Implementation approved with:**
- Smart defaults (all features enabled by default)
- Truncation limits for large projects
- V1+V3+V5 scope
- V6 deferred to future

**Ready to implement.**
