import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("@ankh/types exports generated declaration source", async () => {
  const source = await readFile(new URL("../src/index.ts", import.meta.url), "utf8");
  const generated = await readFile(new URL("../src/generated.d.ts", import.meta.url), "utf8");

  assert.match(source, /generated/);
  assert.match(generated, /UserInfo/);
});
