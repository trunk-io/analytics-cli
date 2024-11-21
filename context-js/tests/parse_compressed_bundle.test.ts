import { describe, expect, it } from "vitest";
import { faker } from "@faker-js/faker";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { compress } from "@mongodb-js/zstd";
import * as tar from "tar";

import { BundleMetaV0_5_34, parse_meta_from_tarball } from "../pkg/context_js";

type RecursiveOmit<T, K extends PropertyKey> = {
  [P in keyof Omit<T, K>]: T[P] extends object ? RecursiveOmit<T[P], K> : T[P];
};

type TestBundleMeta = RecursiveOmit<BundleMetaV0_5_34, "free">;

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
  },
  junit_props: {
    num_files: faker.number.int(100),
    num_tests: faker.number.int(100),
  },
});

const bundleMetaJsonSerializer = (_key: unknown, value: unknown) => {
  if (typeof value === "bigint") {
    return Number(value);
  }
  if (value instanceof Map) {
    const obj: unknown = Object.fromEntries(value.entries());
    return obj;
  }
  return value;
};

const compressAndUploadMeta = async (metaInfoJson: string) => {
  const tmpDir = await fs.mkdtemp(
    path.resolve(os.tmpdir(), "bundle-upload-extract-"),
  );
  const metaInfoFilePath = path.resolve(tmpDir, "meta.json");
  await fs.writeFile(metaInfoFilePath, metaInfoJson);
  const tarPath = path.resolve(tmpDir, `bundle.tar`);
  await tar.create(
    {
      cwd: tmpDir,
      file: tarPath,
    },
    [path.basename(metaInfoFilePath)],
  );

  const tarBuffer = await fs.readFile(tarPath);
  await fs.rm(tmpDir, { recursive: true, force: true });
  return await compress(tarBuffer);
};

describe("context-js", () => {
  it("decompresses and parses meta.json", async () => {
    expect.hasAssertions();

    const uploadMeta = generateBundleMeta();
    const metaInfoJson = JSON.stringify(
      { ...uploadMeta.base_props, ...uploadMeta.junit_props },
      bundleMetaJsonSerializer,
      2,
    );
    const compressedBuffer = await compressAndUploadMeta(metaInfoJson);

    const readableStream = new ReadableStream({
      start(controller) {
        controller.enqueue(compressedBuffer);
        controller.close();
      },
    });

    const res = await parse_meta_from_tarball(readableStream);
    // const expectedMeta = {
    //   schema: "V0_5_34",
    //   ...uploadMeta.base_props,
    //   ...uploadMeta.junit_props,
    //   upload_time_epoch: expect.any(Number),
    //   envs: Object.fromEntries(uploadMeta.base_props.envs.entries()),
    //  };
    //  expectedMeta.repo.repo_head_commit_epoch = expect.any(Number);

    expect(res).toStrictEqual(uploadMeta);
  });

  it("empty meta.json", async () => {
    expect.hasAssertions();

    const emptyJson = "{}";
    const compressedBuffer = await compressAndUploadMeta(emptyJson);

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
});
