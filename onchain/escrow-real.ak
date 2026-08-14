use cardano/address.{Credential}
use cardano/assets.{PolicyId}
use cardano/certificate.{Certificate}
use cardano/governance.{ProposalProcedure, Voter}
use cardano/transaction.{Transaction, OutputReference}

type Party {
  Party {
    address: ByteArray,
    role: ByteArray,
    collateral_amount: Int,
  }
}

type DealDatum {
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

type Action {
  Deposit { amount: Int }
  Release { recipient: ByteArray }
  Slash { at_fault: ByteArray, beneficiary: ByteArray }
  Refund
  Dispute { raised_by: ByteArray }
}

validator escrow {
  mint(_redeemer: Data, _policy_id: PolicyId, _self: Transaction) {
    todo @"mint logic goes here"
  }

  spend(datum: Option<DealDatum>, redeemer: Action, _utxo: OutputReference, _self: Transaction) {
    when redeemer is {
      Deposit { amount } -> {
        expect Some(d) = datum
        if d.funded_so_far + amount > d.total_value { fail }
      }
      Release { recipient } -> {
        expect Some(d) = datum
        if d.status != 1 { fail }
      }
      Slash { at_fault, beneficiary } -> {
        expect Some(d) = datum
        if d.status != 4 { fail }
      }
      Refund -> {
        expect Some(d) = datum
        if d.status != 0 { fail }
      }
      Dispute { raised_by } -> {
        expect Some(d) = datum
        if d.status != 1 { fail }
      }
    }
  }

  withdraw(_redeemer: Data, _account: Credential, _self: Transaction) {
    todo @"withdraw logic goes here"
  }

  publish(_redeemer: Data, _certificate: Certificate, _self: Transaction) {
    todo @"publish logic goes here"
  }

  vote(_redeemer: Data, _voter: Voter, _self: Transaction) {
    todo @"vote logic goes here"
  }

  propose(_redeemer: Data, _proposal: ProposalProcedure, _self: Transaction) {
    todo @"propose logic goes here"
  }
}
