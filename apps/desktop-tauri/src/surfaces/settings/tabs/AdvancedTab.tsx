import { useCallback, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { ShortcutCapture } from "../../../components/ShortcutCapture";
import { Field, Toggle } from "../../../components/FormControls";
import type { TabProps } from "../../Settings";

export default function AdvancedTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const [shortcutError, setShortcutError] = useState<string | null>(null);

  // Persisting the new value routes through `update_settings` →
  // `reregister_shortcut`, which owns the single native toggle binding. We no
  // longer register a second (force-show) handler here — that was what turned a
  // custom shortcut into show-only and could fail with "already registered".
  const commitShortcut = useCallback(
    (accelerator: string) => {
      setShortcutError(null);
      try {
        set({ globalShortcut: accelerator });
      } catch (err: unknown) {
        setShortcutError(err instanceof Error ? err.message : String(err));
      }
    },
    [set],
  );

  // Clearing persists an empty accelerator; the backend unregisters the old
  // binding and succeeds (see `reregister_shortcut`'s empty-value handling).
  const clearShortcut = useCallback(() => {
    setShortcutError(null);
    try {
      set({ globalShortcut: "" });
    } catch (err: unknown) {
      setShortcutError(err instanceof Error ? err.message : String(err));
    }
  }, [set]);

  return (
    <>
      {/* ── Keyboard shortcut ────────────────────────────────────── */}
      <section className="settings-section">
        <h3 className="settings-section__title">{t("SectionKeyboard")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("GlobalShortcutFieldLabel")}
            description={t("GlobalShortcutToggleHelper")}
          >
            <ShortcutCapture
              value={settings.globalShortcut}
              disabled={saving}
              onCommit={(accel) => void commitShortcut(accel)}
              onClear={() => void clearShortcut()}
            />
          </Field>
        </div>
        {shortcutError && (
          <p className="settings-section__error">{shortcutError}</p>
        )}
        <p className="settings-section__hint">{t("ShortcutRecordingHint")}</p>
      </section>

      {/* ── Debug ────────────────────────────────────────────────── */}
      <section className="settings-section">
        <h3 className="settings-section__title">{t("SectionDebug")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("ShowDebugSettingsLabel")}
            description={t("ShowDebugSettingsHelper")}
            leading
          >
            <Toggle
              checked={settings.showDebugSettings}
              disabled={saving}
              onChange={(v) => set({ showDebugSettings: v })}
            />
          </Field>
          <Field
            label={t("SurpriseAnimationsLabel")}
            description={t("SurpriseAnimationsHelper")}
            leading
          >
            <Toggle
              checked={settings.surpriseAnimations}
              disabled={saving}
              onChange={(v) => set({ surpriseAnimations: v })}
            />
          </Field>
        </div>
      </section>

      {/* ── Privacy ──────────────────────────────────────────────── */}
      <section className="settings-section">
        <h3 className="settings-section__title">{t("PrivacyTitle")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("HidePersonalInfo")}
            description={t("HidePersonalInfoHelper")}
            leading
          >
            <Toggle
              checked={settings.hidePersonalInfo}
              disabled={saving}
              onChange={(v) => set({ hidePersonalInfo: v })}
            />
          </Field>
        </div>
      </section>

      {/* ── Keychain access ──────────────────────────────────────── */}
      <section className="settings-section">
        <h3 className="settings-section__title settings-section__title--bold">
          KEYCHAIN ACCESS
        </h3>
        <p className="settings-section__caption">
          Disable all Keychain reads and writes. Browser cookie import is
          unavailable; paste Cookie headers manually in Providers.
        </p>
        <div className="settings-section__group">
          <Field
            label={t("DisableAllKeychainLabel")}
            description={t("DisableAllKeychainHelper")}
            leading
          >
            <Toggle
              checked={settings.disableKeychainAccess}
              disabled={saving}
              onChange={(v) => set({ disableKeychainAccess: v })}
            />
          </Field>
          <Field
            label={t("AvoidKeychainPromptsLabel")}
            description={t("AvoidKeychainPromptsHelper")}
            leading
          >
            <Toggle
              checked={settings.claudeAvoidKeychainPrompts}
              disabled={saving || settings.disableKeychainAccess}
              onChange={(v) => set({ claudeAvoidKeychainPrompts: v })}
            />
          </Field>
        </div>
      </section>
    </>
  );
}
