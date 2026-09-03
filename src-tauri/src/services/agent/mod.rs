//! F017 — the **Constrained Legal Retrieval Agent** orchestrator.
//!
//! A bounded iterative-retrieval loop: the LLM is the *controller* (it decides the
//! next action and when it has enough), the Rust tools are the *executor*. This is
//! explicitly NOT an autonomous agent — the tool set is fixed (`tools::ToolRegistry`),
//! there is a hard `MAX_ITER` cap, and on insufficiency it returns the grounded
//! refusal rather than fabricating (spec FR-006). All model calls go through the
//! F016 `TextGenerator` seam (FR-014), so the agent runs on whichever engine is
//! selected (vLLM sidecar or the remote API) with no engine coupling.
//!
//! This module is the engine of the loop. Real tools (hybrid retrieval,
//! cross-reference, expansion) and the live `chat://` wiring are layered on top;
//! the loop here is unit-tested against a mock generator + mock tool so the control
//! logic (dispatch, no-progress stop, malformed-output safety, iteration cap) is
//! verified without a model or database.

pub mod tools;

use tools::ToolRegistry;

use super::inference::{GenParams, ResponseFormat, TextGenerator};
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

/// The grounded refusal — returned whenever the loop stops without sufficient
/// evidence (FR-006). Never fabricate in its place.
pub const REFUSAL: &str = "This cannot be determined from the available documents.";

/// Loop bounds + generation sizes, resolved from `NOVIQ_AGENT_*` env.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Hard cap on controller↔tool iterations (FR-005). Small by design.
    pub max_iter: usize,
    /// Token budget for a (small, structured) control call.
    pub control_max_tokens: u32,
    /// Token budget for the final synthesis call.
    pub answer_max_tokens: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self { max_iter: 3, control_max_tokens: 512, answer_max_tokens: 900 }
    }
}

impl AgentConfig {
    pub fn from_env() -> Self {
        let d = Self::default();
        let get = |k: &str, def: usize| -> usize {
            std::env::var(k).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(def)
        };
        Self {
            max_iter: get("NOVIQ_AGENT_MAX_ITER", d.max_iter).clamp(1, 8),
            control_max_tokens: get("NOVIQ_AGENT_CONTROL_TOKENS", d.control_max_tokens as usize) as u32,
            answer_max_tokens: get("NOVIQ_AGENT_ANSWER_TOKENS", d.answer_max_tokens as usize) as u32,
        }
    }
}

/// A validated control decision the controller emitted.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentAction {
    /// Run tool `name` with `args`.
    Tool { name: String, args: serde_json::Value },
    /// Stop and answer. `answer` may be empty → the orchestrator synthesizes from
    /// accumulated evidence.
    Finish { answer: String },
}

/// Why the loop stopped (telemetry + tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Controller emitted `finish`.
    Finished,
    /// Hit the iteration cap (FR-005).
    MaxIter,
    /// A tool call surfaced no new evidence (FR-007).
    NoProgress,
    /// No evidence gathered / malformed control output → grounded refusal (FR-006).
    Refused,
    /// Cancelled mid-loop.
    Cancelled,
}

/// The result of one agent turn.
#[derive(Debug, Clone)]
pub struct AgentOutcome {
    pub answer: String,
    pub iterations: usize,
    pub stop_reason: StopReason,
    /// Distinct evidence ids gathered across the run (drives citation checks later).
    pub evidence_ids: Vec<String>,
}

/// JSON Schema constraining the controller's action (guided decoding, FR-003).
fn action_schema(tool_names: &[String]) -> serde_json::Value {
    let tool_enum: Vec<serde_json::Value> =
        tool_names.iter().map(|n| serde_json::Value::String(n.clone())).collect();
    serde_json::json!({
        "type": "object",
        "properties": {
            "thought": { "type": "string" },
            "action": { "type": "string", "enum": ["tool", "finish"] },
            "tool": { "type": "string", "enum": tool_enum },
            "args": { "type": "object" },
            "answer": { "type": "string" }
        },
        "required": ["action"]
    })
}

/// Build the ChatML controller prompt. ChatML markers keep both HTTP engines happy
/// (the remote chat path parses them into messages; vLLM sends them raw).
fn build_controller_prompt(question: &str, tools_desc: &str, transcript: &str) -> String {
    let system = format!(
        "You are a constrained legal-retrieval controller. You do NOT answer from your own \
knowledge — you decide which retrieval TOOL to run next over an offline legal corpus, or \
whether the evidence gathered so far is sufficient to answer.\n\n\
Available tools:\n{tools_desc}\n\n\
Rules:\n\
- Respond with a SINGLE JSON object and nothing else.\n\
- To retrieve: {{\"action\":\"tool\",\"tool\":\"<name>\",\"args\":{{...}}}}.\n\
- When the evidence already answers every part of the question: \
{{\"action\":\"finish\",\"answer\":\"<grounded answer citing the evidence>\"}}.\n\
- If, after retrieving, the corpus does not contain the answer, finish with an empty answer \
so the system emits the grounded refusal. NEVER invent facts or citations."
    );
    let user = if transcript.trim().is_empty() {
        format!("Question: {question}\n\nNo evidence gathered yet. Decide the first action.")
    } else {
        format!("Question: {question}\n\nEvidence gathered so far:{transcript}\n\nDecide the next action.")
    };
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n\
<|im_start|>user\n{user}<|im_end|>\n\
<|im_start|>assistant\n"
    )
}

/// Build the final synthesis prompt (free-form answer over the accumulated evidence).
fn build_synthesis_prompt(question: &str, transcript: &str) -> String {
    let system = "You are a precise legal assistant. Answer the question using ONLY the evidence \
below. Cite the provisions you rely on. If the evidence does not contain the answer, reply \
exactly with the refusal and nothing else.";
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n\
<|im_start|>user\nQuestion: {question}\n\nEvidence:{transcript}<|im_end|>\n\
<|im_start|>assistant\n"
    )
}

/// Pull the first balanced JSON object out of a model response (tolerates ```json
/// fences and leading prose).
fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for i in start..raw.len() {
        let c = bytes[i] as char;
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&raw[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse + validate a controller response into an `AgentAction`. Returns `None`
/// on malformed output or an unregistered tool (caller applies the safe default).
pub fn parse_action(raw: &str, valid_tools: &[String]) -> Option<AgentAction> {
    let json = extract_json_object(raw)?;
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    match v.get("action").and_then(|a| a.as_str())? {
        "finish" => Some(AgentAction::Finish {
            answer: v.get("answer").and_then(|a| a.as_str()).unwrap_or("").to_string(),
        }),
        "tool" => {
            let name = v.get("tool").and_then(|t| t.as_str())?.to_string();
            if !valid_tools.iter().any(|n| n == &name) {
                return None;
            }
            let args = v.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
            Some(AgentAction::Tool { name, args })
        }
        _ => None,
    }
}

/// One control call → validated action, with a bounded retry on malformed output
/// (FR-003). Returns `None` after the retries so the caller falls back safely.
async fn decide(
    gen: &dyn TextGenerator,
    prompt: &str,
    tool_names: &[String],
    cfg: &AgentConfig,
    cancel: &CancellationToken,
) -> Option<AgentAction> {
    let params = GenParams {
        max_tokens: cfg.control_max_tokens,
        temperature: 0.0,
        top_p: 1.0,
        stop: vec!["<|im_end|>".to_string(), "</s>".to_string()],
        response_format: ResponseFormat::JsonSchema {
            name: "agent_action".to_string(),
            schema: action_schema(tool_names),
        },
    };
    for _ in 0..2 {
        match gen.complete(prompt, &params, cancel.clone()).await {
            Ok(out) => {
                if let Some(a) = parse_action(&out.text, tool_names) {
                    return Some(a);
                }
            }
            Err(_) => return None, // engine error → safe default (refuse)
        }
    }
    None
}

/// Final free-form answer from the gathered evidence. Falls back to the refusal on
/// any engine error (never fabricate).
async fn synthesize(
    gen: &dyn TextGenerator,
    question: &str,
    transcript: &str,
    cfg: &AgentConfig,
    cancel: &CancellationToken,
) -> String {
    let params = GenParams {
        max_tokens: cfg.answer_max_tokens,
        ..GenParams::default()
    };
    match gen.complete(&build_synthesis_prompt(question, transcript), &params, cancel.clone()).await {
        Ok(out) if !out.text.trim().is_empty() => out.text.trim().to_string(),
        _ => REFUSAL.to_string(),
    }
}

/// Run one bounded agent turn. `gen` is the active `TextGenerator` (F016 seam);
/// `registry` is the fixed tool set. Never panics; never fabricates.
pub async fn run_agent(
    gen: &dyn TextGenerator,
    registry: &ToolRegistry,
    question: &str,
    cfg: &AgentConfig,
    cancel: CancellationToken,
) -> AgentOutcome {
    let tool_names = registry.names();
    let mut transcript = String::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut evidence: Vec<String> = Vec::new();

    for iter in 0..cfg.max_iter {
        if cancel.is_cancelled() {
            return finish(question, &transcript, &evidence, iter, StopReason::Cancelled, gen, cfg, &cancel).await;
        }

        let prompt = build_controller_prompt(question, &registry.describe(), &transcript);
        let action = decide(gen, &prompt, &tool_names, cfg, &cancel).await;

        match action {
            // Malformed/engine-error after retry → stop safely (FR-003 safe default).
            None => {
                return finish(question, &transcript, &evidence, iter + 1, StopReason::Refused, gen, cfg, &cancel).await;
            }
            Some(AgentAction::Finish { answer }) => {
                let ans = if answer.trim().is_empty() {
                    if evidence.is_empty() {
                        REFUSAL.to_string()
                    } else {
                        synthesize(gen, question, &transcript, cfg, &cancel).await
                    }
                } else {
                    answer.trim().to_string()
                };
                let reason = if ans == REFUSAL { StopReason::Refused } else { StopReason::Finished };
                return AgentOutcome { answer: ans, iterations: iter + 1, stop_reason: reason, evidence_ids: evidence };
            }
            Some(AgentAction::Tool { name, args }) => {
                let Some(tool) = registry.get(&name) else {
                    transcript.push_str(&format!("\n[step {}] tool={name} unavailable.", iter + 1));
                    continue;
                };
                match tool.run(&args).await {
                    Ok(res) => {
                        let new_ids: Vec<String> =
                            res.evidence_ids.iter().filter(|id| !seen.contains(*id)).cloned().collect();
                        for id in &res.evidence_ids {
                            seen.insert(id.clone());
                        }
                        transcript.push_str(&format!(
                            "\n[step {}] tool={name}: {}",
                            iter + 1,
                            res.observation
                        ));
                        // No-progress (FR-007): a non-first iteration that adds no
                        // new evidence stops rather than spinning.
                        if new_ids.is_empty() && iter > 0 {
                            return finish(question, &transcript, &evidence, iter + 1, StopReason::NoProgress, gen, cfg, &cancel).await;
                        }
                        evidence.extend(new_ids);
                    }
                    Err(e) => {
                        transcript.push_str(&format!("\n[step {}] tool={name} ERROR: {e}", iter + 1));
                    }
                }
            }
        }
    }

    // Iteration cap reached (FR-005).
    finish(question, &transcript, &evidence, cfg.max_iter, StopReason::MaxIter, gen, cfg, &cancel).await
}

/// Terminal helper: synthesize from evidence (or refuse if none) and tag the stop.
#[allow(clippy::too_many_arguments)]
async fn finish(
    question: &str,
    transcript: &str,
    evidence: &[String],
    iterations: usize,
    reason: StopReason,
    gen: &dyn TextGenerator,
    cfg: &AgentConfig,
    cancel: &CancellationToken,
) -> AgentOutcome {
    let answer = if evidence.is_empty() {
        REFUSAL.to_string()
    } else {
        synthesize(gen, question, transcript, cfg, cancel).await
    };
    let stop_reason = if evidence.is_empty() { StopReason::Refused } else { reason };
    AgentOutcome { answer, iterations, stop_reason, evidence_ids: evidence.to_vec() }
}

#[cfg(test)]
mod tests {
    use super::tools::test_support::MockTool;
    use super::tools::ToolRegistry;
    use super::*;
    use crate::services::inference::{EngineHealth, GenError, GenOutcome};
    use std::sync::Mutex;

    /// A `TextGenerator` that returns a fixed queue of responses in order (then
    /// repeats the last). Lets loop tests script the controller deterministically.
    struct ScriptedGen {
        responses: Mutex<std::collections::VecDeque<String>>,
    }
    impl ScriptedGen {
        fn new(responses: Vec<&str>) -> Self {
            Self { responses: Mutex::new(responses.into_iter().map(String::from).collect()) }
        }
    }
    #[async_trait::async_trait]
    impl TextGenerator for ScriptedGen {
        async fn generate_stream(
            &self,
            _prompt: &str,
            _params: &GenParams,
            _cancel: CancellationToken,
            on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        ) -> Result<GenOutcome, GenError> {
            let mut q = self.responses.lock().unwrap();
            let text = if q.len() > 1 { q.pop_front().unwrap() } else { q.front().cloned().unwrap_or_default() };
            on_token(&text);
            Ok(GenOutcome { text, was_cancelled: false })
        }
        async fn health(&self) -> EngineHealth {
            EngineHealth { ready: true, ..Default::default() }
        }
    }

    fn registry_with(name: &str, ev: &str) -> (ToolRegistry, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let (tool, calls) = MockTool::new(name, ev);
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(tool));
        (reg, calls)
    }

    #[test]
    fn parse_action_handles_fences_and_validates_tool() {
        let names = vec!["search".to_string()];
        let a = parse_action("```json\n{\"action\":\"tool\",\"tool\":\"search\",\"args\":{\"q\":\"x\"}}\n```", &names);
        assert_eq!(a, Some(AgentAction::Tool { name: "search".into(), args: serde_json::json!({"q":"x"}) }));
        // Unregistered tool rejected.
        assert_eq!(parse_action("{\"action\":\"tool\",\"tool\":\"evil\"}", &names), None);
        // Finish parsed.
        assert_eq!(
            parse_action("{\"action\":\"finish\",\"answer\":\"done\"}", &names),
            Some(AgentAction::Finish { answer: "done".into() })
        );
    }

    #[tokio::test]
    async fn tool_then_finish_produces_direct_answer() {
        let (reg, calls) = registry_with("search", "e1");
        let gen = ScriptedGen::new(vec![
            r#"{"action":"tool","tool":"search","args":{}}"#,
            r#"{"action":"finish","answer":"The limit is Rs 100,000 [1]."}"#,
        ]);
        let out = run_agent(&gen, &reg, "what is the limit?", &AgentConfig::default(), CancellationToken::new()).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(out.stop_reason, StopReason::Finished);
        assert_eq!(out.answer, "The limit is Rs 100,000 [1].");
        assert_eq!(out.evidence_ids, vec!["e1".to_string()]);
    }

    #[tokio::test]
    async fn no_evidence_refuses() {
        let reg = ToolRegistry::new();
        // Controller immediately finishes with an empty answer, no evidence.
        let gen = ScriptedGen::new(vec![r#"{"action":"finish","answer":""}"#]);
        let out = run_agent(&gen, &reg, "unanswerable?", &AgentConfig::default(), CancellationToken::new()).await;
        assert_eq!(out.stop_reason, StopReason::Refused);
        assert_eq!(out.answer, REFUSAL);
    }

    #[tokio::test]
    async fn repeated_same_evidence_stops_on_no_progress() {
        // Same tool, same evidence id every call; controller keeps choosing it.
        let (reg, calls) = registry_with("search", "dup");
        let gen = ScriptedGen::new(vec![r#"{"action":"tool","tool":"search","args":{}}"#]);
        let out = run_agent(&gen, &reg, "loop?", &AgentConfig { max_iter: 5, ..Default::default() }, CancellationToken::new()).await;
        // iter0 gathers "dup"; iter1 sees only "dup" again → no-progress stop.
        assert_eq!(out.stop_reason, StopReason::NoProgress);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn malformed_control_output_refuses_safely() {
        let reg = ToolRegistry::new();
        let gen = ScriptedGen::new(vec!["not json at all"]);
        let out = run_agent(&gen, &reg, "q?", &AgentConfig::default(), CancellationToken::new()).await;
        assert_eq!(out.stop_reason, StopReason::Refused);
        assert_eq!(out.answer, REFUSAL);
    }
}
