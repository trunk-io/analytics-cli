import path from "node:path";
import url from "node:url";

import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import vitest from "@vitest/eslint-plugin";

export const createConfig = (
  /** @type string */
  relativePathToContextJs = ".",
) =>
  tseslint.config(
    {
      ignores: [path.join(relativePathToContextJs, "pkg/**/*")],
    },
    eslint.configs.recommended,
    ...tseslint.configs.strictTypeChecked,
    ...tseslint.configs.stylisticTypeChecked,
    {
      languageOptions: {
        parserOptions: {
          projectService: true,
          tsconfigRootDir: path.dirname(url.fileURLToPath(import.meta.url)),
        },
      },
    },
    {
      rules: {
        "no-console": "error",
      },
    },
    {
      files: [path.join(relativePathToContextJs, "tests/**")],
      plugins: { vitest },
      ...vitest.configs.env,
      rules: {
        ...vitest.configs.recommended.rules,
        ...vitest.configs.all.rules,
      },
    },
  );

export default createConfig();
