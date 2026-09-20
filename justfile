install:
  cargo install --path . --root $HOME/.local

build:
  cargo build

format:
  cargo fmt --all

check:
  cargo test --all-targets --all-features
  cargo fmt --all -- --check
  cargo clippy --all-targets --all-features -- -D warnings

release-dry-run:
  cargo release patch

release:
  cargo release patch --execute
