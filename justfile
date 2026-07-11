build:
	cargo build -p spotuify-desktop

run:
	cargo build --bin spotuify
	SPOTUIFY_BIN="$PWD/target/debug/spotuify" cargo run -p spotuify-desktop

bundle:
	cargo build -p spotuify-desktop --release

doctor:
	cargo check -p spotuify-desktop

test:
	cargo test -p spotuify-desktop
