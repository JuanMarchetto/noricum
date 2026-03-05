/// Dependency graph: extracts and orders C functions for migration.
///
/// Uses simple regex-based heuristics to find function definitions and calls
/// in C source files. A proper implementation would use tree-sitter, but this
/// is sufficient for v0 ordering.
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use regex::Regex;

/// A directed graph of function call dependencies.
pub struct DependencyGraph {
    /// Map from function name to list of functions it calls.
    edges: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    /// Extract dependencies from C source files in a directory.
    ///
    /// Reads all `.c` files, extracts function definitions and calls,
    /// then builds a call graph restricted to functions defined in the project.
    pub fn from_directory(dir: &Path) -> Result<Self, std::io::Error> {
        let mut all_sources = Vec::new();

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("c") {
                let source = std::fs::read_to_string(&path)?;
                all_sources.push(source);
            }
        }

        // Also read .h files for function declarations
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("h") {
                let source = std::fs::read_to_string(&path)?;
                all_sources.push(source);
            }
        }

        // Collect all defined functions across the project
        let mut all_functions: Vec<String> = Vec::new();
        for source in &all_sources {
            all_functions.extend(Self::extract_functions(source));
        }
        all_functions.sort();
        all_functions.dedup();

        // Build call graph
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for source in &all_sources {
            let defined = Self::extract_functions(source);
            for func in &defined {
                let calls = Self::extract_calls(source, &all_functions);
                // Only keep calls that are NOT the function itself
                let filtered: Vec<String> = calls.into_iter().filter(|c| c != func).collect();
                edges.entry(func.clone()).or_default().extend(filtered);
            }
        }

        // Deduplicate call edges
        for calls in edges.values_mut() {
            calls.sort();
            calls.dedup();
        }

        Ok(Self { edges })
    }

    /// Extract function names defined in a C source file.
    ///
    /// Looks for patterns like `type name(` at the beginning of a line,
    /// excluding common keywords and preprocessor directives.
    pub fn extract_functions(c_source: &str) -> Vec<String> {
        // Match function definitions: return_type function_name(
        // This handles: int foo(, void bar(, static int baz(, unsigned long qux(
        // But NOT: if(, while(, for(, switch(, return(, #define FOO(
        let re = Regex::new(
            r"(?m)^\s*(?:static\s+)?(?:inline\s+)?(?:const\s+)?(?:unsigned\s+)?(?:signed\s+)?(?:long\s+)?(?:short\s+)?(?:struct\s+\w+\s*\*?\s*|enum\s+\w+\s+)?(?:void|int|char|float|double|size_t|ssize_t|uint\d+_t|int\d+_t|bool|_Bool|\w+_t)\s*\*?\s*\*?\s*(\w+)\s*\("
        ).expect("invalid regex");

        let keywords: HashSet<&str> = [
            "if",
            "while",
            "for",
            "switch",
            "return",
            "sizeof",
            "typeof",
            "defined",
            "main",
            "__attribute__",
        ]
        .into_iter()
        .collect();

        let mut functions = Vec::new();
        for cap in re.captures_iter(c_source) {
            let name = cap[1].to_string();
            if !keywords.contains(name.as_str()) && !name.starts_with('_') {
                functions.push(name);
            }
        }

        functions.sort();
        functions.dedup();
        functions
    }

    /// Extract function calls from a C source body.
    ///
    /// Looks for `name(` patterns where `name` is in the set of known project functions.
    /// Filters out definitions, keywords, and preprocessor directives.
    pub fn extract_calls(c_source: &str, known_functions: &[String]) -> Vec<String> {
        let known_set: HashSet<&str> = known_functions.iter().map(|s| s.as_str()).collect();

        // Match identifier followed by ( that isn't a definition
        let call_re = Regex::new(r"\b(\w+)\s*\(").expect("invalid regex");

        let keywords: HashSet<&str> = [
            "if",
            "while",
            "for",
            "switch",
            "return",
            "sizeof",
            "typeof",
            "defined",
            "__attribute__",
        ]
        .into_iter()
        .collect();

        let mut calls = Vec::new();
        for cap in call_re.captures_iter(c_source) {
            let name = &cap[1];
            if known_set.contains(name) && !keywords.contains(name) {
                calls.push(name.to_string());
            }
        }

        calls.sort();
        calls.dedup();
        calls
    }

    /// Return functions in topological order (dependencies first).
    ///
    /// Uses Kahn's algorithm. Falls back to alphabetical order if cycles
    /// are detected (i.e., not all nodes can be processed).
    pub fn topological_sort(&self) -> Vec<String> {
        // Collect all nodes
        let mut all_nodes: HashSet<&str> = HashSet::new();
        for (func, calls) in &self.edges {
            all_nodes.insert(func.as_str());
            for call in calls {
                all_nodes.insert(call.as_str());
            }
        }

        // Compute in-degree
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        for node in &all_nodes {
            in_degree.insert(node, 0);
        }
        for calls in self.edges.values() {
            for call in calls {
                *in_degree.entry(call.as_str()).or_insert(0) += 1;
            }
        }

        // Start with nodes that have no dependencies (in-degree 0)
        let mut queue: VecDeque<&str> = VecDeque::new();
        let mut zero_degree: Vec<&str> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(&node, _)| node)
            .collect();
        // Sort for deterministic output
        zero_degree.sort();
        for node in zero_degree {
            queue.push_back(node);
        }

        let mut result: Vec<String> = Vec::new();
        let mut visited = 0usize;

        while let Some(node) = queue.pop_front() {
            result.push(node.to_string());
            visited += 1;

            if let Some(calls) = self.edges.get(node) {
                let mut next: Vec<&str> = Vec::new();
                for call in calls {
                    let deg = in_degree.get_mut(call.as_str()).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        next.push(call.as_str());
                    }
                }
                // Sort for deterministic order
                next.sort();
                for n in next {
                    queue.push_back(n);
                }
            }
        }

        // If we didn't visit all nodes, there's a cycle -> fall back to alphabetical
        if visited < all_nodes.len() {
            let mut all: Vec<String> = all_nodes.iter().map(|s| s.to_string()).collect();
            all.sort();
            return all;
        }

        result
    }

    /// Get direct dependencies (callees) for a function.
    pub fn dependencies_of(&self, function: &str) -> &[String] {
        self.edges
            .get(function)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_functions_simple() {
        let source = r#"
int add(int a, int b) {
    return a + b;
}

void greet(const char *name) {
    printf("Hello %s\n", name);
}
"#;
        let funcs = DependencyGraph::extract_functions(source);
        assert!(funcs.contains(&"add".to_string()));
        assert!(funcs.contains(&"greet".to_string()));
        assert_eq!(funcs.len(), 2);
    }

    #[test]
    fn test_extract_functions_with_static() {
        let source = r#"
static int helper(int x) {
    return x * 2;
}
"#;
        let funcs = DependencyGraph::extract_functions(source);
        assert!(funcs.contains(&"helper".to_string()));
    }

    #[test]
    fn test_extract_functions_excludes_keywords() {
        let source = r#"
int compute(int x) {
    if (x > 0) {
        while (x > 1) {
            x = x / 2;
        }
    }
    return x;
}
"#;
        let funcs = DependencyGraph::extract_functions(source);
        assert!(funcs.contains(&"compute".to_string()));
        assert!(!funcs.contains(&"if".to_string()));
        assert!(!funcs.contains(&"while".to_string()));
    }

    #[test]
    fn test_extract_functions_pointer_return() {
        let source = r#"
char *get_name(int id) {
    return names[id];
}
"#;
        let funcs = DependencyGraph::extract_functions(source);
        assert!(funcs.contains(&"get_name".to_string()));
    }

    #[test]
    fn test_extract_calls() {
        let source = r#"
int compute(int x) {
    int a = helper(x);
    int b = transform(a);
    return a + b;
}
"#;
        let known = vec![
            "compute".to_string(),
            "helper".to_string(),
            "transform".to_string(),
        ];
        let calls = DependencyGraph::extract_calls(source, &known);
        assert!(calls.contains(&"helper".to_string()));
        assert!(calls.contains(&"transform".to_string()));
        // compute itself may appear since extract_calls doesn't know which function body it's in
        // The from_directory method filters self-calls
    }

    #[test]
    fn test_extract_calls_ignores_unknown() {
        let source = "int f() { return printf(\"hi\"); }";
        let known = vec!["f".to_string()];
        let calls = DependencyGraph::extract_calls(source, &known);
        assert!(!calls.contains(&"printf".to_string()));
    }

    #[test]
    fn test_topological_sort_linear() {
        // a -> b -> c (a calls b, b calls c)
        let edges = HashMap::from([
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["c".to_string()]),
            ("c".to_string(), vec![]),
        ]);
        let graph = DependencyGraph { edges };
        let order = graph.topological_sort();

        // c must come before b, b before a
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b, "a should come before b (a calls b)");
        assert!(pos_b < pos_c, "b should come before c (b calls c)");
    }

    #[test]
    fn test_topological_sort_diamond() {
        // a -> b, a -> c, b -> d, c -> d
        let edges = HashMap::from([
            ("a".to_string(), vec!["b".to_string(), "c".to_string()]),
            ("b".to_string(), vec!["d".to_string()]),
            ("c".to_string(), vec!["d".to_string()]),
            ("d".to_string(), vec![]),
        ]);
        let graph = DependencyGraph { edges };
        let order = graph.topological_sort();

        assert_eq!(order.len(), 4);
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_d = order.iter().position(|x| x == "d").unwrap();
        assert!(pos_a < pos_d);
    }

    #[test]
    fn test_topological_sort_cycle_fallback() {
        // a -> b -> a (cycle)
        let edges = HashMap::from([
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
        ]);
        let graph = DependencyGraph { edges };
        let order = graph.topological_sort();

        // Should fall back to alphabetical
        assert_eq!(order, vec!["a", "b"]);
    }

    #[test]
    fn test_topological_sort_empty() {
        let graph = DependencyGraph {
            edges: HashMap::new(),
        };
        let order = graph.topological_sort();
        assert!(order.is_empty());
    }

    #[test]
    fn test_dependencies_of() {
        let edges = HashMap::from([("a".to_string(), vec!["b".to_string(), "c".to_string()])]);
        let graph = DependencyGraph { edges };

        assert_eq!(
            graph.dependencies_of("a"),
            &["b".to_string(), "c".to_string()]
        );
        assert_eq!(graph.dependencies_of("unknown"), &[] as &[String]);
    }

    #[test]
    fn test_from_directory() {
        let tmp = tempfile::tempdir().unwrap();

        std::fs::write(
            tmp.path().join("math.c"),
            r#"
int helper(int x) {
    return x * 2;
}

int compute(int a) {
    return helper(a) + 1;
}
"#,
        )
        .unwrap();

        let graph = DependencyGraph::from_directory(tmp.path()).unwrap();

        // compute should depend on helper
        let deps = graph.dependencies_of("compute");
        assert!(deps.contains(&"helper".to_string()));

        // Topological order: compute before helper (compute calls helper)
        let order = graph.topological_sort();
        assert!(!order.is_empty());
    }

    #[test]
    fn test_from_directory_no_c_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "hello").unwrap();

        let graph = DependencyGraph::from_directory(tmp.path()).unwrap();
        assert!(graph.topological_sort().is_empty());
    }

    #[test]
    fn test_from_directory_multiple_files() {
        let tmp = tempfile::tempdir().unwrap();

        std::fs::write(
            tmp.path().join("util.c"),
            "int util_fn(int x) { return x; }\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("main.c"),
            r#"
int process(int x) {
    return util_fn(x) + 1;
}
"#,
        )
        .unwrap();

        let graph = DependencyGraph::from_directory(tmp.path()).unwrap();
        let deps = graph.dependencies_of("process");
        assert!(deps.contains(&"util_fn".to_string()));
    }
}
