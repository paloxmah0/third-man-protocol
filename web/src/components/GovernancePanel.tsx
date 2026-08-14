import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { Trophy, Scale, ScrollText, Coins } from "lucide-react";
import { motion } from "framer-motion";

/// Compact governance widget shown on the dashboard: points balance, trust-weighted
/// arbiter pool, and recent receipts. Not a checklist — a glance at your standing.
export default function GovernancePanel() {
  const points = useQuery({ queryKey: ["points"], queryFn: api.points.balance });
  const arbiters = useQuery({ queryKey: ["arbiters"], queryFn: api.dispute.list });
  const receipts = useQuery({ queryKey: ["receipts"], queryFn: api.receipts.list });

  return (
    <div className="space-y-4">
      <motion.div layout className="glass rounded-2xl p-5">
        <div className="flex items-center gap-2 mb-3">
          <Trophy className="w-4 h-4 text-warn" />
          <h3 className="font-semibold text-sm uppercase tracking-wider">Your standing</h3>
        </div>
        <div className="text-3xl font-bold font-mono text-warn">
          {points.data?.points ?? 0}
          <span className="text-xs text-slate-500 ml-2 font-sans">points</span>
        </div>
        <p className="text-[11px] text-slate-500 mt-1">Earned on completed contracts & verdicts. Weights you in arbiter governance.</p>
      </motion.div>

      <div className="glass rounded-2xl p-5">
        <div className="flex items-center gap-2 mb-3">
          <Scale className="w-4 h-4 text-accent-glow" />
          <h3 className="font-semibold text-sm uppercase tracking-wider">Arbiter pool</h3>
        </div>
        <div className="space-y-2">
          {(arbiters.data?.arbiters ?? []).slice(0, 5).map(a => (
            <div key={a.user_id} className="flex items-center gap-3 glass-soft rounded-lg p-2.5">
              <div className="w-7 h-7 rounded-full bg-accent/20 grid place-items-center text-[10px] font-mono">
                {a.trust_points}
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-xs font-mono text-slate-300 truncate">{a.did}</div>
                <div className="text-[10px] text-slate-500">{a.cases_resolved}/{a.cases_assigned} resolved</div>
              </div>
              {a.active && <span className="w-2 h-2 rounded-full bg-accent-mint" />}
            </div>
          ))}
          {(!arbiters.data?.arbiters.length) && <p className="text-xs text-slate-500">No arbiters yet.</p>}
        </div>
      </div>

      <div className="glass rounded-2xl p-5">
        <div className="flex items-center gap-2 mb-3">
          <ScrollText className="w-4 h-4 text-accent-cyan" />
          <h3 className="font-semibold text-sm uppercase tracking-wider">Recent receipts</h3>
        </div>
        <div className="space-y-2">
          {(receipts.data?.receipts ?? []).slice(0, 4).map(r => (
            <div key={r.id} className="flex items-center gap-2 text-xs">
              <Coins className="w-3 h-3 text-accent-mint" />
              <span className="font-mono text-slate-400 truncate flex-1">{r.content_hash.slice(0,18)}…</span>
              {r.anchor_tx_hash
                ? <span className="text-[10px] text-accent-mint">anchored</span>
                : <span className="text-[10px] text-warn">pending</span>}
            </div>
          ))}
          {(!receipts.data?.receipts.length) && <p className="text-xs text-slate-500">No receipts yet.</p>}
        </div>
      </div>
    </div>
  );
}
