import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, useMe, useInvalidate, type Agreement } from "../lib/api";
import { motion } from "framer-motion";
import {
  Check, Clock, Upload, X, Loader2, CheckCircle2, AlertTriangle,
  FileText, Image as ImageIcon, Link as LinkIcon, Send, Gavel,
} from "lucide-react";

/// Milestone delivery + proof flow per spec:
/// - Escrow is Active → Party 2 sees milestones to deliver
/// - If proof required: Party 2 uploads proof → Party 1 reviews (accept/reject with reason)
/// - 3 rejections → disputed
/// - All milestones accepted → release available
export default function MilestoneDelivery({ agreement, wallet, onChange }: { agreement: Agreement; wallet: any; onChange: () => void }) {
  const { data: me } = useMe();
  const inv = useInvalidate();
  const milestones = useQuery({ queryKey: ["milestones", agreement.id], queryFn: () => api.milestones.list(agreement.id) });
  const isAuthor = me?.id === agreement.author_id;

  const allAccepted = (milestones.data?.milestones ?? []).length > 0 &&
    (milestones.data?.milestones ?? []).every((m: any) => m.delivery_status === "accepted" || (!m.proof_required && m.delivery_status !== "disputed"));

  return (
    <motion.div layout className="glass rounded-2xl p-5">
      <div className="flex items-center gap-2 mb-4">
        <CheckCircle2 className="w-4 h-4 text-accent-mint" />
        <h3 className="font-semibold text-sm uppercase tracking-wider">Milestone Delivery</h3>
      </div>

      <div className="space-y-3">
        {(milestones.data?.milestones ?? []).map((m: any, i: number) => (
          <MilestoneCard key={i} milestone={m} index={i} agreementId={agreement.id} isAuthor={isAuthor} wallet={wallet} onChange={() => { inv(["milestones"]); onChange(); }} />
        ))}
      </div>

      {allAccepted && (
        <div className="mt-5 p-4 rounded-xl bg-accent-mint/10 border border-accent-mint/30 text-center">
          <CheckCircle2 className="w-6 h-6 text-accent-mint mx-auto mb-2" />
          <p className="text-sm text-accent-mint font-medium">All milestones complete!</p>
          <p className="text-xs text-slate-400 mt-1">Funds can now be released from the escrow.</p>
        </div>
      )}

      {milestones.isLoading && <p className="text-sm text-slate-500">Loading milestones…</p>}
    </motion.div>
  );
}

function MilestoneCard({ milestone: m, index, agreementId, isAuthor, wallet, onChange }: {
  milestone: any; index: number; agreementId: string; isAuthor: boolean; wallet: any; onChange: () => void;
}) {
  const [showUpload, setShowUpload] = useState(false);
  const [proofUrl, setProofUrl] = useState("");
  const [proofName, setProofName] = useState("");
  const [proofHash, setProofHash] = useState("");
  const [rejectionReason, setRejectionReason] = useState("");
  const [showReview, setShowReview] = useState(false);
  const inv = useInvalidate();

  // Fetch attachments so the author can see the submitted proof file link
  const attachments = useQuery({
    queryKey: ["attachments", agreementId],
    queryFn: () => api.attachments.list(agreementId),
  });
  // Find proof attachments for this milestone
  const proofAttachments = (attachments.data?.attachments ?? []).filter(
    (a: any) => a.milestone_index === index && a.purpose === "proof"
  );

  const statusColor: Record<string, string> = {
    pending_delivery: "text-slate-400",
    pending_review: "text-warn",
    accepted: "text-accent-mint",
    rejected: "text-warn",
    disputed: "text-bad",
  };
  const StatusIcon = m.delivery_status === "accepted" ? CheckCircle2 :
                     m.delivery_status === "disputed" ? AlertTriangle :
                     m.delivery_status === "pending_review" ? Clock : FileText;

  const submitProof = useMutation({
    mutationFn: async () => {
      if (!proofUrl || !proofHash) throw new Error("Provide a proof link and hash the file first");
      // upload attachment record with the URL so the author can view the file
      const att = await api.attachments.upload({
        agreement_id: agreementId, milestone_index: index, filename: proofName || "proof",
        file_type: m.proof_kind || "image", content_hash: proofHash, purpose: "proof", label: m.proof_label,
        url: proofUrl,  // store the link (Drive/Dropbox/IPFS)
      });
      return api.proofs.submit({
        agreement_id: agreementId, milestone_index: index,
        attachment_id: att.id, attachment_hash: proofHash,
      });
    },
    onSuccess: () => { setShowUpload(false); setProofUrl(""); setProofName(""); setProofHash(""); inv(["milestones"]); onChange(); },
    onError: (e: any) => console.error("submit proof error:", e),
  });

  // For milestones without proof requirement — just mark as delivered
  const markDelivered = useMutation({
    mutationFn: async () => {
      // Create a minimal attachment record for the delivery
      const att = await api.attachments.upload({
        agreement_id: agreementId, milestone_index: index, filename: "delivery-confirmation",
        file_type: "document", content_hash: "delivered_" + Date.now(), purpose: "proof", label: "Delivery confirmed",
      });
      return api.proofs.submit({
        agreement_id: agreementId, milestone_index: index,
        attachment_id: att.id, attachment_hash: "delivered",
      });
    },
    onSuccess: () => { inv(["milestones"]); onChange(); },
    onError: (e: any) => console.error("mark delivered error:", e),
  });

  const reviewProof = useMutation({
    mutationFn: async (outcome: string) => {
      // Fetch the latest submission for this milestone
      const subs = await api.proofs.listSubmissions(agreementId);
      const milestoneSubs = subs.submissions.filter((s: any) => s.milestone_index === index);
      const latest = milestoneSubs[milestoneSubs.length - 1];
      if (!latest) throw new Error("no submission to review");
      return api.proofs.review({
        submission_id: latest.id,
        outcome,
        rejection_reason: outcome === "rejected" ? rejectionReason : undefined,
      });
    },
    onSuccess: () => { setShowReview(false); setRejectionReason(""); inv(["milestones"]); onChange(); },
    onError: (e: any) => console.error("review proof error:", e),
  });

  async function hashFile(f: File) {
    const buf = await f.arrayBuffer();
    const hash = await crypto.subtle.digest("SHA-256", buf);
    setProofHash(Array.from(new Uint8Array(hash)).map(b => b.toString(16).padStart(2, "0")).join(""));
    setProofName(f.name);
  }

  return (
    <div className="glass-soft rounded-xl p-4">
      <div className="flex items-start gap-3">
        <div className="w-8 h-8 rounded-lg bg-accent/15 grid place-items-center text-xs font-bold text-accent-glow shrink-0">M{index + 1}</div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-medium text-sm">{m.label || "Untitled milestone"}</span>
            <span className="text-accent-glow text-xs">{m.percent}%</span>
          </div>
          {m.deliverables && <p className="text-xs text-slate-400 mt-1 italic">{m.deliverables}</p>}
          {m.due && <p className="text-[10px] text-slate-500 mt-0.5">Due: {m.due}</p>}

          {/* Proof requirement */}
          {m.proof_required && (
            <div className="mt-2 flex items-center gap-1.5 text-xs text-warn">
              <AlertTriangle className="w-3 h-3" />
              Proof required: {m.proof_label || m.proof_kind}
              {m.max_attempts && <span className="text-slate-500">({m.rejection_count}/{m.max_attempts} rejections)</span>}
            </div>
          )}

          {/* Status */}
          <div className={`flex items-center gap-1.5 mt-2 text-xs ${statusColor[m.delivery_status] ?? "text-slate-400"}`}>
            <StatusIcon className="w-3 h-3" />
            <span className="capitalize">{(m.delivery_status || "pending").replace(/_/g, " ")}</span>
          </div>

          {/* Submission history */}
          {m.submissions?.length > 0 && (
            <div className="mt-2 space-y-1">
              {m.submissions.map((s: any, si: number) => (
                <div key={si} className="text-[10px] flex items-center gap-2">
                  <span className="text-slate-500">Attempt {si + 1}:</span>
                  <span className={s.outcome === "accepted" ? "text-accent-mint" : s.outcome === "rejected" ? "text-warn" : "text-slate-400"}>
                    {s.outcome}
                  </span>
                  {s.rejection_reason && <span className="text-slate-500 italic">— "{s.rejection_reason}"</span>}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Actions */}
      <div className="mt-3 pl-11">
        {/* Party 2 (non-author): deliver + upload proof */}
        {!isAuthor && m.delivery_status === "pending_delivery" && (
          <>
            {m.proof_required ? (
              <>
                {!showUpload ? (
                  <button onClick={() => setShowUpload(true)} className="btn btn-ghost text-xs flex items-center gap-1.5 text-accent-glow">
                    <Upload className="w-3.5 h-3.5" /> Submit proof
                  </button>
                ) : (
                  <div className="space-y-2">
                    <input className="input text-xs" value={proofUrl} onChange={e => setProofUrl(e.target.value)} placeholder="Link to proof file (Drive/Dropbox/IPFS)" />
                    <label className="flex items-center gap-1.5 border border-dashed border-white/15 rounded-lg px-3 py-1.5 cursor-pointer hover:border-accent/40 transition w-fit">
                      <Upload className="w-3 h-3 text-slate-500" />
                      <span className="text-[10px] text-slate-400">{proofName || "Hash file locally"}</span>
                      <input type="file" className="hidden" onChange={e => { const f = e.target.files?.[0]; if (f) hashFile(f); }} />
                    </label>
                    {proofHash && <span className="text-[9px] font-mono text-accent-mint block">sha256: {proofHash.slice(0, 24)}…</span>}
                    <div className="flex gap-2">
                      <button onClick={() => setShowUpload(false)} className="btn btn-ghost text-xs">Cancel</button>
                      <button onClick={() => submitProof.mutate()} disabled={submitProof.isPending || !proofUrl || !proofHash} className="btn btn-primary text-xs flex items-center gap-1.5">
                        {submitProof.isPending ? <Loader2 className="w-3 h-3 animate-spin" /> : <Send className="w-3 h-3" />}
                        Submit proof
                      </button>
                    </div>
                    {submitProof.isError && <p className="text-xs text-bad">{(submitProof.error as any).message}</p>}
                    {submitProof.isSuccess && <p className="text-xs text-accent-mint">Proof submitted — awaiting review.</p>}
                  </div>
                )}
              </>
            ) : (
              <button onClick={() => markDelivered.mutate()} disabled={markDelivered.isPending}
                className="btn btn-ghost text-xs flex items-center gap-1.5 text-accent-glow">
                {markDelivered.isPending ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Check className="w-3.5 h-3.5" />} Mark as delivered
              </button>
            )}
          </>
        )}
        {markDelivered.isError && <p className="text-xs text-bad mt-1">{(markDelivered.error as any).message}</p>}

        {/* Party 1 (author): review submitted proof */}
        {isAuthor && m.delivery_status === "pending_review" && (
          <>
            {/* Show the submitted proof file link */}
            {proofAttachments.length > 0 && (
              <div className="mb-2 p-2 rounded-lg glass-soft">
                <div className="text-[10px] uppercase tracking-wider text-slate-500 mb-1">Submitted proof files</div>
                {proofAttachments.map((a: any) => (
                  <div key={a.id} className="flex items-center gap-2 text-xs py-1">
                    {a.file_type === "image" ? <ImageIcon className="w-3.5 h-3.5 text-accent-glow" /> : <FileText className="w-3.5 h-3.5 text-accent-glow" />}
                    <span className="text-slate-300">{a.filename}</span>
                    {a.url && (
                      <a href={a.url} target="_blank" rel="noreferrer" className="text-accent-cyan hover:underline text-[10px]">
                        View file ↗
                      </a>
                    )}
                    <span className="text-[9px] font-mono text-accent-mint ml-auto">{a.content_hash?.slice(0,16)}…</span>
                  </div>
                ))}
              </div>
            )}
            {!showReview ? (
              <button onClick={() => setShowReview(true)} className="btn btn-ghost text-xs flex items-center gap-1.5 text-warn">
                <Clock className="w-3.5 h-3.5" /> Review submission
              </button>
            ) : (
              <div className="space-y-2">
                <textarea className="input text-xs min-h-[50px]" value={rejectionReason} onChange={e => setRejectionReason(e.target.value)}
                  placeholder="If rejecting: why is this insufficient? (mandatory)" />
                <div className="flex gap-2">
                  <button onClick={() => reviewProof.mutate("accepted")} disabled={reviewProof.isPending}
                    className="btn btn-primary text-xs flex items-center gap-1.5 text-accent-mint">
                    {reviewProof.isPending ? <Loader2 className="w-3 h-3 animate-spin" /> : <Check className="w-3 h-3" />}
                    Accept
                  </button>
                  <button onClick={() => { if (!rejectionReason.trim()) { alert("Rejection reason is mandatory."); return; } reviewProof.mutate("rejected"); }}
                    disabled={reviewProof.isPending} className="btn btn-ghost text-xs flex items-center gap-1.5 text-bad border-bad/30">
                    <X className="w-3 h-3" /> Reject
                  </button>
                  <button onClick={() => setShowReview(false)} className="btn btn-ghost text-xs">Cancel</button>
                </div>
                {m.rejection_count >= m.max_attempts - 1 && (
                  <p className="text-[10px] text-bad flex items-center gap-1">
                    <AlertTriangle className="w-3 h-3" /> This is the final attempt — next rejection triggers dispute.
                  </p>
                )}
                {reviewProof.isError && <p className="text-xs text-bad">{(reviewProof.error as any).message}</p>}
              </div>
            )}
          </>
        )}

        {/* Disputed */}
        {m.delivery_status === "disputed" && (
          <div className="p-2 rounded-lg bg-bad/10 border border-bad/30 flex items-center gap-1.5 text-xs text-bad">
            <Gavel className="w-3 h-3" /> Milestone disputed — arbiter review required
          </div>
        )}
      </div>
    </div>
  );
}
