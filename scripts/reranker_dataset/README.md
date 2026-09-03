# Reranker dataset pipeline

Run in order:

1. Stage A (manual): Open EchoAI app, add folder `~/Desktop/Sindh Govt Documents/`, wait for `indexing_status=indexed` on all 6 sources.
2. `python -m scripts.reranker_dataset.stage_b_audit`
3. `python -m scripts.reranker_dataset.stage_c_positives`
4. `python -m scripts.reranker_dataset.stage_d_mine`
5. `python -m scripts.reranker_dataset.stage_e_emit`

Outputs land in `datasets/reranker/v1/`.

Setup:
    python -m venv .venv-reranker
    source .venv-reranker/bin/activate
    pip install -r scripts/reranker_dataset/requirements.txt
