use context::bazel_bep::common::BepParseResult;

pub fn print_bep_results(bep_result: &BepParseResult) {
    if !bep_result.errors.is_empty() {
        tracing::warn!("Errors parsing BEP file: {:?}", &bep_result.errors);
    }

    let (xml_count, cached_xml_count) = bep_result.xml_file_counts();
    tracing::info!(
        "Parsed {} ({} cached) test results from BEP file",
        xml_count,
        cached_xml_count
    );
}
