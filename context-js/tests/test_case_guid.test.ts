import { describe, expect, it } from "vitest";
import { gen_test_case_guid } from "../pkg/context_js";

// The frozen gen_test_case_guid contract, pinned identically in context/src/meta/id.rs.
describe("gen_test_case_guid contract", () => {
  const COLLECTION_ID = "018f6d3a-6f2e-4c4a-9b1e-2f3a4b5c6d7e";
  const REPO_ID = "7a1f0e3d-2b4c-4d5e-8f90-123456789abc";
  const TEST_CASE_ID = "88e5353c-190c-5dce-9d06-0e66c3e062b1";
  const NIL_UUID = "00000000-0000-0000-0000-000000000000";

  it("matches the golden vector with a repo", () => {
    expect.hasAssertions();

    const result = gen_test_case_guid(COLLECTION_ID, REPO_ID, TEST_CASE_ID);

    expect(result).toBe("bfeebcf4-72d1-887d-8bcd-788d0dec7f97");

    // Generate again to ensure it is consistent
    expect(gen_test_case_guid(COLLECTION_ID, REPO_ID, TEST_CASE_ID)).toBe(
      result,
    );
  });

  // --no-repo stores the nil repo UUID, collapsing to one guid per collection.
  it("matches the golden vector with the nil repo id", () => {
    expect.hasAssertions();

    expect(gen_test_case_guid(COLLECTION_ID, NIL_UUID, TEST_CASE_ID)).toBe(
      "943a80af-66b0-84bb-ad01-56b3b72fe363",
    );
  });

  it("normalizes uppercase input to the same guid", () => {
    expect.hasAssertions();

    expect(
      gen_test_case_guid(
        COLLECTION_ID.toUpperCase(),
        REPO_ID.toUpperCase(),
        TEST_CASE_ID.toUpperCase(),
      ),
    ).toBe("bfeebcf4-72d1-887d-8bcd-788d0dec7f97");
  });

  it("is stamped as a v8 UUID", () => {
    expect.hasAssertions();

    const result = gen_test_case_guid(COLLECTION_ID, REPO_ID, TEST_CASE_ID);

    // Version nibble, then the variant nibble consumers validate on.
    expect(result[14]).toBe("8");
    expect(["8", "9", "a", "b"]).toContain(result[19]);
  });

  // Hashing a malformed id would mint an id that resolves to nothing.
  it("throws on a malformed uuid instead of hashing it", () => {
    expect.hasAssertions();

    expect(() =>
      gen_test_case_guid(COLLECTION_ID, "not-a-uuid", TEST_CASE_ID),
    ).toThrowError(/invalid repo_id/);
  });
});
