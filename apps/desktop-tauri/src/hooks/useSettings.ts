import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { SettingsSnapshot, SettingsUpdate } from "../types/bridge";
import { getSettingsSnapshot, updateSettings } from "../lib/tauri";

interface UseSettingsReturn {
  settings: SettingsSnapshot;
  saving: boolean;
  error: string | null;
  update: (patch: SettingsUpdate) => Promise<void>;
}

const SETTINGS_CHANGED_EVENT = "settings-changed";

function publishSettingsUpdated(settings: SettingsSnapshot) {
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent<SettingsSnapshot>("pulsebar:settings-updated", {
      detail: settings,
    }),
  );
}

/**
 * Manages the current settings state and exposes a mutation helper that
 * persists changes through the Tauri bridge and refreshes the local copy.
 */
export function useSettings(initial: SettingsSnapshot): UseSettingsReturn {
  const [settings, setSettings] = useState<SettingsSnapshot>(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    setSettings(initial);

    getSettingsSnapshot()
      .then((fresh) => {
        if (!cancelled) {
          setSettings(fresh);
        }
      })
      .catch(() => {
        // Keep the bootstrap snapshot if the background sync fails.
      });

    const unlistenSettingsChanged = listen<SettingsSnapshot>(
      SETTINGS_CHANGED_EVENT,
      (event) => {
        if (!cancelled) {
          setSettings(event.payload);
          publishSettingsUpdated(event.payload);
        }
      },
    );

    return () => {
      cancelled = true;
      unlistenSettingsChanged.then((fn) => fn());
    };
  }, [initial]);

  const update = useCallback(async (patch: SettingsUpdate) => {
    setSaving(true);
    setError(null);
    try {
      const next = await updateSettings(patch);
      setSettings(next);
      publishSettingsUpdated(next);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      // Re-fetch to stay in sync with disk state on failure
      try {
        const fresh = await getSettingsSnapshot();
        setSettings(fresh);
      } catch {
        // ignore secondary failure
      }
    } finally {
      setSaving(false);
    }
  }, []);

  return { settings, saving, error, update };
}
