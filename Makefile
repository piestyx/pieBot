## ----------------------------------------------------------------------
## This Makefile provides common project automation tasks such as setup, 
## testing, and code checks in a single file so that users don't have to 
## remember multiple commands. Each task is defined as a Makefile target 
## with a brief description provided in comments above each target. 
## ----------------------------------------------------------------------

.PHONY: fold test setup init-gsama check clean-pycache

fold:       ## Folds comments to 70 char rows
	fold -s -w 70 input.txt

help:       ## Show this help message
	@sed -ne '/@sed/!s/## //p' $(MAKEFILE_LIST)

test:       ## Run the test suite
	pytest -q tests

setup:      ## Set up runtime environment
	python3 scripts/setup_runtime.py
	python3 scripts/init_gsama_state.py

init-gsama: ## Initialize GSAMA state
	python3 scripts/init_gsama_state.py

check:      ## Run repository checks
	python3 scripts/ci/check_repo.py

clean-pycache:
	@find . \
		-type d \( -name "__pycache__" -o -name ".pytest_cache" -o -name ".mypy_cache" -o -name ".ruff_cache" \) \
		-prune -exec rm -rf {} + 2>/dev/null || true
	@find . -type f \( -name "*.pyc" -o -name "*.pyo" \) -delete 2>/dev/null || true