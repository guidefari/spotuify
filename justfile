build:
	@xcrun --find metal >/dev/null 2>&1 || (echo 'Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain' >&2; exit 1)
	cargo build -p spotuify-desktop

run: build
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
