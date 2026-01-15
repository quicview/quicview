SHELL := /bin/zsh

.PHONY: all build test web tauri client ctrl-server ctrl-client pin tofu pin-from-pem pin-from-live

all: build

build:
	cargo build --workspace

test:
	cargo test --workspace

web:
	cd apps/leptos-web && trunk build --release

tauri:
	cargo build -p tauri-client && open target/debug/tauri-client.app

client:
	cargo run -p cli --features http-ui,macos-capture,macos-input,clipboard -- \
		client --start --port 21180 --auth-token $${QUICVIEW_TOKEN:-dev-token} \
		--static-dir ./apps/leptos-web/dist

ctrl-server:
	cargo run -p cli --features quic-ctrl -- ctrl-server --token $${QUICVIEW_CTRL_TOKEN:-dev-token}

ctrl-client:
	cargo run -p cli --features http-ui,quic-ctrl -- \
		client --ctrl-addr $${QUICVIEW_CTRL_ADDR:-127.0.0.1:4433} \
		--ctrl-token $${QUICVIEW_CTRL_TOKEN:-dev-token} --open

# Example helpers; override variables on invocation as needed
pin:
	@[ -n "$$PIN_HEX" ] || (echo "Usage: make pin PIN_HEX=<hex> SNI=<name> ADDR=<host:port>" && exit 1)
	cargo run -p cli --features http-ui,quic-ctrl -- \
		client --ctrl-addr $${ADDR:-127.0.0.1:4433} --ctrl-token $${QUICVIEW_CTRL_TOKEN:-dev-token} \
		--ctrl-tls pin:$${PIN_HEX} --ctrl-sni $${SNI:-localhost} --open

tofu:
	cargo run -p cli --features http-ui,quic-ctrl -- \
		client --ctrl-addr $${ADDR:-ctrl.example.com:4433} --ctrl-token $${QUICVIEW_CTRL_TOKEN:-dev-token} \
		--ctrl-tls tofu --ctrl-sni $${SNI:-ctrl.example.com} \
		--ctrl-tofu-pin-file $${PIN_FILE:-$$HOME/.config/quicview/ctrl.pin} --open

# Compute a DER SHA-256 pin (lowercase hex) from a PEM file
pin-from-pem:
	@[ -n "$$PEM" ] || (echo "Usage: make pin-from-pem PEM=<path-to-leaf-cert.pem>" && exit 1)
	@openssl x509 -in "$$PEM" -outform DER | shasum -a 256 | awk '{print $$1}'

# Compute a pin live from a server (requires OpenSSL). Defaults PORT=4433, SNI=HOST
pin-from-live:
	@[ -n "$$HOST" ] || (echo "Usage: make pin-from-live HOST=<hostname> [PORT=4433] [SNI=<name>]" && exit 1)
	@openssl s_client -connect "$${HOST}:$${PORT:-4433}" -servername "$${SNI:-$${HOST}}" -showcerts < /dev/null 2>/dev/null \
		| openssl x509 -outform DER | shasum -a 256 | awk '{print $$1}'
