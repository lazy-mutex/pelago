WASM_TARGET := wasm32-unknown-unknown
WASM_OUT    := target/$(WASM_TARGET)/release/pelago.wasm

.PHONY: all wasm test clippy paper clean

all: wasm

wasm:
	cargo build --release --lib --target $(WASM_TARGET)
	cp $(WASM_OUT) pelago.wasm.tmp && mv pelago.wasm.tmp pelago.wasm

test: wasm
	cargo test

clippy:
	cargo clippy --all-targets

paper: wasm
	typst compile paper.typ paper.pdf

clean:
	cargo clean
	rm -f paper.pdf paper.png
