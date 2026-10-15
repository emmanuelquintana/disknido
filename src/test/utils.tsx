import { invoke } from "@tauri-apps/api/core";
import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { vi } from "vitest";
import { AppProvider, defaultSettings, type Settings } from "../state";

type Handler = (args: Record<string, unknown>) => unknown;

/** Routes mocked `invoke` calls by command name; unknown commands reject. */
export function mockCommands(handlers: Record<string, Handler | unknown>) {
  const fn = vi.mocked(invoke);
  fn.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "set_settings" || cmd === "get_settings") return null;
    if (!(cmd in handlers)) throw new Error(`unmocked command ${cmd}`);
    const h = handlers[cmd];
    return typeof h === "function" ? (h as Handler)((args ?? {}) as Record<string, unknown>) : h;
  });
  return fn;
}

export function renderApp(ui: ReactNode, settings: Partial<Settings> = {}) {
  return render(<AppProvider initialSettings={{ ...defaultSettings, ...settings }}>{ui}</AppProvider>);
}
