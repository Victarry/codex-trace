import { useState, useEffect, useCallback } from "react";
import { invoke } from "../lib/invoke";
import type { CodexSession } from "../../shared/types";
import { useTauriEvent } from "./useTauriEvent";

interface SessionState {
  session: CodexSession | null;
  loading: boolean;
  sessionPath: string;
}

export function useSession() {
  const [state, setState] = useState<SessionState>({
    session: null,
    loading: false,
    sessionPath: "",
  });

  const loadSession = useCallback(async (path: string) => {
    setState((prev) => ({ ...prev, loading: true }));
    try {
      try {
        await invoke<void>("unwatch_session");
      } catch {
        // ignore
      }
      const session = await invoke<CodexSession>("load_session", { path });
      setState({ session, loading: false, sessionPath: path });
      try {
        await invoke<void>("watch_session", { path });
      } catch {
        // watcher is optional
      }
    } catch (err) {
      console.error("Failed to load session:", err);
      setState((prev) => ({ ...prev, loading: false }));
    }
  }, []);

  const clearSession = useCallback(async () => {
    try {
      await invoke<void>("unwatch_session");
    } catch {
      // ignore
    }
    setState({ session: null, loading: false, sessionPath: "" });
  }, []);

  useTauriEvent<{ session: CodexSession }>("session-update", (payload) => {
    setState((prev) => ({ ...prev, session: payload.session }));
  });

  useEffect(() => {
    return () => {
      invoke<void>("unwatch_session").catch(() => {});
    };
  }, []);

  return {
    ...state,
    loadSession,
    clearSession,
  };
}
