import { type Chain, fromHex, hexToBigInt } from "viem";
import * as chains from "viem/chains";

import type { ApiErr, ApiOk } from "./types";

export const ENDPOINT = "http://127.0.0.1:9545";

export const ALL_CHAINS: readonly Chain[] = Object.freeze(Object.values(chains) as Chain[]);

export const getChainById = (id: number) => ALL_CHAINS.find((c) => c.id === id);

export const formatNetwork = (id: number | undefined) => {
  if (id == null) return "Unknown network";
  const name = getChainById(id)?.name ?? `Chain ${id}`;
  return `${name} (chain ID ${id})`;
};

export const transactionTargetChainId = (
  request: Record<string, unknown>,
  connectedChainId: number | undefined,
) => (request.chainId == null ? connectedChainId : parseChainId(request.chainId));

export const parseChainId = (input: unknown): number | undefined => {
  if (typeof input === "number") return Number.isFinite(input) ? input : undefined;
  if (typeof input !== "string") return undefined;
  const s = input.trim();
  if (/^0x[0-9a-fA-F]+$/.test(s)) {
    const n = Number.parseInt(s, 16);
    return Number.isNaN(n) ? undefined : n;
  }
  if (/^\d+$/.test(s)) {
    const n = Number.parseInt(s, 10);
    return Number.isNaN(n) ? undefined : n;
  }
  return undefined;
};

export const applyChainId = (
  raw: unknown,
  setChainId: (n: number | undefined) => void,
  setChain: (c: Chain | undefined) => void,
) => {
  const id = parseChainId(raw);
  setChainId(id);

  if (id != null) {
    let chain = getChainById(id);

    // If chain not found in viem's predefined list, create a custom chain object
    if (!chain) {
      chain = {
        id,
        name: `Chain ${id}`,
        network: `chain-${id}`,
        nativeCurrency: {
          decimals: 18,
          name: "Ether",
          symbol: "ETH",
        },
        rpcUrls: {
          default: { http: [] },
          public: { http: [] },
        },
      } as Chain;
    }

    setChain(chain);
  } else {
    setChain(undefined);
  }
};

export const toBig = (h?: `0x${string}`) => (h ? hexToBigInt(h) : undefined);

export const toNonce = (h?: `0x${string}`) => (h ? fromHex(h, "number") : undefined);

export const api = async <T = unknown>(
  path: string,
  method: "GET" | "POST" = "GET",
  body?: unknown,
): Promise<T> => {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  const token = typeof window !== "undefined" ? window.__SESSION_TOKEN__ : undefined;

  if (token) {
    headers["X-Session-Token"] = token;
  }

  const res = await fetch(`${ENDPOINT}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (!res.ok) {
    throw new Error(`API request failed: ${res.status} ${res.statusText}`);
  }

  try {
    return (await res.json()) as T;
  } catch {
    throw new Error("Invalid JSON response");
  }
};

export const renderJSON = (obj: unknown) =>
  JSON.stringify(obj, (_k, v) => (typeof v === "bigint" ? v.toString() : v), 2);

export const renderMaybeParsedJSON = (value: unknown): string => {
  if (value == null) return renderJSON(value);

  if (
    typeof value === "object" &&
    value !== null &&
    "message" in value &&
    typeof (value as Record<string, unknown>).message === "string"
  ) {
    const obj = value as { message: string };
    try {
      const parsed = JSON.parse(obj.message);
      return renderJSON({ ...value, message: parsed });
    } catch {
      return renderJSON(value);
    }
  }

  return renderJSON(value);
};

export const isOk = <T>(r: ApiOk<T> | ApiErr | null | undefined): r is ApiOk<T> => {
  return !!r && (r as ApiOk<T>).status === "ok";
};
