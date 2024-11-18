import { describe, expect, it } from "vitest";
import { faker } from "@faker-js/faker";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { compress } from "@mongodb-js/zstd";
import * as tar from "tar";

import { BundleMeta, parse_meta_from_tarball } from "../pkg/context_js";

// Based on https://stackoverflow.com/questions/54487137/how-to-recursively-omit-key-from-type
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type OmitDistributive<T, K extends PropertyKey> = T extends any
  ? T extends object
    ? Id<OmitRecursively<T, K>>
    : T
  : never;
type Id<T> = {} & { [P in keyof T]: T[P] };
type OmitRecursively<T, K extends PropertyKey> = Omit<
  { [P in keyof T]: OmitDistributive<T[P], K> },
  K
>;
type TestBundleMeta = OmitRecursively<BundleMeta, "free">;

/* eslint-disable @typescript-eslint/no-empty-function */
const generateBundleMeta = (): TestBundleMeta => ({
  base_props: {
    version: "1",
    bundle_upload_id: faker.string.uuid(),
    cli_version: faker.system.semver(),
    envs: new Map<string, string>([
      ["RUNNER_OS", "Linux"],
      ["GITHUB_REF", "refs/heads/main"],
    ]),
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
const customSerializer = (key: any, value: any) => {
  if (typeof value === "bigint") {
    return Number(value);
  }
  if (typeof value === "object" && value instanceof Map) {
    return Object.fromEntries(value);
  }
  return value;
};

const assertEquality = (obj1: any, obj2: any) => {
  const truthinessChecks = ["codeowners"];
  for (const [key, value] of Object.entries(obj1)) {
    if (
      typeof value === "object" &&
      value !== null &&
      !truthinessChecks.includes(key)
    ) {
      assertEquality(value, obj2[key]);
    } else if (truthinessChecks.includes("codeowners")) {
      expect(obj2[key]).toBeTruthy();
    } else {
      expect(obj2[key]).toStrictEqual(value);
    }
  }
};
/* eslint-enable */

const compressAndUploadMeta = async (
  tmpDir: string,
  metaInfo: TestBundleMeta,
) => {
  const metaInfoJson = JSON.stringify(
    { ...metaInfo.base_props, ...metaInfo.junit_props },
    customSerializer,
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

    // We can't use strict equal because res.meta just stores a wasm ptr
    assertEquality(uploadMeta, res.meta);
  });
});
