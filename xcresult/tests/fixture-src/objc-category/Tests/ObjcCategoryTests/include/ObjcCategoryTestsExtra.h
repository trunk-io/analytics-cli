#import <XCTest/XCTest.h>

@interface ObjcCategoryTests : XCTestCase
@end

@interface ObjcCategoryTests (Extra)
- (void)testDeclaredInACategory;
@end
