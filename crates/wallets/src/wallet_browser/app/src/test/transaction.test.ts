import assert from "node:assert/strict";
import { test } from "node:test";
import { type Chain, createWalletClient, custom } from "viem";
import { mainnet, tempo } from "viem/chains";

import { prepareTransactionRequest } from "../src/utils/transaction.ts";

const from = "0x1111111111111111111111111111111111111111";
const to = "0x2222222222222222222222222222222222222222";
const base = { from, to, gas: "0x5208", nonce: "0x0", value: "0x0", input: "0x" };

async function capture(request: Record<string, unknown>, chain: Chain = mainnet) {
  let captured: Record<string, unknown> | undefined;
  const client = createWalletClient({
    chain,
    transport: custom({
      async request({ method, params }: { method: string; params?: unknown[] }) {
        if (method === "eth_chainId") return `0x${chain.id.toString(16)}`;
        if (method === "eth_sendTransaction") {
          captured = params?.[0] as Record<string, unknown>;
          throw Object.assign(new Error("Mock rejects every send"), { code: 4001 });
        }
        throw new Error(`Unexpected method: ${method}`);
      },
    }),
  });
  const prepared = prepareTransactionRequest(request);
  await assert.rejects(client.sendTransaction({ ...prepared, account: from, chain }));
  assert.ok(captured, "Expected a request at the wallet provider");
  return JSON.parse(JSON.stringify(captured));
}

for (const [type, fees] of [
  ["0x0", { gasPrice: "0x2" }],
  ["0x2", { maxFeePerGas: "0x2", maxPriorityFeePerGas: "0x0" }],
] as const) {
  test(`preserves ${type} and fee fields at the wallet provider`, async () => {
    assert.deepEqual(await capture({ ...base, type, ...fees }), {
      from,
      to,
      gas: "0x5208",
      nonce: "0x0",
      value: "0x0",
      data: "0x",
      type,
      ...fees,
    });
  });
}

for (const [rpc, sdk] of [
  ["0x0", "legacy"],
  ["0x1", "eip2930"],
  ["0x2", "eip1559"],
  ["0x3", "eip4844"],
  ["0x4", "eip7702"],
  ["0x76", "tempo"],
]) {
  test(`normalizes ${rpc} and preserves SDK type ${sdk}`, () => {
    assert.deepEqual(prepareTransactionRequest({ type: rpc }), { type: sdk, account: undefined });
    assert.deepEqual(prepareTransactionRequest({ type: sdk }), { type: sdk, account: undefined });
  });
}

test("leaves an absent type absent at the wallet provider", async () => {
  const request = await capture({ ...base, gasPrice: "0x2" });
  assert.equal(Object.hasOwn(request, "type"), false);
});

test("passes unrecognized types through to viem", () => {
  assert.deepEqual(prepareTransactionRequest({ type: "0x7e" }), {
    type: "0x7e",
    account: undefined,
  });
});

test("preserves Tempo type and calls through its formatter", async () => {
  const calls = [{ to, data: "0x" }];
  const request = await capture({ ...base, type: "0x76", calls }, tempo);
  assert.equal(request.type, "0x76");
  assert.deepEqual(request.calls, [{ to, data: "0x", value: "0x" }]);
});

test("omits empty Tempo calls and preserves the single-call fallback", async () => {
  assert.equal(Object.hasOwn(prepareTransactionRequest({ ...base, calls: [] }), "calls"), false);
  const request = await capture({ ...base, type: "0x76", calls: [] }, tempo);
  assert.equal(request.type, "0x76");
  assert.deepEqual(request.calls, [{ to, data: "0x", value: "0x" }]);
});
