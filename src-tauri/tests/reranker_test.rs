use std::path::PathBuf;

fn model_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("models")
        .join("bge-reranker-base")
}

#[test]
#[ignore = "requires bge-reranker-base in models/ dir"]
fn ranks_relevant_chunk_higher_than_irrelevant() {
    let model = model_dir().join("model.onnx");
    let tok = model_dir().join("tokenizer.json");
    if !model.exists() || !tok.exists() {
        panic!("missing model files at {model:?} / {tok:?}");
    }
    let slot = echo_ai_lib::services::reranker::RerankerSlot::new(model, tok);
    let query = "What is the minimum service for promotion to BPS-18?";
    let candidates = vec![
        "Section 8: For B-18, minimum length of service shall be 5 years in B-17.".to_string(),
        "The cat sat on the mat. Recipes for chicken curry require fresh spices.".to_string(),
        "Rule 12(2) describes age relaxation for candidates exceeding the prescribed age limit."
            .to_string(),
    ];
    let scores = slot.score_pairs(query, &candidates).unwrap();
    assert_eq!(scores.len(), 3);
    assert!(scores[0] > scores[1], "scores: {scores:?}");
    assert!(scores[0] > scores[2], "scores: {scores:?}");
}
