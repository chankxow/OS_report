# `make` builds ./mmap (needs cargo; downloads the pinned crates on the first build).
# mmap reads Linux /proc, so it builds and runs on Linux only (including WSL2 on Windows).
# On macOS, `make docker` builds and runs it in a Linux container instead.
CARGO  ?= cargo
DOCKER ?= docker
ARGS   ?= --demo
RUST_IMAGE = rust:1.75-slim-bookworm
UNAME_S := $(shell uname -s 2>/dev/null)

LINUX_ONLY = if [ "$(UNAME_S)" != "Linux" ]; then \
		echo "error: mmap reads Linux /proc and only runs on Linux."; \
		echo "  Windows: install WSL2 (wsl --install) and build inside it"; \
		echo "  macOS:   make docker   (runs it in a Linux container, see BUILD.md)"; \
		exit 1; fi

.PHONY: all test clean docker

all: mmap

mmap: mmap.rs model.rs procfs.rs ui.rs snapshot.rs demo.rs Cargo.toml Cargo.lock
	@$(LINUX_ONLY)
	@command -v $(CARGO) >/dev/null 2>&1 || { \
		echo "error: cargo not found. Install Rust first:"; \
		echo "  sudo apt install cargo        (Ubuntu/Debian)"; \
		echo "  or see https://rustup.rs"; \
		exit 1; }
	$(CARGO) build --release --locked
	cp target/release/mmap ./mmap

test:
	@$(LINUX_ONLY)
	$(CARGO) test --locked

# Build and run inside a throw-away Linux container. The source folder is mounted
# read-only; the build cache lives in two named Docker volumes (mmap-cargo, mmap-target).
# It sees only the processes inside the container, so the built-in demo is the default.
docker:
	$(DOCKER) run --rm -it --init -e COLORTERM \
		-v "$(CURDIR)":/src:ro -w /src \
		-v mmap-cargo:/usr/local/cargo/registry \
		-v mmap-target:/target -e CARGO_TARGET_DIR=/target \
		$(RUST_IMAGE) sh -c 'cargo build --release --locked && exec /target/release/mmap $(ARGS)'

clean:
	rm -rf target mmap
