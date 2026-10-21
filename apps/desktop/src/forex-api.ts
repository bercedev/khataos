// Foreign-currency commands (src-tauri/src/forex.rs): the currency master,
// a ledger's currency, remembered rates of exchange and the period-end
// revaluation. Kept apart from api.ts, like the desk's commands.
// Rates are never fetched online: they are typed in and remembered.
import { invoke } from "@tauri-apps/api/core";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
  }
}

export interface CurrencyDto {
  code: string;
  symbol: string;
  name: string;
  decimals: number;
}

export interface Currencies {
  /** The company's own currency, once a country is set. */
  base: CurrencyDto | null;
  currencies: CurrencyDto[];
}

/** A remembered rate of exchange: base currency per unit. */
export interface FxRate {
  currency: string;
  date: string;
  rate: string;
  /** Taken from a voucher rather than the rate table. */
  from_voucher: boolean;
}

export interface RevaluationRow {
  ledger_id: string;
  name: string;
  currency: string;
  foreign: string;
  book: string;
  rate: string;
  revalued: string;
  /** Debit to the ledger when positive; the other side is Forex Gain/Loss. */
  difference: string;
}

export const fxApi = {
  currencies: () => call<Currencies>("fx_list_currencies"),
  saveCurrency: (code: string, symbol: string, name: string, decimals: number) =>
    call<Currencies>("fx_save_currency", { code, symbol, name, decimals }),
  /** "" (or the base currency) puts the ledger back in the base currency. */
  setLedgerCurrency: (ledger_id: string, code: string) => call<void>("fx_set_ledger_currency", { ledger_id, code }),
  /** The last rate entered on or before `date`, or null. */
  rate: (code: string, date: string) => call<FxRate | null>("fx_rate", { code, date }),
  /** "" removes the rate for that date. */
  setRate: (code: string, date: string, rate: string) => call<void>("fx_set_rate", { code, date, rate }),
  /** Rates by code; a currency left out uses its remembered rate. */
  revaluation: (as_at: string, rates: Record<string, string> = {}) => call<RevaluationRow[]>("fx_revaluation", { as_at, rates }),
  postRevaluation: (as_at: string, rates: Record<string, string> = {}) => call<string | null>("fx_post_revaluation", { as_at, rates }),
};
