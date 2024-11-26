# context-py

## Setup

Setup virtual environment

```bash
uv venv
```

Install dependencies

```bash
uv pip install -r ./requirements-dev.txt
```

Build type stubs

```bash
cargo run
```

Build bindings and install them into the virtual environment

```bash
maturin dev
```

## Developing / Testing

Edit Rust code, then rerun the commands for building type stubs and bindings

```bash
cargo run
maturin dev
```

To run tests, run the following command

```bash
pytest
```
