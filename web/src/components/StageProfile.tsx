import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, useMe, useInvalidate } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import {
  ShieldCheck, Loader2, UserCircle, BadgeCheck, CheckCircle2,
  Phone, IdCard, Upload, X, ChevronRight, ChevronLeft, Globe, Users, EyeOff,
} from "lucide-react";

const ROLE_OPTIONS = ["Developer", "Freelancer", "Trader", "Buyer", "Investor", "Service Provider", "Organization", "Supplier"];
const SETTLEMENT_OPTIONS = ["ADA", "Stablecoin", "M-Pesa off-ramp", "Mixed"];
const DEAL_SIZES = ["<$100", "$100–1k", "$1k–10k", "$10k+"];
const VISIBILITY = [
  { value: "public", label: "Public", icon: Globe },
  { value: "participants_only", label: "Participants", icon: Users },
  { value: "private", label: "Private", icon: EyeOff },
];

/// 4-step registration wizard per spec.
export default function StageProfile() {
  const me = useMe();
  const inv = useInvalidate();
  const profile = useQuery({ queryKey: ["profile"], queryFn: api.kyc.myProfile, retry: false });
  const kyc = useQuery({ queryKey: ["kyc"], queryFn: api.kyc.myKyc, retry: false });
  const [step, setStep] = useState(2);

  const hasProfile = !!profile.data;
  const kycTier = kyc.data?.tier ?? 0;

  return (
    <motion.div layout className="glass rounded-2xl p-6">
      <div className="flex items-center gap-3 mb-5">
        <div className="w-9 h-9 rounded-lg seal grid place-items-center">
          <UserCircle className="w-5 h-5 text-white" />
        </div>
        <div>
          <h2 className="font-semibold text-lg">Registration</h2>
          <p className="text-xs text-slate-400">Complete your identity to start forging agreements</p>
        </div>
      </div>

      {/* Stepper */}
      <div className="flex items-center gap-1 mb-6">
        {[
          { n: 1, label: "Wallet", done: !!me.data },
          { n: 2, label: "Profile", done: hasProfile },
          { n: 3, label: "KYC", done: kycTier >= 1 },
          { n: 4, label: "Privacy", done: !!profile.data?.privacy_prefs && Object.keys(profile.data.privacy_prefs).length > 0 },
        ].map((s: any, i: number) => (
          <div key={s.n} className="flex items-center flex-1">
            <button onClick={() => s.n <= step + 1 && setStep(s.n)}
              className={`flex items-center gap-2 ${s.n === step ? "text-accent-glow" : s.done ? "text-accent-mint" : "text-slate-500"}`}>
              <div className={`w-7 h-7 rounded-full grid place-items-center text-xs font-bold border-2 transition
                ${s.n === step ? "border-accent bg-accent/15" : s.done ? "border-accent-mint bg-accent-mint/15" : "border-slate-600"}`}>
                {s.done && s.n !== step ? <CheckCircle2 className="w-3.5 h-3.5" /> : s.n}
              </div>
              <span className="text-xs font-medium hidden sm:inline">{s.label}</span>
            </button>
            {i < 3 && <div className={`flex-1 h-px mx-2 ${s.done ? "bg-accent-mint/40" : "bg-slate-700"}`} />}
          </div>
        ))}
      </div>

      <AnimatePresence mode="wait">
        {step === 2 && (
          <motion.div key="profile" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <Step2Profile profile={profile.data} onDone={() => { inv(["profile"]); setStep(3); }} />
          </motion.div>
        )}
        {step === 3 && (
          <motion.div key="kyc" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <Step3Kyc kycData={kyc.data} userId={me.data?.id ?? ""} onDone={() => { inv(["kyc"]); setStep(4); }} onBack={() => setStep(2)} />
          </motion.div>
        )}
        {step === 4 && (
          <motion.div key="privacy" initial={{ opacity: 0, x: 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -20 }}>
            <Step4Privacy profile={profile.data} onDone={() => { inv(["profile"]); setStep(4); }} onBack={() => setStep(3)} />
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}

// ---- Step 2: Basic Profile ----
function Step2Profile({ profile, onDone }: { profile: any; onDone: () => void }) {
  const inv = useInvalidate();
  const [name, setName] = useState(profile?.display_name ?? "");
  const [avatar, setAvatar] = useState(profile?.avatar_url ?? "");
  const [location, setLocation] = useState(profile?.location ?? "");
  const [bio, setBio] = useState(profile?.bio ?? "");
  const [roles, setRoles] = useState<string[]>(profile?.role_types ?? []);
  const [langs, setLangs] = useState((profile?.languages ?? []).join(", "));
  const [links, setLinks] = useState("");
  const [rails, setRails] = useState<string[]>(profile?.settlement_rails ?? []);
  const [dealSize, setDealSize] = useState(profile?.deal_size_range ?? "");
  const [availability, setAvailability] = useState(profile?.availability ?? "");
  const [orgName, setOrgName] = useState(profile?.org_name ?? "");
  const [orgType, setOrgType] = useState(profile?.org_type ?? "");

  const toggle = (arr: string[], v: string) => arr.includes(v) ? arr.filter(x => x !== v) : [...arr, v];

  const submit = useMutation({
    mutationFn: () => api.kyc.submitProfile({
      display_name: name, avatar_url: avatar || undefined, location: location || undefined,
      bio: bio || undefined, role_types: roles, languages: langs.split(",").map((s: string) => s.trim()).filter(Boolean),
      professional_links: links ? [{ type: "url", url: links, visible: false }] : [],
      settlement_rails: rails, deal_size_range: dealSize || undefined, availability: availability || undefined,
      org_name: orgName || undefined, org_type: orgType || undefined,
    }),
    onSuccess: () => { inv(["profile", "me"]); onDone(); },
  });

  return (
    <div className="space-y-4">
      <div className="grid sm:grid-cols-2 gap-3">
        <div>
          <label className="label">Display name / handle *</label>
          <input className="input" value={name} onChange={e => setName(e.target.value)} placeholder="e.g. Alice" />
        </div>
        <div>
          <label className="label">Avatar URL</label>
          <input className="input" value={avatar} onChange={e => setAvatar(e.target.value)} placeholder="https://…" />
        </div>
      </div>

      <div className="grid sm:grid-cols-2 gap-3">
        <div>
          <label className="label">Location (city, country)</label>
          <input className="input" value={location} onChange={e => setLocation(e.target.value)} placeholder="Nairobi, Kenya" />
        </div>
        <div>
          <label className="label">Languages (comma-separated)</label>
          <input className="input" value={langs} onChange={e => setLangs(e.target.value)} placeholder="English, Swahili" />
        </div>
      </div>

      <div>
        <label className="label">Bio (~200 chars)</label>
        <textarea className="input min-h-[60px] resize-y" maxLength={200} value={bio} onChange={e => setBio(e.target.value)} placeholder="Short professional bio…" />
      </div>

      <div>
        <label className="label">Role types (multi-select)</label>
        <div className="flex flex-wrap gap-2">
          {ROLE_OPTIONS.map(r => (
            <button key={r} type="button" onClick={() => setRoles(toggle(roles, r))}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium border transition
                ${roles.includes(r) ? "border-accent bg-accent/15 text-accent-glow" : "border-white/10 text-slate-400 hover:border-white/30"}`}>
              {r}
            </button>
          ))}
        </div>
      </div>

      <div className="grid sm:grid-cols-2 gap-3">
        <div>
          <label className="label">Professional link (GitHub/LinkedIn/X)</label>
          <input className="input" value={links} onChange={e => setLinks(e.target.value)} placeholder="https://github.com/…" />
        </div>
        <div>
          <label className="label">Typical deal size</label>
          <select className="input" value={dealSize} onChange={e => setDealSize(e.target.value)}>
            <option value="">Select…</option>
            {DEAL_SIZES.map(d => <option key={d} value={d}>{d}</option>)}
          </select>
        </div>
      </div>

      <div>
        <label className="label">Preferred settlement rails</label>
        <div className="flex flex-wrap gap-2">
          {SETTLEMENT_OPTIONS.map(r => (
            <button key={r} type="button" onClick={() => setRails(toggle(rails, r))}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium border transition
                ${rails.includes(r) ? "border-accent bg-accent/15 text-accent-glow" : "border-white/10 text-slate-400 hover:border-white/30"}`}>
              {r}
            </button>
          ))}
        </div>
      </div>

      <div>
        <label className="label">Availability / response time</label>
        <input className="input" value={availability} onChange={e => setAvailability(e.target.value)} placeholder="e.g. within 24h" />
      </div>

      <details className="glass-soft rounded-lg p-3">
        <summary className="text-sm text-slate-300 cursor-pointer">Organization mode (optional)</summary>
        <div className="grid sm:grid-cols-2 gap-3 mt-3">
          <div><label className="label">Org name</label><input className="input" value={orgName} onChange={e => setOrgName(e.target.value)} /></div>
          <div>
            <label className="label">Org type</label>
            <select className="input" value={orgType} onChange={e => setOrgType(e.target.value)}>
              <option value="">Select…</option>
              <option>DAO</option><option>Registered entity</option><option>Informal collective</option><option>Solo</option>
            </select>
          </div>
        </div>
      </details>

      <button onClick={() => name && submit.mutate()} disabled={submit.isPending || !name}
        className="btn btn-primary w-full flex items-center justify-center gap-2">
        {submit.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <ShieldCheck className="w-4 h-4" />}
        Save & continue
      </button>
      {submit.isError && <p className="text-xs text-bad">{(submit.error as any).message}</p>}
    </div>
  );
}

// ---- Step 3: Tiered KYC with actual file upload fields ----
function Step3Kyc({ kycData, userId, onDone, onBack }: { kycData: any; userId: string; onDone: () => void; onBack: () => void }) {
  const inv = useInvalidate();
  const [tier, setTier] = useState(1);
  const [phone, setPhone] = useState(kycData?.phone ?? "");
  const [legalName, setLegalName] = useState(kycData?.legal_name ?? "");
  const [docType, setDocType] = useState("passport");

  // File upload state — hashes computed client-side, files never sent raw
  const [docFile, setDocFile] = useState<File | null>(null);
  const [selfieFile, setSelfieFile] = useState<File | null>(null);
  const [docHash, setDocHash] = useState("");
  const [selfieHash, setSelfieHash] = useState("");

  const submit = useMutation({
    mutationFn: async () => {
      if (tier === 1) {
        return api.kyc.submitKyc({ tier: 1, phone });
      } else {
        return api.kyc.submitKyc({
          tier: 2, legal_name: legalName, document_type: docType,
          document_hash: docHash || "hash_placeholder", selfie_hash: selfieHash || "hash_placeholder",
        });
      }
    },
    onSuccess: () => { inv(["kyc"]); },
  });
  const selfVerify = useMutation({
    mutationFn: () => api.kyc.verifyKyc(userId, tier === 1 ? "verified_t1" : "verified_t2"),
    onSuccess: () => { inv(["kyc"]); },
  });

  const isVerified = kycData?.status?.startsWith("verified");

  // hash a file using SHA-256 (browser SubtleCrypto), return hex
  async function hashFile(f: File, setter: (h: string) => void) {
    const buf = await f.arrayBuffer();
    const hash = await crypto.subtle.digest("SHA-256", buf);
    setter(Array.from(new Uint8Array(hash)).map(b => b.toString(16).padStart(2, "0")).join(""));
  }

  return (
    <div className="space-y-4">
      <div className="p-3 rounded-xl bg-accent/10 border border-accent/20 text-xs text-slate-300">
        KYC is <b>optional</b> and non-blocking. Tier 0 lets you transact with mutual-confirm.
        Higher tiers unlock M-Pesa ramp, higher deal caps, and oracle/arbiter-eligible deals.
        Document photos are <b>hashed locally</b> — only the hash is stored, never the raw file.
      </div>

      {/* Tier cards */}
      <div className="grid sm:grid-cols-3 gap-3">
        <TierCard n={0} title="Wallet only" desc="Default — mutual-confirm deals" active={tier === 0 && !isVerified} done={kycData?.tier === 0 && !isVerified} onClick={() => setTier(0)} />
        <TierCard n={1} title="Basic" desc="Phone + OTP → M-Pesa ramp" active={tier === 1} done={kycData?.status === "verified_t1"} onClick={() => setTier(1)} icon={Phone} />
        <TierCard n={2} title="Verified" desc="ID + selfie → higher caps, arbiter-eligible" active={tier === 2} done={kycData?.status === "verified_t2"} onClick={() => setTier(2)} icon={IdCard} />
      </div>

      {isVerified && (
        <div className="p-3 rounded-xl bg-accent-mint/10 border border-accent-mint/30 flex items-center gap-2">
          <BadgeCheck className="w-4 h-4 text-accent-mint" />
          <span className="text-sm text-accent-mint">Verified at Tier {kycData.tier}</span>
          {kycData.attestation_hash && <span className="text-[10px] font-mono text-slate-500 ml-auto truncate">{kycData.attestation_hash.slice(0,20)}…</span>}
        </div>
      )}

      {/* Tier 1: phone */}
      {tier === 1 && !isVerified && (
        <div>
          <label className="label">Phone number</label>
          <input className="input" value={phone} onChange={e => setPhone(e.target.value)} placeholder="+254…" />
        </div>
      )}

      {/* Tier 2: ID + selfie upload */}
      {tier === 2 && !isVerified && (
        <div className="space-y-4">
          <div className="grid sm:grid-cols-2 gap-3">
            <div>
              <label className="label">Legal name</label>
              <input className="input" value={legalName} onChange={e => setLegalName(e.target.value)} placeholder="As on your ID" />
            </div>
            <div>
              <label className="label">Document type</label>
              <select className="input" value={docType} onChange={e => setDocType(e.target.value)}>
                <option value="passport">Passport</option>
                <option value="national_id">National ID</option>
                <option value="drivers_license">Driver's License</option>
              </select>
            </div>
          </div>

          {/* ID document upload */}
          <FileUpload
            label="ID document photo"
            hint="Upload a clear photo of your passport/ID. Hashed locally — raw file never leaves your device."
            file={docFile}
            hash={docHash}
            onFile={(f) => { setDocFile(f); hashFile(f, setDocHash); }}
            onClear={() => { setDocFile(null); setDocHash(""); }}
          />

          {/* Selfie upload */}
          <FileUpload
            label="Selfie photo"
            hint="A selfie for identity match. Hashed locally."
            file={selfieFile}
            hash={selfieHash}
            onFile={(f) => { setSelfieFile(f); hashFile(f, setSelfieHash); }}
            onClear={() => { setSelfieFile(null); setSelfieHash(""); }}
          />
        </div>
      )}

      <div className="flex gap-2">
        <button onClick={onBack} className="btn btn-ghost flex items-center gap-1"><ChevronLeft className="w-4 h-4" /> Back</button>
        {tier > 0 && !isVerified && (
          <button onClick={() => submit.mutate()}
            disabled={submit.isPending || (tier === 1 && !phone) || (tier === 2 && !legalName)}
            className="btn btn-primary flex items-center gap-2">
            {submit.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <ShieldCheck className="w-4 h-4" />}
            Submit for verification
          </button>
        )}
        {submit.isSuccess && !isVerified && (
          <button onClick={() => selfVerify.mutate()} disabled={selfVerify.isPending}
            className="btn btn-ghost text-accent-mint border-accent-mint/30">
            {selfVerify.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
            Verify now (demo)
          </button>
        )}
        <button onClick={onDone} className="btn btn-ghost ml-auto flex items-center gap-1">
          Continue <ChevronRight className="w-4 h-4" />
        </button>
      </div>
      {submit.isError && <p className="text-xs text-bad">{(submit.error as any).message}</p>}
    </div>
  );
}

// ---- Step 4: Privacy ----
function Step4Privacy({ profile, onDone, onBack }: { profile: any; onDone: () => void; onBack: () => void }) {
  const inv = useInvalidate();
  const [prefs, setPrefs] = useState<any>(profile?.privacy_prefs ?? {
    display_name: "public", avatar: "public", location: "public_country", bio: "public",
    role_types: "public", languages: "public", professional_links: "private",
    deal_size_range: "participants_only", settlement_rails: "participants_only",
    org_members: "private", verified_signals: "public", kyc_tier: "public",
    reputation: "public", phone: "participants_only", email: "participants_only", deal_history: "private",
  });

  const fields = [
    { key: "display_name", label: "Display name & avatar" },
    { key: "location", label: "Location" },
    { key: "bio", label: "Bio" },
    { key: "role_types", label: "Role types" },
    { key: "languages", label: "Languages" },
    { key: "professional_links", label: "Professional links" },
    { key: "deal_size_range", label: "Deal size range" },
    { key: "settlement_rails", label: "Settlement preferences" },
    { key: "org_members", label: "Org member wallets" },
    { key: "verified_signals", label: "Verification badges" },
    { key: "kyc_tier", label: "KYC tier badge" },
    { key: "phone", label: "Phone / email" },
    { key: "deal_history", label: "Deal history" },
  ];

  const save = useMutation({
    mutationFn: () => api.kyc.updatePrivacy(prefs),
    onSuccess: () => { inv(["profile"]); onDone(); },
  });

  return (
    <div className="space-y-4">
      <p className="text-xs text-slate-400">
        Set default visibility per field. <b>Public</b> = anyone. <b>Participants-only</b> = current/past counterparties.
        <b>Private</b> = never shown.
      </p>
      <div className="space-y-2">
        {fields.map(f => (
          <div key={f.key} className="flex items-center justify-between glass-soft rounded-lg px-3 py-2">
            <span className="text-sm text-slate-300">{f.label}</span>
            <div className="flex gap-1">
              {VISIBILITY.map(v => {
                const Icon = v.icon;
                const active = prefs[f.key] === v.value || (f.key === "location" && prefs[f.key] === "public_country" && v.value === "public");
                return (
                  <button key={v.value} type="button"
                    onClick={() => setPrefs({ ...prefs, [f.key]: f.key === "location" && v.value === "public" ? "public_country" : v.value })}
                    className={`px-2 py-1 rounded text-[10px] font-medium flex items-center gap-1 transition
                      ${active ? "bg-accent/20 text-accent-glow border border-accent/40" : "text-slate-500 border border-transparent hover:text-slate-300"}`}>
                    <Icon className="w-3 h-3" /> {v.label}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>
      <div className="flex gap-2">
        <button onClick={onBack} className="btn btn-ghost flex items-center gap-1"><ChevronLeft className="w-4 h-4" /> Back</button>
        <button onClick={() => save.mutate()} disabled={save.isPending}
          className="btn btn-primary ml-auto flex items-center gap-2">
          {save.isPending ? <Loader2 className="w-4 h-4 animate-spin" /> : <CheckCircle2 className="w-4 h-4" />}
          Save preferences
        </button>
      </div>
      {save.isSuccess && <p className="text-xs text-accent-mint text-center">Privacy preferences saved — you're all set!</p>}
    </div>
  );
}

// ---- File upload component ----
function FileUpload({ label, hint, file, hash, onFile, onClear }: {
  label: string; hint: string; file: File | null; hash: string;
  onFile: (f: File) => void; onClear: () => void;
}) {
  return (
    <div className="glass-soft rounded-xl p-4">
      <label className="label">{label}</label>
      <p className="text-[11px] text-slate-500 mb-3">{hint}</p>
      {!file ? (
        <label className="flex flex-col items-center justify-center gap-2 border-2 border-dashed border-white/15 rounded-lg py-6 cursor-pointer hover:border-accent/40 transition">
          <Upload className="w-6 h-6 text-slate-500" />
          <span className="text-xs text-slate-400">Click to upload</span>
          <input type="file" accept="image/*" className="hidden"
            onChange={e => { const f = e.target.files?.[0]; if (f) onFile(f); }} />
        </label>
      ) : (
        <div className="flex items-center gap-3">
          {file.type.startsWith("image/") && (
            <img src={URL.createObjectURL(file)} alt="preview" className="w-16 h-16 rounded-lg object-cover border border-white/10" />
          )}
          <div className="flex-1 min-w-0">
            <div className="text-sm text-slate-200 truncate">{file.name}</div>
            <div className="text-[10px] text-slate-500">{(file.size / 1024).toFixed(1)} KB</div>
            {hash && <div className="text-[9px] font-mono text-accent-mint mt-1 truncate">sha256: {hash.slice(0, 24)}…</div>}
          </div>
          <button onClick={onClear} className="text-slate-500 hover:text-bad transition">
            <X className="w-4 h-4" />
          </button>
        </div>
      )}
    </div>
  );
}

// ---- Helpers ----
function TierCard({ n, title, desc, active, done, onClick, icon: Icon }: any) {
  return (
    <button onClick={onClick} type="button"
      className={`relative rounded-xl p-4 border text-left transition
        ${done ? "border-accent-mint bg-accent-mint/10" : active ? "border-accent bg-accent/15" : "border-white/10 hover:border-white/30"}`}>
      <div className="flex items-center gap-2 mb-1">
        {Icon ? <Icon className="w-4 h-4 text-accent-glow" /> : null}
        <span className="font-semibold text-sm">Tier {n}</span>
        <span className="text-xs text-slate-400">{title}</span>
      </div>
      <p className="text-[11px] text-slate-500">{desc}</p>
      {done && <CheckCircle2 className="w-4 h-4 text-accent-mint absolute top-3 right-3" />}
    </button>
  );
}
