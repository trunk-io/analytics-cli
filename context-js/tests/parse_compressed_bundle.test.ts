import { describe, expect, it } from "vitest";
import { faker } from "@faker-js/faker";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { compress } from "@mongodb-js/zstd";
import * as tar from "tar";
import dayjs from "dayjs";
import utc from "dayjs/plugin/utc";

import {
  parse_meta_from_tarball,
  parse_internal_bin_from_tarball,
  parse_internal_bin_and_meta_from_tarball,
  VersionedBundle,
  TestRunnerReportStatus,
  FileSet,
  BundleMetaBaseProps,
} from "../pkg/context_js";

// eslint-disable-next-line vitest/require-hook
dayjs.extend(utc);

const RUBY_INTERNAL_BIN = "../tests/test_internal.bin";
const VARIANT_INTERNAL_BIN = "../tests/test_internal_with_variant.bin";

type RecursiveOmit<T, K extends PropertyKey> = T extends unknown[]
  ? RecursiveOmit<T[number], K>[]
  : {
      [P in keyof Omit<T, K>]: T[P] extends object
        ? RecursiveOmit<T[P], K>
        : T[P];
    };

type OmitWasmUtils<T> = RecursiveOmit<T, "free" | SymbolConstructor["dispose"]>;

const generateBundleMeta = () =>
  ({
    version: "1",
    bundle_upload_id: faker.string.uuid(),
    cli_version: faker.system.semver(),
    envs: {
      RUNNER_OS: "Linux",
      GITHUB_REF: "refs/heads/main",
    },
    file_sets: [
      {
        file_set_type: "Junit",
        files: [
          {
            original_path: "/abs/path/junit.xml",
            original_path_rel: "junit.xml",
            path: "0.xml",
            owners: ["owner"],
            team: "team",
          },
        ],
        glob: "**/*.xml",
        resolved_status: "Passed",
        resolved_start_time_epoch_ms: dayjs
          .utc()
          .subtract(5, "minute")
          .valueOf(),
        resolved_end_time_epoch_ms: dayjs.utc().subtract(2, "minute").valueOf(),
        resolved_label: null,
      },
      {
        file_set_type: "Junit",
        files: [
          {
            original_path: "/abs/path/junit.xml",
            original_path_rel: "junit.xml",
            path: "0.xml",
            owners: ["owner"],
            team: "team",
          },
        ],
        glob: "**/*.xml",
        resolved_status: "Passed",
      },
      {
        file_set_type: "Junit",
        files: [
          {
            original_path: "/abs/path/junit.xml",
            original_path_rel: "junit.xml",
            path: "0.xml",
            owners: ["owner"],
            team: "team",
          },
        ],
        glob: "**/*.xml",
        // NOTE: This is intentional to test backwards compatibility with old bundles
        resolved_status: null as unknown as TestRunnerReportStatus,
      },
      {
        file_set_type: "Junit",
        files: [
          {
            original_path: "/abs/path/junit.xml",
            original_path_rel: "junit.xml",
            path: "0.xml",
            owners: ["owner"],
            team: "team",
          },
        ],
        glob: "**/*.xml",
      },
    ],
    org: faker.company.name(),
    os_info: process.platform,
    quarantined_tests: [],
    codeowners: {
      path: faker.system.filePath(),
    },
    repo: {
      repo_head_branch: faker.git.branch(),
      repo_head_sha: faker.git.commitSha(),
      repo_head_sha_short: faker.git.commitSha().slice(0, 7),
      repo_head_author_email: faker.internet.email(),
      repo_head_author_name: faker.person.fullName(),
      repo_head_commit_message: faker.lorem.sentence(),
      repo_head_commit_epoch: faker.number.bigInt(),
      repo_root: faker.system.directoryPath(),
      repo_url: faker.internet.url(),
      repo: {
        host: "github.com",
        owner: faker.company.name(),
        name: faker.company.catchPhraseNoun(),
      },
      use_uncloned_repo: undefined,
    },
    use_uncloned_repo: null,
    upload_time_epoch: faker.number.int(),
    tags: [],
    // NOTE: This is intentional to test backwards compatibility with old bundles
    test_collection_short_id: null as unknown as string,
    test_command: faker.hacker.verb(),
  }) as const satisfies OmitWasmUtils<BundleMetaBaseProps>;

const bundleMetaJsonSerializer = (_key: unknown, value: unknown) =>
  typeof value === "bigint" ? Number(value) : value;

const compressAndUploadMeta = async ({
  metaInfoJson,
  includeInternalBin,
}: {
  metaInfoJson: string;
  includeInternalBin?: string;
}): Promise<ReadableStream> => {
  const tmpDir = await fs.mkdtemp(
    path.resolve(os.tmpdir(), "bundle-upload-extract-"),
  );
  const metaInfoFilePath = path.resolve(tmpDir, "meta.json");
  await fs.writeFile(metaInfoFilePath, metaInfoJson);

  const fileList: string[] = [path.basename(metaInfoFilePath)];
  if (includeInternalBin) {
    const internalBinSourcePath = path.resolve(__dirname, includeInternalBin);
    const internalBinFile = await fs.readFile(internalBinSourcePath);
    const internalBinDestPath = path.resolve(tmpDir, "internal.bin");
    await fs.writeFile(internalBinDestPath, internalBinFile);
    fileList.push(path.basename(internalBinDestPath));
  }

  const tarPath = path.resolve(tmpDir, `bundle.tar`);
  await tar.create(
    {
      cwd: tmpDir,
      file: tarPath,
    },
    fileList,
  );

  const tarBuffer = await fs.readFile(tarPath);
  await fs.rm(tmpDir, { recursive: true, force: true });
  const compressedBuffer = await compress(tarBuffer);

  const readableStream = new ReadableStream({
    start(controller) {
      controller.enqueue(compressedBuffer);
      controller.close();
    },
  });

  return readableStream;
};

const VERSION_TESTS = [
  { schema: "V0_5_29", ...generateBundleMeta() },
  {
    schema: "V0_5_34",
    ...generateBundleMeta(),
    num_tests: faker.number.int(100),
    num_files: faker.number.int(100),
  },
  {
    schema: "V0_6_2",
    ...generateBundleMeta(),
    num_tests: faker.number.int(100),
    num_files: faker.number.int(100),
    command_line: "trunk-analytics-cli upload --token=***",
    trunk_envs: {},
  },
  {
    schema: "V0_7_7",
    ...generateBundleMeta(),
    num_tests: faker.number.int(100),
    num_files: faker.number.int(100),
    command_line: "trunk-analytics-cli upload --token=***",
    bundle_upload_id_v2: "SOME ID",
    variant: null,
    internal_bundled_file: null,
    trunk_envs: {},
  },
  {
    schema: "V0_7_8",
    ...generateBundleMeta(),
    num_tests: faker.number.int(100),
    num_files: faker.number.int(100),
    command_line: "trunk-analytics-cli upload --token=***",
    bundle_upload_id_v2: "SOME ID",
    variant: null,
    internal_bundled_file: null,
    failed_tests: [],
    trunk_envs: {},
  },
] as const satisfies VersionedBundle[];

const createExpectedVersionedBundle = (
  bundleMeta: OmitWasmUtils<BundleMetaBaseProps>,
) => {
  const { repo: bundleMetaRepo, ...restBundleMeta } = bundleMeta;
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const { use_uncloned_repo: _, ...restBundleMetaRepo } = bundleMetaRepo;
  const expectedMeta = {
    ...restBundleMeta,
    file_sets: restBundleMeta.file_sets.map(
      (fileSet): OmitWasmUtils<FileSet> => {
        if (!("resolved_status" in fileSet)) {
          return fileSet;
        }

        // NOTE: This is intentional to test backwards compatibility with old bundles
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
        if (!fileSet.resolved_status) {
          // eslint-disable-next-line @typescript-eslint/no-unused-vars
          const { resolved_status: _, ...restFileSet } = fileSet;
          return restFileSet;
        }

        return {
          ...fileSet,
          ...(typeof fileSet.resolved_start_time_epoch_ms === "undefined"
            ? {
                resolved_start_time_epoch_ms: 0,
              }
            : {}),
          ...(typeof fileSet.resolved_end_time_epoch_ms === "undefined"
            ? {
                resolved_end_time_epoch_ms: 0,
              }
            : {}),
          ...(typeof fileSet.resolved_label === "undefined"
            ? {
                resolved_label: null,
              }
            : {}),
        };
      },
    ),
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    upload_time_epoch: expect.any(Number),
    repo: {
      ...restBundleMetaRepo,
      // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
      repo_head_commit_epoch: expect.any(Number),
    },
  };

  // NOTE: This is intentional to test backwards compatibility with old bundles
  if (
    "test_collection_short_id" in expectedMeta &&
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
    expectedMeta.test_collection_short_id === null
  ) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { test_collection_short_id: __, ...restExpectedMeta } = expectedMeta;
    return restExpectedMeta;
  }

  return expectedMeta;
};

describe("context-js", () => {
  it.each(VERSION_TESTS)(
    "decompresses and parses meta.json %s",
    async (versionedBundle) => {
      expect.hasAssertions();

      const metaInfoJson = JSON.stringify(
        versionedBundle,
        bundleMetaJsonSerializer,
        2,
      );
      const readableStream = await compressAndUploadMeta({
        metaInfoJson,
        includeInternalBin: RUBY_INTERNAL_BIN,
      });

      const res = await parse_meta_from_tarball(readableStream);
      const expectedMeta = createExpectedVersionedBundle(versionedBundle);

      expect(res).toStrictEqual(expectedMeta);
    },
  );

  it("empty meta.json", async () => {
    expect.hasAssertions();

    const readableStream = await compressAndUploadMeta({
      metaInfoJson: "{}",
      includeInternalBin: RUBY_INTERNAL_BIN,
    });

    await expect(parse_meta_from_tarball(readableStream)).rejects.toThrowError(
      "missing field `version`",
    );
  });

  it("decompresses and parses internal.bin", async () => {
    expect.hasAssertions();

    const uploadMeta = generateBundleMeta();
    const metaInfoJson = JSON.stringify(
      uploadMeta,
      bundleMetaJsonSerializer,
      2,
    );
    const readableStream = await compressAndUploadMeta({
      metaInfoJson,
      includeInternalBin: RUBY_INTERNAL_BIN,
    });

    const bindingsReports =
      await parse_internal_bin_from_tarball(readableStream);

    expect(bindingsReports).toHaveLength(1);

    const result = bindingsReports.at(0);

    expect(result?.tests).toBe(13);
    expect(result?.test_suites).toHaveLength(2);
    expect(result?.variant).toBe("");

    const contextRubySuite = result?.test_suites.find(
      ({ name }) => name === "context_ruby",
    );

    expect(contextRubySuite).toBeDefined();
    expect(contextRubySuite?.test_cases).toHaveLength(5);

    const rspecExpectationsSuite = result?.test_suites.find(
      ({ name }) => name === "RSpec Expectations",
    );

    expect(rspecExpectationsSuite).toBeDefined();
    expect(rspecExpectationsSuite?.test_cases).toHaveLength(8);
  });

  it("decompresses and parses both meta.json and internal.bin", async () => {
    expect.hasAssertions();

    const versionedBundle = {
      schema: "V0_7_7",
      ...generateBundleMeta(),
      num_tests: faker.number.int(100),
      num_files: faker.number.int(100),
      command_line: "trunk-analytics-cli upload --token=***",
      bundle_upload_id_v2: "SOME ID",
      variant: "some-variant",
      internal_bundled_file: null,
      trunk_envs: {},
    } as const satisfies VersionedBundle;
    const metaInfoJson = JSON.stringify(
      versionedBundle,
      bundleMetaJsonSerializer,
      2,
    );
    const readableStream = await compressAndUploadMeta({
      metaInfoJson,
      includeInternalBin: RUBY_INTERNAL_BIN,
    });

    const { bindings_report, versioned_bundle } =
      await parse_internal_bin_and_meta_from_tarball(readableStream);

    const expectedMeta = createExpectedVersionedBundle(versionedBundle);

    expect(versioned_bundle).toStrictEqual(expectedMeta);

    expect(bindings_report).toHaveLength(1);

    const result = bindings_report.at(0);

    expect(result?.tests).toBe(13);
    expect(result?.test_suites).toHaveLength(2);
    expect(result?.variant).toBe("");

    const contextRubySuite = result?.test_suites.find(
      ({ name }) => name === "context_ruby",
    );

    expect(contextRubySuite).toBeDefined();
    expect(contextRubySuite?.test_cases).toHaveLength(5);

    const rspecExpectationsSuite = result?.test_suites.find(
      ({ name }) => name === "RSpec Expectations",
    );

    expect(rspecExpectationsSuite).toBeDefined();
    expect(rspecExpectationsSuite?.test_cases).toHaveLength(8);
  });

  it("throws an error if internal bundle or meta.json is missing when expecting both", async () => {
    expect.hasAssertions();

    const uploadMeta = {
      ...generateBundleMeta(),
      ...{
        num_tests: faker.number.int(100),
        num_files: faker.number.int(100),
        command_line: "trunk-analytics-cli upload --token=***",
        bundle_upload_id_v2: "SOME ID",
      },
    };
    const metaInfoJson = JSON.stringify(
      uploadMeta,
      bundleMetaJsonSerializer,
      2,
    );

    const readableStream = await compressAndUploadMeta({
      metaInfoJson,
    });

    await expect(
      parse_internal_bin_and_meta_from_tarball(readableStream),
    ).rejects.toThrowError("No internal.bin file found in the tarball");
  });

  it("correctly gets and sets variant", async () => {
    expect.hasAssertions();

    const uploadMeta = generateBundleMeta();
    const metaInfoJson = JSON.stringify(
      uploadMeta,
      bundleMetaJsonSerializer,
      2,
    );
    const readableStream = await compressAndUploadMeta({
      metaInfoJson,
      includeInternalBin: VARIANT_INTERNAL_BIN,
    });

    const bindingsReports =
      await parse_internal_bin_from_tarball(readableStream);

    expect(bindingsReports).toHaveLength(1);

    const result = bindingsReports.at(0);

    expect(result?.variant).toBe("test-variant");
  });
});
