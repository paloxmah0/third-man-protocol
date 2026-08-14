// Direct CIP-30 wallet bridge — no wrapper library.
// Fixes #1 (hex→bech32 with CBOR unwrapping) and #10 (partialSign matching tx type).

import { bech32 } from "@scure/base";

export interface Signed {
  cose_sign1: string;
  cose_key: string;
}

export interface CardanoApi {
  getNetworkId(): Promise<number>;
  getUsedAddresses(): Promise<string[]>;
  getChangeAddress(): Promise<string>;
  getRewardAddresses(): Promise<string[]>;
  getBalance(): Promise<string>;
  signData(addr: string, payloadHex: string): Promise<{ signature: string; key: string }>;
  signTx(txCbor: string, partialSign?: boolean): Promise<string>;
  submitTx(txCbor: string): Promise<string>;
}

export interface WalletHandle {
  name: string;
  api: CardanoApi;
  icon?: string;
}

export function availableWallets(): { key: string; name: string; icon?: string }[] {
  const c = (window as any).cardano;
  if (!c) return [];
  const out: { key: string; name: string; icon?: string }[] = [];
  for (const key of Object.keys(c)) {
    const w = c[key];
    if (w && typeof w.enable === "function") {
      out.push({ key, name: w.name ?? key, icon: w.icon });
    }
  }
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

export async function connect(key: string): Promise<WalletHandle> {
  const c = (window as any).cardano;
  if (!c || !c[key]) throw new Error(`wallet '${key}' not found`);
  const api: CardanoApi = await c[key].enable();
  return { name: c[key].name ?? key, api };
}

// ---------------------------------------------------------------------------
// Fix #1: CIP-30 returns cbor<bytes>, not raw address bytes.
// A 29-byte address comes back as 58 1d <29 bytes> = 31 bytes.
// stripCborByteStringHeaderIfPresent unwraps it.
// ---------------------------------------------------------------------------

const NETWORK_HRP = "addr_test";

function stripCborByteStringHeaderIfPresent(bytes: Uint8Array): Uint8Array {
  if (bytes.length === 0) return bytes;
  const first = bytes[0];
  if (first < 0x40 || first > 0x5b) return bytes; // already raw address bytes
  if (first <= 0x57) return bytes.slice(1, 1 + (first - 0x40));
  if (first === 0x58) { const len = bytes[1]; return bytes.slice(2, 2 + len); }
  if (first === 0x59) { const len = (bytes[1] << 8) | bytes[2]; return bytes.slice(3, 3 + len); }
  throw new Error(`unexpected CBOR bytestring prefix 0x${first.toString(16)}`);
}

export function toBech32Address(addrOrHex: string): string {
  if (addrOrHex.startsWith("addr")) return addrOrHex;
  if (!/^[0-9a-fA-F]+$/.test(addrOrHex)) {
    throw new Error(`toBech32Address: '${addrOrHex}' is neither bech32 nor hex`);
  }
  const decoded = Uint8Array.from(Buffer.from(addrOrHex, "hex"));
  const addressBytes = stripCborByteStringHeaderIfPresent(decoded);
  return bech32.encode(NETWORK_HRP, addressBytes);
}

export async function getBech32Address(w: WalletHandle): Promise<string> {
  if (typeof w.api.getChangeAddress === "function") {
    const changeAddr = await w.api.getChangeAddress();
    if (changeAddr) return toBech32Address(changeAddr);
  }
  const used = await w.api.getUsedAddresses();
  if (!used || used.length === 0) throw new Error("wallet returned no used addresses");
  return toBech32Address(used[0]);
}

// ---------------------------------------------------------------------------
// CIP-8 message signing
// ---------------------------------------------------------------------------

export async function signData(w: WalletHandle, address: string, payloadHex: string): Promise<Signed> {
  const sig = await w.api.signData(address, payloadHex);
  if (!sig || !sig.signature || !sig.key) throw new Error("wallet declined signData");
  return { cose_sign1: sig.signature, cose_key: sig.key };
}

// ---------------------------------------------------------------------------
// Fix #10: partialSign must match the tx type
// ---------------------------------------------------------------------------

export async function signLockTx(w: WalletHandle, txCborHex: string): Promise<string> {
  return w.api.signTx(txCborHex, false); // FULL sign — no script inputs
}

export async function signSpendTx(w: WalletHandle, txCborHex: string): Promise<string> {
  return w.api.signTx(txCborHex, true); // PARTIAL sign — script witness added by backend
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export async function getAddress(w: WalletHandle): Promise<string> {
  return getBech32Address(w);
}

export async function signAgreement(w: WalletHandle, agreementId: string): Promise<Signed> {
  const address = await getBech32Address(w);
  const tok = localStorage.getItem("tmp.token") ?? "";
  const res = await fetch(`/agreements/${agreementId}/signable`, {
    headers: tok ? { authorization: `Bearer ${tok}` } : {},
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err?.error ?? `failed to fetch signable payload (${res.status})`);
  }
  const j = await res.json();
  const payload_hex = j?.data?.payload_hex ?? j?.payload_hex;
  if (!payload_hex) throw new Error("backend did not return payload_hex");
  return signData(w, address, payload_hex);
}

export function shortAddr(addr: string): string {
  if (!addr) return "";
  if (addr.length <= 14) return addr;
  return `${addr.slice(0, 10)}…${addr.slice(-6)}`;
}

export const useWallet = () => null;
