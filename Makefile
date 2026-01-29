## ----------------------------------------------------------------------
## This Makefile provides common project automation tasks such as setup, 
## testing, and code checks in a single file so that users don't have to 
## remember multiple commands. Each task is defined as a Makefile target 
## with a brief description provided in comments above each target. 
## ----------------------------------------------------------------------

.PHONY: setup check test

help:   ## Show this help message
	@sed -ne '/@sed/!s/## //p' $(MAKEFILE_LIST)

setup:  ## Set up runtime environment
	python3 scripts/setup_runtime.py

check:  ## Run repository checks
	python3 scripts/ci/check_repo.py

test:   ## Run the test suite
	pytest -q tests