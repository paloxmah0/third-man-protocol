// Third Man Protocol — Escrow types
// Must match TypeScript DealDatum field-for-field

pub type Party {
  Party {
    address: ByteArray,
    role: ByteArray,
    collateral_amount: Int,
  }
}

pub type DealDatum {
  DealDatum {
    deal_id: ByteArray,
    parties: List<Party>,
    total_value: Int,
    release_condition: Int,
    document_hash: ByteArray,
    attachment_hashes: List<ByteArray>,
    dispute_window: Int,
    funding_deadline: Int,
    funded_so_far: Int,
    status: Int,
    created_at: Int,
  }
}

pub type Action {
  Deposit { amount: Int }
  Release { recipient: ByteArray }
  Slash { at_fault: ByteArray, beneficiary: ByteArray }
  Refund
  Dispute { raised_by: ByteArray }
}
