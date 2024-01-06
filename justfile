run-client:
	cd src/client
	cargo +nightly-2023-09-08 autoken check
	RUST_BACKTRACE=1 cargo run -p giaw-client

run-server:
	cd src/server
	cargo +nightly-2023-09-08 autoken check
	cargo run -p giaw-server
