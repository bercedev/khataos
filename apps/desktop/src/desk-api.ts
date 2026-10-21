// The accountant's desk commands (src-tauri/src/desk.rs): notes and to-dos,
// the compliance calendar and reminders from the books. Kept apart from
// api.ts and types.ts so the desk stays one self-contained add-on.
import { invoke } from "@tauri-apps/api/core";
import type { VoucherKind } from "./types";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
  }
}

export type LinkKind = "ledger" | "voucher" | "stock_item";

/** What Alt+W attaches a note to; `label` is shown in the dialog. */
export interface NoteLink {
  kind: LinkKind;
  id: string;
  label: string;
}

export interface NoteDto {
  id: string;
  title: string;
  text: string;
  due: string | null;
  done: boolean;
  link: {
    kind: LinkKind;
    id: string;
    label: string;
    voucher_kind: VoucherKind | null;
    date: string | null;
    /** The record was deleted since. */
    missing: boolean;
  } | null;
  created_at: string;
  created_by: string;
  updated_at: string;
  updated_by: string;
}

export interface NoteInput {
  id?: string | null;
  title: string;
  text?: string;
  due?: string | null;
  done?: boolean;
  link_kind?: LinkKind | null;
  link_id?: string | null;
}

export interface DueDto {
  code: string;
  /** English; shown through t(). */
  title: string;
  rule: string;
  source: string;
  optional: boolean;
  period_from: string;
  period_to: string;
  due: string;
  /** The rule's date before a weekend move. */
  unadjusted: string;
  category: Category;
  shift: { kind: "fixed" } | { kind: "next_working_day"; weekend: "sat_sun" | "fri_sat" } | { kind: "may_move" };
  /** A US state, for its sales tax return. */
  subject: string | null;
  /** What markFiled takes. */
  filed_code: string;
  filed: boolean;
  filed_by: string | null;
}

export type Category = "return" | "payment" | "filing" | "audit";
export type Entity = "company" | "llp" | "partnership" | "sole_proprietor" | "s_corporation";
export type ProfileField = "incorporated" | "entity" | "fy_end" | "vat_from" | "tax_period" | "corporate_tax" | "audit" | "withholding" | "states";
export type PeriodChoice = "monthly_quarterly" | "monthly_quarterly_annual" | "uk_stagger" | "quarter_start" | "quarter_end";

export interface StateFiling {
  state: string;
  frequency: Frequency;
}

/** What the country's rules are told: the profile with defaults filled in. */
export interface CalendarCompany {
  frequency: Frequency | null;
  quarter_end: number | null;
  region: string | null;
  fy_start_month: number;
  withholding: boolean;
  collection: boolean;
  incorporated: string | null;
  entity: Entity | null;
  vat_from: string | null;
  vat_unregistered: boolean;
  corporate_tax: boolean | null;
  audit: boolean;
  states: StateFiling[];
}

export interface BillDue {
  ledger_id: string;
  party: string;
  reference: string;
  date: string | null;
  due: string;
  amount: string;
  receivable: boolean;
}

export interface PostDated {
  voucher_id: string;
  kind: VoucherKind;
  number: string;
  date: string;
  particulars: string;
  amount: string;
}

export interface MsmeDue {
  ledger_id: string;
  party: string;
  reference: string;
  date: string;
  /** Bill date + 45 days. */
  limit: string;
  amount: string;
  msme_party: boolean;
}

export type Frequency = "monthly" | "quarterly" | "annual";

export interface CalendarSettings {
  frequency: Frequency | null;
  quarter_end: number | null;
  withholding: boolean | null;
  collection: boolean | null;
  incorporated?: string | null;
  entity?: Entity | null;
  fy_end_month?: number | null;
  vat_from?: string | null;
  vat_registered?: boolean | null;
  corporate_tax?: boolean | null;
  audit?: boolean | null;
  states?: StateFiling[];
}

export interface CalendarSettingsDto {
  saved: CalendarSettings;
  frequency: Frequency;
  quarter_end: number;
  withholding: boolean;
  collection: boolean;
  defaults: {
    frequency: Frequency;
    quarter_end: number;
    quarterly_allowed: boolean;
    fields: ProfileField[];
    entities: Entity[];
    period: PeriodChoice;
  } | null;
  has_withholding: boolean;
  company: CalendarCompany;
  fy_end_month: number;
  /** Saved at least once; else the desk offers to set it up. */
  profile_set: boolean;
}

export interface Desk {
  as_at: string;
  notes: NoteDto[];
  statutory: DueDto[];
  bills: BillDue[];
  post_dated: PostDated[];
  msme: MsmeDue[];
  settings: CalendarSettingsDto;
  can_write: boolean;
}

export interface DueCount {
  overdue: number;
  soon: number;
}

export const deskApi = {
  overview: (as_at: string, to: string) => call<Desk>("desk_overview", { as_at, to }),
  dueCount: (as_at: string) => call<DueCount>("desk_due_count", { as_at }),
  dueDates: (from: string, to: string) => call<DueDto[]>("desk_due_dates", { from, to }),
  notes: (link_kind?: LinkKind, link_id?: string) => call<NoteDto[]>("desk_notes", { link_kind: link_kind ?? null, link_id: link_id ?? null }),
  saveNote: (note: NoteInput) => call<NoteDto>("desk_save_note", { note }),
  setDone: (id: string, done: boolean) => call<NoteDto>("desk_set_done", { id, done }),
  deleteNote: (id: string) => call<void>("desk_delete_note", { id }),
  markFiled: (code: string, period_to: string, filed: boolean) => call<void>("desk_mark_filed", { code, period_to, filed }),
  calendarSettings: () => call<CalendarSettingsDto>("desk_calendar_settings"),
  setCalendarSettings: (settings: CalendarSettings) => call<CalendarSettingsDto>("desk_set_calendar_settings", { settings }),
};
