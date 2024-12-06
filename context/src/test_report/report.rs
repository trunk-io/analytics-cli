pub struct Report {}

impl Report {
    pub fn new() -> Report {
        Report {}
    }

    pub fn print(&self) {
        println!("Test report");
    }

    pub fn publish(&self) {
        println!("Test report published");
    }

    pub fn save(&self) {
        println!("Test report saved");
    }

    pub fn add_test(&self) {
        println!("Test added");
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

#[cfg(feature = "ruby")]
pub fn ruby_init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    ruby.define_class::<Report>("Report", ruby.class_object())?;
    ruby.defined_method::<Report, _, _>("to_s", magnus::method!(Report::print, 0))?;
    ruby.defined_method::<Report, _, _>("publish", magnus::method!(Report::publish, 0))?;
    ruby.defined_method::<Report, _, _>("save", magnus::method!(Report::save, 0))?;
    ruby.defined_method::<Report, _, _>("add_test", magnus::method!(Report::add_test, 0))?;
    ruby.defined_method::<Report, _, _>(
        "list_quarantined_tests",
        magnus::method!(Report::list_quarantined_tests, 0),
    )?;
    ruby.defined_method::<Report, _, _>("valid_env", magnus::method!(Report::valid_env, 0))?;
    ruby.defined_method::<Report, _, _>("valid_git", magnus::method!(Report::valid_git, 0))?;
    Ok(())
}
