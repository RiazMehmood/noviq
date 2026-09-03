---
id: 116
title: Pipeline demo on real Compendium
stage: explainer
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [teaching, ingestion, demo, vm]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- C:/Users/hp/Desktop/Noviq/Pipeline-Demo-Compendium/ (01-12 .md + raw-outputs/ + 30p pdf)
tests:
- harness built (cargo, --no-default-features --features server) and ran clean; prod tree clean, server active
---

## Prompt

ok now do one thing, as you describe the steps of extraction, now i want you
to use compendium of rules and use its first 20 or 30 pages, and create files
for example at step 1 u show me the document and name it 1, then on step 2 …
[full prompt: stage-by-stage files with function on top and output below,
3a/3b for branches, until final chunk creation; alternative offered = Google
Colab notebook with the exact same code; "so decide accordingly"]

## Response snapshot

Chose the real-run option (Colab would need a Python rewrite — not "exactly
the same code"). Built a 30-page excerpt of the Compendium on the VM
(pdfseparate/pdfunite), added a throwaway worktree + src/bin/dump_stages.rs
harness calling the REAL crate functions stage by stage (two tiny private fns
inlined verbatim), built against the cached target dir in 35 s, ran with the
production sidecar. Results: pdfplumber tier-0 win (quality 1.0), 115 KB→57 KB
normalization shrink, 46 chrome blocks (gazette headers + printed page
numbers), TOC lifted as front_matter, back-matter/table stages correct
no-ops, rules profile from title, 252 headings, 244-path tree, 226 final
chunks with compound citations (Rule 8(3)) and 53 catchline titles applied
(Rule 3 → "Powers and functions of Election Commission"). Assembled
Desktop\Noviq\Pipeline-Demo-Compendium\: 01-12 .md guides (function code on
top, verbatim output below) + raw-outputs/ + the excerpt PDF. Notable honest
edges documented: Rules 1-2 unminted (gazette two-column mangling: "les may
be called…"), (376) gazette-serial wart, split catchline 38/39. Worktree
removed; ~/pipeline-demo kept for re-runs (binary persists in
/opt/noviq/.cargo-target-test/debug/dump_stages).

## Outcome

- ✅ Impact: complete, verifiable stage-by-stage demo from a real corpus document.
- 🧪 Tests: end-to-end run clean; prod repo/service untouched (verified).
- 📁 Files: 13 md files + 15 raw outputs + pdf in Pipeline-Demo-Compendium.
- 🔁 Next prompts: re-run on other docs via the kept harness; adjust page range with pdfseparate -f/-l.
- 🧠 Reflection: the harness makes any future "show me on a real doc" lesson a 2-command exercise.

## Evaluation notes (flywheel)

- Failure modes observed: initial guide files paraphrased outputs; corrected to verbatim after reading raw files (Rules 1-2 reason, tree excerpt).
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
