# get_call_tree Implementation Plan
## Complete Downstream Call Chain & Dependency Tracking

**Date:** 2026-02-07
**Status:** Ready to Implement
**Goal:** Enable agents to understand complete execution flow and architectural dependencies

---

## 🎯 Executive Summary

This plan implements 3 critical features for agent-level code intelligence:

1. **`get_call_tree`** - Downstream recursive call tracking (what this function calls, N levels deep)
2. **Type Resolution Enhancement** - Improve from 70% to 90% accuracy (generics, unions, interfaces)
3. **File-Level Dependency Graph** - Module import/export tracking for architectural impact analysis

All three phases will be implemented to provide comprehensive codebase intelligence equivalent to a senior developer's understanding.

---

## 📋 Phase 1: get_call_tree Tool (Week 1)

**Timeline:** 3-5 days
**Priority:** Critical - Foundation for downstream analysis

### What It Does

Recursively tracks function calls downstream:
```
SignupHandler [id:42]
├─ validateInput [id:78]
│  ├─ checkEmailFormat [id:101]
│  └─ checkPasswordStrength [id:102]
├─ hashPassword [id:79]
│  └─ bcrypt.hash [id:150] (external)
└─ createUser [id:80]
   ├─ db.transaction [id:200]
   │  ├─ users.insert [id:201]
   │  └─ audit_log.insert [id:202]
   └─ sendWelcomeEmail [id:85]

Total: 11 symbols, 4 levels deep
```

### Implementation Tasks

#### 1. Backend: Downstream BFS Algorithm
**File:** `src/indexing/facade.rs`

```rust
/// Get downstream call tree with configurable depth
pub fn get_call_tree(
    &self,
    symbol_id: SymbolId,
    max_depth: usize,
) -> Vec<CallTreeNode> {
    // Implementation:
    // 1. Start with root symbol
    // 2. BFS using get_called_functions_with_metadata
    // 3. Track visited nodes (cycle detection)
    // 4. Build tree structure with depth tracking
    // 5. Return hierarchical nodes
}
```

**Key Components:**
- Use existing `get_called_functions_with_metadata` (already implemented)
- Reverse of `analyze_impact` (upstream BFS)
- Cycle detection: Track visited symbols, break loops
- Depth limiting: Stop at max_depth
- Metadata preservation: File path, line numbers

#### 2. Data Model
**File:** `src/types/call_tree.rs` (new file)

```rust
/// Node in the call tree
#[derive(Debug, Clone)]
pub struct CallTreeNode {
    /// The symbol at this node
    pub symbol: Symbol,

    /// Depth in the tree (0 = root)
    pub depth: usize,

    /// Children nodes (what this symbol calls)
    pub children: Vec<CallTreeNode>,

    /// Metadata (file, line, etc.)
    pub metadata: Option<RelationshipMetadata>,

    /// Whether this node was truncated due to depth/cycle
    pub truncated: bool,
    pub truncation_reason: Option<TruncationReason>,
}

#[derive(Debug, Clone)]
pub enum TruncationReason {
    MaxDepthReached,
    CycleDetected,
    ExternalCall,
}
```

#### 3. MCP Tool Implementation
**File:** `src/mcp/mod.rs`

```rust
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetCallTreeRequest {
    /// Symbol ID to start from
    pub symbol_id: u32,

    /// Maximum depth to traverse (default: 4)
    #[serde(default = "default_call_tree_depth")]
    pub max_depth: u32,

    /// Include source code snippets (default: false)
    #[serde(default = "default_false")]
    pub include_source: bool,

    /// Include metadata (file, line) (default: true)
    #[serde(default = "default_true")]
    pub include_metadata: bool,

    /// Show external library calls (default: false)
    #[serde(default = "default_false")]
    pub show_external_calls: bool,

    /// Maximum nodes to return (default: 100, prevents huge trees)
    #[serde(default = "default_max_nodes")]
    pub max_nodes: u32,
}

fn default_call_tree_depth() -> u32 { 4 }
fn default_max_nodes() -> u32 { 100 }

#[tool(
    description = "Get recursive downstream call tree: what this symbol calls, with full depth. \
                   Returns hierarchical tree showing execution flow. Use this to understand \
                   what happens when a function runs."
)]
pub async fn get_call_tree(
    &self,
    Parameters(GetCallTreeRequest {
        symbol_id,
        max_depth,
        include_source,
        include_metadata,
        show_external_calls,
        max_nodes,
    }): Parameters<GetCallTreeRequest>,
) -> Result<CallToolResult, McpError> {
    let indexer = self.facade.read().await;

    // Get root symbol
    let symbol = indexer.get_symbol(SymbolId(symbol_id))
        .ok_or_else(|| McpError::NotFound(format!("Symbol {} not found", symbol_id)))?;

    // Build call tree
    let tree = indexer.get_call_tree(SymbolId(symbol_id), max_depth as usize);

    // Filter external calls if requested
    let tree = if !show_external_calls {
        filter_external_calls(tree)
    } else {
        tree
    };

    // Limit total nodes
    let tree = limit_tree_size(tree, max_nodes as usize);

    // Format output
    let output = format_call_tree(
        &symbol,
        &tree,
        include_source,
        include_metadata,
        &indexer,
    );

    Ok(CallToolResult::success(vec![Content::text(output)]))
}
```

#### 4. Helper: Tree Formatter
**File:** `src/mcp/mod.rs` (helper functions section)

```rust
/// Format call tree as hierarchical text with tree drawing characters
fn format_call_tree(
    root: &Symbol,
    nodes: &[CallTreeNode],
    include_source: bool,
    include_metadata: bool,
    indexer: &IndexFacade,
) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "{} {} [symbol_id:{}]\n",
        format_symbol_kind(&root.kind),
        root.name,
        root.id.value()
    ));

    if include_metadata {
        output.push_str(&format!("Location: {}:{}\n", root.file_path, root.range.start_line + 1));
    }

    if include_source {
        output.push_str(&format_source_snippet(root, indexer));
    }

    output.push_str("\n### Call Tree\n\n");

    // Recursive tree formatting
    format_tree_nodes(&mut output, nodes, "", true, include_metadata);

    // Summary
    let total_nodes = count_nodes(nodes);
    let max_depth = calculate_max_depth(nodes);
    output.push_str(&format!(
        "\n---\nTotal: {} symbols, {} levels deep\n",
        total_nodes, max_depth
    ));

    // Add warnings
    if has_cycles(nodes) {
        output.push_str("⚠️  Note: Some cycles detected and broken\n");
    }
    if has_truncation(nodes) {
        output.push_str("⚠️  Note: Tree truncated at max depth/size limits\n");
    }

    output
}

/// Recursive tree node formatter with proper indentation
fn format_tree_nodes(
    output: &mut String,
    nodes: &[CallTreeNode],
    prefix: &str,
    is_last: bool,
    include_metadata: bool,
) {
    for (i, node) in nodes.iter().enumerate() {
        let is_node_last = i == nodes.len() - 1;
        let connector = if is_node_last { "└─" } else { "├─" };
        let extension = if is_node_last { "   " } else { "│  " };

        output.push_str(&format!(
            "{}{} {} [id:{}]",
            prefix, connector, node.symbol.name, node.symbol.id.value()
        ));

        if include_metadata {
            if let Some(ref meta) = node.metadata {
                if let Some(line) = meta.line {
                    output.push_str(&format!(" at {}:{}", node.symbol.file_path, line + 1));
                }
            }
        }

        if node.truncated {
            match node.truncation_reason {
                Some(TruncationReason::MaxDepthReached) => output.push_str(" [...]"),
                Some(TruncationReason::CycleDetected) => output.push_str(" [cycle]"),
                Some(TruncationReason::ExternalCall) => output.push_str(" (external)"),
                None => {}
            }
        }

        output.push('\n');

        // Recurse to children
        if !node.children.is_empty() {
            let new_prefix = format!("{}{}", prefix, extension);
            format_tree_nodes(output, &node.children, &new_prefix, is_node_last, include_metadata);
        }
    }
}
```

#### 5. CLI Routing
**File:** `src/cli/commands/mcp.rs`

```rust
// Add to positional arg handler (around line 172)
"get_call_tree" => {
    args_map.insert(
        "symbol_id".to_string(),
        serde_json::Value::Number(pos_arg.parse::<u64>().unwrap_or(0).into()),
    );
}

// Add to tool dispatch (around line 1240)
"get_call_tree" => {
    use crate::mcp::GetCallTreeRequest;

    let symbol_id = arguments
        .as_ref()
        .and_then(|m| m.get("symbol_id"))
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| {
            eprintln!("Error: get_call_tree requires 'symbol_id' parameter");
            std::process::exit(1);
        }) as u32;

    let max_depth = arguments
        .as_ref()
        .and_then(|m| m.get("max_depth"))
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as u32;

    let include_source = arguments
        .as_ref()
        .and_then(|m| m.get("include_source"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let include_metadata = arguments
        .as_ref()
        .and_then(|m| m.get("include_metadata"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let show_external_calls = arguments
        .as_ref()
        .and_then(|m| m.get("show_external_calls"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let max_nodes = arguments
        .as_ref()
        .and_then(|m| m.get("max_nodes"))
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as u32;

    server
        .get_call_tree(Parameters(GetCallTreeRequest {
            symbol_id,
            max_depth,
            include_source,
            include_metadata,
            show_external_calls,
            max_nodes,
        }))
        .await
}
```

#### 6. Testing Strategy

**Test Cases:**

```rust
// tests/call_tree_tests.rs

#[test]
fn test_simple_chain() {
    // A → B → C (linear chain)
    let tree = get_call_tree(symbol_a, 3);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].children.len(), 1);
    assert_eq!(tree[0].children[0].children.len(), 1);
}

#[test]
fn test_branching_tree() {
    // A → B + C (branching)
    let tree = get_call_tree(symbol_a, 2);
    assert_eq!(tree[0].children.len(), 2);
}

#[test]
fn test_cycle_detection() {
    // A → B → A (cycle)
    let tree = get_call_tree(symbol_a, 5);
    // Should detect cycle and truncate
    assert!(has_truncation(&tree));
}

#[test]
fn test_max_depth_cutoff() {
    // Deep chain: A → B → C → D → E
    let tree = get_call_tree(symbol_a, 2);
    // Should stop at depth 2
    assert_eq!(calculate_max_depth(&tree), 2);
}

#[test]
fn test_external_calls_filtering() {
    // SignupHandler → bcrypt.hash (external)
    let tree = get_call_tree(signup_handler, 3);
    // External calls should be marked
    assert!(tree[0].children.iter().any(|n| n.truncation_reason == Some(TruncationReason::ExternalCall)));
}
```

**Manual Testing:**

```bash
# Test 1: Simple function
codanna mcp get_call_tree 42

# Test 2: Deep tree
codanna mcp get_call_tree 42 max_depth:10

# Test 3: With source code
codanna mcp get_call_tree 42 include_source:true

# Test 4: Show external calls
codanna mcp get_call_tree 42 show_external_calls:true
```

### Deliverables

- [ ] Working `get_call_tree` function in `IndexFacade`
- [ ] `CallTreeNode` data model
- [ ] MCP tool with full parameter support
- [ ] CLI routing
- [ ] Tree formatter with proper visualization
- [ ] Test suite (unit + integration)
- [ ] Documentation in tool description

### Success Metrics

- Agent can query "what does this function call?" and get complete tree
- Tree depth of 4 levels returns in <500ms for typical codebases
- Cycle detection prevents infinite loops
- Output is readable and parseable
- Type resolution works for 70%+ of method calls

---

## 📋 Phase 2: Type Resolution Enhancement (Week 2-3)

**Timeline:** 1-2 weeks
**Priority:** High - Critical for call tree accuracy
**Goal:** Improve from 70% to 90% type resolution accuracy

### Current State

**What works (70%):**
- ✅ Direct method calls: `obj.method()`
- ✅ Class method resolution: `UserService.create()`
- ✅ Basic type inference: `const x: Type = ...`

**What doesn't work well:**
- ⚠️ Generic types: `Repository<T>.find()` (T not resolved)
- ❌ Union types: `(A | B).method()` (which type?)
- ❌ Complex inference: Return types, parameter types

### Implementation Tasks

#### 1. Generic Type Instantiation Tracking

**Problem:**
```typescript
class Repository<T> {
  async find(id: string): Promise<T> {
    // ...
  }
}

const userRepo = new Repository<User>();
const user = await userRepo.find("123");  // ← T = User not tracked
user.save();  // ← Can't resolve User.save()
```

**Solution:**
```rust
// src/parsing/typescript/generics.rs (new file)

/// Track generic type instantiations
pub struct GenericInstantiation {
    pub type_param: String,  // "T"
    pub concrete_type: String,  // "User"
    pub scope: Range,  // Where this instantiation is valid
}

/// Parse generic type arguments from AST
pub fn extract_generic_instantiations(
    node: Node,
    source: &str,
) -> Vec<GenericInstantiation> {
    // Parse: new Repository<User>()
    // Extract: T → User mapping
}
```

**Integration:**
- Modify `src/parsing/typescript/parser.rs` to track generic instantiations
- Store in symbol metadata
- Use during call resolution

#### 2. Union Type Member Enumeration

**Problem:**
```typescript
type Handler = AuthHandler | UserHandler | BillingHandler;

function process(handler: Handler) {
  handler.execute();  // ← Which Handler.execute?
}
```

**Solution:**
```rust
// src/types/mod.rs

/// Union type representation
pub struct UnionType {
    pub members: Vec<String>,  // ["AuthHandler", "UserHandler", "BillingHandler"]
}

/// When resolving handler.execute(), return ALL possible targets
pub fn resolve_union_call(
    union: &UnionType,
    method_name: &str,
    indexer: &IndexFacade,
) -> Vec<Symbol> {
    let mut results = Vec::new();
    for member in &union.members {
        if let Some(symbol) = indexer.find_method(member, method_name) {
            results.push(symbol);
        }
    }
    results
}
```

**Call Tree Impact:**
```
process(handler)
├─ AuthHandler.execute [possible]
├─ UserHandler.execute [possible]
└─ BillingHandler.execute [possible]

Note: Union type - all members shown
```

#### 3. Interface Implementation Tracking

**Problem:**
```typescript
interface Service {
  execute(data: any): Promise<void>;
}

class UserService implements Service {
  async execute(data: any) { ... }
}

async function run(service: Service) {
  await service.execute(data);  // ← Can't find UserService.execute
}
```

**Solution:**
```rust
// src/indexing/pipeline/stages/interface_tracking.rs (new file)

/// Track interface implementations
pub struct InterfaceImplementation {
    pub interface_id: SymbolId,
    pub implementor_id: SymbolId,
}

/// New pipeline stage
pub struct InterfaceTrackingStage;

impl PipelineStage for InterfaceTrackingStage {
    fn process(&self, context: &mut PipelineContext) {
        // For each class:
        // 1. Parse `implements` clause
        // 2. Find interface symbol
        // 3. Store implementation relationship
        // 4. Add to index
    }
}
```

**New Relationship Type:**
```rust
// src/symbol/context.rs

pub struct SymbolRelationships {
    // ... existing fields ...

    /// Interfaces this symbol implements
    pub implements_interfaces: Option<Vec<Symbol>>,

    /// Classes that implement this interface
    pub implemented_by: Option<Vec<Symbol>>,
}
```

**Call Tree Impact:**
```
run(service: Service)
└─ Service.execute [interface]
   ├─ UserService.execute [implementation]
   ├─ AuthService.execute [implementation]
   └─ BillingService.execute [implementation]

Note: Interface dispatch - showing implementations
```

#### 4. Return Type Tracking

**Problem:**
```typescript
function getService(): UserService {
  return new UserService();
}

const service = getService();
service.process();  // ← Can't resolve if return type not tracked
```

**Solution:**
- Parse return type annotations
- Store in function symbol metadata
- Use during call resolution

### Testing Strategy

**Accuracy Benchmarks:**

Create test suite with known difficult cases:
```rust
// tests/type_resolution_accuracy.rs

struct TypeResolutionTest {
    name: &'static str,
    code: &'static str,
    expected_call_target: &'static str,
}

const TESTS: &[TypeResolutionTest] = &[
    TypeResolutionTest {
        name: "generic_instantiation",
        code: "const repo = new Repository<User>(); repo.find('1')",
        expected_call_target: "Repository::find",  // ✅ Should resolve
    },
    TypeResolutionTest {
        name: "union_type",
        code: "const handler: Handler = getHandler(); handler.execute()",
        expected_call_target: "AuthHandler::execute|UserHandler::execute",  // All union members
    },
    // ... 50+ test cases
];

#[test]
fn test_type_resolution_accuracy() {
    let mut passed = 0;
    let mut failed = 0;

    for test in TESTS {
        let resolved = resolve_call_target(test.code);
        if resolved == test.expected_call_target {
            passed += 1;
        } else {
            failed += 1;
            println!("FAIL: {} - expected {}, got {}", test.name, test.expected_call_target, resolved);
        }
    }

    let accuracy = (passed as f64) / (passed + failed) as f64 * 100.0;
    println!("Type Resolution Accuracy: {:.1}%", accuracy);

    assert!(accuracy >= 90.0, "Accuracy below 90%: {:.1}%", accuracy);
}
```

### Deliverables

- [ ] Generic type instantiation tracking
- [ ] Union type member enumeration
- [ ] Interface → implementation mapping
- [ ] Return type tracking
- [ ] Accuracy test suite (90%+ pass rate)
- [ ] Integration with get_call_tree
- [ ] Documentation

### Success Metrics

- Type resolution accuracy: 90%+ (measured on test suite)
- get_call_tree completeness: 85%+ (fewer "unknown" nodes)
- No regressions on existing 70% cases

---

## 📋 Phase 3: File-Level Dependency Graph (Week 4-5)

**Timeline:** 1-2 weeks
**Priority:** High - Architectural impact analysis
**Goal:** Track module-level dependencies for blast radius analysis

### What It Does

Tracks which files import/export what, enabling architectural impact analysis:

```bash
codanna mcp get_file_dependencies "auth/utils.ts"

Output:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
File: auth/utils.ts

Exports (3):
  - AuthService (class)
  - validateToken (function)
  - hashPassword (function)

Imports (2):
  ← crypto/hash (bcrypt)
  ← db/client (DatabaseClient)

Imported By (7 files across 4 modules):
  ├─ user/ (3 files)
  │  ├─ user/handler.ts → AuthService, validateToken
  │  ├─ user/service.ts → hashPassword
  │  └─ user/validator.ts → validateToken
  ├─ billing/ (2 files)
  │  ├─ billing/service.ts → validateToken
  │  └─ billing/subscription.ts → AuthService
  ├─ admin/handler.ts → AuthService, validateToken
  └─ api/gateway.ts → validateToken

⚠️  BLAST RADIUS: Changing this file affects 4 domains!
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### Implementation Tasks

#### 1. Data Model
**File:** `src/indexing/file_graph.rs` (new file)

```rust
/// File-level dependency node
#[derive(Debug, Clone)]
pub struct FileNode {
    /// Absolute file path
    pub path: String,

    /// Symbols exported from this file
    pub exports: Vec<FileExport>,

    /// Symbols imported into this file
    pub imports: Vec<FileImport>,
}

/// Exported symbol
#[derive(Debug, Clone)]
pub struct FileExport {
    pub name: String,
    pub kind: SymbolKind,  // class, function, etc.
    pub is_default: bool,
}

/// Imported symbol
#[derive(Debug, Clone)]
pub struct FileImport {
    /// Source file (resolved path)
    pub from: String,

    /// Imported symbols
    pub symbols: Vec<String>,

    /// Is this a default import?
    pub is_default: bool,

    /// Is this a namespace import? (import * as foo)
    pub is_namespace: bool,
}

/// File dependency graph
pub struct FileDependencyGraph {
    nodes: HashMap<String, FileNode>,

    /// Adjacency list: file → files it imports
    imports: HashMap<String, Vec<String>>,

    /// Reverse adjacency: file → files that import it
    imported_by: HashMap<String, Vec<String>>,
}

impl FileDependencyGraph {
    pub fn get_dependencies(&self, file_path: &str) -> Option<&FileNode>;
    pub fn get_importers(&self, file_path: &str) -> Vec<&str>;
    pub fn get_blast_radius(&self, file_path: &str) -> Vec<&str>;
}
```

#### 2. Import/Export Parser
**File:** `src/parsing/typescript/imports.rs` (new file)

```rust
/// Parse import statements from TypeScript/JavaScript
pub fn parse_imports(source: &str, file_path: &str) -> Vec<FileImport> {
    let mut imports = Vec::new();
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_typescript::language_tsx()).unwrap();
    let tree = parser.parse(source, None).unwrap();

    // Query for import statements
    let query = Query::new(
        tree_sitter_typescript::language_tsx(),
        r#"
        (import_statement
          source: (string (string_fragment) @source)
          (import_clause
            (named_imports
              (import_specifier
                name: (identifier) @name))))
        "#,
    ).unwrap();

    // Extract import information
    for match_ in query_cursor.matches(&query, tree.root_node(), source.as_bytes()) {
        // Parse: import { AuthService, validateToken } from './auth/service'
        // Extract: from = './auth/service', symbols = ["AuthService", "validateToken"]

        let from = resolve_import_path(source_str, file_path);
        imports.push(FileImport {
            from,
            symbols: vec![name_str.to_string()],
            is_default: false,
            is_namespace: false,
        });
    }

    imports
}

/// Parse export statements
pub fn parse_exports(source: &str) -> Vec<FileExport> {
    // Query for: export class, export function, export const, export default
    // Extract symbol names and kinds
}

/// Resolve relative import paths to absolute
fn resolve_import_path(import_str: &str, current_file: &str) -> String {
    // './auth/service' + '/path/to/user/handler.ts' → '/path/to/auth/service.ts'
    // Handle: ./, ../, node_modules, etc.
}
```

#### 3. Pipeline Integration
**File:** `src/indexing/pipeline/stages/file_graph.rs` (new file)

```rust
/// Pipeline stage to build file dependency graph
pub struct FileGraphStage {
    graph: Arc<RwLock<FileDependencyGraph>>,
}

impl PipelineStage for FileGraphStage {
    fn name(&self) -> &'static str {
        "FILE_GRAPH"
    }

    fn process(&self, context: &mut PipelineContext) -> Result<()> {
        let mut graph = self.graph.write().unwrap();

        for file in &context.files {
            // Skip non-source files
            if !is_source_file(file) {
                continue;
            }

            let content = read_file(file)?;

            // Parse imports and exports
            let imports = parse_imports(&content, file);
            let exports = parse_exports(&content);

            // Create file node
            let node = FileNode {
                path: file.clone(),
                exports,
                imports: imports.clone(),
            };

            // Add to graph
            graph.add_node(node);

            // Build adjacency lists
            for import in imports {
                graph.add_edge(file, &import.from);
            }
        }

        Ok(())
    }
}
```

**Add to pipeline:**
```rust
// src/indexing/pipeline/mod.rs

pub fn build_pipeline(settings: &Settings) -> Pipeline {
    Pipeline::new(vec![
        Box::new(CollectStage::new()),
        Box::new(IndexStage::new()),
        Box::new(ContextStage::new()),
        Box::new(ResolveStage::new()),
        Box::new(FileGraphStage::new()),  // ← NEW
        Box::new(WriteStage::new()),
    ])
}
```

#### 4. MCP Tool Implementation
**File:** `src/mcp/mod.rs`

```rust
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetFileDependenciesRequest {
    /// File path to analyze
    pub file_path: String,

    /// Show imports (what this file imports)
    #[serde(default = "default_true")]
    pub show_imports: bool,

    /// Show exports (what this file exports)
    #[serde(default = "default_true")]
    pub show_exports: bool,

    /// Show importers (who imports this file)
    #[serde(default = "default_true")]
    pub show_imported_by: bool,

    /// Calculate blast radius (all affected files)
    #[serde(default = "default_true")]
    pub calculate_blast_radius: bool,
}

#[tool(
    description = "Get file-level import/export dependencies. Shows which modules import/export \
                   what, enabling architectural impact analysis. Use this to understand blast \
                   radius of changes to a file."
)]
pub async fn get_file_dependencies(
    &self,
    Parameters(GetFileDependenciesRequest {
        file_path,
        show_imports,
        show_exports,
        show_imported_by,
        calculate_blast_radius,
    }): Parameters<GetFileDependenciesRequest>,
) -> Result<CallToolResult, McpError> {
    let indexer = self.facade.read().await;

    // Get file node from graph
    let file_graph = indexer.get_file_dependency_graph();
    let node = file_graph.get_dependencies(&file_path)
        .ok_or_else(|| McpError::NotFound(format!("File not found: {}", file_path)))?;

    let mut output = String::new();

    output.push_str(&format!("File: {}\n\n", file_path));

    // Exports
    if show_exports && !node.exports.is_empty() {
        output.push_str(&format!("Exports ({}):\n", node.exports.len()));
        for export in &node.exports {
            let default_marker = if export.is_default { " (default)" } else { "" };
            output.push_str(&format!("  - {} ({:?}){}\n", export.name, export.kind, default_marker));
        }
        output.push('\n');
    }

    // Imports
    if show_imports && !node.imports.is_empty() {
        output.push_str(&format!("Imports ({}):\n", node.imports.len()));
        for import in &node.imports {
            let symbols = import.symbols.join(", ");
            output.push_str(&format!("  ← {} ({})\n", import.from, symbols));
        }
        output.push('\n');
    }

    // Imported by
    if show_imported_by {
        let importers = file_graph.get_importers(&file_path);
        if !importers.is_empty() {
            output.push_str(&format!("Imported By ({} files):\n", importers.len()));

            // Group by module/directory
            let grouped = group_by_module(&importers);
            for (module, files) in grouped {
                output.push_str(&format!("  ├─ {} ({} files)\n", module, files.len()));
                for (i, file) in files.iter().enumerate() {
                    let is_last = i == files.len() - 1;
                    let connector = if is_last { "└─" } else { "├─" };
                    output.push_str(&format!("  │  {} {}\n", connector, file));
                }
            }
            output.push('\n');
        }
    }

    // Blast radius
    if calculate_blast_radius {
        let affected = file_graph.get_blast_radius(&file_path);
        let unique_modules = count_unique_modules(&affected);

        if affected.len() > 1 {
            output.push_str(&format!(
                "⚠️  BLAST RADIUS: Changing this file affects {} files across {} modules!\n",
                affected.len(),
                unique_modules
            ));
        }
    }

    Ok(CallToolResult::success(vec![Content::text(output)]))
}
```

#### 5. CLI Routing
**File:** `src/cli/commands/mcp.rs`

```rust
// Positional arg
"get_file_dependencies" => {
    args_map.insert(
        "file_path".to_string(),
        serde_json::Value::String(pos_arg.clone()),
    );
}

// Tool dispatch
"get_file_dependencies" => {
    use crate::mcp::GetFileDependenciesRequest;

    let file_path = arguments
        .as_ref()
        .and_then(|m| m.get("file_path"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            eprintln!("Error: get_file_dependencies requires 'file_path' parameter");
            std::process::exit(1);
        })
        .to_string();

    let show_imports = arguments
        .as_ref()
        .and_then(|m| m.get("show_imports"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let show_exports = arguments
        .as_ref()
        .and_then(|m| m.get("show_exports"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let show_imported_by = arguments
        .as_ref()
        .and_then(|m| m.get("show_imported_by"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let calculate_blast_radius = arguments
        .as_ref()
        .and_then(|m| m.get("calculate_blast_radius"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    server
        .get_file_dependencies(Parameters(GetFileDependenciesRequest {
            file_path,
            show_imports,
            show_exports,
            show_imported_by,
            calculate_blast_radius,
        }))
        .await
}
```

#### 6. Use Cases

**Use Case 1: Impact Analysis Before Change**
```bash
# Before modifying auth/service.ts, check who uses it
codanna mcp get_file_dependencies "auth/service.ts"

# Output shows 7 files across 4 modules import it
# Agent knows: "This is a high-impact change"
```

**Use Case 2: Dead Code Detection**
```bash
# Check if old/deprecated.ts is still used
codanna mcp get_file_dependencies "old/deprecated.ts"

# If "Imported By" is empty → safe to delete
```

**Use Case 3: Circular Dependency Detection**
```bash
# Check for circular imports
codanna mcp get_file_dependencies "module-a.ts"
# Shows: module-a → module-b
codanna mcp get_file_dependencies "module-b.ts"
# Shows: module-b → module-a
# Alert: Circular dependency detected!
```

**Use Case 4: Module Boundary Analysis**
```bash
# Check if internal module is leaking outside
codanna mcp get_file_dependencies "auth/internal/utils.ts"

# Expected: Only imported by auth/* files
# If billing/ imports it → architectural violation!
```

### Deliverables

- [ ] `FileDependencyGraph` data structure
- [ ] Import/export parser (TypeScript/JavaScript)
- [ ] Pipeline integration (FileGraphStage)
- [ ] MCP tool implementation
- [ ] CLI routing
- [ ] Blast radius calculation
- [ ] Module grouping logic
- [ ] Documentation

### Success Metrics

- Correctly parses 95%+ of import/export statements
- Resolves relative paths accurately
- Blast radius calculation completes in <100ms
- Identifies architectural violations (cross-module imports)

---

## 📊 Overall Timeline & Milestones

| Week | Phase | Deliverable | Status |
|------|-------|-------------|--------|
| **Week 1** | Phase 1 | get_call_tree working | To Do |
| **Week 2-3** | Phase 2 | Type resolution 90% | To Do |
| **Week 4-5** | Phase 3 | File dependency graph | To Do |
| **Week 6** | Integration | All features working together | To Do |

---

## 🎯 Success Criteria

**Phase 1 Success:**
- [ ] Agent can query "what does SignupHandler call?" → Gets 4-level tree in 1 call
- [ ] Type resolution works for 70%+ of method calls in tree
- [ ] Output is readable and actionable
- [ ] No infinite loops (cycle detection works)

**Phase 2 Success:**
- [ ] Type resolution accuracy: 90%+ (measured on test suite)
- [ ] Generic types resolve correctly (Repository<User>.find → UserService)
- [ ] Union types show all possible targets
- [ ] Interface dispatch works (Service interface → implementations)

**Phase 3 Success:**
- [ ] File dependency graph covers 100% of source files
- [ ] Import/export parsing: 95%+ accuracy
- [ ] Blast radius correctly identifies affected modules
- [ ] Architectural violations are detectable

**Overall Success:**
- [ ] Agent workflow: "Understand feature" now takes 1-3 tool calls (was 8-12)
- [ ] Deep context: Agent sees execution flow from entry point to database
- [ ] Architectural awareness: Agent knows which modules are affected by changes
- [ ] Senior-level understanding: Agent can reason about impact like a senior dev

---

## 🚀 Getting Started

### Prerequisites
- Codanna development environment set up
- Rust 1.70+
- Familiarity with tree-sitter parsers
- Understanding of BFS algorithms

### Development Workflow

1. **Phase 1 (Week 1):**
   ```bash
   cd codanna
   git checkout -b feature/get-call-tree

   # Implement downstream BFS
   # Add MCP tool
   # Add CLI routing
   # Write tests

   cargo test --test call_tree_tests
   cargo build --release

   # Manual testing
   ./target/release/codanna mcp get_call_tree 42
   ```

2. **Phase 2 (Week 2-3):**
   ```bash
   git checkout -b feature/type-resolution-enhancement

   # Implement generic tracking
   # Add union type handling
   # Create accuracy test suite

   cargo test --test type_resolution_accuracy
   # Target: 90%+ pass rate
   ```

3. **Phase 3 (Week 4-5):**
   ```bash
   git checkout -b feature/file-dependency-graph

   # Implement import/export parser
   # Add pipeline stage
   # Create MCP tool

   cargo test --test file_graph_tests
   ```

### Testing Strategy

**Unit Tests:** Each phase has dedicated test suite
**Integration Tests:** Cross-phase interaction tests
**Manual Testing:** Real-world codebase examples
**Performance Tests:** Ensure <500ms response for typical queries

---

## 📝 Documentation

Each phase will include:
- Tool description in MCP metadata
- Usage examples in tool help text
- Integration guide for combining tools
- Troubleshooting common issues

---

## 🔄 Maintenance & Future Work

### After Initial Implementation

**Monitor:**
- Type resolution accuracy (track regressions)
- Performance (call tree depth vs. response time)
- User feedback (missing features, edge cases)

**Future Enhancements:**
- Call tree visualization (graph UI)
- Cross-language support (Python, Go, etc.)
- Runtime call tracing (dynamic analysis)
- Async flow tracking (promises, callbacks)
- Framework-specific patterns (Express middleware, NestJS interceptors)

---

**Status:** Ready to implement
**Next Step:** Begin Phase 1 - get_call_tree core tool
**Estimated Completion:** 5 weeks (all 3 phases)
