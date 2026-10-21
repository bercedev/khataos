// Entry point: fetch the keymap from Rust, build the shell, show startup.
import { api } from "./api";
import { h } from "./dom";
import { savedLanguage, setLanguage } from "./i18n";
import { Keymap } from "./keys";
import { Shell } from "./shell";
import { FindScreen, GoToScreen, MenuScreen, companyMenu, dataMenu, importMenu, languageMenu } from "./screens/launcher";
import { syncNow } from "./screens/company";
import { exchangeMenu, installSharing, shareMenu } from "./screens/sharing";
import { CountryScreen } from "./screens/country";
import { HomeScreen } from "./screens/home";
import { StartupScreen } from "./screens/startup";
import { startZatcaQueue } from "./screens/zatca";
import { initPlatform } from "./platform";
import { initDisplay } from "./theme";
import { installCrashReporting, offerCrashReport } from "./crash";
import "./mobile.css";

// Never leave a blank window: show uncaught errors and log them to stderr.
function fatal(mount: HTMLElement, what: unknown): void {
  const message = what instanceof Error ? `${what.message}\n${what.stack ?? ""}` : String(what);
  void api.reportError(message).catch(() => undefined);
  mount.replaceChildren(h("div.fatal", "KhataOS hit an error: ", message));
}

/**
 * The print window (opened by the `print_report` command): show the
 * khata-print document, then open the system print dialog (Save as PDF works
 * on every OS).
 */
async function printMode(mount: HTMLElement): Promise<void> {
  const html = await api.takePrintHtml();
  if (!html) {
    mount.replaceChildren(h("div.fatal", "Nothing to print. Close this window and press Ctrl+P on a report."));
    return;
  }
  const doc = new DOMParser().parseFromString(html, "text/html");
  document.title = doc.title || "KhataOS · Print";
  document.head.replaceChildren(...Array.from(doc.head.childNodes).map((n) => document.importNode(n, true)));
  document.body.replaceChildren(...Array.from(doc.body.childNodes).map((n) => document.importNode(n, true)));
  const screen = document.createElement("style");
  screen.textContent = "@media screen { body { margin: 12mm auto; max-width: 190mm; } }";
  document.head.append(screen);
  await api.printWindow();
}

async function main(): Promise<void> {
  const mount = document.getElementById("app")!;
  initPlatform(); // phones: touch layout and helpers; nothing on the desktop
  if ((window as { __KHATA_PRINT__?: boolean }).__KHATA_PRINT__) {
    window.addEventListener("unhandledrejection", (e) => fatal(mount, e.reason));
    await printMode(mount);
    return;
  }
  initDisplay(); // the saved theme before the first screen (the print window above stays print-neutral)
  window.addEventListener("error", (e) => {
    // Benign layout notice, not an exception: never worth blanking the window.
    if (!e.error && /^ResizeObserver loop/.test(e.message)) return;
    fatal(mount, e.error ?? e.message);
  });
  window.addEventListener("unhandledrejection", (e) => fatal(mount, e.reason));
  // This computer's last choice; a user's own applies when they sign in.
  setLanguage(savedLanguage() ?? "en");
  let bindings;
  try {
    bindings = await api.keymap();
  } catch (e) {
    mount.replaceChildren(h("div.fatal", "KhataOS couldn't start: ", String(e instanceof Error ? e.message : e)));
    return;
  }
  const shell = new Shell(new Keymap(bindings), mount);
  shell.setHooks({
    startup: () => new StartupScreen(shell),
    home: () => new HomeScreen(shell),
    country: () => new CountryScreen(shell),
    find: () => new FindScreen(shell),
    goTo: () => new GoToScreen(shell),
    companyMenu: () => companyMenu(shell),
    importMenu: () => importMenu(shell),
    dataMenu: () => dataMenu(shell),
    syncNow: () => syncNow(shell),
    exchangeMenu: () => exchangeMenu(shell),
    shareMenu: () => shareMenu(shell),
    moreMenu: (items) =>
      new MenuScreen(shell, "More…", items.map((i) => ({ id: i.action, label: i.label, sub: i.chord, data: () => shell.press(i.chord) }))),
    languageMenu: () => languageMenu(shell),
  });
  installSharing(shell);
  startZatcaQueue(shell);
  installCrashReporting(shell);
  try {
    await shell.reset(new StartupScreen(shell));
  } catch (e) {
    fatal(mount, e);
    return;
  }
  // Crashed last time: offer a report the user reviews (docs/crash-reporting.md).
  void offerCrashReport(shell);
}

void main();
