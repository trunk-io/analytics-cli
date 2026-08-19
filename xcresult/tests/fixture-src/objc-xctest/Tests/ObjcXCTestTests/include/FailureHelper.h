#import <XCTest/XCTest.h>

@interface XCTestCase (FixtureFailureHelper)
- (void)fixtureFailWithMessage:(NSString *)message;
@end
