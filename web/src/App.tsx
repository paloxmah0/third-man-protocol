import { Routes, Route } from "react-router-dom";
import Shell from "./components/Shell";
import Journey from "./pages/Journey";
import Invite from "./pages/Invite";
import ArbiterConsole from "./components/ArbiterConsole";

export default function App() {
  return (
    <>
      <div className="aurora" />
      <Shell>
        <Routes>
          <Route path="/" element={<Journey />} />
          <Route path="/invite/:code" element={<Invite />} />
          <Route path="/arbiter" element={<ArbiterConsole />} />
          <Route path="*" element={<Journey />} />
        </Routes>
      </Shell>
    </>
  );
}
