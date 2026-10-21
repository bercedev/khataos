// Crash reports (docs/crash-reporting.md). Uncaught page errors and Rust
// panics are saved on this device by src-tauri/src/crash.rs; nothing is sent.
// On the next start, and from "Report a problem", the user reads the exact
// issue text and then opens a pre-filled GitHub issue to submit themselves.
//
// The page tells Rust only codes and screen class names (never titles,
// which can hold ledger names, and never field values).
import { crashApi, type CrashContext, type CrashReport } from "./api";
import { h } from "./dom";
import { language, t } from "./i18n";
import { isMobile } from "./platform";
import { choose } from "./screens/sharing";
import type { Screen, Shell } from "./shell";
import "./crash.css";

const MAX_CRUMBS = 12;
const MAX_PER_RUN = 5;
const crumbs: string[] = [];
const seen = new Set<string>();
let shellRef: Shell | null = null;

/** A screen's class name, or null for anonymous screens (modals, plain objects). */
function className(s: Screen): string | null {
  const n = (s as object).constructor?.name ?? "";
  return n && n !== "Object" && /^[A-Za-z_$][\w$]{0,47}$/.test(n) ? n : null;
}

export function crashContext(): CrashContext {
  return { language: language(), country: shellRef?.company?.country?.code ?? "", screens: [...crumbs] };
}

/** Errors that aren't crashes: browser notices, cancelled dialogs, aborted work. */
function benign(reason: unknown, message: string): boolean {
  if (!message.trim() || message === "Script error.") return true;
  if (/^ResizeObserver loop/.test(message)) return true;
  if (reason instanceof DOMException && reason.name === "AbortError") return true;
  return /\b(?:user )?cancel(?:l)?ed\b/i.test(message);
}

function record(reason: unknown): void {
  const message = reason instanceof Error ? `${reason.name}: ${reason.message}` : String(reason ?? "");
  if (benign(reason, reason instanceof Error ? reason.message : message)) return;
  const stack = reason instanceof Error ? reason.stack ?? "" : "";
  const key = `${message}\n${stack.split("\n").find((l) => l.trim() && !l.includes(message)) ?? ""}`;
  if (seen.has(key) || seen.size >= MAX_PER_RUN) return;
  seen.add(key);
  void crashApi.recordJs(message, stack, crashContext()).catch(() => undefined);
}

/**
 * Note each screen opened (its class name) and save uncaught errors. Call
 * once, after the shell is built. Errors a screen catches and shows with
 * `shell.error` never get here.
 */
export function installCrashReporting(shell: Shell): void {
  shellRef = shell;
  const push = shell.push.bind(shell);
  shell.push = (s: Screen) => {
    const name = className(s);
    if (name && crumbs[crumbs.length - 1] !== name) {
      crumbs.push(name);
      if (crumbs.length > MAX_CRUMBS) crumbs.shift();
      void crashApi.context(crashContext()).catch(() => undefined);
    }
    return push(s);
  };
  window.addEventListener("error", (e) => record(e.error ?? e.message));
  window.addEventListener("unhandledrejection", (e) => record(e.reason));
}

/** Start-up: KhataOS crashed last time, so offer to report it. */
export async function offerCrashReport(shell: Shell): Promise<void> {
  let pending;
  try {
    pending = await crashApi.pending();
  } catch {
    return;
  }
  if (!pending.ask || pending.crashes.length === 0) return;
  const ids = pending.crashes.map((c) => c.id);
  const answer = await choose(shell, "KhataOS closed unexpectedly last time. Send a report?", [
    { key: "report", label: "Review & report", hint: "you see the text first" },
    { key: "later", label: "Not now" },
    { key: "never", label: "Don't ask again" },
  ]);
  try {
    if (answer === "report") {
      if (await reportProblem(shell, pending.crashes[0].id, pending.can_open_folder)) await crashApi.dismiss(ids, false);
    } else if (answer === "never") {
      await crashApi.dismiss(ids, true);
      shell.info(t("KhataOS won't ask again and won't save crash details on this device."));
    }
  } catch (e) {
    shell.error(e);
  }
}

/**
 * "Report a problem": show the exact issue text (a saved crash, or a blank
 * report with the version and OS), then open it on GitHub. Returns whether
 * the issue page was opened.
 */
export async function reportProblem(shell: Shell, crashId: string | null = null, canOpenFolder = false): Promise<boolean> {
  let report: CrashReport;
  try {
    report = await crashApi.reportUrl(crashId, crashContext());
  } catch (e) {
    shell.error(e);
    return false;
  }
  for (;;) {
    const choice = await review(shell, report, crashId !== null && canOpenFolder && !isMobile());
    if (choice === "folder") {
      await crashApi.openFolder().catch((e) => shell.error(e));
      continue;
    }
    if (choice !== "open") return false;
    try {
      await crashApi.openUrl(report.url);
    } catch (e) {
      shell.error(e);
      return false;
    }
    shell.info(t("The report is open in your browser. Check it and submit it on GitHub."), 8000);
    return true;
  }
}

/** The issue text as plain lines, exactly what goes into the form. */
function previewText(r: CrashReport): string {
  const none = t("(none)");
  return [
    `${t("Title")}: ${r.title}`,
    `${t("Version")}: ${r.version}`,
    `${t("Operating system")}: ${r.os}`,
    `${t("Platform")}: ${r.platform}`,
    `${t("Country pack")}: ${r.country || none}`,
    `${t("Language")}: ${r.language || none}`,
    "",
    r.details,
  ].join("\n");
}

/**
 * Review dialog. Enter (or the focused button) acts; Tab moves between the
 * buttons; arrows and Page Up/Down scroll the text; Esc cancels.
 */
function review(shell: Shell, r: CrashReport, folder: boolean): Promise<"open" | "folder" | null> {
  return new Promise((resolve) => {
    const title = t("Review the report");
    const text = h("pre.crash-preview", { tabIndex: -1, dir: "ltr" }, previewText(r));
    const finish = (v: "open" | "folder" | null) => {
      shell.closeModal(m);
      resolve(v);
    };
    const buttons = [
      h("button.crash-open", { type: "button", onclick: () => finish("open") }, t("Open on GitHub")),
      folder ? h("button", { type: "button", onclick: () => finish("folder") }, t("Show crash files")) : null,
      h("button", { type: "button", onclick: () => finish(null) }, t("Cancel")),
    ].filter((b): b is HTMLButtonElement => b !== null);
    let at = 0;
    const go = (i: number) => {
      at = (i + buttons.length) % buttons.length;
      buttons[at].focus();
    };
    buttons.forEach((b, i) => b.addEventListener("focus", () => (at = i)));
    const scroll = (dy: number) => text.scrollBy({ top: dy });
    const m: Screen = {
      title,
      el: h(
        "div.dialog.crash-review",
        { role: "dialog", "aria-label": title, "aria-modal": "true" },
        h("div.dialog-title", title),
        h("p", t("This is everything that will be filled in on GitHub. Nothing is sent until you submit the issue there, and you can still edit it.")),
        text,
        h("div.crash-buttons", ...buttons),
        h("p.muted", t("Enter opens GitHub · Tab moves · arrows scroll · Esc cancels")),
      ),
      contexts: () => ["Dialog"],
      focus: () => go(at),
      key: (e) => {
        const page = text.clientHeight * 0.9;
        if (e.chord === "Esc") finish(null);
        else if (e.chord === "Enter" || e.chord === "Space") buttons[at].click();
        else if (e.chord === "Tab" || e.chord === "Right") go(at + 1);
        else if (e.chord === "Shift+Tab" || e.chord === "Left") go(at - 1);
        else if (e.chord === "Down") scroll(40);
        else if (e.chord === "Up") scroll(-40);
        else if (e.chord === "PageDown") scroll(page);
        else if (e.chord === "PageUp") scroll(-page);
        else if (e.chord === "Home") text.scrollTop = 0;
        else if (e.chord === "End") text.scrollTop = text.scrollHeight;
        return true; // a dialog swallows every other key
      },
    };
    shell.openModal(m);
  });
}
