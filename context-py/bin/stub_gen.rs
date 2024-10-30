use pyo3_stub_gen::Result;

// Run 'cargo run --bin stub_gen' to generate Python stubs.
// Then rename to be snake_case (not kebab).

fn main() -> Result<()> {
    // `stub_info` is a function defined by `define_stub_info_gatherer!` macro.
    let stub = context_py::stub_info()?;
    stub.generate()?;
    Ok(())
}
