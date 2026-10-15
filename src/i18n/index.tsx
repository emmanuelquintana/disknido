import { createContext, useContext } from "react";
import { en } from "./en";
import { es, type Key } from "./es";

export type Lang = "es" | "en";
export type { Key };

const dictionaries: Record<Lang, Record<Key, string>> = { es, en };

export type T = (key: Key, vars?: Record<string, string | number>) => string;

export function translator(lang: Lang): T {
  const dict = dictionaries[lang];
  return (key, vars) => {
    const s = dict[key] ?? es[key] ?? key;
    return vars ? s.replace(/\{(\w+)\}/g, (_, k: string) => String(vars[k] ?? `{${k}}`)) : s;
  };
}

export const I18nContext = createContext<{ lang: Lang; t: T }>({ lang: "es", t: translator("es") });

export function useT(): T {
  return useContext(I18nContext).t;
}

export function useLang(): Lang {
  return useContext(I18nContext).lang;
}
