build:
	@xcrun --find metal >/dev/null 2>&1 || (echo 'Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain' >&2; exit 1)
	cargo build -p spotuify-desktop

run:
	@xcrun --find metal >/dev/null 2>&1 || (echo 'Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain' >&2; exit 1)
	cargo build --bin spotuify
	SPOTUIFY_BIN="$PWD/target/debug/spotuify" cargo run -p spotuify-desktop

bundle:
	cargo build -p spotuify-desktop --release

doctor:
	@xcrun --find metal >/dev/null 2>&1 || (echo 'Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain' >&2; exit 1)
	cargo check -p spotuify-desktop

test:
	cargo test -p spotuify-desktop
