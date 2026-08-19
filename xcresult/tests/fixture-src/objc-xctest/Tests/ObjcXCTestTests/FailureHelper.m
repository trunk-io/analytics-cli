#import "FailureHelper.h"

@implementation XCTestCase (FixtureFailureHelper)

// `XCTFail` expands with this file's `__FILE__`, so the failure is raised here
// rather than at the call site.
- (void)fixtureFailWithMessage:(NSString *)message {
    XCTFail(@"%@", message);
}

@end
