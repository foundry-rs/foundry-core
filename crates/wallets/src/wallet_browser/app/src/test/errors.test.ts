import assert from "node:assert/strict";
import { test } from "node:test";

import { isUserRejection } from "../src/utils/errors.ts";

test("recognizes direct and nested EIP-1193 user rejection", () => {
  assert.equal(isUserRejection({ code: 4001 }), true);
  assert.equal(isUserRejection({ cause: { code: 4001 } }), true);
});

test("does not classify generic provider failures as user rejection", () => {
  assert.equal(isUserRejection({ code: -32603 }), false);
  assert.equal(isUserRejection(new Error("provider disconnected")), false);
});
