import { useState, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, useMe } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import { X, AlertTriangle, ShieldCheck } from "lucide-react";
import StageProfile from "./StageProfile";

/// Full-screen modal that opens the registration wizard for editing/updating.
/// Shows a KYC warning banner if the user skipped KYC (Tier 0).
export default function ProfileEditModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const me = useMe();
  const kyc = useQuery({ queryKey: ["kyc"], queryFn: api.kyc.myKyc, retry: false, enabled: open });

  const kycStatus = kyc.data?.status ?? "none";
  const kycTier = kyc.data?.tier ?? 0;
  const needsKyc = !kycStatus.startsWith("verified");

  useEffect(() => { if (open) document.body.style.overflow = "hidden"; else document.body.style.overflow = ""; }, [open]);

  return (
    <AnimatePresence>
      {open && (
        <motion.div
            initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
            className="fixed inset-0 z-50 bg-ink-900/95 backdrop-blur-md flex items-start sm:items-center justify-center p-6 overflow-y-auto"
          onClick={onClose}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 20 }} animate={{ opacity: 1, scale: 1, y: 0 }} exit={{ opacity: 0, scale: 0.96, y: 20 }}
            transition={{ type: "spring", damping: 22, stiffness: 280 }}
            className="w-full max-w-3xl my-8"
            onClick={e => e.stopPropagation()}
          >
            <div className="rounded-2xl overflow-hidden border border-white/10" style={{ background: "rgba(13, 13, 26, 0.98)", backdropFilter: "blur(20px)" }}>
              {/* Header bar */}
              <div className="flex items-center justify-between px-5 py-3.5 border-b border-white/5">
                <div className="flex items-center gap-2">
                  <ShieldCheck className="w-4 h-4 text-accent-glow" />
                  <span className="font-semibold text-sm">{me.data ? "Update profile & KYC" : "Registration"}</span>
                </div>
                <button onClick={onClose} className="text-slate-500 hover:text-white transition">
                  <X className="w-4 h-4" />
                </button>
              </div>

              {/* KYC warning */}
              {needsKyc && (
                <div className="mx-5 mt-4 p-3 rounded-xl bg-warn/10 border border-warn/30 flex items-start gap-2">
                  <AlertTriangle className="w-4 h-4 text-warn shrink-0 mt-0.5" />
                  <div>
                    <div className="text-sm text-warn font-medium">KYC incomplete</div>
                    <div className="text-xs text-slate-400 mt-0.5">
                      You're on Tier 0 (wallet only). Complete KYC to unlock M-Pesa ramp, higher deal caps, and arbiter-eligible deals.
                      You can still transact, but full KYC registration is recommended.
                    </div>
                  </div>
                </div>
              )}

              {/* The wizard */}
              <div className="p-5 max-h-[70vh] overflow-y-auto">
                <StageProfile />
              </div>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
