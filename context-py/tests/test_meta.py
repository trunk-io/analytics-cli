from context_py import gen_info_id, gen_info_id_base


def test_generates_id_properly_for_trunk():
    def generate_id_for_test():
        id = gen_info_id(
            "example_org",
            "example_repo",
            "unix",
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            "trunk:12345",
        )
        base_id = gen_info_id_base(
            "example_org",
            "example_repo",
            "unix",
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            "trunk:12345",
        )
        return (id, base_id)

    result = generate_id_for_test()
    assert result == (
        "db8c5727-0fe9-560f-863f-7f3ee68df425",
        "4392f63c-8dc9-5cec-bbdc-e7b90c2e5a6b",
    )

    # Generate again to ensure it is consistent
    result2 = generate_id_for_test()
    assert result2 == result


def test_works_properly_with_existing_v5_uuid():
    existing_info_id = "a6e84936-3ee9-57d5-b041-ae124896f654"

    def generate_id_for_test(variant: str = ""):
        id = gen_info_id(
            "example_org",
            "example_repo",
            variant,
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            existing_info_id,
        )
        base_id = gen_info_id_base(
            "example_org",
            "example_repo",
            variant,
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            existing_info_id,
        )
        return (id, base_id)

    result = generate_id_for_test()
    assert result[0] == existing_info_id
    assert result[1] == existing_info_id

    # Generate again to ensure it is consistent
    result2 = generate_id_for_test()
    assert result2 == result

    # Adding a variant changes the ID
    result_with_variant = generate_id_for_test(variant="unix")
    assert result_with_variant == (
        "931cae54-0fcd-56eb-8eac-afa833699e53",
        "8057218b-95e4-5373-afbe-c366d4058615",
    )


def test_works_properly_without_existing_v5_uuid():
    def generate_id_for_test(info_id: str | None = None):
        id = gen_info_id(
            "example_org",
            "example_repo",
            "unix",
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            info_id,
        )
        base_id = gen_info_id_base(
            "example_org",
            "example_repo",
            "unix",
            "src/lib.rs",
            "ExampleClass",
            "parent/fact/path",
            "example_name",
            info_id,
        )
        return (id, base_id)

    result = generate_id_for_test()
    assert result == (
        "1bf61475-b542-5faf-aa85-e66a691257a3",
        "c869cb93-66e2-516d-a0ea-15ff4b413c3f",
    )

    # Generate again to ensure it is consistent
    result2 = generate_id_for_test()
    assert result2 == result

    # Existing UUID is ignored if it isn't V5
    result_for_v4_uuid = generate_id_for_test(
        info_id="08e1c642-3a55-45cf-8bf9-b9d0b21785dd"
    )  # V4
    assert result_for_v4_uuid == result


def test_variant_wrapper_doesnt_change_non_variant_case():
    org_url_slug = "example_org"
    repo_full_name = "example_repo"
    file = "src/lib.rs"
    classname = "ExampleClass"
    parent_fact_path = "parent/fact/path"
    name = "example_name"
    info_id = None
    variant = ""

    result = gen_info_id(
        org_url_slug,
        repo_full_name,
        variant,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
    )

    base_result = gen_info_id(
        org_url_slug,
        repo_full_name,
        variant,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
    )

    expected = "06cb6db5-f807-5198-b072-af67a0636f8a"
    assert result == expected
    assert base_result == expected


def test_variant_wrapper_does_change_variant_case():
    org_url_slug = "example_org"
    repo_full_name = "example_repo"
    file = "src/lib.rs"
    classname = "ExampleClass"
    parent_fact_path = "parent/fact/path"
    name = "example_name"
    info_id = None
    variant = "unix"

    result = gen_info_id(
        org_url_slug,
        repo_full_name,
        variant,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
    )

    base_result = gen_info_id(
        org_url_slug,
        repo_full_name,
        "",
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
    )

    expected = "1bf61475-b542-5faf-aa85-e66a691257a3"
    assert result == expected
    assert base_result != expected
