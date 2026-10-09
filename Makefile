.PHONY: bench install install-local build run test lint fmt check version package-release smoke-install release release-push vibe-kernel-path vibe-kernel-set vibe-pull

CARGO_TARGET_DIR ?= $(HOME)/construction_side/fleet-scope/target
DIST_DIR ?= $(HOME)/construction_side/fleet-scope/dist
INSTALL_DIR ?= $(HOME)/.x-cli-fleet-scope
export CARGO_TARGET_DIR

build:
	cargo build --release --locked

install: install-local

install-local: package-release
	sh scripts/install.sh --archive "$(DIST_DIR)/fleet-scope-macos-aarch64.tar.gz" --install-dir "$(INSTALL_DIR)"

bench: build
	"$(CARGO_TARGET_DIR)/release/fleet-scope" benchmark --samples 20

run:
	cargo run --locked --

test:
	cargo test --locked --all-targets

lint:
	cargo clippy --locked --all-targets -- -D warnings

fmt:
	cargo fmt --all

check:
	cargo fmt --all -- --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked --all-targets
	cargo build --release --locked
	sh -n scripts/install.sh scripts/package-release.sh scripts/release.sh
	python3 scripts/check-delivery.py
	python3 scripts/benchmark-intervals.py --help >/dev/null

version:
	cargo run --locked -- --version

package-release: build
	sh scripts/package-release.sh "$(CARGO_TARGET_DIR)/release/fleet-scope" "$(DIST_DIR)"

smoke-install: package-release
	python3 scripts/check-delivery.py --archive "$(DIST_DIR)/fleet-scope-macos-aarch64.tar.gz"

release:
	sh scripts/release.sh

release-push:
	@set -eu; \
	test "$$(git branch --show-current)" = main || { echo "ERROR: release requires main" >&2; exit 1; }; \
	version="$$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"; \
	git rev-parse --verify "refs/tags/v$$version" >/dev/null; \
	git push origin main --follow-tags

vibe-kernel-path:
	@test -f .vibe/KERNEL_SOURCE || { echo "Missing .vibe/KERNEL_SOURCE. Run: make vibe-kernel-set" >&2; exit 1; }
	@sed -n '1p' .vibe/KERNEL_SOURCE

vibe-kernel-set:
	@mkdir -p .vibe; \
	if [ -n "$(KERNEL)" ]; then kernel_root="$(KERNEL)"; else printf "Kernel path: "; read -r kernel_root; fi; \
	case "$$kernel_root" in /*) ;; *) echo "ERROR: kernel path must be absolute." >&2; exit 1;; esac; \
	test -f "$$kernel_root/tools/vibe-pull" || { echo "ERROR: invalid kernel path: $$kernel_root" >&2; exit 1; }; \
	printf "%s\n" "$$kernel_root" > .vibe/KERNEL_SOURCE

vibe-pull:
	@test -f .vibe/KERNEL_SOURCE || { echo "Missing .vibe/KERNEL_SOURCE. Run: make vibe-kernel-set" >&2; exit 1; }
	@kernel_root="$$(sed -n '1p' .vibe/KERNEL_SOURCE)"; \
	python3 "$$kernel_root/tools/vibe-pull" .

# VIBE:KERNEL_MAKE_START

.PHONY: vibe-propose

vibe-propose:
	@test -f .vibe/KERNEL_SOURCE || { echo "Missing .vibe/KERNEL_SOURCE. Run: make vibe-kernel-set" >&2; exit 1; }
	@kernel_root="$$(sed -n '1p' .vibe/KERNEL_SOURCE)"; \
	test -f "$$kernel_root/tools/vibe-propose" || { echo "Missing $$kernel_root/tools/vibe-propose. Update the kernel source first." >&2; exit 1; }; \
	python3 "$$kernel_root/tools/vibe-propose" .

# VIBE:KERNEL_MAKE_END
