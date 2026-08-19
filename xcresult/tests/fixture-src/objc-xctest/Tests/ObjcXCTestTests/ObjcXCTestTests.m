#import "FailureHelper.h"

@interface ObjcXCTestTests : XCTestCase
@end

@implementation ObjcXCTestTests

// Symbolicates as `-[ObjcXCTestTests testFailsInsideSharedHelper]`, which is the
// Objective-C spelling the frame matching has to recognize.
- (void)testFailsInsideSharedHelper {
    [self fixtureFailWithMessage:@"raised from the shared Objective-C helper"];
}

@end
