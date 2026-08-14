import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, useMe, useInvalidate, type Agreement } from "../lib/api";
import { useWallet } from "../lib/walletContext";
import { motion, AnimatePresence } from "framer-motion";
import { Plus, FileText, ArrowRight, Trash2 } from "lucide-react";
import { useMutation } from "@tanstack/react-query";
import WalletGate from "../components/WalletGate";
import ProtocolRibbon from "../components/ProtocolRibbon";
import StageFlow from "../components/StageFlow";
import StageForge from "../components/StageForge";
import StageProfile from "../components/StageProfile";
import GovernancePanel from "../components/GovernancePanel";

export default function Journey() {
  const me = useMe();
  const walletCtx = useWallet();
  const wallet = walletCtx?.wallet ?? null;
  const inv = useInvalidate();
  const [activeId, setActiveId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const kyc = useQuery({
    queryKey: ["profile"],
    queryFn: () => api.kyc.myProfile().catch((e: any) => {
      if (e?.status === 401 || e?.status === 404) return null; // no profile yet — not an error
      throw e;
    }),
    enabled: !!me.data,
    retry: false,
  });
  const mine = useQuery({ queryKey: ["agreements"], queryFn: api.agreements.list, enabled: !!me.data });
  const active = useQuery({ queryKey: ["agreement", activeId], queryFn: () => api.agreements.get(activeId!), enabled: !!activeId });

  const hasProfile = !!kyc.data;
  const hasAgreements = (mine.data?.length ?? 0) > 0;
  // Show profile wizard only if: no profile AND no agreements (first-time user).
  // If they already have agreements, they've been through the flow — don't nag.
  const showProfileWizard = !hasProfile && !hasAgreements && !creating && !activeId;
  const canForge = true; // per spec: wallet IS the identity, profile/KYC is optional

  // ---- Not authed: wallet connect + DID mint ----
  if (!me.data) {
    return (
      <div className="py-10">
        <Hero />
        <WalletGate />
      </div>
    );
  }

  // ---- Forge agreement (gated by verified) ----
  if (creating) {
    return <StageForge onDone={(a) => { setCreating(false); setActiveId(a.id); }} onBack={() => setCreating(false)} />;
  }

  // ---- Active agreement: the morphing stage flow ----
  if (activeId) {
    const ag = active.data;
    // Fetch ledger records to pass tx hashes to the ribbon
    const ledgerRecords = mine.data; // not ideal but works for now
    return (
      <div className="flex gap-8 py-4">
        <ProtocolRibbon
          status={ag?.status ?? "draft"}
          txHashes={{}}
        />
        <div className="flex-1 min-w-0">
          <button onClick={() => setActiveId(null)} className="text-xs text-slate-500 hover:text-slate-300 mb-3">← back</button>
          <AnimatePresence mode="wait">
            {ag && (
              <motion.div key={ag.id + ag.status}
                initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={{ duration: 0.25 }}>
                <StageFlow agreement={ag} wallet={wallet} onChange={() => active.refetch()} />
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
    );
  }

  // ---- Dashboard ----
  return (
    <div className="grid lg:grid-cols-[1fr_300px] gap-8 py-4">
      <div className="min-w-0 space-y-5">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Your agreements</h1>
          <p className="text-sm text-slate-400">Forge a contract-like deal, invite a counterparty, sign it on-chain.</p>
        </div>

        {/* Profile wizard — only for first-time users with no agreements */}
        {showProfileWizard && <StageProfile />}

        {/* Forge */}
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wider">Agreements</h2>
          <button
            onClick={() => setCreating(true)}
            className="btn btn-primary flex items-center gap-2"
          >
            <Plus className="w-4 h-4" /> Forge agreement
          </button>
        </div>

        {mine.isLoading && <p className="text-sm text-slate-500">Loading…</p>}
        {mine.data && mine.data.length === 0 && (
          <div className="glass rounded-2xl p-10 text-center">
            <FileText className="w-10 h-10 text-accent/40 mx-auto mb-3" />
            <p className="text-slate-300">No agreements yet.</p>
            <p className="text-xs text-slate-500 mt-1">Forge your first contract above.</p>
          </div>
        )}
        <div className="grid gap-3">
          {mine.data?.map((a) => {
            const canDelete = (a.status === "draft" || a.status === "negotiating") && a.author_id === me.data?.id;
            return (
              <div key={a.id} className="glass rounded-xl p-4 hover:border-accent/40 transition group flex items-center gap-4">
                <button onClick={() => setActiveId(a.id)} className="flex-1 min-w-0 text-left">
                  <div className="font-medium truncate">{a.title}</div>
                  <div className="text-xs text-slate-500 mt-0.5 flex items-center gap-3">
                    <span className="font-mono">{(a.agreement_value / 1_000_000).toFixed(2)} ₳</span>
                    <span>weight {a.weight}</span>
                    <StatusPill status={a.status} />
                  </div>
                </button>
                {canDelete && (
                  <button onClick={async () => {
                    if (confirm("Delete this draft? This cannot be undone. Any invited parties will lose access.")) {
                      try {
                        await api.agreements.delete(a.id);
                        inv(["agreements"]);
                      } catch (e: any) {
                        alert("Delete failed: " + (e?.message ?? "unknown error"));
                      }
                    }
                  }} className="text-slate-500 hover:text-bad transition p-1.5 rounded-lg hover:bg-bad/10" title="Delete draft">
                    <Trash2 className="w-4 h-4" />
                  </button>
                )}
                <ArrowRight className="w-4 h-4 text-slate-500 group-hover:text-accent-glow transition cursor-pointer" onClick={() => setActiveId(a.id)} />
              </div>
            );
          })}
        </div>
      </div>
      <GovernancePanel />
    </div>
  );
}

function StatusPill({ status }: { status: string }) {
  const colors: Record<string, string> = {
    draft: "bg-slate-700/50 text-slate-300",
    negotiating: "bg-accent/15 text-accent-glow",
    agreed: "bg-accent-mint/15 text-accent-mint",
    active: "bg-accent-cyan/15 text-accent-cyan",
    completed: "bg-accent-mint/15 text-accent-mint",
    disputed: "bg-warn/15 text-warn",
    slashed: "bg-bad/15 text-bad",
  };
  return <span className={`px-2 py-0.5 rounded-full text-[10px] uppercase tracking-wider ${colors[status] ?? colors.draft}`}>{status}</span>;
}

function Hero() {
  return (
    <div className="text-center mb-10 max-w-2xl mx-auto">
      <motion.h1 initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
        className="text-4xl font-bold tracking-tight bg-gradient-to-r from-white via-accent-glow to-accent-cyan bg-clip-text text-transparent">
        Trust, sealed on-chain.
      </motion.h1>
      <p className="text-slate-400 mt-3">
        A Cardano escrow protocol: forge a contract, invite a counterparty, both wallets sign,
        funds lock in a Plutus validator, release on completion — disputes judged by a trust-weighted arbiter pool.
      </p>
    </div>
  );
}
