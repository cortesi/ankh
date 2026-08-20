import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { expect, test } from "vitest";

test("@ankh/auth-react exports shared modules", async () => {
  const source = await readFile(join(process.cwd(), "src/index.ts"), "utf8");

  expect(source).toMatch(/"\.\/api"/);
  expect(source).toMatch(/"\.\/components"/);
  expect(source).toMatch(/"\.\/context"/);
});
