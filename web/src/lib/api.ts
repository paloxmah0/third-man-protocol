// Typed client mirroring the Rust gateway 1:1. Same origin (Vite proxy in dev).
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const TOKEN_KEY = "tmp.token";
export const getToken = () => localStorage.getItem(TOKEN_KEY) ?? "";
export const setToken = (t: string) => localStorage.setItem(TOKEN_KEY, t);
export const clearToken = () => localStorage.removeItem(TOKEN_KEY);

async function req<T>(path: string, opts: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = { "content-type": "application/json", ...(opts.headers as any) };
  const tok = getToken();
  if (tok) headers["authorization"] = `Bearer ${tok}`;
  const res = await fetch(path, { ...opts, headers });
  const text = await res.text();
  const json = text ? JSON.parse(text) : null;
  if (!res.ok) throw new ApiError(json?.error ?? `HTTP ${res.status}`, res.status);
  return (json?.data ?? json) as T;
}

export class ApiError extends Error {
  status: number;
  constructor(msg: string, status: number) { super(msg); this.status = status; }
}

// ---- Domain types (mirror backend serde structs) ----
export type Role = "unassigned" | "supplier" | "buyer" | "arbiter";
export type AgStatus = "draft" | "negotiating" | "agreed" | "locked" | "active" | "releasing" | "completed" | "disputed" | "slashed";

export interface Session { token: string; user_id: string; did: string; address: string; role: Role; new_user: boolean; }
export interface Me { id: string; did: string; address: string; role: Role; }
export interface Challenge { challenge_id: string; nonce: string; purpose: string; }
export interface Profile {
  id: string; user_id: string; display_name: string; avatar_url?: string;
  location?: string; bio?: string; role_types: string[]; languages: string[];
  professional_links: any[]; settlement_rails: string[]; deal_size_range?: string;
  availability?: string; org_name?: string; org_type?: string; org_members: string[];
  verified_signals: any[]; privacy_prefs: any; created_at: string; updated_at: string;
}
export interface KycTier {
  id: string; user_id: string; tier: number; status: string;
  phone?: string; phone_verified: boolean; legal_name?: string;
  attestation_hash?: string; issued_at?: string; expiry_at?: string;
}
export interface Agreement {
  id: string; author_id: string; title: string; terms: any; terms_hash: string;
  weight: number; agreement_value: number; collateral_amount: number;
  max_participants: number; currency_asset?: string;
  release_condition?: string; dispute_window_days: number;
  arbiter_fee_percent: number; arbiter_fee_paid_by?: string;
  status: AgStatus; created_at: string; updated_at: string;
}
export interface Participant { user_id: string; role: string; status: string; did: string; address: string; }
export interface Signature { user_id: string; terms_hash: string; payload_hash: string; verified: boolean; signed_at: string; }
export interface Signable { payload_hex: string; terms_hash: string; }
export interface Otp { id: string; agreement_id: string; code: string; link: string; max_uses: number; uses: number; expires_at: string; }
export interface SmartContract { id: string; agreement_id: string; validator_hash: string; validator_addr: string; datum_hash?: string; state: string; }
export interface Dispute { id: string; agreement_id: string; raised_by: string; state: string; arbiter_id?: string; verdict?: string; created_at: string; }
export interface CollateralEntry { user_id: string; amount: number; status: string; }
export interface LedgerRecord {
  tx_hash: string; kind: string; ref_id: string; content_hash: string;
  block?: number; confirmed: boolean; pushed_by?: string;
  created_at: string; confirmed_at?: string; payload: any;
}
export interface Receipt { id: string; contract_id: string; content_hash: string; anchor_tx_hash?: string; saved_at: string; content: any; }
export interface Points { user_id: string; points: number; }
export interface Arbiter { user_id: string; did: string; active: boolean; trust_points: number; cases_assigned: number; cases_resolved: number; }
export interface NegotiationStatus { agreement_id: string; terms_hash: string; status: string; participants: number; accepted: number; }

// ---- Endpoints ----
export const api = {
  auth: {
    challenge: (address: string, purpose = "login") => req<Challenge>("/auth/challenge", { method: "POST", body: JSON.stringify({ address, purpose }) }),
    verify: (challenge_id: string, cose_sign1: string, cose_key: string) => req<Session>("/auth/verify", { method: "POST", body: JSON.stringify({ challenge_id, cose_sign1, cose_key }) }),
    me: () => req<Me>("/auth/me"),
    logout: () => req<{ logged_out: boolean }>("/auth/logout", { method: "POST" }),
  },
  kyc: {
    submitProfile: (b: { display_name: string; avatar_url?: string; location?: string; bio?: string;
      role_types?: string[]; languages?: string[]; professional_links?: any[];
      settlement_rails?: string[]; deal_size_range?: string; availability?: string;
      org_name?: string; org_type?: string; org_members?: string[]; verified_signals?: any[] }) =>
      req<Profile>("/profile", { method: "POST", body: JSON.stringify(b) }),
    myProfile: () => req<Profile>("/profile"),
    viewProfile: (userId: string) => req<any>(`/profile/${userId}`),
    updatePrivacy: (prefs: any) => req<{ updated: boolean; prefs: any }>("/profile/privacy", { method: "PATCH", body: JSON.stringify({ prefs }) }),
    myKyc: () => req<KycTier>("/kyc"),
    submitKyc: (b: { tier: number; phone?: string; legal_name?: string; document_type?: string; document_hash?: string; selfie_hash?: string }) =>
      req<KycTier>("/kyc/submit", { method: "POST", body: JSON.stringify(b) }),
    verifyKyc: (user_id: string, status: string) => req<KycTier>("/kyc/verify", { method: "POST", body: JSON.stringify({ user_id, status }) }),
  },
  agreements: {
    create: (b: { title: string; terms: any; weight?: number; agreement_value?: number; max_participants?: number; currency_asset?: string; release_condition?: string; dispute_window_days?: number; arbiter_fee_percent?: number; arbiter_fee_paid_by?: string }) => req<Agreement>("/agreements", { method: "POST", body: JSON.stringify(b) }),
    list: () => req<Agreement[]>("/agreements"),
    get: (id: string) => req<Agreement>(`/agreements/${id}`),
    delete: (id: string) => req<{ deleted: boolean; id: string }>(`/agreements/${id}`, { method: "DELETE" }),
    updateTerms: (id: string, b: { terms: any; weight?: number; agreement_value?: number }) => req<Agreement>(`/agreements/${id}/terms`, { method: "PATCH", body: JSON.stringify(b) }),
    participants: (id: string) => req<{ participants: Participant[] }>(`/agreements/${id}/participants`),
    signable: (id: string) => req<Signable>(`/agreements/${id}/signable`),
    sign: (id: string, cose_sign1: string, cose_key: string) => req<{ id: string; verified: boolean; terms_hash: string; payload_hash: string; signed_at: string }>(`/agreements/${id}/sign`, { method: "POST", body: JSON.stringify({ cose_sign1, cose_key }) }),
    signatures: (id: string) => req<{ signatures: Signature[] }>(`/agreements/${id}/signatures`),
    acceptTerms: (id: string) => req<NegotiationStatus>(`/agreements/${id}/accept-terms`, { method: "POST" }),
    negotiation: (id: string) => req<NegotiationStatus>(`/agreements/${id}/negotiation`),
    collateral: (id: string) => req<{ collateral: CollateralEntry[] }>(`/agreements/${id}/collateral`),
  },
  otp: {
    create: (agreement_id: string, max_uses?: number, ttl_seconds?: number) => req<Otp>("/otp", { method: "POST", body: JSON.stringify({ agreement_id, max_uses, ttl_seconds }) }),
    redeem: (code: string, role?: string) => req<{ joined: boolean; agreement_id: string; role: string }>(`/otp/redeem?code=${encodeURIComponent(code)}${role ? `&role=${role}` : ""}`, { method: "POST" }),
  },
  attachments: {
    upload: (b: { agreement_id: string; milestone_index?: number; filename: string; file_type: string; file_size?: number; content_hash: string; label?: string; purpose: string; url?: string }) =>
      req<any>("/attachments", { method: "POST", body: JSON.stringify(b) }),
    list: (agreement_id: string) => req<{ attachments: any[] }>(`/attachments?agreement_id=${agreement_id}`),
  },
  proofs: {
    setRequirement: (b: { agreement_id: string; milestone_index: number; kind: string; label?: string; max_attempts?: number }) =>
      req<any>("/proofs/require", { method: "POST", body: JSON.stringify(b) }),
    listRequirements: (agreement_id: string) => req<{ requirements: any[] }>(`/proofs/requirements?agreement_id=${agreement_id}`),
    submit: (b: { agreement_id: string; milestone_index: number; attachment_id: string; attachment_hash: string }) =>
      req<any>("/proofs/submit", { method: "POST", body: JSON.stringify(b) }),
    listSubmissions: (agreement_id: string) => req<{ submissions: any[] }>(`/proofs/submissions?agreement_id=${agreement_id}`),
    review: (b: { submission_id: string; outcome: string; rejection_reason?: string }) =>
      req<any>("/proofs/review", { method: "POST", body: JSON.stringify(b) }),
  },
  milestones: {
    list: (agreement_id: string) => req<{ milestones: any[] }>(`/milestones?agreement_id=${agreement_id}`),
  },
  collateral: {
    lock: (agreement_id: string) => req<any>("/collateral/lock", { method: "POST", body: JSON.stringify({ agreement_id }) }),
    submit: (collateral_id: string, witness: string) => req<any>("/collateral/submit", { method: "POST", body: JSON.stringify({ collateral_id, witness }) }),
  },
  escrow: {
    init: (agreement_id: string) => req<SmartContract>("/escrow/init", { method: "POST", body: JSON.stringify({ agreement_id }) }),
    buildLockTx: (id: string) => req<any>(`/escrow/${id}/lock-tx`),
    submitLockTx: (id: string, contribution_id: string, witness: string) => req<any>(`/escrow/${id}/submit-lock-tx`, { method: "POST", body: JSON.stringify({ contribution_id, witness }) }),
    getByAgreement: (agreement_id: string) => req<any>(`/escrow/by-agreement/${agreement_id}`),
    buildSpendTx: (id: string, body: any) => req<any>(`/escrow/${id}/build-spend-tx`, { method: "POST", body: JSON.stringify(body) }),
    submitSpendTx: (id: string, txCbor: string, witness: string) => req<any>(`/escrow/${id}/submit-spend-tx`, { method: "POST", body: JSON.stringify({ tx_cbor: txCbor, witness }) }),
    complete: (id: string) => req<any>(`/escrow/${id}/complete`, { method: "POST" }),
    release: (id: string, witnesses: Record<string, string>) => req<{ released: boolean; tx_hash: string; smart_contract_id: string }>(`/escrow/${id}/release`, { method: "POST", body: JSON.stringify({ witnesses }) }),
  },
  dispute: {
    raise: (agreement_id: string, reason: string) => req<Dispute>("/disputes", { method: "POST", body: JSON.stringify({ agreement_id, reason }) }),
    get: (id: string) => req<Dispute>(`/disputes/${id}`),
    oracle: (id: string, source: string, query: string) => req<{ oracle_id: string; status: string }>(`/disputes/${id}/oracle`, { method: "POST", body: JSON.stringify({ source, query }) }),
    verdict: (id: string, b: { verdict: string; rationale: string; cose_sign1: string; cose_key: string }) => req<Dispute>(`/disputes/${id}/verdict`, { method: "POST", body: JSON.stringify(b) }),
    enroll: () => req<{ ok: boolean; user_id: string }>("/arbiters/enroll", { method: "POST" }),
    list: () => req<{ arbiters: Arbiter[] }>("/arbiters"),
  },
  points: { balance: () => req<Points>("/points"), ledger: () => req<{ ledger: any[] }>("/points/ledger") },
  receipts: { list: () => req<{ receipts: Receipt[] }>("/receipts"), get: (id: string) => req<Receipt>(`/receipts/${id}`) },
  ledger: {
    push: (b: { kind: string; ref_id?: string; payload: any }) => req<{ pushed: boolean; tx_hash: string }>("/ledger/push", { method: "POST", body: JSON.stringify(b) }),
    list: (q?: { kind?: string; confirmed?: number; limit?: number }) => req<{ records: LedgerRecord[] }>(`/ledger${q ? "?" + new URLSearchParams(q as any).toString() : ""}`),
    get: (tx_hash: string) => req<LedgerRecord>(`/ledger/${tx_hash}`),
    confirm: (tx_hash: string, b: { block?: number; anchor_tx_hash?: string }) => req<{ confirmed: boolean; tx_hash: string }>(`/ledger/${tx_hash}/confirm`, { method: "POST", body: JSON.stringify(b) }),
  },
};

// ---- React Query hook conveniences ----
export const useMe = () => useQuery({
  queryKey: ["me"],
  queryFn: () => api.auth.me().catch((e: any) => {
    if (e instanceof ApiError && e.status === 401) return null; // not logged in — not an error
    throw e;
  }),
  retry: false,
});
export const useInvalidate = () => { const qc = useQueryClient(); return (k: string[]) => qc.invalidateQueries({ queryKey: k }); };
export { useQuery, useMutation, useQueryClient };
