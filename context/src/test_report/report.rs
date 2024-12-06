#[cfg(feature = "ruby")]
use magnus::{Module, Object};
#[cfg(feature = "pyo3")]
use pyo3::pyclass;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::gen_stub_pyclass;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[cfg_attr(feature = "ruby", magnus::wrap(class = "Test", free_immediately, size))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub name: String,
    pub status: String,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[cfg_attr(
    feature = "ruby",
    magnus::wrap(class = "TestReport", free_immediately, size)
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestReport {
    tests: Vec<Test>,
}

impl TestReport {
    pub fn new() -> TestReport {
        TestReport { tests: Vec::new() }
    }

    pub fn publish(&self) {
        println!("Test test_report published");
    }

    pub fn save(&self) {
        println!("Test test_report saved");
    }

    pub fn add_test(&mut self, test: Test) {
        println!("Test added");
        self.tests.push(test);
    }

    pub fn list_quarantined_tests(&self) {
        println!("List quarantined");
    }

    pub fn valid_env(&self) {
        println!("Valid env");
    }

    pub fn valid_git(&self) {
        println!("Valid git");
    }
}

impl Into<&str> for TestReport {
    fn into(self) -> &'static str {
        "Test Report"
    }
}

impl ToString for TestReport {
    fn to_string(&self) -> String {
        String::from(Into::<&str>::into(<TestReport as Clone>::clone(&self)))
    }
}

#[cfg(feature = "ruby")]
impl TestReport {
    pub fn to_string(&self) -> &str {
        self.clone().into()
    }
}

#[cfg(feature = "ruby")]
pub fn ruby_init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    let test_report = ruby.define_class("TestReport", ruby.class_object())?;
    test_report.define_singleton_method("new", magnus::function!(TestReport::new, 0))?;
    test_report.define_method("to_s", magnus::method!(TestReport::to_string, 0))?;
    test_report.define_method("publish", magnus::method!(TestReport::publish, 0))?;
    test_report.define_method("save", magnus::method!(TestReport::save, 0))?;
    test_report.define_method("add_test", magnus::method!(TestReport::add_test, 1))?;
    test_report.define_method(
        "list_quarantined_tests",
        magnus::method!(TestReport::list_quarantined_tests, 0),
    )?;
    test_report.define_method("valid_env", magnus::method!(TestReport::valid_env, 0))?;
    test_report.define_method("valid_git", magnus::method!(TestReport::valid_git, 0))?;
    Ok(())
}
