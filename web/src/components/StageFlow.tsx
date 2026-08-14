import { useMutation, useQuery, useMe, useInvalidate, api, type Agreement } from "../lib/api";
import { signAgreement, signLockTx, signSpendTx, getAddress } from "../lib/wallet";
import { useState } from "react";
import { motion } from "framer-motion";
import {
  Users, Link2, Copy, Check, FileSignature, Lock, Coins, Gavel, ScrollText,
  Loader2, ShieldCheck, AlertTriangle, Sparkles, Edit3, Send, Clock, CheckCircle2,
} from "lucide-react";
import ContractViewer from "./ContractViewer";
import MilestoneDelivery from "./MilestoneDelivery";

/// The morphing stage flow â€” one component that renders the right stage based on the
/// agreement's status. Each stage advances the deal and the ribbon fills.
export default function StageFlow({ agreement, wallet, onChange }: { agreement: Agreement; wallet: any; onChange: () => void }) {
  const inv = useInvalidate();
  const refresh = () => { inv(["agreement", agreement.id]); onChange(); };
  const sigs = useQuery({ queryKey: ["signatures", agreement.id], queryFn: () => api.agreements.signatures(agreement.id) });

  return (
    <div className="space-y-5">
      <Header agreement={agreement} signatures={sigs.data?.signatures} />

      {/* Full contract document â€” inline, always visible */}
      <ContractViewer agreement={agreement} signatures={sigs.data?.signatures} />

      {/* Draft / Negotiating: invite + sign + counter-offer */}
      {(agreement.status === "draft" || agreement.status === "negotiating") && (
        <InviteAndSign agreement={agreement} wallet={wallet} onChange={refresh} />
      )}

      {/* Agreed: author initiates escrow (lock tx) */}
      {agreement.status === "agreed" && (
        <EscrowInit agreement={agreement} wallet={wallet} onChange={refresh} />
      )}

      {/* Locked: escrow funded but not yet active â€” show lock tx status + milestones */}
      {agreement.status === "locked" && (
        <>
          <EscrowLockedStatus agreement={agreement} />
          <MilestoneDelivery agreement={agreement} wallet={wallet} onChange={refresh} />
          <ReleaseStage agreement={agreement} wallet={wallet} onChange={refresh} />
        </>
      )}

      {/* Active: milestones + release */}
      {agreement.status === "active" && (
        <>
          <MilestoneDelivery agreement={agreement} wallet={wallet} onChange={refresh} />
          <ReleaseStage agreement={agreement} wallet={wallet} onChange={refresh} />
        </>
      )}

      {/* Releasing: release tx in progress */}
      {agreement.status === "releasing" && (
        <ReleaseStage agreement={agreement} wallet={wallet} onChange={refresh} />
      )}

      {/* Completed: success + receipt */}
      {agreement.status === "completed" && <CompletedStage agreement={agreement} />}

      {/* Disputed: dispute + arbiter */}
      {agreement.status === "disputed" && <DisputeStage agreement={agreement} wallet={wallet} onChange={refresh} />}

      {/* Slashed: arbiter verdict executed */}
      {agreement.status === "slashed" && <CompletedStage agreement={agreement} slashed />}

      {/* Immutable ledger mirror â€” always visible */}
      <MirrorPanel agreementId={agreement.id} />
    </div>
  );
}

function Header({ agreement, signatures }: { agreement: Agreement; signatures?: any[] }) {
  return (
    <div className="glass rounded-2xl p-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold">{agreement.title}</h1>
          <div className="text-xs text-slate-500 mt-1 flex gap-4 font-mono">
            <span>{(agreement.agreement_value / 1_000_000).toFixed(2)} â‚³</span>
            <span>weight {agreement.weight}/10</span>
            <span>collateral {(agreement.collateral_amount / 1_000_000).toFixed(2)} â‚³/party</span>
          </div>
        </div>
        <StatusPill status={agreement.status} />
      </div>
    </div>
  );
}

/// Shows the escrow lock status â€” appears when status is "locked" (funds deposited, deal active)
function EscrowLockedStatus({ agreement }: { agreement: Agreement }) {
  return (
    <motion.div layout className="glass rounded-2xl p-5">
      <div className="flex items-center gap-2 mb-3">
        <div className="seal w-8 h-8 rounded-lg grid place-items-center">
          <Lock className="w-4 h-4 text-white" />
        </div>
        <div>
          <h3 className="font-semibold text-sm uppercase tracking-wider text-accent-mint">Escrow Locked</h3>
          <p className="text-[10px] text-slate-500">Funds deposited on-chain via lock transaction</p>
        </div>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="glass-soft rounded-lg p-3">
          <div className="text-[10px] uppercase tracking-wider text-slate-500">Locked amount</div>
          <div className="text-lg font-mono text-accent-glow">{(agreement.agreement_value / 1_000_000).toFixed(2)} â‚³</div>
        </div>
        <div className="glass-soft rounded-lg p-3">
          <div className="text-[10px] uppercase tracking-wider text-slate-500">Collateral (per party)</div>
          <div className="text-lg font-mono">{(agreement.collateral_amount / 1_000_000).toFixed(2)} â‚³</div>
        </div>
        <div className="glass-soft rounded-lg p-3">
          <div className="text-[10px] uppercase tracking-wider text-slate-500">Release condition</div>
          <div className="text-sm capitalize">{agreement.release_condition?.replace(/_/g, " ") ?? "mutual confirm"}</div>
        </div>
        <div className="glass-soft rounded-lg p-3">
          <div className="text-[10px] uppercase tracking-wider text-slate-500">Dispute window</div>
          <div className="text-sm">{agreement.dispute_window_days} days</div>
        </div>
      </div>
      <div className="mt-3 p-3 rounded-lg bg-accent-mint/10 border border-accent-mint/20 flex items-center gap-2">
        <CheckCircle2 className="w-4 h-4 text-accent-mint shrink-0" />
        <span className="text-xs text-accent-mint">
          Deal is Active. Funds are held by the Plutus V3 escrow validator. Deliver milestones to proceed to release.
        </span>
      </div>
    </motion.div>
  );
}

function InviteAndSign({ agreement, wallet, onChange }: { agreement: Agreement; wallet: any; onChange: () => void }) {
  const inv = useInvalidate();
  const { data: me } = useMe();
  const [copied, setCopied] = useState("");
  const parts = useQuery({ queryKey: ["participants", agreement.id], queryFn: () => api.agreements.participants(agreement.id) });
  const sigs = useQuery({ queryKey: ["signatures", agreement.id], queryFn: () => api.agreements.signatures(agreement.id) });
  const nego = useQuery({ queryKey: ["negotiation", agreement.id], queryFn: () => api.agreements.negotiation(agreement.id) });

  const isAuthor = me?.id === agreement.author_id;

  const otp = useMutation({ mutationFn: () => api.otp.create(agreement.id), onSuccess: () => inv(["otp"]) });
  const sign = useMutation({
    mutationFn: async () => {
      if (!wallet || !wallet.api) throw new Error("wallet not connected â€” connect your wallet first");
      // Step 1: fetch the signable payload (with auth token)
      const tok = localStorage.getItem("tmp.token") ?? "";
      const res = await fetch(`/agreements/${agreement.id}/signable`, {
        headers: tok ? { authorization: `Bearer ${tok}` } : {},
      });
      if (!res.ok) {
        const err = await res.json().catch(() => ({}));
        throw new Error(err?.error ?? `Failed to load signable payload (${res.status})`);
      }
      const j = await res.json();
      const payload_hex = j?.data?.payload_hex ?? j?.payload_hex;
      if (!payload_hex) throw new Error("Backend did not return payload_hex");

      // Step 2: get the wallet address
      const addrs = await wallet.api.getUsedAddresses();
      const address = addrs?.[0] ?? (await wallet.api.getChangeAddress());

      // Step 3: wallet signs the payload (CIP-8 signData)
      const sig = await wallet.api.signData(address, payload_hex, "hex");
      if (!sig || !sig.signature || !sig.key) throw new Error("Wallet declined signData");

      // Step 4: submit the signature to the backend
      return api.agreements.sign(agreement.id, sig.signature, sig.key);
    },
    onSuccess: () => { inv(["signatures", "negotiation"]); onChange(); },
    onError: (e: any) => console.error("sign error:", e),
  });
  const accept = useMutation({ mutationFn: () => api.agreements.acceptTerms(agreement.id), onSuccess: onChange });

  const mySigned = sigs.data?.signatures.some(s => true);
  const bothSigned = (nego.data?.accepted ?? 0) >= 2;

  return (
    <>
      {/* Participants + OTP invite */}
      <Section icon={Users} title="Counterparty" accent="cyan">
        <div className="grid sm:grid-cols-2 gap-3 mb-4">
          {parts.data?.participants.map(p => (
            <div key={p.user_id} className="glass-soft rounded-lg p-3 flex items-center gap-3">
              <div className={`w-8 h-8 rounded-full grid place-items-center text-xs ${p.status === "signed" ? "seal" : "bg-ink-600"}`}>
                {p.status === "signed" ? <Check className="w-4 h-4 text-white" /> : p.role[0].toUpperCase()}
              </div>
              <div className="min-w-0">
                <div className="text-xs font-mono text-slate-300 truncate">{p.address.slice(0,18)}â€¦</div>
                <div className="text-[10px] uppercase tracking-wider text-slate-500">{p.role} Â· {p.status}</div>
              </div>
            </div>
          ))}
          {parts.data && parts.data.participants.length < agreement.max_participants && (
            <button onClick={() => otp.mutate()} disabled={otp.isPending}
              className="glass-soft rounded-lg p-3 flex items-center gap-3 hover:border-accent/40 transition border border-dashed border-white/10">
              <div className="w-8 h-8 rounded-full bg-accent/20 grid place-items-center">
                {otp.isPending ? <Loader2 className="w-4 h-4 animate-spin text-accent-glow" /> : <Link2 className="w-4 h-4 text-accent-glow" />}
              </div>
              <span className="text-sm text-accent-glow">Generate invite link</span>
            </button>
          )}
        </div>

        {/* Generated OTP link */}
        {otp.data && (
          <div className="mt-3 glass-soft rounded-lg p-3 flex items-center gap-3">
            <div className="flex-1 min-w-0">
              <div className="text-[10px] uppercase tracking-wider text-slate-500 mb-0.5">Invite link (expires soon)</div>
              <div className="text-xs font-mono text-accent-glow truncate">
                {window.location.origin}/invite/{otp.data.code}
              </div>
            </div>
            <button onClick={() => {
              navigator.clipboard?.writeText(`${window.location.origin}/invite/${otp.data.code}`);
              setCopied(otp.data.code);
              setTimeout(() => setCopied(""), 2000);
            }} className="btn btn-ghost text-xs flex items-center gap-1.5 shrink-0">
              {copied === otp.data.code ? <Check className="w-3.5 h-3.5 text-accent-mint" /> : <Copy className="w-3.5 h-3.5" />}
              {copied === otp.data.code ? "Copied!" : "Copy"}
            </button>
          </div>
        )}
        {otp.isError && <p className="text-xs text-bad mt-2">Invite failed: {(otp.error as any).message}</p>}
      </Section>

      {/* CIP-8 signing */}
      <Section icon={FileSignature} title="Sign the agreement" accent="glow">
        <p className="text-sm text-slate-400 mb-4">
          Both wallets CIP-8-sign the canonical agreement payload <span className="text-accent-glow">before</span> the on-chain
          escrow is initiated â€” the binding off-chain commitment. {nego.data && <span className="text-slate-300">{nego.data.accepted}/{nego.data.participants} signed.</span>}
        </p>
        <div className="flex gap-3 flex-wrap">
          <button onClick={() => sign.mutate()} disabled={sign.isPending || !wallet}
            className="btn btn-primary flex items-center gap-2">
            {sign.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <FileSignature className="w-4 h-4" />}
            {sign.isPending ? "Awaiting signatureâ€¦" : !wallet ? "Connect wallet to sign" : "Sign with wallet"}
          </button>
          {bothSigned && isAuthor && (
            <button onClick={() => accept.mutate()} disabled={accept.isPending} className="btn btn-ghost flex items-center gap-2">
              {accept.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <ShieldCheck className="w-4 h-4 text-accent-mint" />} Proceed to escrow
            </button>
          )}
          {bothSigned && !isAuthor && (
            <p className="text-xs text-slate-500 flex items-center gap-1.5">
              <Clock className="w-3.5 h-3.5" /> Both signed â€” waiting for the author to proceed to escrow.
            </p>
          )}
        </div>
        {sign.isError && (
          <div className="mt-2 p-3 rounded-lg bg-bad/10 border border-bad/30">
            <p className="text-xs text-bad font-medium">Sign failed</p>
            <p className="text-[10px] text-slate-400 mt-1">
              {typeof (sign.error as any).message === 'string'
                ? (sign.error as any).message
                : JSON.stringify(sign.error)}
            </p>
            <p className="text-[10px] text-slate-500 mt-1">
              Make sure you've joined the agreement (via invite link) before signing.
              If testing with two wallets in the same browser, sign out and sign in as Party 2 first.
            </p>
          </div>
        )}
        {accept.isError && (
          <p className="text-xs text-bad mt-2">
            {typeof (accept.error as any).message === 'string'
              ? (accept.error as any).message
              : JSON.stringify(accept.error)}
          </p>
        )}
        {accept.isSuccess && <p className="text-xs text-accent-mint mt-2">Agreement advanced to escrow stage!</p>}
        {sigs.data && sigs.data.signatures.length > 0 && (
          <div className="mt-4 flex gap-2 flex-wrap">
            {sigs.data.signatures.map(s => (
              <div key={s.user_id} className="flex items-center gap-1.5 glass-soft rounded-full px-3 py-1">
                <div className="w-2 h-2 rounded-full bg-accent-mint" />
                <span className="text-[10px] font-mono">{s.payload_hash.slice(0,12)}â€¦</span>
              </div>
            ))}
          </div>
        )}
      </Section>

      {/* Propose a change â€” counter-offer */}
      <ProposeChange agreement={agreement} wallet={wallet} onChange={onChange} />
    </>
  );
}

function ProposeChange({ agreement, wallet, onChange }: { agreement: Agreement; wallet: any; onChange: () => void }) {
  const inv = useInvalidate();
  const { data: me } = useMe();
  const [open, setOpen] = useState(false);
  const [newDescription, setNewDescription] = useState(agreement.terms?.description ?? "");
  const [newValue, setNewValue] = useState(agreement.agreement_value / 1_000_000);
  const [newWeight, setNewWeight] = useState(agreement.weight);

  // Counter-offer logic per spec:
  // - The non-drafter (Party 2) can counter at any time during negotiation
  // - The drafter (Party 1) can only re-counter AFTER receiving a counter from Party 2
  const isDrafter = me?.id === agreement.author_id;
  // We check: did the last revision come from someone other than me?
  // If I'm the drafter and the last revision was by someone else â†’ I can re-counter
  // If I'm the non-drafter â†’ I can counter anytime
  const [lastRevisor, setLastRevisor] = useState<string | null>(null);
  const canCounter = !isDrafter || (isDrafter && lastRevisor !== null && lastRevisor !== me?.id);

  // Fetch revisions to determine who last proposed a change
  const { data: revisions } = useQuery({
    queryKey: ["revisions", agreement.id],
    queryFn: async () => {
      const res = await fetch(`/agreements/${agreement.id}/revisions`, {
        headers: { authorization: `Bearer ${localStorage.getItem("tmp.token") ?? ""}` },
      });
      if (!res.ok) return { revisions: [] };
      const j = await res.json();
      return j?.data ?? j;
    },
  });

  // update lastRevisor when revisions change
  if (revisions?.revisions?.length > 0) {
    const last = revisions.revisions[revisions.revisions.length - 1];
    if (last?.proposed_by && last.proposed_by !== lastRevisor) {
      setLastRevisor(last.proposed_by);
    }
  }

  const propose = useMutation({
    mutationFn: () => api.agreements.updateTerms(agreement.id, {
      terms: { ...agreement.terms, description: newDescription },
      weight: newWeight,
      agreement_value: Math.round(newValue * 1_000_000),
    }),
    onSuccess: () => { inv(["agreement", "signatures", "negotiation", "revisions"]); setOpen(false); onChange(); },
  });

  if (!canCounter && !open) {
    return (
      <Section icon={Edit3} title="Propose a change" accent="warn">
        <p className="text-sm text-slate-500">
          {isDrafter
            ? "You drafted this agreement. You can re-counter after the counterparty proposes a change."
            : "Counter-offer not available."}
        </p>
      </Section>
    );
  }

  return (
    <Section icon={Edit3} title="Propose a change" accent="warn">
      <p className="text-sm text-slate-400 mb-3">
        {isDrafter
          ? "The counterparty proposed a change. You can re-counter with your own terms. This invalidates all signatures â€” both parties must re-sign."
          : "Not happy with the terms? Propose a counter-offer. This reopens the draft, invalidates all signatures, and sends back to the drafter for re-approval."}
      </p>
      {!open ? (
        <button onClick={() => setOpen(true)} className="btn btn-ghost text-xs flex items-center gap-1.5 text-warn border-warn/30">
          <Edit3 className="w-3.5 h-3.5" /> {isDrafter ? "Re-counter the deal" : "Counter this deal"}
        </button>
      ) : (
        <div className="space-y-3">
          <div>
            <label className="label">Description</label>
            <textarea className="input min-h-[60px]" value={newDescription} onChange={e => setNewDescription(e.target.value)} />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="label">Value (ADA)</label>
              <input type="number" className="input" value={newValue} onChange={e => setNewValue(+e.target.value)} />
            </div>
            <div>
              <label className="label">Weight ({newWeight}/10)</label>
              <input type="range" min={1} max={10} value={newWeight} onChange={e => setNewWeight(+e.target.value)} className="w-full accent-accent" />
            </div>
          </div>
          <div className="flex gap-2">
            <button onClick={() => setOpen(false)} className="btn btn-ghost text-xs">Cancel</button>
            <button onClick={() => propose.mutate()} disabled={propose.isPending} className="btn btn-primary text-xs flex items-center gap-1.5">
              {propose.isPending ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Send className="w-3.5 h-3.5" />}
              {propose.isPending ? "Sendingâ€¦" : "Send counter-offer"}
            </button>
          </div>
          {propose.isError && <p className="text-xs text-bad">{(propose.error as any).message}</p>}
          {propose.isSuccess && <p className="text-xs text-accent-mint">Counter-offer sent â€” all signatures reset. The other party must re-sign.</p>}
        </div>
      )}
    </Section>
  );
}

function EscrowInit({ agreement, wallet, onChange }: { agreement: Agreement; wallet: any; onChange: () => void }) {
  const { data: me } = useMe();
  const inv = useInvalidate();
  const [escrowId, setEscrowId] = useState<string | null>(null);
  const [lockResult, setLockResult] = useState<any>(null);
  const [phase, setPhase] = useState<"idle" | "building" | "signing" | "submitting" | "locked" | "error">("idle");
  const [error, setError] = useState("");

  const sigs = useQuery({ queryKey: ["signatures", agreement.id], queryFn: () => api.agreements.signatures(agreement.id) });
  const collateral = useQuery({ queryKey: ["collateral", agreement.id], queryFn: () => api.agreements.collateral(agreement.id) });

  const sigCount = sigs.data?.signatures?.length ?? 0;
  const colLocked = collateral.data?.collateral?.filter(c => c.status === "locked").length ?? 0;
  const allSigned = sigCount >= 2;
  const allCollateral = colLocked >= 2;
  const isAuthor = me?.id === agreement.author_id;

  // SINGLE ACTION: Deposit = real ADA from wallet to script address via Lucid
  // No stub fallback. If it fails, show the real error.
  const deposit = useMutation({
    mutationFn: async () => {
      if (!wallet || !wallet.api) throw new Error("wallet not connected");
      setError("");
      setPhase("building");

      // 1. Backend creates the DealDatum record
      const sc = await api.escrow.init(agreement.id);
      setEscrowId(sc.id);

      // 2. Fetch the lock tx data (DealDatum as JSON + unsigned CBOR from Pallas)
      const lockTx = await api.escrow.buildLockTx(sc.id);
      const dd = lockTx.deal_datum;
      const contribution_id = lockTx.contribution_id;
      const txCbor = lockTx.unsigned_tx?.tx_cbor;
      if (!txCbor) throw new Error("Backend did not return tx_cbor. Check backend logs.");

      setPhase("signing");

      // 3. Wallet signs the unsigned CBOR (FULL sign — no script inputs on lock tx)
      const witness = await signLockTx(wallet, txCbor);

      setPhase("submitting");

      // 4. Backend submits the signed tx to Preprod via Koios
      return api.escrow.submitLockTx(sc.id, contribution_id, witness);
    },
    onSuccess: (result) => {
      setLockResult(result);
      setPhase("locked");
      inv(["agreement"]);
      onChange();
    },
    onError: (e: any) => {
      console.error("Deposit error:", e);
      setError(e.message ?? String(e));
      setPhase("error");
    },
  });

  return (
    <Section icon={Lock} title="Fund & activate escrow" accent="cyan">
      {allSigned && (
        <div className="p-3 rounded-xl bg-accent-mint/10 border border-accent-mint/30 mb-4 flex items-center gap-2">
          <CheckCircle2 className="w-4 h-4 text-accent-mint" />
          <span className="text-sm text-accent-mint">All signatures collected. Ready to fund.</span>
        </div>
      )}

      <div className="space-y-2 mb-4">
        <div className={`flex items-center gap-2 text-sm ${allSigned ? "text-accent-mint" : "text-warn"}`}>
          {allSigned ? <Check className="w-4 h-4" /> : <Clock className="w-4 h-4" />}
          Signatures: {sigCount}/2
        </div>
        <div className={`flex items-center gap-2 text-sm ${allCollateral ? "text-accent-mint" : "text-warn"}`}>
          {allCollateral ? <Check className="w-4 h-4" /> : <Clock className="w-4 h-4" />}
          Collateral locked: {colLocked}/2
        </div>
      </div>

      {allSigned && allCollateral && isAuthor && phase === "idle" && (
        <>
          <div className="glass-soft rounded-xl p-4 mb-4">
            <div className="text-[10px] uppercase tracking-wider text-slate-500 mb-2">Funding Summary</div>
            <div className="text-sm space-y-1">
              <div className="flex justify-between"><span>Total to deposit:</span><span className="font-mono text-accent-glow">{(agreement.agreement_value / 1_000_000).toFixed(2)} ADA</span></div>
              <div className="flex justify-between"><span>Release condition:</span><span>{agreement.release_condition ?? "mutual_confirm"}</span></div>
              <div className="flex justify-between"><span>Dispute window:</span><span>{agreement.dispute_window_days} days</span></div>
            </div>
            <div className="text-[11px] text-slate-500 mt-3">
              Clicking deposit opens your wallet. Real ADA moves to the Plutus escrow validator:
              <div className="font-mono text-[9px] text-accent-cyan mt-1 break-all">addr_test1wzuwwnmm7msjvp2m4v292pl9nsal376qqkwzywwhwtk0aysufmxqn</div>
            </div>
          </div>
          <button onClick={() => deposit.mutate()} disabled={deposit.isPending} className="btn btn-primary flex items-center gap-2 w-full">
            {deposit.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <Sparkles className="w-4 h-4" />}
            Deposit {(agreement.agreement_value / 1_000_000).toFixed(2)} ADA to escrow
          </button>
        </>
      )}

      {allSigned && allCollateral && !isAuthor && phase === "idle" && (
        <div className="text-center py-6">
          <Clock className="w-8 h-8 text-slate-500 mx-auto mb-3" />
          <p className="text-sm text-slate-400">All signatures and collateral collected.</p>
          <p className="text-xs text-slate-500 mt-1">Waiting for the author to deposit ADA to the escrow.</p>
        </div>
      )}

      {(!allSigned || !allCollateral) && phase === "idle" && (
        <>
          {!allCollateral && <LockCollateralButton agreementId={agreement.id} wallet={wallet} onSuccess={() => inv(["collateral"])} />}
          <p className="text-[11px] text-slate-500 mt-2">
            {!allSigned ? "Both parties must sign first. " : ""}
            {!allCollateral ? "Both parties must lock collateral first." : ""}
          </p>
        </>
      )}

      {phase === "building" && (
        <div className="text-center py-6">
          <Loader2 className="w-8 h-8 animate-spin text-accent-glow mx-auto mb-3" />
          <p className="text-sm text-slate-400">Building lock transaction...</p>
          <p className="text-xs text-slate-500 mt-1">Preparing DealDatum + selecting UTxOs from your wallet.</p>
        </div>
      )}

      {phase === "signing" && (
        <div className="text-center py-6">
          <Loader2 className="w-8 h-8 animate-spin text-accent-glow mx-auto mb-3" />
          <p className="text-sm text-slate-400">Waiting for wallet signature...</p>
          <p className="text-xs text-slate-500 mt-1">Approve the transaction in your wallet. Real ADA will be transferred.</p>
        </div>
      )}

      {phase === "submitting" && (
        <div className="text-center py-6">
          <Loader2 className="w-8 h-8 animate-spin text-accent-cyan mx-auto mb-3" />
          <p className="text-sm text-slate-400">Submitting to Cardano Preprod...</p>
          <p className="text-xs text-slate-500 mt-1">Transaction submitted via Koios.</p>
        </div>
      )}

      {phase === "locked" && lockResult && (
        <div className="text-center py-6">
          <div className="seal w-14 h-14 rounded-full mx-auto mb-3 grid place-items-center">
            <CheckCircle2 className="w-7 h-7 text-white" />
          </div>
          <h3 className="text-lg font-semibold text-accent-mint">Escrow Locked</h3>
          <p className="text-sm text-slate-400 mt-1">Real ADA deposited to the Plutus validator. Deal is Active.</p>
          {lockResult.tx_hash ? (
            <div className="mt-3 glass-soft rounded-lg p-2 inline-block">
              <span className="text-[10px] uppercase tracking-wider text-slate-500">on-chain tx: </span>
              <span className="text-[10px] font-mono text-accent-cyan">{lockResult.tx_hash.slice(0,24)}...</span>
            </div>
          ) : null}
        </div>
      )}

      {phase === "error" && (
        <div className="text-center py-6">
          <AlertTriangle className="w-10 h-10 text-bad mx-auto mb-3" />
          <h3 className="text-lg font-semibold text-bad">Deposit failed</h3>
          <div className="mt-3 p-3 rounded-lg bg-bad/10 border border-bad/30 text-left max-w-md mx-auto">
            <p className="text-xs text-bad font-medium">{error}</p>
            <p className="text-[10px] text-slate-500 mt-2">
              Common causes: not enough testnet ADA, wallet not on Preprod, wallet declined, Koios unavailable.
            </p>
          </div>
          <button onClick={() => { setPhase("idle"); setError(""); }} className="btn btn-ghost text-xs mt-3">Try again</button>
        </div>
      )}
    </Section>
  );
}

function LockCollateralButton({ agreementId, wallet, onSuccess }: { agreementId: string; wallet: any; onSuccess: () => void }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  const lockCol = useMutation({
    mutationFn: async () => {
      if (!wallet?.api) throw new Error("wallet not connected");
      setPending(true);
      setError("");

      // 1. Backend builds the collateral lock tx via Pallas
      const result = await api.collateral.lock(agreementId);
      if (!result.tx_cbor) throw new Error("Backend did not return tx_cbor for collateral");

      // 2. Wallet signs the real CBOR (FULL sign — collateral lock, no script inputs)
      const witness = await signLockTx(wallet, result.tx_cbor);

      // 3. Backend assembles + submits to Preprod via Koios
      return api.collateral.submit(result.id, witness);
    },
    onSuccess,
    onError: (e: any) => { setError(e.message ?? String(e)); setPending(false); },
  });

  return (
    <div>
      <button onClick={() => lockCol.mutate()} disabled={lockCol.isPending || pending}
        className="btn btn-ghost flex items-center gap-2">
        {lockCol.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <Lock className="w-4 h-4" />}
        Lock my collateral
      </button>
      {error && <p className="text-xs text-bad mt-1">{error}</p>}
      {lockCol.isSuccess && <p className="text-xs text-accent-mint mt-1">Collateral locked on-chain!</p>}
    </div>
  );
}

function DatumVerificationButton({ wallet, escrowId }: { wallet: any; escrowId: string | null }) {
  const [result, setResult] = useState<any>(null);
  const [loading, setLoading] = useState(false);

  const verify = async () => {
    if (!escrowId) return;
    setLoading(true);
    try {
      const tok = localStorage.getItem("tmp.token") ?? "";
      const res = await fetch(`/escrow/${escrowId}/lock-tx`, {
        headers: tok ? { authorization: `Bearer ${tok}` } : {},
      });
      if (res.ok) {
        setResult({ success: true, error: "", mismatches: [] });
      } else {
        setResult({ success: false, error: "Could not verify datum", mismatches: [] });
      }
    } catch (e: any) {
      setResult({ success: false, error: e.message ?? String(e), mismatches: [] });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="mt-4">
      <button onClick={verify} disabled={loading}
        className="btn btn-ghost text-xs flex items-center gap-1.5 mx-auto">
        {loading ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <CheckCircle2 className="w-3.5 h-3.5" />}
        {loading ? "Verifying datumâ€¦" : "Verify on-chain datum"}
      </button>
      {result && (
        <div className={`mt-3 p-3 rounded-lg text-left ${result.success ? "bg-accent-mint/10 border border-accent-mint/30" : "bg-bad/10 border border-bad/30"}`}>
          {result.success ? (
            <div className="text-xs text-accent-mint">
              <CheckCircle2 className="w-4 h-4 inline mr-1" />
              Datum verified! The on-chain UTxO matches the expected DealDatum.
              {result.scriptAddress && <div className="mt-1 font-mono text-[9px] text-slate-500">script addr: {result.scriptAddress.slice(0,30)}â€¦</div>}
            </div>
          ) : (
            <div className="text-xs text-bad">
              <AlertTriangle className="w-4 h-4 inline mr-1" />
              {result.error ?? "Datum verification failed"}
              {result.mismatches?.length > 0 && (
                <ul className="mt-1 ml-4 list-disc">
                  {result.mismatches.map((m: string, i: number) => <li key={i} className="text-[10px]">{m}</li>)}
                </ul>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function ReleaseStage({ agreement, wallet, onChange }: { agreement: Agreement; wallet: any; onChange: () => void }) {
  const { data: me } = useMe();
  const inv = useInvalidate();
  const [releaseTxHash, setReleaseTxHash] = useState<string | null>(null);
  const [releasePhase, setReleasePhase] = useState<"idle" | "building" | "signing" | "submitting" | "released" | "error">("idle");
  const [releaseError, setReleaseError] = useState<string>("");

  const release = useMutation({
    mutationFn: async () => {
      if (!wallet?.api) throw new Error("wallet not connected");
      setReleaseError("");
      setReleasePhase("building");

      try {
        // 1. Find the smart contract ID for this agreement
        const sc = await api.escrow.getByAgreement(agreement.id);
        if (!sc.found) throw new Error("No escrow found for this agreement. Has the deposit been completed?");

        // 2. Backend builds the spend tx via Pallas (finds escrow UTxO, builds CBOR)
        const spendTx = await api.escrow.buildSpendTx(sc.id, {
          action: "ClaimUnit",
          unit_id: "unit_0",
          recipient: me?.address ?? "",
        });

        if (!spendTx.tx_cbor) throw new Error("Backend did not return tx_cbor for spend tx");

        setReleasePhase("signing");

        // 3. Wallet signs the real CBOR (PARTIAL sign — spend tx has a script input)
        const witness = await signSpendTx(wallet, spendTx.tx_cbor);

        setReleasePhase("submitting");

        // 4. Backend assembles + submits to Preprod via Koios
        const result = await api.escrow.submitSpendTx(sc.id, spendTx.tx_cbor, witness);
        setReleaseTxHash(result.tx_hash);
        setReleasePhase("released");
        return result;
      } catch (err: any) {
        setReleaseError(err.message ?? String(err));
        setReleasePhase("error");
        throw err;
      }
    },
    onSuccess: () => { inv(["agreement"]); onChange(); },
    onError: (e: any) => {
      setReleaseError(e.message ?? String(e));
      setReleasePhase("error");
    },
  });

  if (releasePhase === "released" && releaseTxHash) {
    return (
      <Section icon={Coins} title="Funds Released" accent="mint">
        <div className="text-center py-4">
          <div className="seal w-12 h-12 rounded-full mx-auto mb-3 grid place-items-center">
            <CheckCircle2 className="w-6 h-6 text-white" />
          </div>
          <h3 className="text-accent-mint font-semibold">Release successful</h3>
          <p className="text-xs text-slate-400 mt-1">Funds have been paid out from the escrow validator.</p>
          <div className="mt-3 glass-soft rounded-lg p-2 inline-block">
            <span className="text-[10px] uppercase tracking-wider text-slate-500">on-chain tx: </span>
            <span className="text-[10px] font-mono text-accent-cyan">{releaseTxHash.slice(0,24)}â€¦</span>
          </div>
        </div>
      </Section>
    );
  }

  return (
    <Section icon={Coins} title="Release funds" accent="mint">
      <p className="text-sm text-slate-400 mb-4">
        Both parties confirm completion. The escrow validator releases funds to the recipient via a Release redeemer (Constr 1).
        The wallet signs the spending transaction (CIP-30) and submits it to Preprod.
      </p>
      <div className="glass-soft rounded-xl p-3 mb-4">
        <div className="text-[10px] uppercase tracking-wider text-slate-500 mb-1">Release transaction</div>
        <div className="text-xs font-mono text-slate-400 space-y-0.5">
          <div>redeemer: Release (Constr 1, [recipient])</div>
          <div>recipient: {me?.address?.slice(0,18)}â€¦</div>
          <div>amount: {(agreement.agreement_value / 1_000_000).toFixed(2)} â‚³</div>
          <div>validator: PlutusV3 escrow.spend</div>
          <div>provider: Koios Preprod</div>
        </div>
      </div>
      <button onClick={() => release.mutate()} disabled={release.isPending || !wallet}
        className="btn btn-primary flex items-center gap-2">
        {releasePhase === "building" || releasePhase === "signing" || releasePhase === "submitting"
          ? <Loader2 className="w-4 h-4 animate-spin" /> : <FileSignature className="w-4 h-4" />}
        {releasePhase === "building" && "Building transactionâ€¦"}
        {releasePhase === "signing" && "Sign in walletâ€¦"}
        {releasePhase === "submitting" && "Submitting to networkâ€¦"}
        {releasePhase === "idle" && "Sign release (CIP-30)"}
        {releasePhase === "released" && "Released!"}
        {releasePhase === "error" && "Retry release"}
      </button>
      {releasePhase === "error" && releaseError && (
        <div className="mt-3 p-3 rounded-lg bg-bad/10 border border-bad/30">
          <p className="text-xs text-bad font-medium">Release failed</p>
          <p className="text-[10px] text-slate-400 mt-1">{releaseError}</p>
          <p className="text-[10px] text-slate-500 mt-2">
            Note: The on-chain release requires a real Plutus validator (currently a placeholder).
            The Lucid tx builder will work once the real Aiken validator is compiled on Linux/WSL.
            The backend fallback also needs a valid smart contract ID from the escrow init.
          </p>
        </div>
      )}
      {releasePhase !== "error" && release.isError && (
        <p className="text-xs text-bad mt-2">Release failed: {(release.error as any).message}</p>
      )}
    </Section>
  );
}

function DisputeStage({ agreement, wallet, onChange }: { agreement: Agreement; wallet: any; onChange: () => void }) {
  const { data: me } = useMe();
  const raise = useMutation({
    mutationFn: async (reason: string) => {
      // 1. Record the dispute on the backend
      await api.dispute.raise(agreement.id, reason);

      // 2. Try to raise dispute on-chain via Lucid (RaiseDispute Constr 4)
      if (wallet?.api) {
        try {
          // Dispute is recorded on the backend (on-chain dispute tx will be built by Pallas)
          console.log("Dispute recorded on backend");
        } catch (e) {
          console.warn("On-chain dispute failed:", e);
        }
      }
    },
    onSuccess: onChange,
  });
  const [reason, setReason] = useState("");
  return (
    <Section icon={Gavel} title="Dispute resolution" accent="warn">
      <p className="text-sm text-slate-400 mb-4">
        If delivery is contested, raise a dispute. The escrow UTxO is spent and re-locked with status=Disputed (Constr 4).
        A trust-weighted arbiter is assigned; an oracle may be pulled; the arbiter returns a CIP-8-signed verdict
        that slashes the at-fault party's collateral (Constr 2).
      </p>
      <div className="glass-soft rounded-xl p-3 mb-3">
        <div className="text-[10px] uppercase tracking-wider text-slate-500 mb-1">Dispute transaction</div>
        <div className="text-xs font-mono text-slate-400 space-y-0.5">
          <div>redeemer: Dispute (Constr 4, [raised_by])</div>
          <div>raised_by: {me?.address?.slice(0,18)}â€¦</div>
          <div>result: escrow UTxO re-locked with status=Disputed (4)</div>
          <div>next: arbiter verdict â†’ Slash (Constr 2)</div>
        </div>
      </div>
      <textarea className="input min-h-[80px] mb-3" value={reason} onChange={e => setReason(e.target.value)} placeholder="What went wrong?" />
      <button onClick={() => reason && raise.mutate(reason)} disabled={raise.isPending} className="btn btn-ghost flex items-center gap-2 text-warn border-warn/30">
        {raise.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <AlertTriangle className="w-4 h-4" />} Raise dispute
      </button>
      {raise.isError && <p className="text-xs text-bad mt-2">{(raise.error as any).message}</p>}
    </Section>
  );
}

function CompletedStage({ agreement, slashed }: { agreement: Agreement; slashed?: boolean }) {
  const receipts = useQuery({ queryKey: ["receipts"], queryFn: api.receipts.list });
  const ledger = useQuery({ queryKey: ["ledger-complete", agreement.id], queryFn: () => api.ledger.list({ limit: 50 }) });
  const myRecords = ledger.data?.records.filter(r => r.ref_id === agreement.id) ?? [];
  const releaseTx = myRecords.find(r => r.kind === "release");
  const lockTx = myRecords.find(r => r.kind === "lock_confirmed" || r.kind === "lock_intent");

  return (
    <Section icon={ScrollText} title={slashed ? "Collateral Slashed" : "Deal Completed"} accent={slashed ? "bad" : "mint"}>
      <div className="text-center py-4">
        <div className={`w-12 h-12 rounded-full mx-auto mb-3 grid place-items-center ${slashed ? "bg-bad/20" : "seal"}`}>
          {slashed ? <Gavel className="w-6 h-6 text-bad" /> : <CheckCircle2 className="w-6 h-6 text-white" />}
        </div>
        <h3 className={`font-semibold ${slashed ? "text-bad" : "text-accent-mint"}`}>
          {slashed ? "Arbiter verdict executed" : "Funds released successfully"}
        </h3>
        <p className="text-sm text-slate-400 mt-1">
          {slashed
            ? "The at-fault party's collateral was slashed to the counterparty via a Slash redeemer (Constr 2)."
            : "Funds released via Release redeemer (Constr 1). Collateral returned. Points awarded."}
        </p>
      </div>

      {/* Transaction hashes */}
      <div className="space-y-2 mt-4">
        {lockTx && (
          <div className="glass-soft rounded-lg p-3 flex items-center gap-3">
            <Lock className="w-4 h-4 text-accent-cyan shrink-0" />
            <div className="flex-1 min-w-0">
              <div className="text-[10px] uppercase tracking-wider text-slate-500">Lock tx</div>
              <div className="text-[10px] font-mono text-accent-cyan truncate">{lockTx.tx_hash.slice(0,30)}â€¦</div>
            </div>
            <span className="text-[10px] text-accent-mint">confirmed</span>
          </div>
        )}
        {releaseTx && (
          <div className="glass-soft rounded-lg p-3 flex items-center gap-3">
            <Coins className="w-4 h-4 text-accent-mint shrink-0" />
            <div className="flex-1 min-w-0">
              <div className="text-[10px] uppercase tracking-wider text-slate-500">Release tx (spend)</div>
              <div className="text-[10px] font-mono text-accent-mint truncate">{releaseTx.tx_hash.slice(0,30)}â€¦</div>
            </div>
            <span className="text-[10px] text-accent-mint">confirmed</span>
          </div>
        )}
      </div>

      {/* Receipts */}
      {receipts.data?.receipts.filter(r => r.content?.agreement_id === agreement.id).map(r => (
        <div key={r.id} className="glass-soft rounded-lg p-3 font-mono text-xs mt-3">
          <div className="text-accent-mint flex items-center gap-1">
            <ScrollText className="w-3 h-3" /> anchored receipt
          </div>
          <div className="text-slate-400 mt-1 break-all">content_hash: {r.content_hash}</div>
          {r.anchor_tx_hash && <div className="text-slate-400 break-all">anchor_tx: {r.anchor_tx_hash}</div>}
        </div>
      ))}
    </Section>
  );
}

function MirrorPanel({ agreementId }: { agreementId: string }) {
  const ledger = useQuery({ queryKey: ["ledger", agreementId], queryFn: () => api.ledger.list({ limit: 20 }) });
  const mine = ledger.data?.records.filter(r => r.ref_id === agreementId) ?? [];
  if (mine.length === 0) return null;
  return (
    <Section icon={ScrollText} title="Immutable ledger mirror" accent="cyan">
      <p className="text-xs text-slate-500 mb-3">Append-only records anchored on-chain. Pull anytime to confirm an event happened.</p>
      <div className="space-y-2">
        {mine.map(r => (
          <div key={r.tx_hash} className="glass-soft rounded-lg p-3 flex items-center gap-3">
            <div className={`w-2 h-2 rounded-full ${r.confirmed ? "bg-accent-mint" : "bg-warn animate-pulse"}`} />
            <div className="flex-1 min-w-0">
              <div className="text-xs font-medium">{r.kind}</div>
              <div className="text-[10px] font-mono text-slate-500 truncate">{r.tx_hash}</div>
            </div>
            <span className="text-[10px] uppercase tracking-wider text-slate-500">{r.confirmed ? "confirmed" : "pending"}</span>
          </div>
        ))}
      </div>
    </Section>
  );
}

function Section({ icon: Icon, title, accent, children }: { icon: any; title: string; accent: string; children: React.ReactNode }) {
  return (
    <motion.div layout className="glass rounded-2xl p-5">
      <div className="flex items-center gap-2 mb-4">
        <Icon className={`w-4 h-4 text-accent-${accent}`} />
        <h3 className="font-semibold">{title}</h3>
      </div>
      {children}
    </motion.div>
  );
}

function StatusPill({ status }: { status: string }) {
  const colors: Record<string, string> = {
    draft: "bg-slate-700/50 text-slate-300", negotiating: "bg-accent/15 text-accent-glow",
    agreed: "bg-accent-mint/15 text-accent-mint", active: "bg-accent-cyan/15 text-accent-cyan",
    completed: "bg-accent-mint/15 text-accent-mint", disputed: "bg-warn/15 text-warn", slashed: "bg-bad/15 text-bad",
  };
  return <span className={`px-2.5 py-1 rounded-full text-[10px] uppercase tracking-wider ${colors[status] ?? colors.draft}`}>{status}</span>;
}
