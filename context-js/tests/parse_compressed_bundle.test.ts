import { describe, expect, it } from "vitest";
import { faker } from "@faker-js/faker";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { compress } from "@mongodb-js/zstd";
import * as tar from "tar";

import {
  parse_meta_from_tarball,
  parse_internal_bin_from_tarball,
  VersionedBundle,
} from "../pkg/context_js";

type RecursiveOmit<T, K extends PropertyKey> = {
  [P in keyof Omit<T, K>]: T[P] extends object ? RecursiveOmit<T[P], K> : T[P];
};

type TestBundleMeta = Omit<RecursiveOmit<VersionedBundle, "free">, "schema">;

const generateBundleMeta = (): TestBundleMeta => ({
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
      resolved_status: null,
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
  },
  upload_time_epoch: faker.number.int(),
  tags: [],
  test_command: faker.hacker.verb(),
});

const bundleMetaJsonSerializer = (_key: unknown, value: unknown) =>
  typeof value === "bigint" ? Number(value) : value;

const compressAndUploadMetaWithInternalBin = async (metaInfoJson: string) => {
  const tmpDir = await fs.mkdtemp(
    path.resolve(os.tmpdir(), "bundle-upload-extract-"),
  );
  const metaInfoFilePath = path.resolve(tmpDir, "meta.json");
  await fs.writeFile(metaInfoFilePath, metaInfoJson);

  const internalBinSourcePath = path.resolve(
    __dirname,
    "../tests/test_internal.bin",
  );
  const internalBinFile = await fs.readFile(internalBinSourcePath);
  const internalBinDestPath = path.resolve(tmpDir, "internal.bin");
  await fs.writeFile(internalBinDestPath, internalBinFile);

  const tarPath = path.resolve(tmpDir, `bundle.tar`);
  await tar.create(
    {
      cwd: tmpDir,
      file: tarPath,
    },
    [path.basename(metaInfoFilePath), path.basename(internalBinDestPath)],
  );

  const tarBuffer = await fs.readFile(tarPath);
  await fs.rm(tmpDir, { recursive: true, force: true });
  return await compress(tarBuffer);
};

describe("context-js", () => {
  type versions = Pick<VersionedBundle, "schema">["schema"];
  const versionTests: [versions, Partial<VersionedBundle>][] = [
    ["V0_5_29", {}],
    [
      "V0_5_34",
      { num_tests: faker.number.int(100), num_files: faker.number.int(100) },
    ],
    [
      "V0_6_2",
      {
        num_tests: faker.number.int(100),
        num_files: faker.number.int(100),
        command_line: "trunk-analytics-cli upload --token=***",
      },
    ],
    [
      "V0_6_3",
      {
        num_tests: faker.number.int(100),
        num_files: faker.number.int(100),
        command_line: "trunk-analytics-cli upload --token=***",
        bundle_upload_id_v2: "SOME ID",
      },
    ],
  ];

  it.each(versionTests)(
    "decompresses and parses meta.json %s",
    async (schema, extras) => {
      expect.hasAssertions();

      const uploadMeta = { ...generateBundleMeta(), ...extras };
      const metaInfoJson = JSON.stringify(
        uploadMeta,
        bundleMetaJsonSerializer,
        2,
      );
      const compressedBuffer =
        await compressAndUploadMetaWithInternalBin(metaInfoJson);

      const readableStream = new ReadableStream({
        start(controller) {
          controller.enqueue(compressedBuffer);
          controller.close();
        },
      });

      const res = await parse_meta_from_tarball(readableStream);
      const expectedMeta = {
        schema,
        ...uploadMeta,
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
        upload_time_epoch: expect.any(Number),
      };
      // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
      expectedMeta.repo.repo_head_commit_epoch = expect.any(Number);

      expect(res).toStrictEqual(expectedMeta);
    },
  );

  it("empty meta.json", async () => {
    expect.hasAssertions();

    const emptyJson = "{}";
    const compressedBuffer =
      await compressAndUploadMetaWithInternalBin(emptyJson);

    const readableStream = new ReadableStream({
      start(controller) {
        controller.enqueue(compressedBuffer);
        controller.close();
      },
    });

    await expect(
      async () => await parse_meta_from_tarball(readableStream),
    ).rejects.toThrow("missing field `version`");
  });

  it("decompresses and parses internal.bin", async () => {
    expect.hasAssertions();

    const compressedBuffer = await compressAndUploadMetaWithInternalBin("{}");

    const readableStream = new ReadableStream({
      start(controller) {
        controller.enqueue(compressedBuffer);
        controller.close();
      },
    });

    const bindingsReports =
      await parse_internal_bin_from_tarball(readableStream);

    expect(bindingsReports).toHaveLength(1);

    const result = bindingsReports.at(0);

    expect(result?.tests).toBe(13);
    expect(result?.test_suites).toHaveLength(2);

    const contextRubySuite = result?.test_suites.find(
      ({ name }) => name === "context_ruby",
    );

    expect(contextRubySuite).toBeDefined();
    expect(contextRubySuite?.test_cases).toHaveLength(5);

    const rspecExpectationsSuite = result?.test_suites.find(
      ({ name }) => name === "RSpec Expectation",
    );

    expect(rspecExpectationsSuite).toBeDefined();
    expect(rspecExpectationsSuite?.test_cases).toHaveLength(8);
  });
});
