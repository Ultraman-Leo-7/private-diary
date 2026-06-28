import { useEffect, useState } from "react";
import { api } from "./api";
import type { Phase } from "./types";
import SetupScreen from "./components/SetupScreen";
import LockScreen from "./components/LockScreen";
import MainScreen from "./components/MainScreen";
import "./App.css";

export default function App() {
  const [phase, setPhase] = useState<Phase>("loading");

  useEffect(() => {
    api
      .vaultExists()
      .then((exists) => setPhase(exists ? "locked" : "setup"))
      .catch(() => setPhase("setup"));
  }, []);

  if (phase === "loading") {
    return <div className="center-screen">加载中…</div>;
  }
  if (phase === "setup") {
    return <SetupScreen onDone={() => setPhase("unlocked")} />;
  }
  if (phase === "locked") {
    return <LockScreen onUnlock={() => setPhase("unlocked")} />;
  }
  return <MainScreen onLock={() => api.lock().finally(() => setPhase("locked"))} />;
}
