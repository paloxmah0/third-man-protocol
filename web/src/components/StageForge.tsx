import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, useMe, useInvalidate, type Agreement } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import {
  ArrowLeft, Loader2, ShieldCheck, Plus, Trash2, ChevronRight, ChevronLeft,
  Scroll, Coins, Scale, Gavel, Clock, Eye, Check, PenLine, Sparkles,
  UserPlus, Link2, Users, FileText, Calendar, Upload, X, Paperclip, ShieldQuestion,
} from "lucide-react";

const TEMPLATES = [
  { id: "one_time",   icon: Coins,    title: "One-time payment", desc: "Pay someone for goods or services, once" },
  { id: "split",      icon: Users,    title: "Payment split",     desc: "Split a total among several people" },
  { id: "gradual",    icon: Calendar, title: "Gradual release",   desc: "Funds unlock over time on a schedule" },
  { id: "recurring",  icon: Clock,    title: "Recurring payment", desc: "Regular payments on a cycle" },
  { id: "custom",     icon: Sparkles, title: "Custom",            desc: "Build from scratch, full control" },
];

const DISPUTE_OPTIONS = [
  { id: "mutual_confirm",     icon: Check,  title: "Mutual confirm",            desc: "Both sides manually confirm completion before funds release" },
  { id: "oracle",            icon: Clock,  title: "Automatic (oracle)",       desc: "A trusted outside check confirms delivery automatically" },
  { id: "timeout_to_dispute", icon: Clock, title: "Timeout-to-dispute",        desc: "Funds release after a waiting period if no one objects" },
  { id: "hybrid_arbiter",    icon: Gavel,  title: "Hybrid arbiter",            desc: "Mutual confirm, but a neutral third party steps in on disagreement" },
];

interface Party { label: string; address: string; }
interface Milestone { label: string; percent: number; due: string; deliverables: string; proofRequired: boolean; proofKind: string; proofLabel: string; }
interface Recipient { label: string; address: string; percent: number; }
interface AttachmentItem { filename: string; file_type: string; content_hash: string; file_size?: number; url: string; }

/// Agreement drafting — 7-step guided flow per spec.
/// Template chooser → plain-language description → add parties → adaptive terms →
/// dispute resolution → live document preview → send for signing.
export default function StageForge({ onDone, onBack }: { onDone: (a: Agreement) => void; onBack: () => void }) {
  const me = useMe();
  const inv = useInvalidate();
  const [step, setStep] = useState(1);

  // Step 1
  const [template, setTemplate] = useState("one_time");

  // Step 2
  const [description, setDescription] = useState("");

  // Step 3
  const [parties, setParties] = useState<Party[]>([{ label: "Party 2", address: "" }]);
  const [showInvite, setShowInvite] = useState(false);

  // Step 4 — adaptive terms
  const [totalValue, setTotalValue] = useState(0);
  const [currency, setCurrency] = useState("ADA");
  const [recipients, setRecipients] = useState<Recipient[]>([{ label: "Party 2", address: "", percent: 100 }]);
  const [milestones, setMilestones] = useState<Milestone[]>([{ label: "Full delivery", percent: 100, due: "", deliverables: "", proofRequired: false, proofKind: "image", proofLabel: "" }]);
  const [recurringAmount, setRecurringAmount] = useState(0);
  const [recurringFreq, setRecurringFreq] = useState("monthly");
  const [recurringCount, setRecurringCount] = useState(0); // 0 = forever
  const [weight, setWeight] = useState(3);

  // Attachments (supporting material) — hash computed client-side, file never uploaded
  const [attachments, setAttachments] = useState<AttachmentItem[]>([]);

  // Step 5
  const [dispute, setDispute] = useState("mutual_confirm");
  const [disputeWindow, setDisputeWindow] = useState(7);
  const [arbiterFee, setArbiterFee] = useState(0);
  const [arbiterFeeBy, setArbiterFeeBy] = useState("party1");

  // Collateral gauge
  const BASE = 2_000_000, BPS = 500, CAP = 20_000_000;
  const valueLovelace = currency === "ADA" ? totalValue * 1_000_000 : totalValue;
  const collateral = Math.min(CAP, Math.max(BASE, BASE + (valueLovelace * BPS / 10000) * weight));

  // Build the structured terms JSON from the chosen template
  const terms = {
    template,
    description,
    parties: [{ label: "Party 1 (initiator)", address: me.data?.address ?? "" }, ...parties.filter(p => p.address)],
    value: { total: totalValue, currency },
    attachments: attachments.map((a, i) => ({ filename: a.filename, type: a.file_type, hash: a.content_hash, url: a.url, exhibit: String.fromCharCode(65+i) })),
    ...(template === "split" ? { recipients } : {}),
    ...((template === "gradual" || template === "custom" || template === "one_time") ? {
      milestones: milestones.map((m, i) => ({
        ...m,
        index: i,
        deliverables: m.deliverables,
        proof: m.proofRequired ? { required: true, kind: m.proofKind, label: m.proofLabel, max_attempts: 3 } : undefined,
      })),
    } : {}),
    ...(template === "recurring" ? { recurring: { amount: recurringAmount, frequency: recurringFreq, count: recurringCount } } : {}),
    obligations: [
      { party: "Party 1", task: "Deposit funds to escrow within 24h of signing" },
      { party: "Party 2", task: "Deliver per scope by the agreed date" },
    ],
  };

  const totalRecipientsPct = recipients.reduce((s, r) => s + r.percent, 0);

  const create = useMutation({
    mutationFn: () => api.agreements.create({
      title: description || "Untitled Agreement",
      terms, weight, agreement_value: valueLovelace, max_participants: parties.length + 1,
      release_condition: dispute, dispute_window_days: disputeWindow,
      arbiter_fee_percent: arbiterFee, arbiter_fee_paid_by: arbiterFeeBy,
    }),
    onSuccess: (a) => { inv(["agreements"]); onDone(a); },
  });

  return (
    <div className="max-w-4xl py-4">
      <button onClick={onBack} className="text-xs text-slate-500 hover:text-slate-300 mb-3 flex items-center gap-1">
        <ArrowLeft className="w-3 h-3" /> back
      </button>

      {/* Stepper */}
      <div className="flex items-center gap-1 mb-6 overflow-x-auto">
        {["Template", "Describe", "Parties", "Terms", "Disputes", "Preview", "Send"].map((s, i) => (
          <div key={s} className="flex items-center shrink-0">
            <div className={`w-7 h-7 rounded-full grid place-items-center text-xs font-bold border-2 transition
              ${step === i+1 ? "border-accent bg-accent/15 text-accent-glow" : step < i+1 ? "border-slate-700 text-slate-600" : "border-accent-mint bg-accent-mint/15 text-accent-mint"}`}>
              {step < i+1 ? i+1 : <Check className="w-3.5 h-3.5" />}
            </div>
            <span className={`text-xs ml-1.5 hidden sm:inline ${step === i+1 ? "text-accent-glow" : step < i+1 ? "text-slate-600" : "text-accent-mint"}`}>{s}</span>
            {i < 6 && <div className={`w-4 sm:w-8 h-px mx-1 ${step > i+1 ? "bg-accent-mint/40" : "bg-slate-700"}`} />}
          </div>
        ))}
      </div>

      <AnimatePresence mode="wait">
        {/* STEP 1 — Template chooser */}
        {step === 1 && (
          <motion.div key="s1" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <h2 className="text-lg font-semibold mb-1">What are you trying to do?</h2>
            <p className="text-sm text-slate-400 mb-5">Pick a starting point — it just pre-fills defaults, nothing is locked in.</p>
            <div className="grid sm:grid-cols-2 gap-3">
              {TEMPLATES.map(t => {
                const Icon = t.icon;
                return (
                  <button key={t.id} onClick={() => { setTemplate(t.id); setStep(2); }}
                    className={`glass rounded-xl p-4 text-left transition hover:border-accent/40 ${template === t.id ? "border-accent bg-accent/10" : "border-white/10"}`}>
                    <div className="flex items-center gap-3 mb-2">
                      <div className={`w-9 h-9 rounded-lg grid place-items-center ${template === t.id ? "seal" : "bg-ink-600"}`}>
                        <Icon className="w-4 h-4 text-white" />
                      </div>
                      <span className="font-medium">{t.title}</span>
                    </div>
                    <p className="text-xs text-slate-500">{t.desc}</p>
                  </button>
                );
              })}
            </div>
          </motion.div>
        )}

        {/* STEP 2 — Plain-language description */}
        {step === 2 && (
          <motion.div key="s2" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <h2 className="text-lg font-semibold mb-1">Describe the deal in plain language</h2>
            <p className="text-sm text-slate-400 mb-5">A sentence or two about what this agreement is for. This becomes the opening line of the final document.</p>
            <textarea className="input min-h-[120px] text-base" value={description} onChange={e => setDescription(e.target.value)}
              placeholder="e.g. I'm hiring Jane to redesign our company website — 5 pages, responsive, with a CMS. She gets paid in ADA when it's done." />
            <NavButtons step={step} setStep={setStep} nextDisabled={!description} />
          </motion.div>
        )}

        {/* STEP 3 — Add parties */}
        {step === 3 && (
          <motion.div key="s3" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <h2 className="text-lg font-semibold mb-1">Add the other parties</h2>
            <p className="text-sm text-slate-400 mb-5">Enter their wallet address, or generate a shareable invite link if they don't have a wallet ready.</p>
            <div className="space-y-3">
              {parties.map((p, i) => (
                <div key={i} className="glass-soft rounded-xl p-4 flex items-center gap-3">
                  <div className="w-10 h-10 rounded-full bg-accent/15 grid place-items-center text-accent-glow font-bold">{i+2}</div>
                  <div className="flex-1">
                    <input className="input" value={p.address} onChange={e => setParties(parties.map((x,j) => j===i ? {...x, address: e.target.value} : x))}
                      placeholder="Wallet address (addr1q...) or handle" />
                  </div>
                  {parties.length > 1 && <button onClick={() => setParties(parties.filter((_,j) => j!==i))} className="text-slate-500 hover:text-bad"><Trash2 className="w-4 h-4" /></button>}
                </div>
              ))}
            </div>
            <div className="flex gap-2 mt-4">
              <button onClick={() => setParties([...parties, { label: `Party ${parties.length+2}`, address: "" }])}
                className="btn btn-ghost text-xs flex items-center gap-1.5"><Plus className="w-3.5 h-3.5" /> Add another party</button>
              <button onClick={() => setShowInvite(!showInvite)} className="btn btn-ghost text-xs flex items-center gap-1.5 text-accent-glow">
                <Link2 className="w-3.5 h-3.5" /> Generate invite link instead
              </button>
            </div>
            {showInvite && (
              <div className="mt-3 glass-soft rounded-lg p-3 text-xs text-slate-400">
                An OTP invite link will be generated after you save the draft. The counterparty opens it, connects their wallet, and joins the agreement.
              </div>
            )}
            <NavButtons step={step} setStep={setStep} />
          </motion.div>
        )}

        {/* STEP 4 — Adaptive terms */}
        {step === 4 && (
          <motion.div key="s4" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <h2 className="text-lg font-semibold mb-1">Set the terms</h2>
            <p className="text-sm text-slate-400 mb-5">
              {template === "one_time" && "Enter the single amount and who it goes to."}
              {template === "split" && "Add recipients and how the total splits between them."}
              {template === "gradual" && "Set a schedule — dates and how much unlocks at each one."}
              {template === "recurring" && "Set the recurring amount, frequency, and duration."}
              {template === "custom" && "Configure the value, milestones, and any custom terms."}
            </p>

            {/* Common: total value + currency */}
            <div className="grid grid-cols-3 gap-3 mb-4">
              <div className="col-span-2">
                <label className="label">Total value</label>
                <input type="number" className="input" value={totalValue} onChange={e => setTotalValue(+e.target.value)} />
              </div>
              <div>
                <label className="label">Currency</label>
                <select className="input" value={currency} onChange={e => setCurrency(e.target.value)}>
                  <option>ADA</option><option>Stablecoin</option>
                </select>
              </div>
            </div>

            {/* Split: recipients with percentages */}
            {template === "split" && (
              <div className="space-y-2 mb-4">
                <label className="label">Recipients & split</label>
                {recipients.map((r, i) => (
                  <div key={i} className="flex gap-2 items-center">
                    <input className="input flex-1" value={r.address} onChange={e => setRecipients(recipients.map((x,j) => j===i ? {...x, address: e.target.value} : x))} placeholder="addr1q…" />
                    <input type="number" className="input w-20" value={r.percent} onChange={e => setRecipients(recipients.map((x,j) => j===i ? {...x, percent: +e.target.value} : x))} />
                    <span className="text-xs text-slate-500">%</span>
                    {recipients.length > 1 && <button onClick={() => setRecipients(recipients.filter((_,j) => j!==i))} className="text-slate-500 hover:text-bad"><Trash2 className="w-4 h-4" /></button>}
                  </div>
                ))}
                <button onClick={() => setRecipients([...recipients, { label: "", address: "", percent: 0 }])} className="text-xs text-accent-glow flex items-center gap-1"><Plus className="w-3 h-3" /> Add recipient</button>
                <div className={`text-xs font-mono ${totalRecipientsPct === 100 ? "text-accent-mint" : "text-warn"}`}>
                  Total: {totalRecipientsPct}% {totalRecipientsPct === 100 ? "✓" : "— must add up to 100%"}
                </div>
              </div>
            )}

            {/* Gradual/Custom: milestones with dates + proof requirements */}
            {(template === "gradual" || template === "custom" || template === "one_time") && (
              <div className="space-y-3 mb-4">
                <label className="label">Milestones {template === "one_time" && "(release schedule)"}</label>
                {milestones.map((m, i) => (
                  <div key={i} className="glass-soft rounded-lg p-3 space-y-2">
                    <div className="flex gap-2 items-center">
                      <span className="text-[10px] font-mono text-slate-500 w-6">M{i+1}</span>
                      <input className="input flex-1" value={m.label} onChange={e => setMilestones(milestones.map((x,j) => j===i ? {...x, label: e.target.value} : x))} placeholder="Milestone label" />
                      <input type="number" className="input w-16" value={m.percent} onChange={e => setMilestones(milestones.map((x,j) => j===i ? {...x, percent: +e.target.value} : x))} />
                      <span className="text-xs text-slate-500">%</span>
                      <input type="date" className="input w-36" value={m.due} onChange={e => setMilestones(milestones.map((x,j) => j===i ? {...x, due: e.target.value} : x))} />
                      {milestones.length > 1 && <button onClick={() => setMilestones(milestones.filter((_,j) => j!==i))} className="text-slate-500 hover:text-bad"><Trash2 className="w-4 h-4" /></button>}
                    </div>
                    {/* Deliverables — what must be completed within this milestone */}
                    <div className="pl-8">
                      <input className="input text-xs min-h-[50px] resize-y" value={m.deliverables} onChange={e => setMilestones(milestones.map((x,j) => j===i ? {...x, deliverables: e.target.value} : x))} placeholder="What must be completed/delivered in this milestone? e.g. Design mockups approved, 5 pages built, CMS integrated" />
                    </div>
                    {/* Proof requirement toggle */}
                    <div className="pl-8">
                      <label className="flex items-center gap-2 cursor-pointer">
                        <input type="checkbox" checked={m.proofRequired} onChange={e => setMilestones(milestones.map((x,j) => j===i ? {...x, proofRequired: e.target.checked} : x))} className="accent-accent" />
                        <span className="text-xs text-slate-400 flex items-center gap-1"><ShieldQuestion className="w-3 h-3" /> Require proof before this releases</span>
                      </label>
                      {m.proofRequired && (
                        <div className="flex gap-2 mt-2">
                          <select className="input w-28 text-xs" value={m.proofKind} onChange={e => setMilestones(milestones.map((x,j) => j===i ? {...x, proofKind: e.target.value} : x))}>
                            <option value="image">Image</option><option value="document">Document</option><option value="link">Link</option>
                          </select>
                          <input className="input flex-1 text-xs" value={m.proofLabel} onChange={e => setMilestones(milestones.map((x,j) => j===i ? {...x, proofLabel: e.target.value} : x))} placeholder="e.g. Photo of completed work" />
                        </div>
                      )}
                    </div>
                  </div>
                ))}
                <button onClick={() => setMilestones([...milestones, { label: "", percent: 0, due: "", deliverables: "", proofRequired: false, proofKind: "image", proofLabel: "" }])} className="text-xs text-accent-glow flex items-center gap-1"><Plus className="w-3 h-3" /> Add milestone</button>
              </div>
            )}

            {/* Attachments — supporting material (spec sheets, briefs, etc.) */}
            <div className="space-y-2 mb-4">
              <label className="label flex items-center gap-1.5"><Paperclip className="w-3.5 h-3.5" /> Attachments (supporting material)</label>
              <p className="text-[11px] text-slate-500 mb-2">
                Add a link to your file (Google Drive, Dropbox, IPFS, etc.) + upload a local copy to generate a tamper-proof hash.
                The other party sees the link to view the file + the hash to verify it wasn't swapped. Files are never stored on our servers.
              </p>
              {attachments.length > 0 && (
                <div className="space-y-1.5">
                  {attachments.map((a, i) => (
                    <div key={i} className="flex items-center gap-2 glass-soft rounded-lg p-2">
                      <FileText className="w-4 h-4 text-accent-glow shrink-0" />
                      <div className="flex-1 min-w-0">
                        <div className="text-xs truncate">{a.filename}</div>
                        <a href={a.url} target="_blank" rel="noreferrer" className="text-[10px] text-accent-cyan hover:underline truncate block">{a.url}</a>
                      </div>
                      <span className="text-[9px] font-mono text-accent-mint shrink-0">{a.content_hash.slice(0,12)}…</span>
                      <button onClick={() => setAttachments(attachments.filter((_,j) => j!==i))} className="text-slate-500 hover:text-bad"><X className="w-3.5 h-3.5" /></button>
                    </div>
                  ))}
                </div>
              )}
              <div className="space-y-2 glass-soft rounded-lg p-3">
                <input className="input text-xs" id="att-url" placeholder="Paste link to file (https://drive.google.com/... or ipfs://...)" />
                <div className="flex gap-2">
                  <input className="input text-xs" id="att-name" placeholder="Filename (e.g. design-brief.pdf)" />
                  <label className="flex items-center gap-1.5 border-2 border-dashed border-white/15 rounded-lg px-3 py-1.5 cursor-pointer hover:border-accent/40 transition shrink-0">
                    <Upload className="w-3.5 h-3.5 text-slate-500" />
                    <span className="text-[10px] text-slate-400 whitespace-nowrap">Hash locally</span>
                    <input type="file" className="hidden" onChange={async e => {
                      const f = e.target.files?.[0];
                      if (!f) return;
                      const urlEl = document.getElementById("att-url") as HTMLInputElement;
                      const nameEl = document.getElementById("att-name") as HTMLInputElement;
                      const url = urlEl?.value?.trim();
                      if (!url) { alert("Paste a link first so the other party can view the file."); return; }
                      const buf = await f.arrayBuffer();
                      const hash = await crypto.subtle.digest("SHA-256", buf);
                      const hexHash = Array.from(new Uint8Array(hash)).map(b => b.toString(16).padStart(2,"0")).join("");
                      setAttachments([...attachments, {
                        filename: nameEl?.value?.trim() || f.name,
                        file_type: f.type.startsWith("image/") ? "image" : "document",
                        content_hash: hexHash, file_size: f.size, url,
                      }]);
                      if (urlEl) urlEl.value = "";
                      if (nameEl) nameEl.value = "";
                    }} />
                  </label>
                </div>
              </div>
            </div>

            {/* Recurring */}
            {template === "recurring" && (
              <div className="grid grid-cols-3 gap-3 mb-4">
                <div>
                  <label className="label">Amount per cycle</label>
                  <input type="number" className="input" value={recurringAmount} onChange={e => setRecurringAmount(+e.target.value)} />
                </div>
                <div>
                  <label className="label">Frequency</label>
                  <select className="input" value={recurringFreq} onChange={e => setRecurringFreq(e.target.value)}>
                    <option value="weekly">Weekly</option><option value="monthly">Monthly</option><option value="quarterly">Quarterly</option>
                  </select>
                </div>
                <div>
                  <label className="label"># of payments (0 = forever)</label>
                  <input type="number" className="input" value={recurringCount} onChange={e => setRecurringCount(+e.target.value)} />
                </div>
              </div>
            )}

            {/* Weight slider */}
            <div className="mb-4">
              <div className="flex justify-between mb-1.5">
                <span className="label mb-0">Severity weight (scales collateral)</span>
                <span className="text-xs font-mono text-accent-glow">{weight}/10</span>
              </div>
              <input type="range" min={1} max={10} value={weight} onChange={e => setWeight(+e.target.value)} className="w-full accent-accent" />
            </div>

            {/* Collateral preview */}
            <div className="p-4 rounded-xl bg-gradient-to-br from-accent/15 to-accent-cyan/10 border border-accent/20">
              <div className="text-[10px] uppercase tracking-wider text-accent-glow">Collateral per party</div>
              <div className="text-2xl font-bold font-mono mt-1">{(collateral / 1_000_000).toFixed(2)} ₳</div>
            </div>

            <NavButtons step={step} setStep={setStep} />
          </motion.div>
        )}

        {/* STEP 5 — Dispute resolution */}
        {step === 5 && (
          <motion.div key="s5" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <h2 className="text-lg font-semibold mb-1">How should disputes get resolved?</h2>
            <p className="text-sm text-slate-400 mb-5">Each option shows what happens in practice. Pick the one that fits your deal.</p>
            <div className="space-y-2 mb-4">
              {DISPUTE_OPTIONS.map(d => {
                const Icon = d.icon;
                return (
                  <button key={d.id} onClick={() => setDispute(d.id)}
                    className={`w-full glass rounded-xl p-4 text-left transition flex items-start gap-3
                      ${dispute === d.id ? "border-accent bg-accent/10" : "border-white/10 hover:border-white/20"}`}>
                    <div className={`w-9 h-9 rounded-lg grid place-items-center shrink-0 ${dispute === d.id ? "seal" : "bg-ink-600"}`}>
                      <Icon className="w-4 h-4 text-white" />
                    </div>
                    <div className="flex-1">
                      <div className="font-medium text-sm flex items-center gap-2">{d.title} {dispute === d.id && <Check className="w-3.5 h-3.5 text-accent-mint" />}</div>
                      <div className="text-xs text-slate-500 mt-0.5">{d.desc}</div>
                    </div>
                  </button>
                );
              })}
            </div>
            <div className="grid grid-cols-2 gap-3 mb-4">
              <div>
                <label className="label">Dispute window (days)</label>
                <input type="number" className="input" value={disputeWindow} onChange={e => setDisputeWindow(+e.target.value)} />
              </div>
              {dispute === "hybrid_arbiter" && (
                <>
                  <div>
                    <label className="label">Arbiter fee (%)</label>
                    <input type="number" className="input" value={arbiterFee} onChange={e => setArbiterFee(+e.target.value)} />
                  </div>
                  <div>
                    <label className="label">Fee paid by</label>
                    <select className="input" value={arbiterFeeBy} onChange={e => setArbiterFeeBy(e.target.value)}>
                      <option value="party1">Party 1</option><option value="party2">Party 2</option><option value="split">Split</option>
                    </select>
                  </div>
                </>
              )}
            </div>
            <NavButtons step={step} setStep={setStep} />
          </motion.div>
        )}

        {/* STEP 6 — Preview document */}
        {step === 6 && (
          <motion.div key="s6" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <h2 className="text-lg font-semibold mb-1">Preview the document</h2>
            <p className="text-sm text-slate-400 mb-5">This is exactly what the other parties will see. Read it top to bottom before sending.</p>
            <DocPreview
              title={description || "Untitled Agreement"}
              date={new Date().toISOString().slice(0,10)}
              status="Draft"
              initiator={me.data?.address ?? ""}
              parties={parties.filter(p => p.address)}
              description={description}
              template={template}
              totalValue={totalValue}
              currency={currency}
              recipients={template === "split" ? recipients : undefined}
              milestones={milestones}
              attachments={attachments}
              recurring={template === "recurring" ? { amount: recurringAmount, frequency: recurringFreq, count: recurringCount } : undefined}
              dispute={dispute}
              disputeWindow={disputeWindow}
              arbiterFee={arbiterFee}
              arbiterFeeBy={arbiterFeeBy}
              collateral={collateral}
              weight={weight}
            />
            <NavButtons step={step} setStep={setStep} nextLabel="Continue to send" />
          </motion.div>
        )}

        {/* STEP 7 — Send for signing */}
        {step === 7 && (
          <motion.div key="s7" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }} className="text-center py-10">
            <div className="w-16 h-16 rounded-2xl seal grid place-items-center mx-auto mb-4">
              <PenLine className="w-8 h-8 text-white" />
            </div>
            <h2 className="text-xl font-semibold">Ready to send for signing</h2>
            <p className="text-sm text-slate-400 mt-2 max-w-md mx-auto">
              The document goes to the other parties. They can read it, sign it as-is, or propose a change.
              Nothing becomes binding until every required party has signed.
            </p>
            <div className="mt-6 flex justify-center gap-3">
              <button onClick={() => setStep(6)} className="btn btn-ghost flex items-center gap-1.5"><ChevronLeft className="w-4 h-4" /> Back to preview</button>
              <button onClick={() => create.mutate()} disabled={create.isPending}
                className="btn btn-primary flex items-center gap-2">
                {create.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <ShieldCheck className="w-4 h-4" />}
                {create.isPending ? "Saving draft…" : "Save & send for signing"}
              </button>
            </div>
            {create.isError && <p className="text-xs text-bad mt-3">{(create.error as any).message}</p>}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

// ---- Document preview (renders like a real contract) ----
function DocPreview(props: any) {
  const DISPUTE_LABELS: Record<string, string> = {
    mutual_confirm: "Mutual Confirm", oracle: "Automatic (Oracle)",
    timeout_to_dispute: "Timeout-to-Dispute", hybrid_arbiter: "Hybrid Arbiter",
  };
  const DISPUTE_DESC: Record<string, string> = {
    mutual_confirm: "Both sides manually confirm completion before funds release.",
    oracle: "A trusted outside check confirms delivery automatically.",
    timeout_to_dispute: "Funds release after a waiting period if no one objects.",
    hybrid_arbiter: "Mutual confirm, with a neutral third party stepping in on disagreement.",
  };

  return (
    <div className="glass rounded-2xl p-6 sm:p-8 lg:p-10" style={{ fontFamily: "'Georgia', 'Times New Roman', serif" }}>
      <div className="text-center mb-8 pb-6 border-b-2 border-accent/30">
        <div className="text-[10px] uppercase tracking-[0.3em] text-slate-500" style={{ fontFamily: "'Inter', sans-serif" }}>Third Man Protocol</div>
        <h1 className="text-2xl font-bold mt-2 text-slate-100">{props.title}</h1>
        <div className="flex justify-center gap-6 mt-3 text-xs text-slate-500" style={{ fontFamily: "'JetBrains Mono', monospace" }}>
          <span>Date: {props.date}</span>
          <span className="px-2 py-0.5 rounded-full bg-warn/15 text-warn uppercase tracking-wider" style={{ fontFamily: "'Inter', sans-serif" }}>{props.status}</span>
        </div>
      </div>

      <DocSec n="1" title="Parties">
        <div className="text-sm space-y-1">
          <div><b>Party 1 (Initiator):</b> {props.initiator.slice(0,18)}…{props.initiator.slice(-6)}</div>
          {props.parties.map((p: Party, i: number) => (
            <div key={i}><b>Party {i+2}:</b> {p.address.slice(0,18)}…{p.address.slice(-6)}</div>
          ))}
        </div>
      </DocSec>

      <DocSec n="2" title="Recitals">
        <p className="text-sm leading-relaxed text-slate-300 italic">{props.description}</p>
      </DocSec>

      <DocSec n="3" title="Terms">
        <div className="text-sm space-y-1">
          <div><b>Total value:</b> {props.totalValue} {props.currency}</div>
        </div>
        {props.recipients && (
          <div className="mt-3 space-y-1">
            <div className="text-xs uppercase tracking-wider text-slate-500" style={{ fontFamily: "'Inter', sans-serif" }}>Recipients</div>
            {props.recipients.map((r: Recipient, i: number) => (
              <div key={i} className="text-sm flex gap-3"><span className="flex-1">{r.address?.slice(0,18) ?? "—"}…</span><span className="text-accent-glow">{r.percent}%</span></div>
            ))}
          </div>
        )}
        {props.milestones && (
          <div className="mt-3 space-y-1">
            <div className="text-xs uppercase tracking-wider text-slate-500" style={{ fontFamily: "'Inter', sans-serif" }}>Milestones</div>
            {props.milestones.map((m: Milestone, i: number) => (
              <div key={i} className="text-sm flex gap-3"><span className="font-mono text-slate-500">M{i+1}</span><span className="flex-1">{m.label || "—"}</span><span className="text-accent-glow">{m.percent}%</span>{m.due && <span className="text-slate-500 text-xs">due {m.due}</span>}</div>
            ))}
          </div>
        )}
        {props.recurring && (
          <div className="mt-3 text-sm space-y-1">
            <div><b>Recurring:</b> {props.recurring.amount} {props.currency} {props.recurring.frequency}</div>
            <div><b>Duration:</b> {props.recurring.count === 0 ? "Ongoing (forever)" : `${props.recurring.count} payments`}</div>
          </div>
        )}
      </DocSec>

      <DocSec n="4" title="Dispute Resolution">
        <div className="text-sm space-y-1">
          <div><b>Method:</b> {DISPUTE_LABELS[props.dispute]}</div>
          <div className="text-xs text-slate-500">{DISPUTE_DESC[props.dispute]}</div>
          <div><b>Dispute window:</b> {props.disputeWindow} days</div>
          {props.arbiterFee > 0 && <div><b>Arbiter fee:</b> {props.arbiterFee}% paid by {props.arbiterFeeBy}</div>}
        </div>
      </DocSec>

      <DocSec n="5" title="Collateral">
        <div className="text-sm">
          Each party locks <b>{(props.collateral / 1_000_000).toFixed(2)} ₳</b> as collateral
          (severity weight: {props.weight}/10). On successful completion, collateral is returned.
          On a fault or arbiter verdict, the at-fault party's collateral is slashed.
        </div>
      </DocSec>

      <DocSec n="6" title="Signatures">
        <div className="grid grid-cols-2 gap-6 mt-4">
          <div className="border border-dashed border-white/20 rounded-lg p-4 text-center">
            <div className="text-xs text-slate-500 mb-3" style={{ fontFamily: "'Inter', sans-serif" }}>Party 1 (Initiator)</div>
            <div className="h-14 border-b border-slate-600 flex items-end justify-center pb-1">
              <span className="text-[10px] text-slate-600" style={{ fontFamily: "'Inter', sans-serif" }}>Awaiting signature…</span>
            </div>
          </div>
          <div className="border border-dashed border-white/20 rounded-lg p-4 text-center">
            <div className="text-xs text-slate-500 mb-3" style={{ fontFamily: "'Inter', sans-serif" }}>Party 2</div>
            <div className="h-14 border-b border-slate-600 flex items-end justify-center pb-1">
              <span className="text-[10px] text-slate-600" style={{ fontFamily: "'Inter', sans-serif" }}>Awaiting signature…</span>
            </div>
          </div>
        </div>
      </DocSec>

      <div className="mt-8 pt-6 border-t border-white/10 text-center text-[10px] text-slate-600" style={{ fontFamily: "'Inter', sans-serif" }}>
        This document is enforced by a smart contract on Cardano.
        Terms above are encoded on-chain and cannot be altered unilaterally after signing.
      </div>
    </div>
  );
}

// ---- Helpers ----
function NavButtons({ step, setStep, nextDisabled, nextLabel }: { step: number; setStep: (n: number) => void; nextDisabled?: boolean; nextLabel?: string }) {
  return (
    <div className="flex justify-between mt-6">
      <button onClick={() => setStep(step - 1)} className="btn btn-ghost flex items-center gap-1.5"><ChevronLeft className="w-4 h-4" /> Back</button>
      <button onClick={() => setStep(step + 1)} disabled={nextDisabled} className="btn btn-primary flex items-center gap-1.5">
        {nextLabel ?? "Continue"} <ChevronRight className="w-4 h-4" />
      </button>
    </div>
  );
}

function DocSec({ n, title, children }: { n: string; title: string; children: React.ReactNode }) {
  return (
    <div className="mb-6">
      <h2 className="text-sm font-semibold uppercase tracking-wider text-accent-glow mb-2" style={{ fontFamily: "'Inter', sans-serif" }}>{n}. {title}</h2>
      <div style={{ fontFamily: "Georgia, serif" }}>{children}</div>
    </div>
  );
}
