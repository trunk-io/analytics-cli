import Testing

/// Records a failure against whatever test is running, attributed to *this* file.
///
/// `#filePath` in a function body is the file the function is defined in, so the
/// issue is raised from the dependency's source rather than from the caller's —
/// the same shape as a snapshot-testing assertion helper, a mocking framework, or
/// a page object that reports the failure at its own location.
public func recordIssueFromDependency(_ message: String) {
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
