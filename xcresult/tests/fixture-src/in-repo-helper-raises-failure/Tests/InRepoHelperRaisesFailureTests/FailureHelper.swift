import Testing

/// The same shape as the dependency helper, but living in the test target itself:
/// the issue is raised at this file, not at the caller's.
func recordIssueFromHelper(_ message: String) {
    Issue.record(
        Comment(rawValue: message),
        sourceLocation: SourceLocation(
            fileID: #fileID,
            filePath: #filePath,
            line: #line,
            column: #column
        )
    )
}
