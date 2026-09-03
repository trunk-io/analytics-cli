#import <XCTest/XCTest.h>

#import "include/ObjcCategoryTestsExtra.h"

// The include is spelled relative to this file rather than relying on the `-I include`
// the build passes: `documentSymbol` is answered with no build settings at all, and a
// header clangd cannot open leaves the container named `<<error-type>>(Extra)` — the
// class name is simply gone, and the declaration can never be matched.
//
// A category adds the method to `ObjcCategoryTests`, so the run reports it as
// `ObjcCategoryTests/testDeclaredInACategory` while it is declared here.
// `documentSymbol` names the container `ObjcCategoryTests(Extra)`, which has to be read
// back as the class it extends for the declaration to be found at all.
@implementation ObjcCategoryTests (Extra)

- (void)testDeclaredInACategory {
    XCTFail(@"declared in an Objective-C category, not in the class's own file");
}

@end
