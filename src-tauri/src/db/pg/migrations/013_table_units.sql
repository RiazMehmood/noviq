-- 013 — structured-table capture (Feature 023).
--
-- PDF tables (e.g. RoB Schedule-I 39-department distribution-of-business table)
-- are now captured as first-class units instead of being shredded into hundreds
-- of clause micro-chunks. Per detected table we emit ONE whole-table chunk
-- (unit_type='table', markdown header + all rows — for counting/list/summary)
-- and one chunk PER ROW (unit_type='table_row', header-labelled cells — for
-- targeted retrieval), mirrored into the graph as a 'table' node that CONTAINS
-- its 'table_row' children. See specs/023-structured-table-capture/.
--
-- Additive: widen the two CHECK vocabularies to a strict superset. No column or
-- table drop; existing rows always satisfy the widened constraint.

ALTER TABLE chunks DROP CONSTRAINT IF EXISTS chunks_unit_type_check;
ALTER TABLE chunks ADD CONSTRAINT chunks_unit_type_check
    CHECK (unit_type IN (
        'document','part','chapter','article','clause','subclause','section','leaf',
        'table','table_row'
    ));

ALTER TABLE graph_nodes DROP CONSTRAINT IF EXISTS graph_nodes_node_type_check;
ALTER TABLE graph_nodes ADD CONSTRAINT graph_nodes_node_type_check
    CHECK (node_type IN (
        'document','part','chapter','article','clause','subclause',
        'table','table_row'
    ));
