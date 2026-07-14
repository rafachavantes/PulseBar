import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../../i18n/LocaleProvider";
import { buildBundle } from "../../../test/localeHarness";
import type {
  ApiKeyInfoBridge,
  ApiKeyProviderInfoBridge,
} from "../../../types/bridge";
import { ApiKeySection } from "./ApiKeySection";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
  getApiKeyProviders: vi.fn(),
  getApiKeys: vi.fn(),
  setApiKey: vi.fn(),
  removeApiKey: vi.fn(),
  refreshProviders: vi.fn(),
}));

const eventMocks = vi.hoisted(() => ({
  listen: vi.fn(),
}));

vi.mock("../../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../lib/tauri")>()),
  ...tauriMocks,
}));
vi.mock("@tauri-apps/api/event", () => eventMocks);

const PROVIDER: ApiKeyProviderInfoBridge = {
  id: "codex",
  displayName: "Codex",
  envVar: null,
  help: null,
  dashboardUrl: null,
};

const SAVED_KEY: ApiKeyInfoBridge = {
  providerId: "codex",
  provider: "Codex",
  maskedKey: "sk-…abcd",
  savedAt: "just now",
  label: null,
};

describe("ApiKeySection saved feedback", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle({ CredentialSaved: "Saved ✓" }),
    );
    tauriMocks.getApiKeyProviders.mockResolvedValue([PROVIDER]);
    tauriMocks.getApiKeys.mockResolvedValue([]);
    tauriMocks.setApiKey.mockResolvedValue([SAVED_KEY]);
    tauriMocks.refreshProviders.mockResolvedValue(undefined);
    eventMocks.listen.mockResolvedValue(() => {});
  });

  it("shows the CredentialSaved banner after a key save resolves", async () => {
    render(
      <LocaleProvider>
        <ApiKeySection providerId="codex" onSaved={vi.fn()} />
      </LocaleProvider>,
    );

    // Enter edit mode, type a key, and save.
    fireEvent.click(await screen.findByRole("button", { name: "Add Key" }));
    fireEvent.change(screen.getByPlaceholderText("Paste API key…"), {
      target: { value: "sk-test-1234" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    // The green flash paints (it must survive: the section is no longer keyed
    // on credentialRevision, so onSaved does not remount and discard it).
    expect(await screen.findByText("Saved ✓")).toBeInTheDocument();
    expect(tauriMocks.setApiKey).toHaveBeenCalledWith(
      "codex",
      "sk-test-1234",
      undefined,
    );
  });
});
