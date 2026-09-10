import type { Address } from "viem";

import { toBig, toNonce } from "./helpers.ts";

const transactionTypes: Record<string, string> = {
  "0x0": "legacy",
  "0x1": "eip2930",
  "0x2": "eip1559",
  "0x3": "eip4844",
  "0x4": "eip7702",
  "0x76": "tempo",
};

// Pass unrecognized types through so the selected chain's formatter can handle them.
function normalizeTransactionType(type: unknown): unknown {
  return typeof type === "string"
    ? Object.hasOwn(transactionTypes, type)
      ? transactionTypes[type]
      : type
    : type;
}

/** Convert an RPC request to viem's sendTransaction parameters. */
export function prepareTransactionRequest(request: Record<string, unknown>) {
  const {
    from,
    input,
    to,
    maxFeePerGas,
    maxPriorityFeePerGas,
    gasPrice,
    gas,
    nonce,
    value,
    calls,
    type,
    ...txFields
  } = request;

  // The Tempo accounts SDK's eth_sendTransaction handler uses nullish
  // coalescing to fall back from calls[] to {to, data} — but an empty
  // array [] is truthy and bypasses the fallback. Omit calls when empty
  // so the SDK correctly converts to+data into a call entry.
  const resolvedCalls = Array.isArray(calls) && calls.length > 0 ? { calls } : {};

  // Convert hex-encoded numeric fields to BigInt/number for viem
  // compatibility. gasPrice (legacy) and EIP-1559 fee fields are
  // mutually exclusive.
  const feeFields =
    maxFeePerGas || maxPriorityFeePerGas
      ? {
          ...(maxFeePerGas ? { maxFeePerGas: toBig(maxFeePerGas as `0x${string}`) } : {}),
          ...(maxPriorityFeePerGas
            ? { maxPriorityFeePerGas: toBig(maxPriorityFeePerGas as `0x${string}`) }
            : {}),
        }
      : {
          ...(gasPrice ? { gasPrice: toBig(gasPrice as `0x${string}`) } : {}),
        };

  const typeFields: Record<string, unknown> =
    type == null ? {} : { type: normalizeTransactionType(type) };

  return {
    ...txFields,
    ...typeFields,
    ...resolvedCalls,
    account: from as Address | undefined,
    ...(input ? { data: input as `0x${string}` } : {}),
    ...(to ? { to: to as Address } : {}),
    ...feeFields,
    ...(gas ? { gas: toBig(gas as `0x${string}`) } : {}),
    ...(nonce ? { nonce: toNonce(nonce as `0x${string}`) } : {}),
    ...(value ? { value: toBig(value as `0x${string}`) } : {}),
  };
}
