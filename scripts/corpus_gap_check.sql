-- Corpus completeness check (2026-07-28) — find MISSING PROVISIONS in every
-- indexed document, without needing to know in advance what each should contain.
--
-- The idea: legal provisions are numbered consecutively, so the corpus can be
-- audited against itself. If a document has Articles 57, 58, 59, 62, 63 then
-- 60 and 61 were lost somewhere between the PDF and the database.
--
-- Run:
--   psql -h 127.0.0.1 -p <port> -U postgres -d echo -f corpus_gap_check.sql
--
-- Caveat, read before acting: a gap is a CANDIDATE, not proof. Legal texts do
-- legitimately omit numbers ("Article 152A. Omitted"), and a repealed provision
-- may be absent by design. Section 3 of this report prints the neighbouring
-- text so a human can tell "lost by the parser" from "not in the statute".

\pset pager off

\echo '== 1. PROVISION COVERAGE PER DOCUMENT =================================='
SELECT s.display_name AS document,
       m.unit,
       COUNT(*)                       AS provisions_found,
       MIN(m.n)                       AS lowest,
       MAX(m.n)                       AS highest,
       MAX(m.n) - MIN(m.n) + 1        AS expected_if_contiguous,
       (MAX(m.n) - MIN(m.n) + 1) - COUNT(*) AS apparently_missing
  FROM (
    SELECT DISTINCT c.source_id,
           (regexp_match(c.unit_label, '^(Article|Rule|Section|Regulation) ([0-9]+)$'))[1] AS unit,
           ((regexp_match(c.unit_label, '^(Article|Rule|Section|Regulation) ([0-9]+)$'))[2])::int AS n
      FROM chunks c
     WHERE c.unit_label ~ '^(Article|Rule|Section|Regulation) [0-9]+$'
  ) m
  JOIN sources s ON s.id = m.source_id
 GROUP BY 1, 2
 HAVING (MAX(m.n) - MIN(m.n) + 1) - COUNT(*) > 0
 ORDER BY apparently_missing DESC;

\echo ''
\echo '== 2. THE ACTUAL GAPS (which numbers are absent) ======================='
\echo '   likely_cause: TABLE/TOC = the missing numbers sit in a table region'
\echo '   (a contents listing or a schedule serial run), so they were never'
\echo '   provisions to begin with — noise, not loss. UNEXPLAINED = look at it.'
WITH nums AS (
  SELECT DISTINCT c.source_id,
         (regexp_match(c.unit_label, '^(Article|Rule|Section|Regulation) ([0-9]+)$'))[1] AS unit,
         ((regexp_match(c.unit_label, '^(Article|Rule|Section|Regulation) ([0-9]+)$'))[2])::int AS n
    FROM chunks c
   WHERE c.unit_label ~ '^(Article|Rule|Section|Regulation) [0-9]+$'
), seq AS (
  SELECT source_id, unit, n,
         LEAD(n) OVER (PARTITION BY source_id, unit ORDER BY n) AS next_n
    FROM nums
), gaps AS (
  SELECT source_id, unit, n + 1 AS gap_from, next_n - 1 AS gap_to, next_n - n - 1 AS how_many
    FROM seq
   WHERE next_n IS NOT NULL AND next_n - n > 1
)
SELECT s.display_name AS document,
       g.unit,
       g.gap_from,
       g.gap_to,
       g.how_many,
       CASE WHEN EXISTS (
              SELECT 1 FROM chunks c
               WHERE c.source_id = g.source_id
                 AND c.unit_label LIKE 'Table%'
                 AND c.content ~ ('(^|[[:space:]])' || g.gap_from || '\.[[:space:]]')
            ) THEN 'TABLE/TOC'
            ELSE 'UNEXPLAINED'
       END AS likely_cause
  FROM gaps g
  JOIN sources s ON s.id = g.source_id
 ORDER BY likely_cause, how_many DESC, document, g.gap_from
 LIMIT 60;

\echo ''
\echo '== 3. TRIAGE HELP: is the missing text nearby under another label? ====='
\echo '   (rows here mean the CONTENT survived but the LABEL was lost —'
\echo '    a parser miss. No rows means the text itself never made it in.)'
WITH nums AS (
  SELECT DISTINCT c.source_id,
         ((regexp_match(c.unit_label, '^(Article|Rule|Section|Regulation) ([0-9]+)$'))[2])::int AS n
    FROM chunks c
   WHERE c.unit_label ~ '^(Article|Rule|Section|Regulation) [0-9]+$'
), seq AS (
  SELECT source_id, n, LEAD(n) OVER (PARTITION BY source_id ORDER BY n) AS next_n FROM nums
), gaps AS (
  SELECT source_id, generate_series(n + 1, next_n - 1) AS missing_n
    FROM seq WHERE next_n IS NOT NULL AND next_n - n > 1
)
SELECT s.display_name AS document,
       g.missing_n    AS missing_provision,
       c.unit_label   AS found_under_label,
       c.page_number  AS page,
       LEFT(regexp_replace(c.content, '[[:space:]]+', ' ', 'g'), 90) AS text_starts_with
  FROM gaps g
  JOIN sources s ON s.id = g.source_id
  JOIN chunks c  ON c.source_id = g.source_id
                AND c.content ~ ('(^|[[:space:]])\[?' || g.missing_n || '\.[[:space:]]')
 ORDER BY document, g.missing_n
 LIMIT 40;
