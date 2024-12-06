// NOTE: This lint isn't applicable since we compile with nightly
/* trunk-ignore(clippy/E0554) */
#![feature(round_char_boundary)]

// Include the `test_run` module, which is generated from test_context.proto.
// It is important to maintain the same structure as in the proto.
pub mod test_context {
    pub mod test_run {
        include!(concat!(env!("OUT_DIR"), "/test_context.test_run.rs"));
    }
}

use test_context::test_run;

pub fn create_test_run() -> test_run::TestCaseRun {
    let test_case_run = test_run::TestCaseRun::default();
    test_case_run
}
