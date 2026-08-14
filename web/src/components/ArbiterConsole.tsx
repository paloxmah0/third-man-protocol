import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, useMe, useInvalidate } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import {
  Gavel, Scale, CheckCircle2, AlertTriangle, Loader2, FileText,
  Image as ImageIcon, Link as LinkIcon, Clock, Award, Users, Send, X,
} from "lucide-react";

/// Arbiter console — a dedicated view for arbiters to:
/// 1. See their trust points + active status
/// 2. View assigned disputes with full milestone submission/rejection trail
/// 3. Submit CIP-8-signed verdicts (favor_buyer / favor_supplier / split)
/// 4. See the evidence trail (all proof submissions + rejection reasons)
export default function ArbiterConsole() {
  const me = useMe();
  const inv = useInvalidate();
  const [selectedDispute, setSelectedDispute] = useState<string | null>(null);

  const arbiters = useQuery({ queryKey: ["arbiters"], queryFn: api.dispute.list });
  const myArbiterEntry = arbiters.data?.arbiters.find((a: any) => a.user_id === me.data?.id);

  const enroll = useMutation({
    mutationFn: async () => {
      // First update the user's role to arbiter via profile
      // Then enroll in the arbiter pool
      return api.dispute.enroll();
    },
    onSuccess: () => inv(["arbiters", "me"]),
  });

  if (!me.data) {
    return <div className="text-center py-20 text-slate-500">Connect your wallet first.</div>;
  }

  const isArbiter = me.data.role === "arbiter" || !!myArbiterEntry;

  return (
    <div className="max-w-4xl py-6 space-y-5">
      {/* Header */}
      <div className="glass rounded-2xl p-6">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-12 h-12 rounded-xl seal grid place-items-center">
            <Gavel className="w-6 h-6 text-white" />
          </div>
          <div>
            <h1 className="text-xl font-semibold">Arbiter Console</h1>
            <p className="text-xs text-slate-400">Review disputes, examine evidence, deliver verdicts</p>
          </div>
        </div>

        {/* Arbiter status */}
        {isArbiter && myArbiterEntry ? (
          <div className="grid grid-cols-4 gap-3">
            <StatCard icon={Award} label="Trust Points" value={myArbiterEntry.trust_points} color="text-warn" />
            <StatCard icon={CheckCircle2} label="Resolved" value={myArbiterEntry.cases_resolved} color="text-accent-mint" />
            <StatCard icon={Clock} label="Assigned" value={myArbiterEntry.cases_assigned} color="text-accent-glow" />
            <StatCard icon={Users} label="Status" value={myArbiterEntry.active ? "Active" : "Inactive"} color="text-accent-cyan" />
          </div>
        ) : (
          <div className="text-center py-6">
            <Scale className="w-10 h-10 text-slate-600 mx-auto mb-3" />
            <p className="text-sm text-slate-400 mb-4">
              You're not an arbiter yet. Enroll to join the trust-weighted arbiter pool.
              Arbiters earn governance points for resolving disputes.
            </p>
            <button onClick={() => enroll.mutate()} disabled={enroll.isPending}
              className="btn btn-primary flex items-center gap-2 mx-auto">
              {enroll.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <Gavel className="w-4 h-4" />}
              Enroll as Arbiter
            </button>
            {enroll.isError && (
              <p className="text-xs text-bad mt-2">
                {(enroll.error as any).message}
                <br />
                <span className="text-slate-500">Note: Set your role to "arbiter" in your profile first.</span>
              </p>
            )}
          </div>
        )}
      </div>

      {/* Dispute list */}
      {isArbiter && (
        <>
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wider">Assigned Disputes</h2>
          </div>
          <DisputeList arbiterId={me.data.id} onSelect={setSelectedDispute} />
        </>
      )}

      {/* Selected dispute detail */}
      <AnimatePresence>
        {selectedDispute && (
          <motion.div
            initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }}
          >
            <DisputeDetail disputeId={selectedDispute} onClose={() => setSelectedDispute(null)} />
          </motion.div>
        )}
      </AnimatePresence>

      {/* Arbiter pool leaderboard */}
      {isArbiter && (
        <div className="glass rounded-2xl p-5">
          <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wider mb-3">Arbiter Pool</h2>
          <div className="space-y-2">
            {(arbiters.data?.arbiters ?? []).slice(0, 10).map((a: any) => (
              <div key={a.user_id} className={`flex items-center gap-3 glass-soft rounded-lg p-2.5 ${a.user_id === me.data?.id ? "border-accent/40" : ""}`}>
                <div className="w-8 h-8 rounded-full bg-warn/20 grid place-items-center text-xs font-mono text-warn">
                  {a.trust_points}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-mono text-slate-300 truncate">{a.did}</div>
                  <div className="text-[10px] text-slate-500">{a.cases_resolved}/{a.cases_assigned} resolved</div>
                </div>
                {a.active && <span className="w-2 h-2 rounded-full bg-accent-mint" />}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function StatCard({ icon: Icon, label, value, color }: { icon: any; label: string; value: any; color: string }) {
  return (
    <div className="glass-soft rounded-lg p-3">
      <div className="flex items-center gap-1.5 mb-1">
        <Icon className={`w-3 h-3 ${color}`} />
        <span className="text-[10px] uppercase tracking-wider text-slate-500">{label}</span>
      </div>
      <div className={`text-lg font-bold ${color}`}>{value}</div>
    </div>
  );
}

function DisputeList({ arbiterId, onSelect }: { arbiterId: string; onSelect: (id: string) => void }) {
  // Fetch all disputes — in production this would filter by arbiter_id
  const ledger = useQuery({
    queryKey: ["all-disputes"],
    queryFn: () => api.ledger.list({ kind: "dispute_verdict", limit: 20 }),
  });

  // For now, show a placeholder — the backend doesn't have a "list disputes by arbiter" endpoint
  return (
    <div className="glass rounded-2xl p-5">
      <p className="text-sm text-slate-400">
        No disputes assigned to you yet. When a dispute is raised on an agreement where you're the
        assigned arbiter, it will appear here with the full evidence trail.
      </p>
      <p className="text-xs text-slate-500 mt-2">
        Disputes are assigned automatically to the arbiter with the highest trust points.
      </p>
    </div>
  );
}

function DisputeDetail({ disputeId, onClose }: { disputeId: string; onClose: () => void }) {
  const me = useMe();
  const inv = useInvalidate();
  const dispute = useQuery({ queryKey: ["dispute", disputeId], queryFn: () => api.dispute.get(disputeId) });
  const submissions = useQuery({
    queryKey: ["dispute-subs", disputeId],
    queryFn: async () => {
      // We need the agreement_id from the dispute to fetch submissions
      const d = await api.dispute.get(disputeId);
      return api.proofs.listSubmissions(d.agreement_id);
    },
    enabled: !!disputeId,
  });
  const milestones = useQuery({
    queryKey: ["dispute-milestones", disputeId],
    queryFn: async () => {
      const d = await api.dispute.get(disputeId);
      return api.milestones.list(d.agreement_id);
    },
    enabled: !!disputeId,
  });

  const [verdict, setVerdict] = useState<"favor_buyer" | "favor_supplier" | "split">("favor_buyer");
  const [rationale, setRationale] = useState("");
  const [coseSign1, setCoseSign1] = useState("");
  const [coseKey, setCoseKey] = useState("");

  const submitVerdict = useMutation({
    mutationFn: () => api.dispute.verdict(disputeId, {
      verdict, rationale, cose_sign1: coseSign1, cose_key: coseKey,
    }),
    onSuccess: () => { inv(["dispute", "arbiters"]); onClose(); },
  });

  // For demo: generate a dummy CIP-8 signature
  const generateDemoSig = () => {
    setCoseSign1("demo_" + Math.random().toString(36).slice(2));
    setCoseKey("demo_key_" + Math.random().toString(36).slice(2));
  };

  const d = dispute.data;

  return (
    <div className="glass rounded-2xl p-6">
      <div className="flex items-center justify-between mb-4">
        <h3 className="font-semibold">Dispute Review</h3>
        <button onClick={onClose} className="text-slate-500 hover:text-white"><X className="w-4 h-4" /></button>
      </div>

      {/* Dispute info */}
      {d && (
        <div className="glass-soft rounded-lg p-3 mb-4">
          <div className="text-xs space-y-1">
            <div><b>Dispute ID:</b> {d.id.slice(0,18)}…</div>
            <div><b>Agreement:</b> {d.agreement_id.slice(0,18)}…</div>
            <div><b>Raised by:</b> {d.raised_by.slice(0,18)}…</div>
            <div><b>State:</b> <span className="text-warn capitalize">{d.state}</span></div>
            {d.verdict && <div><b>Verdict:</b> <span className="text-accent-mint">{d.verdict}</span></div>}
          </div>
        </div>
      )}

      {/* Evidence trail — milestone submissions + rejection reasons */}
      <div className="mb-4">
        <h4 className="text-xs uppercase tracking-wider text-slate-500 mb-2">Evidence Trail</h4>
        {milestones.data?.milestones.filter((m: any) => m.proof_required || m.submissions?.length > 0).map((m: any, i: number) => (
          <div key={i} className="glass-soft rounded-lg p-3 mb-2">
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-bold text-accent-glow">M{i+1}</span>
              <span className="text-sm">{m.label}</span>
              <span className={`text-[10px] px-2 py-0.5 rounded-full ${
                m.delivery_status === "disputed" ? "bg-bad/15 text-bad" :
                m.delivery_status === "accepted" ? "bg-accent-mint/15 text-accent-mint" :
                "bg-warn/15 text-warn"
              }`}>{m.delivery_status}</span>
            </div>
            {m.deliverables && <p className="text-xs text-slate-400 italic mb-2">{m.deliverables}</p>}
            {m.submissions?.length > 0 && (
              <div className="space-y-1">
                {m.submissions.map((s: any, si: number) => (
                  <div key={si} className="text-[10px] flex items-start gap-2 p-1.5 rounded bg-ink-800/50">
                    <span className="text-slate-500 shrink-0">Attempt {si+1}:</span>
                    <div className="flex-1">
                      <span className={s.outcome === "accepted" ? "text-accent-mint" : s.outcome === "rejected" ? "text-warn" : "text-slate-400"}>
                        {s.outcome}
                      </span>
                      {s.rejection_reason && (
                        <span className="text-slate-500 block italic mt-0.5">Reason: "{s.rejection_reason}"</span>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            )}
            {m.rejection_count > 0 && (
              <div className="text-[10px] text-bad mt-1">
                {m.rejection_count}/{m.max_attempts} rejections — {m.rejection_count >= m.max_attempts ? "triggered dispute" : `${m.max_attempts - m.rejection_count} remaining`}
              </div>
            )}
          </div>
        ))}
        {milestones.isLoading && <p className="text-xs text-slate-500">Loading evidence…</p>}
      </div>

      {/* Verdict form */}
      {d?.state !== "resolved" && d?.state !== "closed" && (
        <div className="space-y-3">
          <h4 className="text-xs uppercase tracking-wider text-slate-500">Submit Verdict</h4>

          {/* Verdict choice */}
          <div className="grid grid-cols-3 gap-2">
            {[
              { v: "favor_buyer", label: "Favor Buyer", icon: "🛒" },
              { v: "favor_supplier", label: "Favor Supplier", icon: "🏭" },
              { v: "split", label: "Split", icon: "⚖️" },
            ].map(opt => (
              <button key={opt.v} onClick={() => setVerdict(opt.v as any)}
                className={`rounded-lg p-3 border text-center transition ${verdict === opt.v ? "border-accent bg-accent/15" : "border-white/10"}`}>
                <div className="text-lg mb-1">{opt.icon}</div>
                <div className="text-xs">{opt.label}</div>
              </button>
            ))}
          </div>

          {/* Rationale */}
          <div>
            <label className="label">Rationale (required)</label>
            <textarea className="input min-h-[80px]" value={rationale} onChange={e => setRationale(e.target.value)}
              placeholder="Explain your verdict based on the evidence above…" />
          </div>

          {/* CIP-8 signature */}
          <div>
            <label className="label flex items-center gap-1">
              CIP-8 Signature (verdict payload signed by your wallet)
              <button onClick={generateDemoSig} className="text-[10px] text-accent-glow hover:underline ml-auto">
                Generate demo sig
              </button>
            </label>
            <input className="input text-xs font-mono mb-2" value={coseSign1} onChange={e => setCoseSign1(e.target.value)}
              placeholder="hex COSE_Sign1…" />
            <input className="input text-xs font-mono" value={coseKey} onChange={e => setCoseKey(e.target.value)}
              placeholder="hex COSE_Key…" />
          </div>

          <button
            onClick={() => submitVerdict.mutate()}
            disabled={submitVerdict.isPending || !rationale || !coseSign1 || !coseKey}
            className="btn btn-primary w-full flex items-center justify-center gap-2">
            {submitVerdict.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <Send className="w-4 h-4" />}
            Submit Verdict
          </button>
          {submitVerdict.isError && <p className="text-xs text-bad">{(submitVerdict.error as any).message}</p>}
          {submitVerdict.isSuccess && (
            <div className="p-3 rounded-lg bg-accent-mint/10 border border-accent-mint/30 text-xs text-accent-mint">
              <CheckCircle2 className="w-4 h-4 inline mr-1" />
              Verdict submitted! The at-fault party's collateral will be slashed to the beneficiary.
            </div>
          )}
        </div>
      )}
    </div>
  );
}
