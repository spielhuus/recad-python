VENV = $(shell pwd)/.venv
PYTHON = $(VENV)/bin/python3
MATURIN = $(VENV)/bin/maturin

TARGET = $(VENV)/bin/recad
ACTIVATE = $(VENV)/bin/activate 

SPHINXOPTS    ?=
SPHINXBUILD   ?= $(VENV)/bin/sphinx-build
SOURCEDIR     = docs
BUILDDIR      = target/site

SOURCES = $(shell find src/ -name "*.rs") Makefile Cargo.toml pyproject.toml
DOCS = $(shell find docs/ -name "*.md") docs/conf.py

all: test doc $(TARGET) ## run test, doc and build target

$(ACTIVATE): pyproject.toml Cargo.toml
	uv venv
	uv pip install -U pip maturin neovim sphinx furo sphinx-exec-code scipy sympy pandas patchelf

clean: ## remove all build files.
	cargo clean
	rm -rf $(VENV)
	rm -rf target

$(TARGET): $(ACTIVATE) $(SOURCES)
	${MATURIN} develop --no-default-features --features=svg

test: $(TARGET) ## run all the test cases.
	$(PYTHON) -m unittest test.test_schema.TestSchemaLoad

doc: Makefile $(DOCS)
	$(SPHINXBUILD) "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) 

serve: doc
	$(PYTHON) -m http.server -d target/site

.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'
