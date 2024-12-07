use colored::Colorize;
use context::bazel_bep::parser::BazelBepParser;

pub fn print_bep_results(parser: &BazelBepParser) {
    if !parser.errors().is_empty() {
        println!(
            "{} {:?}",
            "Errors parsing BEP file:".yellow(),
            &parser.errors()
        );
    }

    let (test_count, cached_count) = parser.test_counts();
    println!(
        "Parsed {} ({} cached) test results from BEP file",
        test_count, cached_count
    );
}
