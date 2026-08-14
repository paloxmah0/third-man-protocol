import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { api, setToken, useInvalidate } from "../lib/api";
import { useWallet } from "../lib/walletContext";
import { signData, getAddress } from "../lib/wallet";
import { motion, AnimatePresence } from "framer-motion";
import { Wallet, KeyRound, Loader2, CheckCircle2, ChevronDown } from "lucide-react";

/// Full wallet connect + nonce challenge + CIP-8 verify → DID mint. Single button flow.
export default function WalletGate() {
  const { wallet, available, connect, connecting, rescan } = useWallet();
  const [open, setOpen] = useState(false);
  const [stage, setStage] = useState<"idle" | "challenge" | "verify" | "done" | "error">("idle");
  const [err, setErr] = useState("");
  const invalidate = useInvalidate();

  const login = useMutation({
    mutationFn: async () => {
      if (!wallet) throw new Error("wallet not connected");
      const address = await getAddress(wallet);
      if (!address) throw new Error("no address available");
      setStage("challenge");
      const ch = await api.auth.challenge(address, "login");
      setStage("verify");
      const sig = await signData(wallet, address, ch.nonce);
      const session = await api.auth.verify(ch.challenge_id, sig.cose_sign1, sig.cose_key);
      setToken(session.token);
      setStage("done");
      return session;
    },
    onSuccess: () => invalidate(["me", "kyc"]),
    onError: (e: any) => { setErr(e.message ?? String(e)); setStage("error"); },
  });

  return (
    <div className="glass rounded-2xl p-8 flex flex-col items-center text-center max-w-md mx-auto">
      <motion.div
        initial={{ scale: 0.9, opacity: 0 }} animate={{ scale: 1, opacity: 1 }}
        className="w-16 h-16 rounded-2xl seal grid place-items-center mb-4"
      >
        <Wallet className="w-8 h-8 text-white" />
      </motion.div>
      <h2 className="text-xl font-semibold">Connect your Cardano wallet</h2>
      <p className="text-sm text-slate-400 mt-2 max-w-sm">
        Your wallet proves ownership of your identity. We mint a <span className="font-mono text-accent-glow">did:cardano</span> identifier
        by having you sign a server nonce with CIP-8 — no passwords, no custodial keys.
      </p>

      <div className="mt-6 w-full">
        {!wallet ? (
          <div className="relative">
            <button
              onClick={() => { setOpen(o => !o); rescan(); }}
              disabled={connecting}
              className="btn btn-primary w-full flex items-center justify-between gap-2"
            >
              <span className="flex items-center gap-2">
                {connecting ? <Loader2 className="w-4 h-4 animate-spin" /> : <Wallet className="w-4 h-4" />}
                {connecting ? "Connecting…" : available.length ? "Choose wallet" : "No wallet detected"}
              </span>
              {available.length > 0 && <ChevronDown className={`w-4 h-4 transition ${open ? "rotate-180" : ""}`} />}
            </button>
            <AnimatePresence>
              {open && available.length > 0 && (
                <motion.div
                  initial={{ opacity: 0, y: -6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -6 }}
                  className="absolute z-10 mt-2 w-full glass rounded-xl p-1.5 max-h-60 overflow-auto"
                >
                  {available.map(w => (
                    <button key={w.key} onClick={() => { setOpen(false); connect(w.key); }}
                      className="w-full text-left px-3 py-2 rounded-lg hover:bg-accent/15 transition flex items-center gap-3">
                      {w.icon
                        ? <img src={w.icon} alt="" className="w-5 h-5 rounded" />
                        : <div className="w-5 h-5 rounded bg-accent/30 grid place-items-center text-[9px]">{w.name[0]}</div>}
                      <span className="text-sm">{w.name}</span>
                    </button>
                  ))}
                </motion.div>
              )}
            </AnimatePresence>
            {available.length === 0 && (
              <button onClick={() => rescan()} className="text-[11px] text-accent-glow hover:underline mt-2">
                ↻ Rescan for wallets
              </button>
            )}
            <p className="text-[11px] text-slate-500 mt-2">
              Install a CIP-30 wallet extension (Typhon, Nami, Eternl, Lace…) and refresh.
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-3 items-center">
            <div className="glass-soft rounded-lg px-3 py-1.5 text-xs font-mono text-slate-300 flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-accent-mint animate-pulse" /> {wallet.name} connected
            </div>
            <button onClick={() => login.mutate()} disabled={login.isPending}
              className="btn btn-primary w-full flex items-center justify-center gap-2">
              {login.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <KeyRound className="w-4 h-4" />}
              {login.isPending ? "Proving ownership…" : "Sign in with wallet"}
            </button>

            <AnimatePresence mode="wait">
              {stage === "challenge" && (
                <motion.p initial={{opacity:0}} animate={{opacity:1}} exit={{opacity:0}} className="text-xs text-slate-400 flex items-center gap-1.5">
                  <Loader2 className="w-3 h-3 animate-spin" /> Issuing nonce challenge…
                </motion.p>
              )}
              {stage === "verify" && (
                <motion.p initial={{opacity:0}} animate={{opacity:1}} exit={{opacity:0}} className="text-xs text-slate-400 flex items-center gap-1.5">
                  <Loader2 className="w-3 h-3 animate-spin" /> Awaiting your CIP-8 signature…
                </motion.p>
              )}
              {stage === "done" && (
                <motion.p initial={{opacity:0}} animate={{opacity:1}} exit={{opacity:0}} className="text-xs text-accent-mint flex items-center gap-1.5">
                  <CheckCircle2 className="w-3.5 h-3.5" /> DID minted — welcome.
                </motion.p>
              )}
              {stage === "error" && (
                <motion.p initial={{opacity:0}} animate={{opacity:1}} exit={{opacity:0}} className="text-xs text-bad">{err}</motion.p>
              )}
            </AnimatePresence>
          </div>
        )}
      </div>
    </div>
  );
}
