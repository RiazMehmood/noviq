//! Feature 005 — definition-of-done acceptance gate.
//!
//! Runs without an LLM. Asserts that:
//!   1. Every Sindh fixture gets a non-`generic` doc_type.
//!   2. Random unstructured text falls to `generic` and still produces a tree.
//!   3. A real Rules fixture, when persisted, produces graph_nodes > 0.

use std::fs;
use std::path::PathBuf;

fn read_fixture(name: &str) -> String {
    let p = PathBuf::from(format!("tests/fixtures/structured_indexing/{name}.txt"));
    fs::read_to_string(&p).unwrap_or_else(|_| panic!("missing fixture {p:?}"))
}

#[test]
fn every_sindh_fixture_gets_non_generic_doc_type() {
    // Mappings derived from the live-indexed corpus (memory note
    // project-echoai-feature004-eval): efficiency_and_disciplane_rules and
    // appointment_promotion_and_transfer_rules → rules; the Removal Act → act;
    // the Manual → manual.
    let cases = [
        ("appointment_promotion_and_transfer_rules_pdf",  "rules"),
        ("efficiency_and_disciplane_rules_pdf",           "rules"),
        ("REMOVAL_FROM_SERVICE_SPECIAL_POWERS_SINDH_pdf", "act"),
        ("Govt_of_Sindh_Manual_of_Secretariat_instruction_2010_pdf", "manual"),
    ];
    for (file, expected_type) in cases {
        let text = read_fixture(file);
        let out = echo_ai_lib::services::structured_importer::import(&text, file, None, &[], &Default::default());
        assert_eq!(
            out.doc_type, expected_type,
            "wrong doc_type for {file}: got {} (structured={})", out.doc_type, out.structured
        );
        assert!(out.structured, "{file} should be structured");
    }
}

#[test]
fn generic_doc_produces_a_tree_with_at_least_one_chapter() {
    let text = "1. Introduction\n\nbody body body\n\n1.1 Background\n\nmore body\n\n2. Method\n\nbody";
    let out = echo_ai_lib::services::structured_importer::import(text, "memo.txt", None, &[], &Default::default());
    assert_eq!(out.doc_type, "generic");
    assert!(!out.tree.children.is_empty(), "generic should still produce some structure");
}

#[test]
fn rules_fixture_persists_chunks_and_graph_nodes() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = echo_ai_lib::db::testing::test_pool().await;
        sqlx::query(
            "INSERT INTO sources \
                 (id, file_path, file_name, format, file_size_bytes, file_hash, \
                  last_modified, indexing_status, created_at) \
             VALUES \
                 ('s', '/x', 'x.pdf', 'pdf', 1, 'h', 0, 'processing', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let text = read_fixture("efficiency_and_disciplane_rules_pdf");
        let out  = echo_ai_lib::services::structured_importer::import(&text, "x.pdf", None, &[], &Default::default());
        assert!(out.structured, "efficiency rules should detect structure");
        // 6th arg (`tables`) added by F023 structured-table capture on 2026-07-12.
        // This call site was not updated, so the whole acceptance binary stopped
        // compiling — and therefore stopped running — for two weeks. That is part
        // of why the contents-leak defect (trace a72f1539) reached production.
        let stats =
            echo_ai_lib::services::structured_importer::persist(&pool, "s", &out, &[], "", &[])
                .await
                .unwrap();
        assert!(stats.chunks      >= 10, "expected ≥10 chunks, got {}", stats.chunks);
        assert!(stats.graph_nodes >= 10, "expected ≥10 graph_nodes, got {}", stats.graph_nodes);
    });
}
