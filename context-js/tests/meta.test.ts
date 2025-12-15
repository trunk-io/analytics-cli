import { describe, expect, it } from "vitest";
import { gen_info_id, gen_info_id_base } from "../pkg/context_js";

describe("context-js", () => {
  // These tests match the tests in context/src/meta/id.rs.
  // While they don't need to match, it proves both the bindings and
  // rust code are generating the same IDs.
  describe("gen_info_id", () => {
    it("generates ID properly for trunk", () => {
      expect.hasAssertions();

      const generateIdForTest = () => ({
        id: gen_info_id(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          "trunk:12345",
          "unix",
        ),
        base_id: gen_info_id_base(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          "trunk:12345",
          "unix",
        ),
      });

      const result = generateIdForTest();

      expect(result.base_id).toBe("4392f63c-8dc9-5cec-bbdc-e7b90c2e5a6b");
      expect(result.id).toBe("db8c5727-0fe9-560f-863f-7f3ee68df425");

      // Generate again to ensure it is consistent
      const result2 = generateIdForTest();

      expect(result2).toStrictEqual(result);
    });

    it("works properly with existing v5 UUID", () => {
      expect.hasAssertions();

      const existingInfoId = "a6e84936-3ee9-57d5-b041-ae124896f654";
      const generateIdForTest = ({ variant = "" }: { variant?: string }) => ({
        id: gen_info_id(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          existingInfoId,
          variant,
        ),
        base_id: gen_info_id_base(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          existingInfoId,
          variant,
        ),
      });

      const result = generateIdForTest({});

      expect(result.id).toBe(existingInfoId);
      expect(result.base_id).toBe(existingInfoId);

      // Generate again to ensure it is consistent
      const result2 = generateIdForTest({});

      expect(result2).toStrictEqual(result);

      // Adding a variant changs the ID.
      const resultWithVariant = generateIdForTest({ variant: "unix" });

      expect(resultWithVariant.id).toBe("931cae54-0fcd-56eb-8eac-afa833699e53");
      expect(resultWithVariant.base_id).toBe(
        "8057218b-95e4-5373-afbe-c366d4058615",
      );
    });

    it("works properly without existing v5 UUID", () => {
      expect.hasAssertions();

      const generateIdForTest = ({ infoId }: { infoId?: string }) => ({
        id: gen_info_id(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          infoId,
          "unix",
        ),
        base_id: gen_info_id_base(
          "example_org",
          "example_repo",
          "src/lib.rs",
          "ExampleClass",
          "parent/fact/path",
          "example_name",
          infoId,
          "unix",
        ),
      });

      const result = generateIdForTest({});

      expect(result.base_id).toBe("c869cb93-66e2-516d-a0ea-15ff4b413c3f");
      expect(result.id).toBe("1bf61475-b542-5faf-aa85-e66a691257a3");

      // Generate again to ensure it is consistent
      const result2 = generateIdForTest({});

      expect(result2).toStrictEqual(result);

      // Existing UUID is ignored if it isn't V5
      const resultForV4Uuid = generateIdForTest({
        infoId: "08e1c642-3a55-45cf-8bf9-b9d0b21785dd", // V4
      });

      expect(resultForV4Uuid).toStrictEqual(result);
    });

    it("doesn't change non-variant case", () => {
      expect.hasAssertions();

      const org_url_slug = "example_org";
      const repo_full_name = "example_repo";
      const file = "src/lib.rs";
      const classname = "ExampleClass";
      const parent_fact_path = "parent/fact/path";
      const name = "example_name";
      const info_id = null;
      const variant = "";

      const result = gen_info_id(
        org_url_slug,
        repo_full_name,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
        variant,
      );

      const base_result = gen_info_id(
        org_url_slug,
        repo_full_name,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
        variant,
      );

      const expected = "06cb6db5-f807-5198-b072-af67a0636f8a";

      expect(result).toBe(expected);
      expect(base_result).toBe(expected);
    });

    it("does change variant case", () => {
      expect.hasAssertions();

      const org_url_slug = "example_org";
      const repo_full_name = "example_repo";
      const file = "src/lib.rs";
      const classname = "ExampleClass";
      const parent_fact_path = "parent/fact/path";
      const name = "example_name";
      const info_id = null;
      const variant = "unix";

      const result = gen_info_id(
        org_url_slug,
        repo_full_name,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
        variant,
      );

      const base_result = gen_info_id(
        org_url_slug,
        repo_full_name,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
        "",
      );

      const expected = "1bf61475-b542-5faf-aa85-e66a691257a3";

      expect(result).toBe(expected);
      expect(base_result).not.toBe(expected);
    });
  });
});
