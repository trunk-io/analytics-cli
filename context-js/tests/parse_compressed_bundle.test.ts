import { describe, expect, it } from "vitest";
import { faker } from "@faker-js/faker";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { compress } from "@mongodb-js/zstd";
import * as tar from "tar";

import {
  parse_meta_from_tarball,
  type BundleMeta,
  type BundleRepo,
  type RepoUrlParts,
} from "../pkg/context_js";

type TestRepoUrlParts = Omit<RepoUrlParts, "free">;
type TestBundleRepo = Omit<
  BundleRepo,
  "free" | "repo_head_commit_epoch" | "repo"
> & { repo: TestRepoUrlParts };
type TestBundleMeta = Omit<
  BundleMeta,
  "free" | "upload_time_epoch" | "repo"
> & { repo: TestBundleRepo };

/* eslint-disable @typescript-eslint/no-empty-function */
const generateBundleMeta = (
  overrides?: Partial<TestBundleMeta>,
): TestBundleMeta => ({
  version: faker.system.semver(),
  bundle_upload_id: faker.string.uuid(),
  cli_version: faker.system.semver(),
  // codeowners: {}},
  envs: new Map(),
  file_sets: [],
  num_files: faker.number.int(100),
  num_tests: faker.number.int(100),
  org: faker.company.name(),
  os_info: process.platform,
  quarantined_tests: [],
  repo: {
    repo_head_branch: faker.git.branch(),
    repo_head_sha: faker.git.commitSha(),
    repo_head_author_email: faker.internet.email(),
    repo_head_author_name: faker.person.fullName(),
    repo_head_commit_message: faker.lorem.sentence(),
    repo_head_sha_short: faker.git.commitSha().slice(0, 7),
    repo_root: faker.system.directoryPath(),
    repo_url: faker.internet.url(),
    repo: {
      host: "github.com",
      owner: faker.company.name(),
      name: faker.company.catchPhraseNoun(),
    },
  },
  tags: [],
  test_command: faker.hacker.verb(),
  ...overrides,
});

const compressAndUploadMeta = async (
  tmpDir: string,
  metaInfo: TestBundleMeta,
) => {
  const metaInfoJson = JSON.stringify(metaInfo, null, 2);
  const metaInfoFilePath = path.resolve(tmpDir, "meta.json");
  await fs.writeFile(metaInfoFilePath, metaInfoJson);
  const tarPath = path.resolve(tmpDir, `${metaInfo.repo.repo_head_sha}.tar`);
  await tar.create(
    {
      cwd: tmpDir,
      file: tarPath,
    },
    [path.basename(metaInfoFilePath)],
  );

  console.log("tar path is ", tarPath);
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

    expect(res).toStrictEqual(uploadMeta);
  });
});
