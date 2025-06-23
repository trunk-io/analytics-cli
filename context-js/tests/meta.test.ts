import { describe, expect, it } from "vitest";
import { gen_info_id } from "../pkg/context_js";

describe("context-js", () => {
  // Tese tests match the tests in context/src/meta/id.rs.
  // While they don't need to match, it proves both the bindings and
  // rust code are generating the same IDs.
  describe("gen_info_id", () => {
    it("generates ID properly for trunk", () => {
      expect.hasAssertions();

      const generateIdForTest = () =>
        gen_info_id(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          "trunk:12345",
          "unix",
        );

      const result = generateIdForTest();

      expect(result).toBe("4392f63c-8dc9-5cec-bbdc-e7b90c2e5a6b");

      // Generate again to ensure it is consistent
      const result2 = generateIdForTest();

      expect(result2).toBe(result);
    });

    it("works properly with existing v5 UUID", () => {
      expect.hasAssertions();

      const existingInfoId = "a6e84936-3ee9-57d5-b041-ae124896f654";
      const generateIdForTest = ({ variant = "" }: { variant?: string }) =>
        gen_info_id(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          existingInfoId,
          variant,
        );

      const result = generateIdForTest({});

      expect(result).toBe(existingInfoId);

      // Generate again to ensure it is consistent
      const result2 = generateIdForTest({});

      expect(result2).toBe(result);

      // Adding a variant changs the ID.
      const resultWithVariant = generateIdForTest({ variant: "unix" });

      expect(resultWithVariant).toBe("8057218b-95e4-5373-afbe-c366d4058615");
    });

    it("works properly without existing v5 UUID", () => {
      expect.hasAssertions();

      const generateIdForTest = ({ infoId }: { infoId?: string }) =>
        gen_info_id(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          infoId,
          "unix",
        );

      const result = generateIdForTest({});

      expect(result).toBe("c869cb93-66e2-516d-a0ea-15ff4b413c3f");

      // Generate again to ensure it is consistent
      const result2 = generateIdForTest({});

      expect(result2).toBe(result);

      // Existing UUID is ignored if it isn't V5
      const resultForV4Uuid = generateIdForTest({
        infoId: "08e1c642-3a55-45cf-8bf9-b9d0b21785dd", // V4
      });

      expect(resultForV4Uuid).toBe(result);
    });
  });
});
