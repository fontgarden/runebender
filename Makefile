# Run the editor with the nightly compiler's parallel frontend:
#
#   make run FONT=path/to/Font.designspace

FONT ?=

.PHONY: run
run:
	RUSTFLAGS=-Zthreads=8 cargo +nightly run -- $(FONT)
