import Testing

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
