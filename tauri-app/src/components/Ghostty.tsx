import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef } from "react";

export type GhosttyRect = {
  x: number;
  y: number;
  width: number;
  height: number;
  viewportWidth: number;
  viewportHeight: number;
  style: GhosttyStyle;
};

export type GhosttyInsets = {
  top: number;
  right: number;
  bottom: number;
  left: number;
};

export type GhosttyStyle = {
  insets: GhosttyInsets;
  cornerRadius: number;
};

export type GhosttyOptions = {
  fontSize?: number;
  workingDirectory?: string;
  command?: string;
};

type GhosttyProps = {
  id: string;
  options?: GhosttyOptions;
  className?: string;
  styleSource?: "self" | "parent";
  visible?: boolean;
};

function readStyle(el: HTMLElement): GhosttyStyle {
  const style = getComputedStyle(el);
  const toPx = (value: string) => {
    const parsed = parseFloat(value);
    return Number.isFinite(parsed) ? parsed : 0;
  };

  const paddingTop = toPx(style.paddingTop);
  const paddingRight = toPx(style.paddingRight);
  const paddingBottom = toPx(style.paddingBottom);
  const paddingLeft = toPx(style.paddingLeft);

  const borderTop = toPx(style.borderTopWidth);
  const borderRight = toPx(style.borderRightWidth);
  const borderBottom = toPx(style.borderBottomWidth);
  const borderLeft = toPx(style.borderLeftWidth);

  const cornerRadius = Math.max(
    toPx(style.borderTopLeftRadius),
    toPx(style.borderTopRightRadius),
    toPx(style.borderBottomRightRadius),
    toPx(style.borderBottomLeftRadius),
  );

  return {
    insets: {
      top: paddingTop + borderTop,
      right: paddingRight + borderRight,
      bottom: paddingBottom + borderBottom,
      left: paddingLeft + borderLeft,
    },
    cornerRadius,
  };
}

function readRect(el: HTMLElement): GhosttyRect {
  const rect = el.getBoundingClientRect();
  return {
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
    viewportWidth: window.innerWidth,
    viewportHeight: window.innerHeight,
    style: readStyle(el),
  };
}

export function Ghostty({
  id,
  options,
  className,
  styleSource = "parent",
  visible = true,
}: GhosttyProps) {
  const ref = useRef<HTMLDivElement | null>(null);
  const optionsRef = useRef<GhosttyOptions | undefined>(options);
  const visibleRef = useRef(visible);
  const createdRef = useRef(false);

  useEffect(() => {
    optionsRef.current = options;
  }, [options]);

  // Keep visibleRef in sync
  useEffect(() => {
    visibleRef.current = visible;
  }, [visible]);

  // Handle visibility changes after creation
  useEffect(() => {
    if (!createdRef.current) return;

    if (visible) {
      // Show: update rect, make visible, focus
      const el = ref.current;
      if (el) {
        const sourceEl = styleSource === "parent" ? el.parentElement ?? el : el;
        invoke("ghostty_update_rect", { id, rect: readRect(sourceEl) }).catch(console.error);
      }
      invoke("ghostty_set_visible", { id, visible: true }).catch(console.error);
      invoke("ghostty_focus", { id, focused: true }).catch(console.error);
    } else {
      // Hide: unfocus, then hide
      invoke("ghostty_focus", { id, focused: false }).catch(console.error);
      invoke("ghostty_set_visible", { id, visible: false }).catch(console.error);
    }
  }, [visible, id, styleSource]);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;

    const sourceEl =
      styleSource === "parent" ? el.parentElement ?? el : el;

    let destroyed = false;
    let frameHandle: number | null = null;

    const create = async () => {
      await invoke("ghostty_create", {
        id,
        rect: readRect(sourceEl),
        options: optionsRef.current,
      });
      createdRef.current = true;
      // If created hidden, hide immediately
      if (!visibleRef.current) {
        await invoke("ghostty_set_visible", { id, visible: false });
      }
    };

    const update = async () => {
      if (!ref.current || destroyed) return;
      // Skip rect updates for hidden terminals
      if (!visibleRef.current) return;
      await invoke("ghostty_update_rect", {
        id,
        rect: readRect(sourceEl),
      });
    };

    const scheduleUpdate = () => {
      if (frameHandle !== null) return;
      frameHandle = window.requestAnimationFrame(() => {
        frameHandle = null;
        update().catch(console.error);
      });
    };

    create().then(scheduleUpdate).catch(console.error);

    const resizeObserver = new ResizeObserver(() => {
      scheduleUpdate();
    });
    resizeObserver.observe(sourceEl);

    const onScroll = () => scheduleUpdate();
    const onResize = () => scheduleUpdate();

    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onResize);

    return () => {
      destroyed = true;
      createdRef.current = false;
      if (frameHandle !== null) {
        window.cancelAnimationFrame(frameHandle);
      }
      resizeObserver.disconnect();
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onResize);
      invoke("ghostty_destroy", { id }).catch(console.error);
    };
  }, [id, styleSource]);

  return (
    <div
      ref={ref}
      className={className}
      tabIndex={0}
      onFocus={() => invoke("ghostty_focus", { id, focused: true })}
      onBlur={() => invoke("ghostty_focus", { id, focused: false })}
      onMouseDown={() => invoke("ghostty_focus", { id, focused: true })}
    />
  );
}
