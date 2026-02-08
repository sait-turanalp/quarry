//! React hooks state graph extraction using tree-sitter AST parsing.
//!
//! Parses a React component/hook function source and extracts:
//! - useState calls with state name, setter, type annotation, initial value
//! - useRef calls with ref name, type annotation, initial value
//! - useEffect calls with dependency arrays and line numbers
//! - useCallback calls with name and dependency arrays
//! - useMemo calls with name and dependency arrays

use tree_sitter::{Language, Node, Parser};

#[derive(Debug, Default)]
pub struct StateGraph {
    pub states: Vec<StateEntry>,
    pub refs: Vec<RefEntry>,
    pub effects: Vec<EffectEntry>,
    pub callbacks: Vec<CallbackEntry>,
    pub memos: Vec<MemoEntry>,
}

#[derive(Debug)]
pub struct StateEntry {
    pub name: String,
    pub setter: String,
    pub type_annotation: Option<String>,
    pub initial_value: Option<String>,
}

#[derive(Debug)]
pub struct RefEntry {
    pub name: String,
    pub type_annotation: Option<String>,
    pub initial_value: Option<String>,
}

#[derive(Debug)]
pub struct EffectEntry {
    pub deps: Vec<String>,
    pub has_deps_array: bool,
    pub line: u32,
}

#[derive(Debug)]
pub struct CallbackEntry {
    pub name: String,
    pub deps: Vec<String>,
}

#[derive(Debug)]
pub struct MemoEntry {
    pub name: String,
    pub deps: Vec<String>,
}

impl StateGraph {
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
            && self.refs.is_empty()
            && self.effects.is_empty()
            && self.callbacks.is_empty()
            && self.memos.is_empty()
    }

    pub fn format(&self) -> String {
        let mut output = String::new();

        if !self.states.is_empty() {
            output.push_str(&format!("### useState ({})\n", self.states.len()));
            for s in &self.states {
                let type_info = s
                    .type_annotation
                    .as_ref()
                    .map(|t| format!(" <{t}>"))
                    .unwrap_or_default();
                let init = s
                    .initial_value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "  - {}/{}{}{}\n",
                    s.name, s.setter, type_info, init
                ));
            }
            output.push('\n');
        }

        if !self.refs.is_empty() {
            output.push_str(&format!("### useRef ({})\n", self.refs.len()));
            for r in &self.refs {
                let type_info = r
                    .type_annotation
                    .as_ref()
                    .map(|t| format!(" <{t}>"))
                    .unwrap_or_default();
                let init = r
                    .initial_value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                output.push_str(&format!("  - {}{}{}\n", r.name, type_info, init));
            }
            output.push('\n');
        }

        if !self.effects.is_empty() {
            output.push_str(&format!("### useEffect ({})\n", self.effects.len()));
            for e in &self.effects {
                let deps = if !e.has_deps_array {
                    "no deps array (runs every render)".to_string()
                } else if e.deps.is_empty() {
                    "[] (mount only)".to_string()
                } else {
                    format!("[{}]", e.deps.join(", "))
                };
                output.push_str(&format!("  - L{}: deps: {}\n", e.line, deps));
            }
            output.push('\n');
        }

        if !self.callbacks.is_empty() {
            output.push_str(&format!("### useCallback ({})\n", self.callbacks.len()));
            for c in &self.callbacks {
                let deps = if c.deps.is_empty() {
                    "[]".to_string()
                } else {
                    format!("[{}]", c.deps.join(", "))
                };
                output.push_str(&format!("  - {} deps: {}\n", c.name, deps));
            }
            output.push('\n');
        }

        if !self.memos.is_empty() {
            output.push_str(&format!("### useMemo ({})\n", self.memos.len()));
            for m in &self.memos {
                let deps = if m.deps.is_empty() {
                    "[]".to_string()
                } else {
                    format!("[{}]", m.deps.join(", "))
                };
                output.push_str(&format!("  - {} deps: {}\n", m.name, deps));
            }
            output.push('\n');
        }

        output
    }
}

/// Extract React hooks from a TypeScript/TSX source string.
pub fn extract_react_hooks(source: &str) -> StateGraph {
    let mut parser = Parser::new();
    let language: Language = tree_sitter_typescript::LANGUAGE_TSX.into();
    if parser.set_language(&language).is_err() {
        return StateGraph::default();
    }

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return StateGraph::default(),
    };

    let mut graph = StateGraph::default();
    traverse_for_hooks(&tree.root_node(), source, &mut graph);
    graph
}

fn traverse_for_hooks(node: &Node, source: &str, graph: &mut StateGraph) {
    if node.kind() == "call_expression" {
        if let Some(callee) = node.child_by_field_name("function") {
            let callee_text = &source[callee.byte_range()];
            match callee_text {
                "useState" => extract_use_state(node, source, graph),
                "useRef" => extract_use_ref(node, source, graph),
                "useEffect" | "useLayoutEffect" => extract_use_effect(node, source, graph),
                "useCallback" => extract_use_callback(node, source, graph),
                "useMemo" => extract_use_memo(node, source, graph),
                _ => {}
            }
        }
    }

    for child in node.children(&mut node.walk()) {
        traverse_for_hooks(&child, source, graph);
    }
}

/// Extract type arguments from a call expression like `useState<string>(...)`.
fn extract_type_args(node: &Node, source: &str) -> Option<String> {
    // Look for type_arguments child on the call_expression
    for child in node.children(&mut node.walk()) {
        if child.kind() == "type_arguments" {
            let text = &source[child.byte_range()];
            // Strip outer < >
            let inner = text.trim_start_matches('<').trim_end_matches('>').trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }
    None
}

/// Get the arguments node from a call_expression.
fn get_arguments<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    node.child_by_field_name("arguments")
}

/// Extract first argument text from arguments node.
fn first_arg_text(args: &Node, source: &str) -> Option<String> {
    // arguments = ( arg1, arg2, ... )
    // Named children skip the punctuation
    for child in args.children(&mut args.walk()) {
        let kind = child.kind();
        if kind != "(" && kind != ")" && kind != "," {
            let text = source[child.byte_range()].trim().to_string();
            // Truncate long initial values
            if text.len() > 60 {
                return Some(format!("{}...", &text[..57]));
            }
            return Some(text);
        }
    }
    None
}

/// Extract dependency array from the second argument of a hook call.
fn extract_deps_array(args: &Node, source: &str) -> (bool, Vec<String>) {
    let mut non_punc_idx = 0;
    for child in args.children(&mut args.walk()) {
        let kind = child.kind();
        if kind == "(" || kind == ")" || kind == "," {
            continue;
        }
        if non_punc_idx == 1 {
            // This is the second argument — should be an array
            if child.kind() == "array" {
                let deps = extract_array_elements(&child, source);
                return (true, deps);
            }
            // Not an array — might be a variable reference
            return (true, vec![source[child.byte_range()].trim().to_string()]);
        }
        non_punc_idx += 1;
    }
    (false, Vec::new())
}

/// Extract element texts from an array node.
fn extract_array_elements(array_node: &Node, source: &str) -> Vec<String> {
    let mut elements = Vec::new();
    for child in array_node.children(&mut array_node.walk()) {
        let kind = child.kind();
        if kind != "[" && kind != "]" && kind != "," {
            elements.push(source[child.byte_range()].trim().to_string());
        }
    }
    elements
}

/// Get the variable name from the parent variable_declarator of a call_expression.
/// Returns (variable_name, parent_node) for further analysis.
fn get_variable_name_from_parent(node: &Node, source: &str) -> Option<String> {
    let parent = node.parent()?;
    if parent.kind() != "variable_declarator" {
        return None;
    }
    let name_node = parent.child_by_field_name("name")?;
    Some(source[name_node.byte_range()].trim().to_string())
}

/// Extract useState: `const [name, setter] = useState<T>(init)`
fn extract_use_state(node: &Node, source: &str, graph: &mut StateGraph) {
    let parent = match node.parent() {
        Some(p) if p.kind() == "variable_declarator" => p,
        _ => return,
    };

    let name_node = match parent.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };

    let (name, setter) = if name_node.kind() == "array_pattern" {
        let mut names = Vec::new();
        for child in name_node.children(&mut name_node.walk()) {
            let kind = child.kind();
            if kind != "[" && kind != "]" && kind != "," {
                names.push(source[child.byte_range()].trim().to_string());
            }
        }
        if names.len() >= 2 {
            (names[0].clone(), names[1].clone())
        } else if names.len() == 1 {
            (names[0].clone(), String::new())
        } else {
            return;
        }
    } else {
        let name = source[name_node.byte_range()].trim().to_string();
        (name, String::new())
    };

    let type_annotation = extract_type_args(node, source);
    let initial_value = get_arguments(node).and_then(|args| first_arg_text(&args, source));

    graph.states.push(StateEntry {
        name,
        setter,
        type_annotation,
        initial_value,
    });
}

/// Extract useRef: `const name = useRef<T>(init)`
fn extract_use_ref(node: &Node, source: &str, graph: &mut StateGraph) {
    let name = match get_variable_name_from_parent(node, source) {
        Some(n) => n,
        None => return,
    };

    let type_annotation = extract_type_args(node, source);
    let initial_value = get_arguments(node).and_then(|args| first_arg_text(&args, source));

    graph.refs.push(RefEntry {
        name,
        type_annotation,
        initial_value,
    });
}

/// Extract useEffect/useLayoutEffect: `useEffect(() => { ... }, [deps])`
fn extract_use_effect(node: &Node, source: &str, graph: &mut StateGraph) {
    let line = node.start_position().row as u32 + 1;
    let (has_deps_array, deps) = match get_arguments(node) {
        Some(args) => extract_deps_array(&args, source),
        None => (false, Vec::new()),
    };

    graph.effects.push(EffectEntry {
        deps,
        has_deps_array,
        line,
    });
}

/// Extract useCallback: `const name = useCallback(() => { ... }, [deps])`
fn extract_use_callback(node: &Node, source: &str, graph: &mut StateGraph) {
    let name = match get_variable_name_from_parent(node, source) {
        Some(n) => n,
        None => "anonymous".to_string(),
    };

    let (_, deps) = match get_arguments(node) {
        Some(args) => extract_deps_array(&args, source),
        None => (false, Vec::new()),
    };

    graph.callbacks.push(CallbackEntry { name, deps });
}

/// Extract useMemo: `const name = useMemo(() => { ... }, [deps])`
fn extract_use_memo(node: &Node, source: &str, graph: &mut StateGraph) {
    let name = match get_variable_name_from_parent(node, source) {
        Some(n) => n,
        None => "anonymous".to_string(),
    };

    let (_, deps) = match get_arguments(node) {
        Some(args) => extract_deps_array(&args, source),
        None => (false, Vec::new()),
    };

    graph.memos.push(MemoEntry { name, deps });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_use_state() {
        let source = r#"
function useMyHook() {
    const [count, setCount] = useState<number>(0);
    const [name, setName] = useState("hello");
}
"#;
        let graph = extract_react_hooks(source);
        assert_eq!(graph.states.len(), 2);
        assert_eq!(graph.states[0].name, "count");
        assert_eq!(graph.states[0].setter, "setCount");
        assert_eq!(graph.states[0].type_annotation.as_deref(), Some("number"));
        assert_eq!(graph.states[0].initial_value.as_deref(), Some("0"));
        assert_eq!(graph.states[1].name, "name");
        assert_eq!(graph.states[1].setter, "setName");
    }

    #[test]
    fn test_extract_use_ref() {
        let source = r#"
function useMyHook() {
    const canvasRef = useRef<HTMLCanvasElement>(null);
}
"#;
        let graph = extract_react_hooks(source);
        assert_eq!(graph.refs.len(), 1);
        assert_eq!(graph.refs[0].name, "canvasRef");
        assert_eq!(
            graph.refs[0].type_annotation.as_deref(),
            Some("HTMLCanvasElement")
        );
        assert_eq!(graph.refs[0].initial_value.as_deref(), Some("null"));
    }

    #[test]
    fn test_extract_use_effect() {
        let source = r#"
function useMyHook() {
    useEffect(() => {
        console.log("mount");
    }, []);
    useEffect(() => {
        console.log(count);
    }, [count, name]);
}
"#;
        let graph = extract_react_hooks(source);
        assert_eq!(graph.effects.len(), 2);
        assert!(graph.effects[0].has_deps_array);
        assert!(graph.effects[0].deps.is_empty());
        assert_eq!(graph.effects[1].deps, vec!["count", "name"]);
    }

    #[test]
    fn test_extract_use_callback() {
        let source = r#"
function useMyHook() {
    const handleClick = useCallback(() => {
        setCount(c => c + 1);
    }, [setCount]);
}
"#;
        let graph = extract_react_hooks(source);
        assert_eq!(graph.callbacks.len(), 1);
        assert_eq!(graph.callbacks[0].name, "handleClick");
        assert_eq!(graph.callbacks[0].deps, vec!["setCount"]);
    }

    #[test]
    fn test_extract_use_memo() {
        let source = r#"
function useMyHook() {
    const doubled = useMemo(() => count * 2, [count]);
}
"#;
        let graph = extract_react_hooks(source);
        assert_eq!(graph.memos.len(), 1);
        assert_eq!(graph.memos[0].name, "doubled");
        assert_eq!(graph.memos[0].deps, vec!["count"]);
    }
}
