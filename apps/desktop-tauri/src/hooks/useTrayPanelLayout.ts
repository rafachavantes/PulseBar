import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import {
  getWorkAreaRect,
  reanchorTrayPanel,
  revealTrayPanelWindow,
} from "../lib/tauri";

const TRAY_WIDTH = 328;
const TRAY_MAX_MEASURE_HEIGHT = 920;
const TRAY_OVERVIEW_MIN_HEIGHT = 200;
const TRAY_DETAIL_MIN_HEIGHT = 420;
const TRAY_DENSE_OVERVIEW_HEIGHT = 776;

export interface TrayPanelLayoutOptions {
  canMeasure: boolean;
  denseOverview: boolean;
  detailMode: boolean;
  layoutKey: string;
}

export interface TrayPanelLayout {
  layoutReady: boolean;
  requestLayout: () => void;
}

export function useTrayPanelLayout({
  canMeasure,
  denseOverview,
  detailMode,
  layoutKey,
}: TrayPanelLayoutOptions): TrayPanelLayout {
  const [layoutReady, setLayoutReady] = useState(false);
  const [layoutRevision, setLayoutRevision] = useState(0);
  const layoutReadyRef = useRef(false);
  const resizeRunRef = useRef(0);
  const layoutTimerRef = useRef<number | undefined>(undefined);
  const lastSizeRef = useRef<{ width: number; height: number } | null>(null);
  const revealedRef = useRef(false);

  // One-shot native reveal for the tray window. Deliberately decoupled from the
  // measurement run and from `canMeasure`: a starved or cancelled measurement
  // pass (see the resize effect below) must never leave the window hidden — the
  // REOPEN-FAILS symptom. Rust keeps its own ~300 ms fallback as a last resort.
  const revealOnce = useCallback(() => {
    if (revealedRef.current) return;
    revealedRef.current = true;
    layoutReadyRef.current = true;
    setLayoutReady(true);
    // Reveal after a frame so the committed content paints before the OS shows
    // the window; measurement (if any) keeps resizing afterwards.
    requestAnimationFrame(() => {
      void Promise.resolve(revealTrayPanelWindow()).catch(() => {});
    });
  }, []);

  const requestLayout = useCallback(() => {
    if (layoutTimerRef.current !== undefined) {
      window.clearTimeout(layoutTimerRef.current);
    }
    layoutTimerRef.current = window.setTimeout(() => {
      setLayoutRevision((current) => current + 1);
    }, layoutReadyRef.current ? 100 : 16);
  }, []);

  useEffect(() => {
    requestLayout();
  }, [layoutKey, requestLayout]);

  useEffect(() => {
    const surface = document.querySelector<HTMLElement>(".menu-surface--tray");
    if (!surface || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => requestLayout());
    observer.observe(surface);
    return () => observer.disconnect();
  }, [requestLayout]);

  useEffect(() => {
    return () => {
      if (layoutTimerRef.current !== undefined) {
        window.clearTimeout(layoutTimerRef.current);
      }
    };
  }, []);

  // Guarantee the first reveal even when measurement is starved: `canMeasure`
  // stays false, or every measurement run gets cancelled by layout-revision
  // churn. This one-shot fires shortly after the first commit regardless.
  useEffect(() => {
    const timer = window.setTimeout(() => revealOnce(), 120);
    return () => window.clearTimeout(timer);
  }, [revealOnce]);

  useEffect(() => {
    if (!canMeasure) return;

    const minHeight = detailMode
      ? TRAY_DETAIL_MIN_HEIGHT
      : denseOverview
        ? TRAY_DENSE_OVERVIEW_HEIGHT
        : TRAY_OVERVIEW_MIN_HEIGHT;

    const resize = async () => {
      const run = ++resizeRunRef.current;
      const win = getCurrentWindow();
      const surface = document.querySelector<HTMLElement>(".menu-surface--tray");
      if (!surface) return;
      const html = document.documentElement;
      const pageBody = document.body;
      const workArea = await getWorkAreaRect().catch(() => null);
      const maxHeight = Math.max(
        minHeight,
        Math.min(
          TRAY_MAX_MEASURE_HEIGHT,
          (workArea?.height ?? TRAY_MAX_MEASURE_HEIGHT) - 16,
        ),
      );

      const body = surface.querySelector<HTMLElement>(".menu-surface__body");
      const stack = surface.querySelector<HTMLElement>(".menu-stack");

      const previous = {
        htmlOverflow: html.style.overflow,
        bodyOverflow: pageBody.style.overflow,
        bodyMinHeight: pageBody.style.minHeight,
        surfaceMinHeight: surface.style.minHeight,
        surfaceHeight: surface.style.height,
        surfaceMaxHeight: surface.style.maxHeight,
        surfaceOverflow: surface.style.overflow,
        bodyInnerOverflow: body?.style.overflow,
        bodyFlex: body?.style.flex,
        stackOverflow: stack?.style.overflow,
      };
      let committedHeight = false;

      html.style.overflow = "visible";
      pageBody.style.overflow = "visible";
      pageBody.style.minHeight = "0";
      surface.style.minHeight = "0";
      surface.style.height = "auto";
      surface.style.maxHeight = "none";
      surface.style.overflow = "visible";
      if (body) {
        body.style.overflow = "visible";
        body.style.flex = "0 0 auto";
      }
      if (stack) {
        stack.style.overflow = "visible";
      }

      try {
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

        if (run !== resizeRunRef.current) return;

        const surfaceRect = surface.getBoundingClientRect();
        let contentHeight = Math.max(
          surface.scrollHeight,
          Math.ceil(surfaceRect.height),
        );
        let maxBottom = surfaceRect.top + contentHeight;
        const bodyRect = body?.getBoundingClientRect();
        if (bodyRect && bodyRect.height > 0 && bodyRect.bottom > maxBottom) {
          maxBottom = bodyRect.bottom;
        }
        const footer = surface.querySelector<HTMLElement>(".menu-surface__footer");
        const footerRect = footer?.getBoundingClientRect();
        if (footerRect && footerRect.height > 0 && footerRect.bottom > maxBottom) {
          maxBottom = footerRect.bottom;
        }
        contentHeight = Math.ceil(maxBottom - surfaceRect.top) + 4;

        const height = Math.min(Math.max(contentHeight, minHeight), maxHeight);
        surface.style.maxHeight = `${height}px`;
        committedHeight = true;

        const previousSize = lastSizeRef.current;
        const shouldResize =
          previousSize === null ||
          previousSize.width !== TRAY_WIDTH ||
          Math.abs(previousSize.height - height) > 2;
        if (shouldResize) {
          await win.setSize(new LogicalSize(TRAY_WIDTH, height));
          lastSizeRef.current = { width: TRAY_WIDTH, height };
          await Promise.resolve(reanchorTrayPanel()).catch(() => {});
        }

        revealOnce();
      } catch (error) {
        console.warn("PulseBar tray panel resize failed", error);
        revealOnce();
      } finally {
        if (!committedHeight) {
          surface.style.maxHeight = previous.surfaceMaxHeight;
        }
        surface.style.minHeight = previous.surfaceMinHeight;
        surface.style.height = previous.surfaceHeight;
        surface.style.overflow = previous.surfaceOverflow;
        html.style.overflow = previous.htmlOverflow;
        pageBody.style.overflow = previous.bodyOverflow;
        pageBody.style.minHeight = previous.bodyMinHeight;
        if (body) {
          body.style.overflow = previous.bodyInnerOverflow ?? "";
          body.style.flex = previous.bodyFlex ?? "";
        }
        if (stack) {
          stack.style.overflow = previous.stackOverflow ?? "";
        }
      }
    };

    const timer = window.setTimeout(
      () => void resize(),
      layoutReadyRef.current ? 25 : 0,
    );

    return () => {
      window.clearTimeout(timer);
      resizeRunRef.current += 1;
    };
  }, [canMeasure, denseOverview, detailMode, layoutRevision, revealOnce]);

  return { layoutReady, requestLayout };
}
