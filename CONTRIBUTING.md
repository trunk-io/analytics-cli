# Contributing

These are instructions for building, running, and testing the Rust CLI locally. Note that any changes are tightly coupled with Trunk Flaky Test services.

## Prerequisites

- Install a nightly version of Cargo using [rustup](https://doc.rust-lang.org/cargo/getting-started/installation.html)
- Install [CMake](https://cliutils.gitlab.io/modern-cmake/chapters/intro/installing.html)
- Install [protoc](https://grpc.io/docs/protoc-installation/)
- Run `trunk tools install`

### Optional Prerequisites

These are necessary for building particular targets.

- Install [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)
- Install [maturin](https://www.maturin.rs/installation.html)

## Build

### Build Everything

```bash
cargo build
```

The CLI will be built to `target/debug/trunk-analytics-cli`

### Python Bindings

```bash
pip install maturin
trunk run generate-pyi
maturin build --release --out dist --find-interpreter --manifest-path ./context-py/Cargo.toml
```

The 2 wheels will be available in `dist/`

### WASM Bindings

```bash
pnpm install --dir ./context-js
pnpm run --dir ./context-js build
```

The package will be available in `context-js/pkg/`

### Ruby Bindings

```bash
cd context-ruby
bundle install
bundle exec rake compile
```

The output will be available in `context-ruby/tmp`

## Run

```bash
cargo build
./target/debug/trunk-analytics-cli upload --org-url-slug=trunk-io --token=${API_TOKEN} --junit-paths=junit.xml
```

You can generate sample junit files by running

```bash
cargo run --bin junit-mock .
```

You can change the API endpoint by setting `TRUNK_PUBLIC_API_ADDRESS=https://api.trunk.io`. To use localhost, you should use `TRUNK_PUBLIC_API_ADDRESS=http://localhost:9010 DEBUG_STRIP_VERSION_PREFIX=true`

## Test

```bash
# Rust tests
cargo test

# Javascript tests
pnpm install --dir ./context-js
pnpm run --dir ./context-js build_and_test

# Python test
pip install maturin uv
trunk run generate-pyi
maturin build --release --out dist --find-interpreter --manifest-path ./context-py/Cargo.toml
cd ./context-py
uv venv
source .venv/bin/activate
uv pip install -r requirements-dev.txt
uv pip install context-py --find-links ../dist --force-reinstall
pytest

# Ruby test
cd ./context-ruby
bundle install
bundle exec rake test
```
