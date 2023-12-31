run:
	cargo +nightly-2023-09-08 autoken check
	RUST_BACKTRACE=1 cargo run -p giaw-client
