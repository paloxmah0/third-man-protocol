import { useState } from "react";
import type { Agreement } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import { Scroll, X, Maximize2, FileSignature, CheckCircle2, Paperclip } from "lucide-react";

const RELEASE_LABELS: Record<string, string> = {
  mutual_confirm: "Mutual Confirm",
  oracle: "Automatic (Oracle)",
  timeout_to_dispute: "Timeout-to-Dispute",
  hybrid_arbiter: "Hybrid Arbiter",
};

/// Contract document — renders INLINE by default (full-width, always visible),
/// with a "expand to fullscreen" button for immersive reading.
export default function ContractViewer({ agreement, signatures }: { agreement: Agreement; signatures?: any[] }) {
  const [fullscreen, setFullscreen] = useState(false);
  const t = agreement.terms ?? {};
  const description = t.description ?? t.recitals ?? "";
  const milestones = t.milestones ?? [];
  const obligations = t.obligations ?? [];
  const attachments = t.attachments ?? [];
  const sigs = signatures ?? [];

  // Build sections — only include ones that have content, numbered sequentially
  const sections: { title: string; body: React.ReactNode }[] = [];

  // 1. Parties
  sections.push({
    title: "Parties",
    body: (
      <div className="text-sm space-y-1">
        <div><b>Party 1 (Initiator):</b> {agreement.author_id?.slice(0,18) ?? "—"}…</div>
        {(t.parties ?? []).filter((p: any) => p.address).slice(1).map((p: any, i: number) => (
          <div key={i}><b>Party {i+2}:</b> {p.address?.slice(0,18)}…{p.address?.slice(-6)}</div>
        ))}
      </div>
    ),
  });

  // 2. Recitals / Description
  if (description) {
    sections.push({
      title: "Recitals",
      body: <p className="text-sm leading-relaxed text-slate-300 italic">Whereas, {description}</p>,
    });
  }

  // 3. Terms
  sections.push({
    title: "Terms",
    body: (
      <div>
        <div className="text-sm space-y-1 mb-2">
          <div><b>Total value:</b> {(agreement.agreement_value / 1_000_000).toFixed(2)} ₳</div>
          <div><b>Release condition:</b> {RELEASE_LABELS[agreement.release_condition ?? "mutual_confirm"] ?? agreement.release_condition}</div>
          <div><b>Dispute window:</b> {agreement.dispute_window_days} days</div>
          {agreement.arbiter_fee_percent > 0 && (
            <div><b>Arbiter fee:</b> {agreement.arbiter_fee_percent}% paid by {agreement.arbiter_fee_paid_by}</div>
          )}
        </div>
        {milestones.length > 0 && (
          <div className="mt-3 space-y-2">
            <div className="text-xs uppercase tracking-wider text-slate-500">Milestones</div>
            {milestones.map((m: any, i: number) => (
              <div key={i} className="text-sm border-l-2 border-accent/20 pl-3">
                <div className="flex gap-3">
                  <span className="font-mono text-slate-500">M{i+1}</span>
                  <span className="flex-1">{m.label || "—"}</span>
                  <span className="text-accent-glow">{m.percent}%</span>
                  {m.due && <span className="text-slate-500 text-xs">due {m.due}</span>}
                </div>
                {m.deliverables && (
                  <div className="text-xs text-slate-400 mt-1 ml-7 italic">Deliverables: {m.deliverables}</div>
                )}
                {m.proof?.required && (
                  <div className="text-xs text-warn mt-1 ml-7">⚠ Proof required: {m.proof.label || m.proof.kind} (max {m.proof.max_attempts ?? 3} attempts)</div>
                )}
              </div>
            ))}
          </div>
        )}
        {attachments.length > 0 && (
          <div className="mt-3 space-y-1">
            <div className="text-xs uppercase tracking-wider text-slate-500 flex items-center gap-1"><Paperclip className="w-3 h-3" /> Exhibits (Attachments)</div>
            {attachments.map((a: any, i: number) => (
              <div key={i} className="text-sm flex items-center gap-2">
                <span className="font-mono text-slate-500">Exhibit {a.exhibit || String.fromCharCode(65+i)}:</span>
                <a href={a.url} target="_blank" rel="noreferrer" className="text-accent-cyan hover:underline">{a.filename}</a>
                <span className="text-[9px] font-mono text-accent-mint">{a.hash?.slice(0,16)}…</span>
              </div>
            ))}
          </div>
        )}
      </div>
    ),
  });

  // 4. Obligations
  if (obligations.length > 0) {
    sections.push({
      title: "Obligations",
      body: (
        <div className="space-y-1.5">
          {obligations.map((o: any, i: number) => (
            <div key={i} className="text-sm flex gap-2">
              <span className="text-accent-glow">•</span>
              <span><b>{o.party}:</b> {o.task || "—"}</span>
            </div>
          ))}
        </div>
      ),
    });
  }

  // 5. Collateral
  sections.push({
    title: "Collateral",
    body: (
      <div className="text-sm">
        Each party locks <b>{(agreement.collateral_amount / 1_000_000).toFixed(2)} ₳</b> as collateral
        (severity weight: {agreement.weight}/10). On successful completion, collateral is returned.
        On a fault or arbiter verdict, the at-fault party's collateral is slashed and paid to the counterparty.
      </div>
    ),
  });

  // 6. Signatures
  sections.push({
    title: "Signatures",
    body: (
      <div>
        <div className="grid grid-cols-2 gap-6 mt-4">
          {sigs.length === 0 ? (
            <div className="col-span-2 text-center text-sm text-slate-500 italic py-4">No signatures yet — be the first to sign.</div>
          ) : (
            sigs.map((s: any) => (
              <div key={s.user_id} className="border border-dashed border-white/20 rounded-lg p-4 text-center">
                <div className="seal w-10 h-10 rounded-full mx-auto mb-2 grid place-items-center">
                  <FileSignature className="w-4 h-4 text-white" />
                </div>
                <div className="text-xs text-accent-mint flex items-center justify-center gap-1">
                  <CheckCircle2 className="w-3 h-3" /> Signed
                </div>
                <div className="text-[10px] text-slate-500 mt-1 font-mono truncate">{s.payload_hash?.slice(0,18)}…</div>
                <div className="text-[10px] text-slate-500">{s.signed_at?.slice(0,19)}</div>
              </div>
            ))
          )}
        </div>
        <p className="text-[10px] text-slate-500 mt-4 italic">
          Wallet signature = legal signature (CIP-8 message signing). Deal goes "Active" once all required parties have signed.
        </p>
      </div>
    ),
  });

  const doc = (
    <div style={{ fontFamily: "'Georgia', 'Times New Roman', serif" }}>
      {/* Title header */}
      <div className="text-center mb-8 pb-6 border-b-2 border-accent/30">
        <div className="text-[10px] uppercase tracking-[0.3em] text-slate-500" style={{ fontFamily: "'Inter', sans-serif" }}>Third Man Protocol</div>
        <h1 className="text-2xl font-bold mt-2 text-slate-100">{agreement.title}</h1>
        <div className="flex justify-center gap-6 mt-3 text-xs text-slate-500" style={{ fontFamily: "'JetBrains Mono', monospace" }}>
          <span>Date: {agreement.created_at.slice(0,10)}</span>
          <span className="px-2 py-0.5 rounded-full bg-warn/15 text-warn uppercase tracking-wider" style={{ fontFamily: "'Inter', sans-serif" }}>{agreement.status}</span>
        </div>
      </div>

      {/* Numbered sections — always starts at 1 */}
      {sections.map((s, i) => (
        <div key={i} className="mb-6">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-accent-glow mb-2" style={{ fontFamily: "'Inter', sans-serif" }}>
            {i+1}. {s.title}
          </h2>
          <div style={{ fontFamily: "Georgia, serif" }}>{s.body}</div>
        </div>
      ))}

      {/* Footer */}
      <div className="mt-8 pt-6 border-t border-white/10 text-center text-[10px] text-slate-600" style={{ fontFamily: "'Inter', sans-serif" }}>
        This document is enforced by a smart contract on Cardano.
        Terms above are encoded on-chain and cannot be altered unilaterally after signing.
      </div>
      <div className="mt-4 text-[9px] font-mono text-slate-600 text-center" style={{ fontFamily: "'JetBrains Mono', monospace" }}>
        document hash: {agreement.terms_hash}
      </div>
    </div>
  );

  return (
    <>
      {/* INLINE — renders the full document right here in the page */}
      <div className="glass rounded-2xl p-6 sm:p-8 lg:p-10 relative">
        <button onClick={() => setFullscreen(true)}
          className="absolute top-4 right-4 text-slate-400 hover:text-accent-glow transition p-2 rounded-lg hover:bg-white/5"
          title="Open fullscreen">
          <Maximize2 className="w-4 h-4" />
        </button>
        {doc}
      </div>

      {/* FULLSCREEN — immersive reading mode */}
      <AnimatePresence>
        {fullscreen && (
          <motion.div
            initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
            className="fixed inset-0 z-50 bg-ink-900/95 backdrop-blur-md flex flex-col"
            onClick={() => setFullscreen(false)}
          >
            <motion.div
              initial={{ opacity: 0, scale: 0.98, y: 10 }} animate={{ opacity: 1, scale: 1, y: 0 }} exit={{ opacity: 0, scale: 0.98, y: 10 }}
              transition={{ type: "spring", damping: 24, stiffness: 300 }}
              className="w-full h-full flex flex-col"
              onClick={e => e.stopPropagation()}
            >
              <div className="flex-1 overflow-y-auto" style={{ background: "rgba(10, 10, 20, 0.98)" }}>
                <div className="sticky top-0 z-10 flex items-center justify-between px-8 py-4 border-b border-white/5" style={{ background: "rgba(10, 10, 20, 0.9)", backdropFilter: "blur(8px)" }}>
                  <div className="flex items-center gap-2">
                    <Scroll className="w-4 h-4 text-accent-glow" />
                    <span className="font-semibold text-sm" style={{ fontFamily: "'Inter', sans-serif" }}>Agreement Document</span>
                  </div>
                  <button onClick={() => setFullscreen(false)} className="text-slate-400 hover:text-white transition p-2 rounded-lg hover:bg-white/5">
                    <X className="w-5 h-5" />
                  </button>
                </div>
                <div className="min-h-full px-6 sm:px-12 lg:px-20 py-10">{doc}</div>
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}
