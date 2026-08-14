import { motion } from "framer-motion";
import { Check, Lock, FileSignature, Coins, Gavel, ScrollText, Send, Users } from "lucide-react";
import type { AgStatus } from "../lib/api";

/// The journey ribbon — a left rail that fills as the deal advances.
/// Each node lights + shows its on-chain tx hash when the stage is reached.
const STAGES: { key: string; label: string; icon: any; statuses: AgStatus[] }[] = [
  { key: "forge",    label: "Forge Agreement",      icon: FileSignature, statuses: ["draft"] },
  { key: "invite",   label: "Invite Counterparty",   icon: Users,         statuses: ["draft", "negotiating"] },
  { key: "sign",     label: "Both Wallets Sign",     icon: Send,          statuses: ["negotiating", "agreed"] },
  { key: "lock",     label: "Lock Tx (Fund Escrow)", icon: Lock,          statuses: ["agreed", "locked"] },
  { key: "deliver",  label: "Milestone Delivery",    icon: Check,         statuses: ["locked", "active"] },
  { key: "release",  label: "Release Tx (Spend)",    icon: Coins,         statuses: ["active", "releasing", "completed"] },
  { key: "dispute",  label: "Dispute + Arbiter",     icon: Gavel,         statuses: ["disputed", "slashed"] },
  { key: "mirror",   label: "Anchored Receipt",      icon: ScrollText,    statuses: ["completed", "slashed"] },
];

export default function ProtocolRibbon({ status, txHashes = {} }: { status: AgStatus; txHashes?: Record<string, string> }) {
  // Find the furthest stage reached — the first stage whose statuses include the current status
  const reached = STAGES.findIndex(s => s.statuses.includes(status));
  const safeReached = reached === -1 ? 0 : reached;

  return (
    <aside className="hidden lg:flex flex-col gap-1 w-64 shrink-0 sticky top-20 self-start">
      <div className="text-[10px] uppercase tracking-[0.25em] text-slate-500 px-2 mb-2">Protocol thread</div>
      <div className="relative pl-7">
        {/* spine */}
        <div className="absolute left-[11px] top-2 bottom-2 w-px bg-white/10" />
        <motion.div
          className="absolute left-[11px] top-2 w-px bg-gradient-to-b from-accent to-accent-cyan"
          initial={{ height: 0 }}
          animate={{ height: `${(safeReached / (STAGES.length - 1)) * 100}%` }}
          transition={{ duration: 0.6, ease: "easeOut" }}
        />
        <ul className="space-y-3">
          {STAGES.map((s, i) => {
            const done = i < safeReached;
            const active = i === safeReached;
            const Icon = s.icon;
            return (
              <li key={s.key} className="relative">
                <motion.div
                  className={`absolute -left-7 top-0.5 w-6 h-6 rounded-full grid place-items-center border transition-all
                    ${done ? "bg-accent border-accent text-white" : active ? "bg-accent/20 border-accent text-accent-glow animate-pulse-slow" : "bg-ink-700 border-white/10 text-slate-600"}`}
                  initial={false}
                  animate={{ scale: active ? 1.15 : 1 }}
                >
                  <Icon className="w-3 h-3" />
                </motion.div>
                <div className={`text-sm ${done || active ? "text-slate-200" : "text-slate-500"}`}>{s.label}</div>
                {txHashes[s.key] && (
                  <div className="text-[10px] font-mono text-accent-mint mt-0.5 truncate max-w-[180px]">
                    tx {txHashes[s.key].slice(0, 14)}…
                  </div>
                )}
                {active && (
                  <div className="text-[9px] text-accent-glow mt-0.5 uppercase tracking-wider">← current</div>
                )}
              </li>
            );
          })}
        </ul>
      </div>
    </aside>
  );
}
