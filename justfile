build:
	@xcrun --find metal >/dev/null 2>&1 || (echo 'Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain' >&2; exit 1)
	cargo build -p spotuify-desktop

_stop-desktop:
	@pkill -TERM -x spotuify-desktop 2>/dev/null || true; pkill -TERM -x SpotuifyDesktop 2>/dev/null || true
	@for attempt in $(seq 1 20); do if ! pgrep -x spotuify-desktop >/dev/null && ! pgrep -x SpotuifyDesktop >/dev/null; then exit 0; fi; sleep 0.1; done; echo 'Desktop app did not stop cleanly; forcing exit' >&2; pkill -KILL -x spotuify-desktop 2>/dev/null || true; pkill -KILL -x SpotuifyDesktop 2>/dev/null || true

run: _stop-desktop build
	@xcrun --find metal >/dev/null 2>&1 || (echo 'Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain' >&2; exit 1)
	cargo build --bin spotuify
	bash scripts/package-macos-app.sh debug
	open target/dist/Spotuify.app

bundle:
	@xcrun --find metal >/dev/null 2>&1 || (echo 'Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain' >&2; exit 1)
	cargo build -p spotuify-desktop --release
	cargo build --bin spotuify --release
	bash scripts/package-macos-app.sh release

doctor:
	@xcrun --find metal >/dev/null 2>&1 || (echo 'Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain' >&2; exit 1)
	cargo check -p spotuify-desktop

test:
	cargo test -p spotuify-desktop
