# Noviq — laptop convenience targets.
.PHONY: deploy

# F014 (T034) — promote the current branch to `release`, which the GPU server's
# poll-timer (deploy/noviq-deploy.timer) picks up within ~2 min and auto-builds.
# Normal workflow: work + push on your feature branch freely; `make deploy` when
# a change is ready for the testers. The VM always tracks `release`, so starting a
# new feature branch never requires touching the VM (verified 2026-06-27,
# VM_HANDOFF_VERIFY_DEPLOY.md). Override the target ref via DEPLOY_BRANCH=.
DEPLOY_BRANCH ?= release
deploy: ## push current branch -> origin/$(DEPLOY_BRANCH) (triggers server auto-deploy)
	git push origin $$(git rev-parse --abbrev-ref HEAD):$(DEPLOY_BRANCH)
