-- Delete a source and the graph rows go with it.
--
-- `chunks.source_id` is ON DELETE CASCADE (001_baseline.sql:92), but the graph
-- tables built on top of chunks were created with bare REFERENCES, which in
-- PostgreSQL means NO ACTION:
--
--     sources --CASCADE--> chunks --NO ACTION--> graph_nodes --NO ACTION--> graph_edges
--
-- So `DELETE FROM sources WHERE id = $1` cascades into `chunks` and then hits a
-- restrict, aborting the whole delete for any source whose legal graph has been
-- built. Four call sites work around this by hand-deleting edges, then nodes,
-- then chunks in order (`repositories::chunks::delete_by_source`,
-- `commands::folders::remove_folder_source`, `api::documents::delete_folder_source`
-- and `::hard_delete_source`) — but two others do not, and they are the ones that
-- fail quietly:
--
--   * `commands::sources::clean_orphan_sources` logs the FK error and `continue`s,
--     so orphan cleanup silently skips every graphed source and they accumulate.
--   * `repositories::sources` (the "source is now unreferenced" path) propagates
--     the error to the caller.
--
-- Making the constraints cascade fixes both without asking every future call
-- site to remember the ordering. The existing hand-written deletions stay: they
-- are now redundant for graph rows but still required for `chunk_embeddings`,
-- which is a runtime vec0 virtual table and cannot carry a foreign key.
--
-- Constraint names are discovered from the catalogue rather than assumed, so a
-- database whose FKs were auto-named differently is still repaired.

DO $$
DECLARE
    tgt record;
    con record;
BEGIN
    FOR tgt IN
        SELECT * FROM (VALUES
            ('graph_nodes', 'chunk_id',  'chunks',      'id'),
            ('graph_edges', 'from_node', 'graph_nodes', 'id'),
            ('graph_edges', 'to_node',   'graph_nodes', 'id')
        ) AS t(child, col, parent, pcol)
    LOOP
        -- drop whatever single-column FK currently guards this column
        FOR con IN
            SELECT c.conname
            FROM pg_constraint c
            JOIN pg_class     r ON r.oid = c.conrelid
            JOIN pg_attribute a ON a.attrelid = r.oid AND a.attnum = ANY (c.conkey)
            WHERE c.contype = 'f'
              AND r.relname = tgt.child
              AND a.attname = tgt.col
              AND array_length(c.conkey, 1) = 1
        LOOP
            EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', tgt.child, con.conname);
        END LOOP;

        EXECUTE format(
            'ALTER TABLE %I ADD CONSTRAINT %I FOREIGN KEY (%I) '
            'REFERENCES %I(%I) ON DELETE CASCADE',
            tgt.child,
            tgt.child || '_' || tgt.col || '_fkey',
            tgt.col, tgt.parent, tgt.pcol);
    END LOOP;
END $$;
