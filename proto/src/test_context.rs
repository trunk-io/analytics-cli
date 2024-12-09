// Include the `test_run` module, which is generated from test_context.proto.
// It is important to maintain the same structure as in the proto.
pub mod test_run {
    include!(concat!(env!("OUT_DIR"), "/test_context.test_run.rs"));
}

#[cfg(test)]
mod tests {
    use crate::test_context::test_run::TestCaseRun;
    #[test]
    fn create_test_run() {
        let test_case_run = TestCaseRun::default();
        assert_eq!(test_case_run, TestCaseRun::default());
    }
}
