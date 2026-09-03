# Investigation: why the 0.5B never emits `[N]` citations

**Date:** 2026-06-13
**Question:** is the missing `[N]` citation a prompt, model-capability, parser, or
post-processing problem?
**Method:** prompt/cleanup code review + the two saved traces + a controlled ollama
experiment using the **production** 0.5B GGUF (`sindh05`) and comparison models.

## 1. Prompt instructions given to the model

System prompt (Grounding v5.2.0, verbatim from the trace's own prompt dump):

> "You are a Pakistan government legal assistant. No specific provision was named. Answer the
> officer's question using ONLY the retrieved context. **Cite every claim as [n].** If the answer
> is not in the context, say so explicitly."

- The context **is** labelled `[1] <file>, page P\n<text>` — `build_context_string`
  (`query_router.rs:1755-1770`). So the model is shown the indices it should cite.
- The user turn is just `Question: {query}` — `build_rag_prompt` (`query_router.rs:1786-1804`).

## 2. Examples sent to the model

**None.** `build_rag_prompt` injects system + (optional prior turns for follow-ups) + context +
`Question:` — there is **no few-shot example** demonstrating `[n]` usage anywhere. The only
citation signal is the single imperative "Cite every claim as [n]."

## 3. Raw model outputs

Both traces log "**Raw model output (pre-cleanup)**" — neither contains any `[n]`:
- Log 1: "The rule that applies … is Rule 2017 of the Sindh Civil Services Rules."
- Log 2: "The Election Commission will conduct the election for local governments."

So the model genuinely does not emit `[n]` (the absence is at generation time, not after cleanup).

## 4. Citation syntax in training examples

Not directly inspectable — the fine-tune data is not in the repo. Indirect evidence: the related
noviq fine-tune notes record that the model was trained to emit **inline "Article N"**, not the
bracketed form (`[Art N]` scored 0/10 on held-out). Consistent with a fine-tune that did not
teach the `[n]` index-marker convention.

## 5. Does a stronger model emit `[N]` under (near-)identical prompts?

Controlled ollama experiment — same system + 3 labelled context passages + same question:

| Model | Prompt | Emits `[N]`? | Note |
|---|---|---|---|
| `sindh05` (production 0.5B) | zero-shot (current) | **NO** | output also degenerated into number-spam |
| `noviq:latest` (3B FT) | zero-shot (current) | **YES** `[1]` | stronger model follows the instruction |
| `qwen2.5:1.5b` | zero-shot | (HTTP 400, inconclusive) | template error, retest needed |
| `granite4:3b` | zero-shot | (HTTP 400, inconclusive) | template error, retest needed |
| **`sindh05` (production 0.5B)** | **+ ONE one-shot example** | **YES** `[2]` `[3]` | **citing once shown a demonstration** |

(Mistral-7B build did not finish in time; `noviq` 3B already demonstrates the stronger-model case.)

## 6. Is the parser stripping citations after generation?

**No.** `clean_llm_output` (`query_router.rs:1892-1944`) only: strips `<think>…</think>`, drops
stray "Part 1/Part 2/References" header lines, and collapses duplicate sentences. It does **not**
remove `[n]` markers. (The citation-marker regexes at L1041/L1141 operate on other grounding
paths, not the displayed answer.) And the raw pre-cleanup output already had none — so there was
nothing to strip.

## Determination

| Hypothesis | Verdict | Evidence |
|---|---|---|
| **Prompt issue** | **PRIMARY** | zero-shot "Cite as [n]" with **no demonstration** fails on the 0.5B; adding **one one-shot example** made the same production model emit `[2]`,`[3]`; a stronger model follows even zero-shot |
| **Model capability** | **SECONDARY** | the 0.5B is weak/unstable (number-spam in test 1); even when it cites, quality is poor → a stronger generator is more reliable |
| **Parser issue** | **RULED OUT** | `clean_llm_output` doesn't touch `[n]` |
| **Post-processing issue** | **RULED OUT** | raw pre-cleanup output already lacks `[n]` |

**Conclusion:** the citation failure is **primarily a prompt issue** (no few-shot demonstration),
with a **secondary model-capability** factor. It is NOT a parser/post-processing problem.

## Recommended fix (cheap, no re-index, no model swap required)

1. Add a **one-shot citation demonstration** to `build_rag_prompt` (a tiny worked example showing
   `… [1].`), and tighten the instruction. The experiment shows this alone flips the production
   0.5B from NO → YES.
2. Pair with the grounding-gate fix (refuse/flag when no `[n]` / grounding 0.00) so any remaining
   uncited answers don't ship as authoritative.
3. Optional, higher-reliability: route chat through a stronger generator (noviq-3B / Mistral-7B),
   which cites even zero-shot.

## Reproduction

`ollama create sindh05 -f Modelfile` (FROM the production GGUF), then `POST /api/chat` with
`temperature:0.1`; compare the current single-instruction prompt vs. the same prompt with one
prior `user`/`assistant` pair demonstrating `[n]` usage.
