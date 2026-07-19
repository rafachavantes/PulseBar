import "@testing-library/jest-dom/vitest";

// jsdom does not expose `localStorage` for vitest's opaque-origin documents,
// so components that read it on mount (e.g. useUpdateState's update-throttle)
// crash the test renderer. Provide an in-memory implementation.
if (typeof globalThis.localStorage === "undefined") {
  let store: Record<string, string> = {};
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (k: string) => (k in store ? store[k] : null),
      setItem: (k: string, v: string) => {
        store[k] = String(v);
      },
      removeItem: (k: string) => {
        delete store[k];
      },
      clear: () => {
        store = {};
      },
      key: (i: number) => Object.keys(store)[i] ?? null,
      get length() {
        return Object.keys(store).length;
      },
    },
  });
}
