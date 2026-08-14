import { useParams } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { api, useMe } from "../lib/api";
import { useWallet } from "../lib/walletContext";
import { motion } from "framer-motion";
import { Ticket, Loader2, Check, ArrowRight, LogIn, AlertTriangle } from "lucide-react";
import WalletGate from "../components/WalletGate";
import { Link } from "react-router-dom";

/// Deep-linked OTP invite landing: `/invite/:code`. Counterparty connects + signs in,
/// then redeems the code to join the agreement.
export default function Invite() {
  const { code } = useParams<{ code: string }>();
  const me = useMe();
  const wallet = useWallet();
  const [role, setRole] = useState<"buyer" | "supplier">("buyer");

  const redeem = useMutation({
    mutationFn: () => api.otp.redeem(code!, role),
    onError: (e: any) => { /* error shown in UI */ },
  });

  // If redeem fails with 404, the agreement was likely deleted
  const agreementDeleted = redeem.isError && ((redeem.error as any)?.message?.includes("not found") || (redeem.error as any)?.message?.includes("404"));

  if (!me.data) {
    return (
      <div className="py-10 max-w-md mx-auto">
        <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} className="text-center mb-6">
          <div className="w-14 h-14 rounded-2xl seal grid place-items-center mx-auto mb-3">
            <Ticket className="w-7 h-7 text-white" />
          </div>
          <h1 className="text-2xl font-semibold">You've been invited</h1>
          <p className="text-sm text-slate-400 mt-1">Connect your wallet to join this agreement.</p>
        </motion.div>
        <WalletGate />
      </div>
    );
  }

  return (
    <div className="py-10 max-w-md mx-auto">
      <div className="glass rounded-2xl p-6">
        <div className="flex items-center gap-2 mb-4">
          <Ticket className="w-5 h-5 text-accent-glow" />
          <h2 className="text-lg font-semibold">Redeem invite</h2>
        </div>
        <p className="text-sm text-slate-400 mb-4 font-mono">code: <span className="text-accent-glow">{code}</span></p>

        <div className="mb-4">
          <label className="label">Joining as</label>
          <div className="grid grid-cols-2 gap-2">
            {(["buyer", "supplier"] as const).map(r => (
              <button key={r} onClick={() => setRole(r)} className={`btn ${role === r ? "btn-primary" : "btn-ghost"}`}>{r}</button>
            ))}
          </div>
        </div>

        <button onClick={() => redeem.mutate()} disabled={redeem.isPending} className="btn btn-primary w-full flex items-center justify-center gap-2">
          {redeem.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <LogIn className="w-4 h-4" />}
          {redeem.isPending ? "Joining…" : "Join agreement"}
        </button>

        {redeem.isSuccess && (
          <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="mt-4 p-3 rounded-lg bg-accent-mint/10 border border-accent-mint/30 flex items-center gap-2">
            <Check className="w-4 h-4 text-accent-mint" />
            <span className="text-sm text-accent-mint">Joined!</span>
            <Link to="/" className="ml-auto text-xs text-accent-glow flex items-center gap-1 hover:underline">
              Open <ArrowRight className="w-3 h-3" />
            </Link>
          </motion.div>
        )}
        {redeem.isError && (
          <div className="mt-4 p-3 rounded-lg bg-bad/10 border border-bad/30">
            {agreementDeleted ? (
              <div className="text-center">
                <AlertTriangle className="w-6 h-6 text-bad mx-auto mb-2" />
                <p className="text-sm text-bad font-medium">Agreement no longer available</p>
                <p className="text-xs text-slate-400 mt-1">
                  The author has deleted this draft. The invite link is no longer valid.
                </p>
                <Link to="/" className="text-xs text-accent-glow hover:underline mt-2 inline-block">← Back to dashboard</Link>
              </div>
            ) : (
              <p className="text-xs text-bad">{(redeem.error as any).message}</p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
