import { ReactNode, useState, useRef, useEffect } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api, useMe, clearToken, useInvalidate } from "../lib/api";
import { useWallet } from "../lib/walletContext";
import { Shield, ExternalLink, UserCircle, BadgeCheck, ChevronDown,
  ShieldCheck, Clock, Copy, LogOut, Settings, FileText, Coins, AlertTriangle, Gavel } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import ProfileEditModal from "./ProfileEditModal";

export default function Shell({ children }: { children: ReactNode }) {
  const me = useMe();
  const [open, setOpen] = useState(false);
  const [editOpen, setEditOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  return (
    <div className="relative z-10 min-h-screen flex flex-col">
      <header className="sticky top-0 z-30 backdrop-blur-md bg-ink-900/60 border-b border-white/5">
        <div className="max-w-7xl mx-auto px-5 h-16 flex items-center justify-between">
          <Link to="/" className="flex items-center gap-2.5 group">
            <div className="w-9 h-9 rounded-xl seal grid place-items-center">
              <Shield className="w-5 h-5 text-white" />
            </div>
            <div className="leading-tight">
              <div className="font-semibold tracking-tight">Third Man</div>
              <div className="text-[10px] uppercase tracking-[0.2em] text-slate-500">Protocol</div>
            </div>
          </Link>

          <div className="flex items-center gap-3">
            <Link to="/arbiter" className="text-xs text-slate-400 hover:text-accent-glow transition flex items-center gap-1">
              <Gavel className="w-3.5 h-3.5" /> Arbiter
            </Link>
            <a href="http://127.0.0.1:8080/health" target="_blank" rel="noreferrer"
               className="text-slate-400 hover:text-white transition" title="Gateway health">
              <ExternalLink className="w-4 h-4" />
            </a>

            {me.data && <ProfileDropdown open={open} setOpen={setOpen} refProp={ref as React.MutableRefObject<HTMLDivElement>} onEdit={() => { setOpen(false); setEditOpen(true); }} />}
          </div>
        </div>
      </header>

      <main className="flex-1 max-w-7xl w-full mx-auto px-5 py-6">{children}</main>

      <footer className="text-center text-[11px] text-slate-600 py-6">
        Cardano escrow · CIP-8 message signing · CIP-30 tx signing · immutable ledger mirror
      </footer>

      <ProfileEditModal open={editOpen} onClose={() => setEditOpen(false)} />
    </div>
  );
}

function ProfileDropdown({ open, setOpen, refProp, onEdit }: { open: boolean; setOpen: (v: boolean) => void; refProp: React.MutableRefObject<HTMLDivElement>; onEdit: () => void }) {
  const me = useMe();
  const inv = useInvalidate();
  const wallet = useWallet();
  const profile = useQuery({ queryKey: ["profile"], queryFn: api.kyc.myProfile, retry: false });
  const kyc = useQuery({ queryKey: ["kyc"], queryFn: api.kyc.myKyc, retry: false });
  const points = useQuery({ queryKey: ["points"], queryFn: api.points.balance, retry: false });
  const receipts = useQuery({ queryKey: ["receipts"], queryFn: api.receipts.list, retry: false });

  const logout = () => { clearToken(); wallet.disconnect(); inv(["me", "profile", "kyc"]); setOpen(false); window.location.href = "/"; };
  const copyAddr = () => navigator.clipboard?.writeText(me.data?.address ?? "");

  const kycTier = kyc.data?.tier ?? 0;
  const kycStatus = kyc.data?.status ?? "none";
  const kycLabel = kycStatus.startsWith("verified_t1") ? "Tier 1 — Basic" :
                   kycStatus.startsWith("verified_t2") ? "Tier 2 — Verified" :
                   kycStatus.startsWith("pending") ? `Pending Tier ${kycTier}` : "Tier 0 — Wallet only";
  const kycColor = kycStatus.startsWith("verified") ? "text-accent-mint" :
                   kycStatus.startsWith("pending") ? "text-warn" : "text-slate-400";
  const kycIcon = kycStatus.startsWith("verified") ? BadgeCheck : kycStatus.startsWith("pending") ? Clock : ShieldCheck;

  const KycIcon = kycIcon;
  const needsKyc = !kycStatus.startsWith("verified");

  return (
    <div ref={refProp} className="relative">
      <button onClick={() => setOpen(!open)}
        className="flex items-center gap-2 glass-soft rounded-full pl-1 pr-2.5 py-1 hover:border-accent/40 transition">
        <div className="w-7 h-7 rounded-full seal grid place-items-center text-xs font-bold">
          {profile.data?.display_name?.[0]?.toUpperCase() ?? me.data?.role?.[0]?.toUpperCase() ?? <UserCircle className="w-4 h-4" />}
        </div>
        <ChevronDown className={`w-3.5 h-3.5 text-slate-400 transition ${open ? "rotate-180" : ""}`} />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -8, scale: 0.97 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: -8, scale: 0.97 }}
            transition={{ duration: 0.15 }}
            className="absolute right-0 top-12 w-80 rounded-2xl p-4 z-50 shadow-2xl border border-white/10"
            style={{ background: "rgba(13, 13, 26, 0.97)", backdropFilter: "blur(20px)" }}
          >
            {/* Identity header */}
            <div className="flex items-center gap-3 mb-4">
              <div className="w-12 h-12 rounded-xl seal grid place-items-center text-lg font-bold">
                {profile.data?.display_name?.[0]?.toUpperCase() ?? "U"}
              </div>
              <div className="min-w-0 flex-1">
                <div className="font-semibold truncate">{profile.data?.display_name ?? "Unregistered"}</div>
                <button onClick={copyAddr} className="flex items-center gap-1 text-[10px] font-mono text-slate-500 hover:text-accent-glow transition">
                  {me.data?.address?.slice(0, 12)}…{me.data?.address?.slice(-6)}
                  <Copy className="w-2.5 h-2.5" />
                </button>
              </div>
            </div>

            {/* Status grid */}
            <div className="grid grid-cols-2 gap-2 mb-3">
              <StatusCard icon={KycIcon} label="KYC" value={kycLabel} valueClass={kycColor} />
              <StatusCard icon={Coins} label="Points" value={String(points.data?.points ?? 0)} valueClass="text-warn" />
              <StatusCard icon={UserCircle} label="Role" value={me.data?.role ?? "—"} />
              <StatusCard icon={FileText} label="Receipts" value={String(receipts.data?.receipts?.length ?? 0)} />
            </div>

            {/* Role types badges */}
            {profile.data?.role_types && profile.data.role_types.length > 0 && (
              <div className="mb-3">
                <div className="text-[10px] uppercase tracking-wider text-slate-500 mb-1.5">Roles</div>
                <div className="flex flex-wrap gap-1">
                  {profile.data.role_types.map((r: string) => (
                    <span key={r} className="px-2 py-0.5 rounded-full text-[10px] bg-accent/15 text-accent-glow border border-accent/20">{r}</span>
                  ))}
                </div>
              </div>
            )}

            {/* KYC detail */}
            <div className="glass-soft rounded-lg p-2.5 mb-3 flex items-center gap-2">
              <KycIcon className={`w-4 h-4 ${kycColor} shrink-0`} />
              <div className="flex-1 min-w-0">
                <div className="text-[10px] uppercase tracking-wider text-slate-500">KYC Status</div>
                <div className={`text-xs font-medium ${kycColor}`}>{kycLabel}</div>
              </div>
              {kyc.data?.attestation_hash && (
                <div className="text-[9px] font-mono text-slate-500 truncate max-w-[100px]" title={kyc.data.attestation_hash}>
                  {kyc.data.attestation_hash.slice(0, 16)}…
                </div>
              )}
            </div>

            {/* DID */}
            <div className="glass-soft rounded-lg p-2.5 mb-3">
              <div className="text-[10px] uppercase tracking-wider text-slate-500 mb-0.5">DID</div>
              <div className="text-[10px] font-mono text-slate-400 truncate">{me.data?.did}</div>
            </div>

            {/* Actions */}
            <div className="space-y-2 pt-2 border-t border-white/5">
              {needsKyc && (
                <button onClick={onEdit} className="btn btn-primary w-full text-xs flex items-center justify-center gap-1.5 animate-pulse-slow">
                  <ShieldCheck className="w-3.5 h-3.5" /> Complete KYC registration
                </button>
              )}
              <div className="flex gap-2">
                <button onClick={onEdit} className="btn btn-ghost flex-1 text-xs flex items-center justify-center gap-1.5">
                  <Settings className="w-3.5 h-3.5" /> Update profile
                </button>
                <button onClick={logout} className="btn btn-ghost flex-1 text-xs flex items-center justify-center gap-1.5 text-bad hover:bg-bad/10">
                  <LogOut className="w-3.5 h-3.5" /> Sign out
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function StatusCard({ icon: Icon, label, value, valueClass = "text-slate-200" }: {
  icon: any; label: string; value: string; valueClass?: string;
}) {
  return (
    <div className="glass-soft rounded-lg p-2.5">
      <div className="flex items-center gap-1.5 mb-1">
        <Icon className="w-3 h-3 text-slate-400" />
        <span className="text-[10px] uppercase tracking-wider text-slate-500">{label}</span>
      </div>
      <div className={`text-xs font-medium truncate ${valueClass}`}>{value}</div>
    </div>
  );
}
