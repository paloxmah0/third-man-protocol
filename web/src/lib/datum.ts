// DealDatum types — matches the Aiken validator struct.
// No Lucid — the backend (Pallas) handles CBOR encoding.

export interface PartyT { address: string; label: string; }
export interface AllocationT { recipient: string; amount: bigint; }
export interface ProofRequirementT {
  required: boolean; attachment_hash: string; submitted_by: string;
  rejection_count: bigint; max_attempts: bigint; accepted: boolean;
}
export interface ReleaseUnitT {
  unit_id: string; allocation: AllocationT; condition: any;
  proof: ProofRequirementT; claimed: boolean;
}
export interface DealDatumT {
  deal_id: string; parties: PartyT[]; total_value: bigint;
  release_units: ReleaseUnitT[]; release_condition: any;
  document_hash: string; attachment_hashes: string[];
  dispute_window: bigint; funding_deadline: bigint;
  funded_so_far: bigint; status: bigint; created_at: bigint;
}

export const Status = {
  PendingFunding: 0n, Active: 1n, Disputed: 2n, Completed: 3n, Expired: 4n,
} as const;
