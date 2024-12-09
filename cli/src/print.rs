use context::bazel_bep::parser::BazelBepParser;

pub fn print_bep_results(parser: &BazelBepParser) {
    if !parser.errors().is_empty() {
        log::warn!("Errors parsing BEP file: {:?}", &parser.errors());
    }

    let (test_count, cached_count) = parser.test_counts();
    log::info!(
        "Parsed {} ({} cached) test results from BEP file",
        test_count,
        cached_count
    );
}
