import { useCallback, useEffect, useRef, useState } from "react";
import {
  getProviderWorkspaceId,
  refreshProviders,
  setProviderWorkspaceId,
} from "../../../lib/tauri";
import { useLocale } from "../../../hooks/useLocale";

interface Props {
  providerId: string;
  /** Fired after the workspace ID is saved and providers are refreshed. */
  onSaved?: () => void;
}

const PLACEHOLDER = "wrk_... (from https://opencode.ai/workspace/<id>/go)";

export function WorkspaceIdSection({ providerId, onSaved }: Props) {
  const { t } = useLocale();
  const [value, setValue] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<"saving" | "saved" | null>(null);
  const flashTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (flashTimer.current) clearTimeout(flashTimer.current);
    },
    [],
  );

  const showSavedFlash = useCallback(() => {
    setFlash("saved");
    if (flashTimer.current) clearTimeout(flashTimer.current);
    flashTimer.current = setTimeout(() => setFlash(null), 2000);
  }, []);

  useEffect(() => {
    const signal = { stale: false };
    setLoaded(false);
    setError(null);
    setValue("");
    getProviderWorkspaceId(providerId)
      .then((v) => {
        if (signal.stale) return;
        setValue(v ?? "");
      })
      .catch((err: unknown) => {
        if (signal.stale) return;
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!signal.stale) setLoaded(true);
      });
    return () => {
      signal.stale = true;
    };
  }, [providerId]);

  const handleSave = async () => {
    const trimmed = value.trim();
    setBusy(true);
    setError(null);
    setFlash("saving");
    try {
      await setProviderWorkspaceId(providerId, trimmed);
      await refreshProviders();
      onSaved?.();
      showSavedFlash();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
      setFlash(null);
    } finally {
      setBusy(false);
    }
  };

  if (!loaded) return null;

  const isSet = value.trim().length > 0;

  return (
    <section className="provider-detail-section">
      <h4>Workspace ID</h4>

      {error && (
        <div className="settings-status settings-status--error">{error}</div>
      )}

      {flash === "saved" && (
        <div className="settings-status settings-status--ok">
          {t("CredentialSaved")}
        </div>
      )}

      <div className="credential-add-form">
        <input
          className="text-input"
          type="text"
          placeholder={PLACEHOLDER}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          disabled={busy}
        />
        <button
          className="credential-btn credential-btn--primary"
          disabled={busy || !value.trim()}
          onClick={() => void handleSave()}
        >
          {flash === "saving" ? t("CredentialSaving") : isSet ? "Update" : "Save"}
        </button>
      </div>
    </section>
  );
}
