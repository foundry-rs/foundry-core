import assert from "node:assert/strict";
import { test } from "node:test";
import { mainnet } from "viem/chains";

import { formatNetwork, transactionTargetChainId } from "../src/utils/helpers.ts";

test("shows known and custom network names with chain IDs", () => {
  assert.equal(formatNetwork(mainnet.id), `${mainnet.name} (chain ID 1)`);
  assert.equal(
    formatNetwork(Number.MAX_SAFE_INTEGER),
    "Chain 9007199254740991 (chain ID 9007199254740991)",
  );
  assert.equal(formatNetwork(undefined), "Unknown network");
});

test("uses the transaction chain ID before the connected wallet chain", () => {
  assert.equal(transactionTargetChainId({ chainId: "0xa" }, mainnet.id), 10);
  assert.equal(transactionTargetChainId({ chainId: "10" }, mainnet.id), 10);
  assert.equal(transactionTargetChainId({}, mainnet.id), mainnet.id);
});

test("does not display a malformed requested chain as the connected network", () => {
  assert.equal(transactionTargetChainId({ chainId: "not-a-chain" }, mainnet.id), undefined);
});
