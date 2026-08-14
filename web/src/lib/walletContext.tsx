import { createContext, useContext, useState, ReactNode, useCallback, useEffect } from "react";
import { WalletHandle, connect as connectWallet, availableWallets } from "./wallet";

interface WalletCtx {
  wallet: WalletHandle | null;
  connecting: boolean;
  available: { key: string; name: string; icon?: string }[];
  connect: (key: string) => Promise<void>;
  disconnect: () => void;
  rescan: () => void;
}

const Ctx = createContext<WalletCtx>(null as any);
export const useWallet = () => useContext(Ctx);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [wallet, setWallet] = useState<WalletHandle | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [available, setAvailable] = useState<{ key: string; name: string; icon?: string }[]>([]);

  const rescan = useCallback(() => setAvailable(availableWallets()), []);

  // scan on mount + when the document finishes loading (extensions inject late)
  useEffect(() => {
    rescan();
    const t1 = setTimeout(rescan, 800);
    const t2 = setTimeout(rescan, 2500);
    window.addEventListener("load", rescan);
    return () => { clearTimeout(t1); clearTimeout(t2); window.removeEventListener("load", rescan); };
  }, [rescan]);

  const connect = useCallback(async (key: string) => {
    setConnecting(true);
    try {
      const w = await connectWallet(key);
      setWallet(w);
      localStorage.setItem("tmp.wallet", key);
    } finally {
      setConnecting(false);
    }
  }, []);

  const disconnect = useCallback(() => {
    setWallet(null);
    localStorage.removeItem("tmp.wallet");
  }, []);

  // auto-reconnect on mount
  const last = localStorage.getItem("tmp.wallet");
  useEffect(() => {
    if (last) {
      connect(last).catch(() => localStorage.removeItem("tmp.wallet"));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Ctx.Provider value={{ wallet, connecting, available, connect, disconnect, rescan }}>
      {children}
    </Ctx.Provider>
  );
}
