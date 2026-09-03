# Feature Specification: Constitution v1 — Hierarchical + Graph Retrieval

**Feature**: 003-constitution-hierarchical-graph
**Date**: 2026-05-21
**Status**: spec
**Design source**: `docs/research/legal-corpus-hierarchy-architecture.md` (§1–§7)

## Problem

Indexing `constitution.pdf` through the current pipeline produces **1 document /
11 sections / 253 leaves** — 6 section labels absorb 93 % of all leaves
(`Article 69` alone = 48 leaves spanning ~60 articles). Cause: PDF text
extraction lost ~93 % of article headings (`‖`/`]]` artifacts; only 11 of ~290
article boundaries survived as detectable). Section labels mislabel ~93 % of
leaves, so citations are wrong and the reranker cannot disambiguate articles —
the root cause of incorrect constitutional answers (Tier 3 failures F1–F5).

## Goal

Index the Constitution of Pakistan as a structure-aware **hierarchical tree +
graph**, from a clean HTML source, with leaves at **sub-clause** granularity, so
that retrieval is article/clause-precise and citations are correct.

## Scope (v1)

In: constitution only, from the clean pakistani.org HTML source. Out (→ v2):
generic OCR/layout-aware extraction, Sindh Act/Rules doc-type profiles, reranker
fine-tune, multi-hop (>1) graph reasoning.

## Requirements

- **R1** Acquire the constitution from a clean structured source (pakistani.org)
  via a **dev-time/offline import tool** — no runtime network access (Principle I).
- **R2** Parse into the hierarchy `Document → Part → Chapter → Article → Clause →
  Sub-clause`; provisos/explanations kept attached in-leaf, never split from the
  clause they modify.
- **R3** Each leaf carries a canonical citation path + structural, amendment, and
  domain metadata.
- **R4** Build an in-`echo.db` graph: `contains`, `cross_reference`,
  `non_obstante`, `amended_by` edges.
- **R5** Embed multi-level: sub-clause leaves (heading + chapeau prepended) +
  article + part chunks.
- **R6** Retrieval: citation fast-path → hybrid BM25/FTS5 ⊕ vector RRF → reranker
  → small-to-big expansion → 1-hop graph expansion → clause-precise citations.

## Acceptance criteria (the v1 gate)

v1 must pass all four before v2/OCR work begins:

- **A1 Structural** — ~12 Parts, ~30 Chapters, ~290 Articles, ~1,500 Clauses,
  ~3–5k sub-clause leaves; **no parent holding >90 % of leaves**; orphans = 0.
- **A2 Citation accuracy** — every sampled leaf's `citation_path` matches the
  article it actually contains.
- **A3 Graph integrity** — `cross_reference` edges resolve to real nodes;
  `amended_by` edges present; a known non-obstante article spot-checked.
- **A4 Retrieval** — re-run F1–F5: article-number queries (F1/F2/F4) retrieve
  the correct article via the citation fast-path. F3/F5 should improve (graph
  expansion) but may still need the fine-tuned reranker — tracked separately.

## Non-goals

No frontend changes in v1. No change to the LLM tier/model. No new database file.
