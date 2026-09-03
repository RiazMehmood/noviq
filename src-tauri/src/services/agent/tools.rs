//! F017 — the agent's **tool** abstraction (the "Rust executes" half).
//!
//! The LLM is the controller; it never touches the database or the network. It
//! emits a structured action naming one of these tools, and the orchestrator
//! dispatches to the matching `Tool::run`. Tools are deterministic local
//! Rust/PostgreSQL operations (retrieval, cross-reference, expansion) — there is
//! **no** open-ended/internet tool (spec Out-of-Scope). The registry is fixed at
//! construction, so the controller can only ever pick from a closed set.

use async_trait::async_trait;

/// One tool execution result handed back to the controller as an observation.
#[derive(Debug, Clone, Default)]
pub struct ToolResult {
    /// Human/LLM-readable summary of what the tool found, fed back into the next
    /// controller prompt.
    pub observation: String,
    /// Opaque evidence identifiers (e.g. chunk ids) gathered this call. Used by the
    /// orchestrator for **no-progress detection** (an iteration that surfaces only
    /// already-seen evidence stops the loop) and downstream citation checks.
    pub evidence_ids: Vec<String>,
}

/// A deterministic action the controller may invoke by name.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Stable identifier the controller emits (e.g. `"search"`).
    fn name(&self) -> &str;
    /// One-line description injected into the controller prompt so the model knows
    /// when to pick this tool and what `args` it takes.
    fn description(&self) -> &str;
    /// Execute with the controller-supplied JSON args. Errors are surfaced to the
    /// controller as an observation, never bubbled as a loop-crashing panic.
    async fn run(&self, args: &serde_json::Value) -> anyhow::Result<ToolResult>;
}

/// The fixed set of tools available to one agent run.
#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) -> &mut Self {
        self.tools.push(tool);
        self
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|b| b.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Tool names, for the JSON-schema `enum` that constrains the controller's
    /// tool choice (guided decoding can only pick a registered tool).
    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name().to_string()).collect()
    }

    /// `- name: description` lines injected into the controller prompt.
    pub fn describe(&self) -> String {
        self.tools
            .iter()
            .map(|t| format!("- {}: {}", t.name(), t.description()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A deterministic tool for loop tests: records call count and returns a
    /// configurable observation + evidence id.
    pub struct MockTool {
        pub tool_name: String,
        pub calls: Arc<AtomicUsize>,
        pub evidence_id: String,
    }

    impl MockTool {
        pub fn new(name: &str, evidence_id: &str) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    tool_name: name.to_string(),
                    calls: calls.clone(),
                    evidence_id: evidence_id.to_string(),
                },
                calls,
            )
        }
    }

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            &self.tool_name
        }
        fn description(&self) -> &str {
            "test tool"
        }
        async fn run(&self, _args: &serde_json::Value) -> anyhow::Result<ToolResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ToolResult {
                observation: format!("observation from {}", self.tool_name),
                evidence_ids: vec![self.evidence_id.clone()],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockTool;
    use super::*;

    #[test]
    fn registry_describe_and_lookup() {
        let (t, _) = MockTool::new("search", "e1");
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(t));
        assert!(reg.get("search").is_some());
        assert!(reg.get("missing").is_none());
        assert_eq!(reg.names(), vec!["search".to_string()]);
        assert!(reg.describe().contains("- search: test tool"));
    }
}
