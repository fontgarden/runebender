# Run the editor from this checkout.
#
#   make run FONT=path/to/Font.designspace
#   make run-release FONT=path/to/Font.designspace

FONT ?=

# Development build with the parallel front end of the nightly compiler.
# It compiles fast, and the editor runs slowly.
.PHONY: run
run:
	RUSTFLAGS=-Zthreads=8 cargo +nightly run -- $(FONT)

# Optimized build for this machine: the `performance` profile in Cargo.toml and the
# instruction set of the local processor.
# The first build takes several minutes, and the binary may not run on another processor.
.PHONY: run-release
run-release:
	RUSTFLAGS="-C target-cpu=native" cargo run --profile performance --locked -- $(FONT)
