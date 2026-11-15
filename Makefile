WASM_TARGET := wasm32-unknown-unknown
WASM_OUT    := target/$(WASM_TARGET)/release/pelago.wasm
PKG_VERSION := 0.1.0

ifeq ($(shell uname -s),Darwin)
  TYPST_DATA_DIR ?= $(HOME)/Library/Application Support
else
  TYPST_DATA_DIR ?= $(or $(XDG_DATA_HOME),$(HOME)/.local/share)
endif
LOCAL_PKG_DIR := $(TYPST_DATA_DIR)/typst/packages/local/pelago/$(PKG_VERSION)

.PHONY: all wasm install test clippy paper clean

all: wasm

wasm:
	cargo build --release --lib --target $(WASM_TARGET)
	cp $(WASM_OUT) pelago.wasm.tmp && mv pelago.wasm.tmp pelago.wasm

install:
	mkdir -p "$(LOCAL_PKG_DIR)"
	cp typst.toml lib.typ pelago.wasm README.md LICENSE "$(LOCAL_PKG_DIR)/"
	@echo "Installed @local/pelago:$(PKG_VERSION) -> $(LOCAL_PKG_DIR)"

test: wasm
	cargo test

clippy:
	cargo clippy --all-targets

paper: wasm
	typst compile paper.typ paper.pdf

clean:
	cargo clean
	rm -f paper.png
