import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("@ankh/ui exports primitive components", async () => {
  const source = await readFile(new URL("../src/index.ts", import.meta.url), "utf8");

  assert.match(source, /"\.\/components"/);
});
