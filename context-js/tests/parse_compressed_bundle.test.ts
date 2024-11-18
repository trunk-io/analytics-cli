import { describe, expect, it } from "vitest";
import { faker } from "@faker-js/faker";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { compress } from "@mongodb-js/zstd";
import * as tar from "tar";

import {
  BundleMeta,
  parse_meta_from_tarball,
  type BundleMetaBaseProps,
  type BundleMetaJunitProps,
  type BundleRepo,
  type RepoUrlParts,
} from "../pkg/context_js";

type TestRepoUrlParts = Omit<RepoUrlParts, "free">;
type TestBundleRepo = Omit<BundleRepo, "free" | "repo"> & {
  repo: TestRepoUrlParts;
};
type TestBundleBase = Omit<BundleMetaBaseProps, "free" | "repo" | "envs"> & {
  repo: TestBundleRepo;
  envs: Record<string, string>;
};
type TestBundleJunit = Omit<BundleMetaJunitProps, "free">;
type TestBundleMeta = Omit<
  BundleMeta,
  "free" | "base_props" | "junit_props"
> & { base_props: TestBundleBase; junit_props: TestBundleJunit };

/* eslint-disable @typescript-eslint/no-empty-function */
const generateBundleMeta = (): TestBundleMeta => ({
  base_props: {
    version: "1",
    bundle_upload_id: faker.string.uuid(),
    cli_version: faker.system.semver(),
    envs: {
      RUNNER_OS: "Linux",
      GITHUB_REF: "refs/heads/main",
    },
    file_sets: [],
    org: faker.company.name(),
    os_info: process.platform,
    quarantined_tests: [],
    codeowners: {
      path: faker.system.filePath(),
    },
    repo: {
      repo_head_branch: faker.git.branch(),
      repo_head_sha: faker.git.commitSha(),
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
    upload_time_epoch: faker.number.bigInt(),
    tags: [],
    test_command: faker.hacker.verb(),
  },
  junit_props: {
    num_files: faker.number.int(100),
    num_tests: faker.number.int(100),
  },
});

/* eslint-disable */
const bigIntSerializer = (key: any, value: any) =>
  typeof value === "bigint" ? Number(value) : value;
/* eslint-enable */

const compressAndUploadMeta = async (
  tmpDir: string,
  metaInfo: TestBundleMeta,
) => {
  const metaInfoJson = JSON.stringify(
    { ...metaInfo.base_props, ...metaInfo.junit_props },
    bigIntSerializer,
    2,
  );
  const metaInfoFilePath = path.resolve(tmpDir, "meta.json");
  await fs.writeFile(metaInfoFilePath, metaInfoJson);
  const tarPath = path.resolve(
    tmpDir,
    `${metaInfo.base_props.repo.repo_head_sha}.tar`,
  );
  await tar.create(
    {
      cwd: tmpDir,
      file: tarPath,
    },
    [path.basename(metaInfoFilePath)],
  );

  const tarBuffer = await fs.readFile(tarPath);
  return await compress(tarBuffer);
};

describe("context-js", () => {
  it("decompresses and parses meta.json", async () => {
    expect.hasAssertions();

    const tmpDir = await fs.mkdtemp(
      path.resolve(os.tmpdir(), "bundle-upload-extract-"),
    );

    const uploadMeta = generateBundleMeta();
    const compressedBuffer = await compressAndUploadMeta(tmpDir, uploadMeta);

    // Convert compressedBuffer into a stream
    const readableStream = new ReadableStream({
      start(controller) {
        controller.enqueue(compressedBuffer);
        controller.close();
      },
    });

    const res = await parse_meta_from_tarball(readableStream);

    // DONOTLAND: TODO: TYLER FIX THIS ASSERTION
    expect(res.meta as BundleMeta).toContain(uploadMeta);
  });
});
