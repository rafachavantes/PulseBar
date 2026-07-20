import { useEffect, useState } from "react";
import { getAppInfo, openExternalUrl } from "../../../lib/tauri";
import { useUpdateState } from "../../../hooks/useUpdateState";
import type { AppInfoBridge } from "../../../types/bridge";
import type { TabProps } from "../../Settings";
import pulsebarLogo from "../../../assets/pulsebar-logo.svg";

const ABOUT_LINKS = [
  {
    label: "GitHub",
    url: "https://github.com/rafachavantes/PulseBar",
  },
  {
    label: "Website",
    url: "https://pulsebar.app",
  },
  {
    label: "Original Project",
    url: "https://github.com/steipete/CodexBar",
  },
] as const;

export default function AboutTab(_props: TabProps) {
  const [appInfo, setAppInfo] = useState<AppInfoBridge | null>(null);
  const [linkError, setLinkError] = useState<string | null>(null);
  const [upToDate, setUpToDate] = useState(false);
  const { updateState, checkNow } = useUpdateState();

  useEffect(() => {
    void getAppInfo().then(setAppInfo);
  }, []);

  const handleCheck = async () => {
    setUpToDate(false);
    const payload = await checkNow();
    if (payload && payload.status !== "available" && !payload.error) {
      setUpToDate(true);
      setTimeout(() => setUpToDate(false), 3000);
    }
  };

  const openAboutLink = (url: string) => {
    setLinkError(null);
    openExternalUrl(url).catch((error) => {
      setLinkError(String(error));
    });
  };

  if (!appInfo) {
    return (
      <section className="settings-section">
        <p className="settings-section__hint">Loading…</p>
      </section>
    );
  }

  return (
    <section className="settings-section about-section">
      <div className="about-header">
        <img className="about-icon" src={pulsebarLogo} alt="PulseBar" />
        <div className="about-title-block">
          <h2 className="about-title">{appInfo.name}</h2>
          <p className="about-version">
            Version {appInfo.version}
            {appInfo.buildNumber !== "dev" && ` (${appInfo.buildNumber})`}
          </p>
          <p className="about-tagline">{appInfo.tagline}</p>
        </div>
      </div>

      <div className="about-links">
        {ABOUT_LINKS.map((link) => (
          <button
            key={link.url}
            type="button"
            className="about-link"
            onClick={() => openAboutLink(link.url)}
          >
            {link.label}
          </button>
        ))}
      </div>
      {linkError && <p className="about-update-msg">Error: {linkError}</p>}

      <div className="about-divider" />

      <div className="about-updates">
        <button type="button" className="about-link" onClick={() => void handleCheck()}>
          Check for Updates
        </button>
        {updateState.status === "available" && (
          <p className="about-update-msg">
            PulseBar {updateState.version} is available!
          </p>
        )}
        {upToDate && (
          <p className="about-update-msg">You're on the latest version.</p>
        )}
        {updateState.error && (
          <p className="about-update-msg">Error: {updateState.error}</p>
        )}
      </div>

      <p className="about-copyright">
        PulseBar by Rafa Chavantes. Based on{" "}
        <button
          type="button"
          className="about-link about-link--inline"
          onClick={() => openAboutLink("https://github.com/steipete/CodexBar")}
        >
          CodexBar
        </button>{" "}
        (Windows port by NessZerra) by steipete. MIT License.
      </p>
    </section>
  );
}
