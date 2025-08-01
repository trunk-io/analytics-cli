def test_parse_codeowners_from_bytes_basic():
    from context_py import codeowners_parse

    codeowners_text = b"""* @trunk/test"""
    codeowners = codeowners_parse(codeowners_text)

    assert codeowners is not None

    gitlab_owners = codeowners.get_gitlab_owners()
    assert gitlab_owners is not None
    assert gitlab_owners.of("*.py") == ["@trunk/test"]


def test_parse_codeowners_from_bytes_gitlab_sections():
    from context_py import codeowners_parse

    codeowners_text = b"""
        [Documentation]
        ee/docs    @gl-docs
        docs       @gl-docs

        [Database]
        README.md  @gl-database
        model/db   @gl-database

        [dOcUmEnTaTiOn]
        README.md  @gl-docs

        [Two Words]
        README.md  @gl-database
        model/db   @gl-database

        [Double::Colon]
        README.md  @gl-database
        model/db   @gl-database

        [DefaultOwners] @config-owner @gl-docs
        README.md
        model/db

        [OverriddenOwners] @config-owner
        README.md  @gl-docs
        model/db   @gl-docs
    """
    codeowners = codeowners_parse(codeowners_text)

    assert codeowners is not None

    gitlab_owners = codeowners.get_gitlab_owners()
    assert gitlab_owners is not None
    assert gitlab_owners.of("README.md") == [
        "@gl-docs",
        "@gl-database",
        "@gl-database",
        "@gl-database",
        "@config-owner",
        "@gl-docs",
        "@gl-docs",
    ]

    codeowners_text = b"""
        # This is an example of a CODEOWNERS file.
        # Lines that start with `#` are ignored.

        # app/ @commented-rule

        # Specify a default Code Owner by using a wildcard:
        * @default-codeowner

        # Specify multiple Code Owners by using a tab or space:
        * @multiple @code @owners

        # Rules defined later in the file take precedence over the rules
        # defined before.
        # For example, for all files with a filename ending in `.rb`:
        *.rb @ruby-owner

        # Specify multiple Code Owners separated by spaces or tabs.
        # In the following case the CODEOWNERS file from the root of the repo
        # has 3 Code Owners (@multiple @code @owners):
        CODEOWNERS @multiple @code @owners

        # You can use both usernames or email addresses to match
        # users. Everything else is ignored. For example, this code
        # specifies the `@legal` and a user with email `janedoe@gitlab.com` as the
        # owner for the LICENSE file:
        LICENSE @legal this_does_not_match janedoe@gitlab.com

        # Use group names to match groups, and nested groups to specify
        # them as owners for a file:
        README @group @group/with-nested/subgroup

        # End a path in a `/` to specify the Code Owners for every file
        # nested in that directory, on any level:
        /docs/ @all-docs

        # End a path in `/*` to specify Code Owners for every file in
        # a directory, but not nested deeper. This code matches
        # `docs/index.md` but not `docs/projects/index.md`:
        /docs/* @root-docs

        # Include `/**` to specify Code Owners for all subdirectories
        # in a directory. This rule matches `docs/projects/index.md` or
        # `docs/development/index.md`
        /docs/**/*.md @root-docs

        # This code makes matches a `lib` directory nested anywhere in the repository:
        lib/ @lib-owner

        # This code match only a `config` directory in the root of the repository:
        /config/ @config-owner

        # Code Owners section:
        [Documentation]
        ee/docs    @docs
        docs       @docs

        # Use of default owners for a section. In this case, all files (*) are owned by the dev team except the README.md and data-models which are owned by other teams.
        [Development] @dev-team
        *
        README.md @docs-team
        data-models/ @data-science-team

        # This section is combined with the previously defined [Documentation] section:
        [DOCUMENTATION]
        README.md  @docs
    """
    codeowners = codeowners_parse(codeowners_text)

    assert codeowners is not None

    gitlab_owners = codeowners.get_gitlab_owners()
    assert gitlab_owners is not None
    assert gitlab_owners.of("README.md") == [
        "@code",
        "@multiple",
        "@owners",
        "@docs",
        "@docs-team",
    ]
    assert gitlab_owners.of("foo.go") == [
        "@code",
        "@multiple",
        "@owners",
        "@dev-team",
    ]
    assert gitlab_owners.of("foo.rb") == [
        "@ruby-owner",
        "@dev-team",
    ]


def test_parse_and_associate_multithreaded():
    from context_py import (
        make_codeowners_file,  # trunk-ignore(pyright/reportUnknownVariableType)
    )
    from context_py import (
        CodeOwnersFile,
        associate_codeowners_n_threads,
        parse_many_codeowners_n_threads,
    )

    def make_codeowners_bytes(i: int) -> CodeOwnersFile:
        return make_codeowners_file(f"{i}.txt @user{i}".encode())

    num_codeowners_files = 100
    num_files_to_associate_owners = 1000
    num_threads = 4

    codeowners_files = [
        make_codeowners_bytes(i) for i in range(0, num_codeowners_files)
    ]
    to_associate = [
        (
            f"{i % num_codeowners_files}",
            f"{i % num_codeowners_files if i % 2 == 0 else 'foo'}.txt",
        )
        for i in range(0, num_files_to_associate_owners)
    ]

    parsed_codeowners = parse_many_codeowners_n_threads(codeowners_files, num_threads)
    codeowners_matchers = {
        f"{i}": codeowners_matcher
        for i, codeowners_matcher in enumerate(parsed_codeowners)
    }
    owners = associate_codeowners_n_threads(
        codeowners_matchers, to_associate, num_threads
    )

    assert len(owners) == num_files_to_associate_owners

    for i in range(0, num_files_to_associate_owners):
        if i % 2 == 0:
            assert len(owners[i]) == 1
            assert owners[i][0] == f"@user{i % num_codeowners_files}"
        else:
            assert len(owners[i]) == 0
