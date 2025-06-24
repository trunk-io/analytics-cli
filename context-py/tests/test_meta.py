from context_py import gen_info_id


def test_generates_id_properly_for_trunk():
    def generate_id_for_test():
        return gen_info_id(
            "example_org",
            "example_repo",
            "unix",
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            "trunk:12345",
        )

    result = generate_id_for_test()
    assert result == "4392f63c-8dc9-5cec-bbdc-e7b90c2e5a6b"

    # Generate again to ensure it is consistent
    result2 = generate_id_for_test()
    assert result2 == result


def test_works_properly_with_existing_v5_uuid():
    existing_info_id = "a6e84936-3ee9-57d5-b041-ae124896f654"

    def generate_id_for_test(variant: str = ""):
        return gen_info_id(
            "example_org",
            "example_repo",
            variant,
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            existing_info_id,
        )

    result = generate_id_for_test()
    assert result == existing_info_id

    # Generate again to ensure it is consistent
    result2 = generate_id_for_test()
    assert result2 == result

    # Adding a variant changes the ID
    result_with_variant = generate_id_for_test(variant="unix")
    assert result_with_variant == "8057218b-95e4-5373-afbe-c366d4058615"


def test_works_properly_without_existing_v5_uuid():
    def generate_id_for_test(info_id: str | None = None):
        return gen_info_id(
            "example_org",
            "example_repo",
            "unix",
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            info_id,
        )

    result = generate_id_for_test()
    assert result == "c869cb93-66e2-516d-a0ea-15ff4b413c3f"

    # Generate again to ensure it is consistent
    result2 = generate_id_for_test()
    assert result2 == result

    # Existing UUID is ignored if it isn't V5
    result_for_v4_uuid = generate_id_for_test(
        info_id="08e1c642-3a55-45cf-8bf9-b9d0b21785dd"
    )  # V4
    assert result_for_v4_uuid == result
