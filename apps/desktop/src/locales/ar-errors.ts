// Arabic for messages that come from Rust in English: command errors,
// khata-core's BookError / VoucherError, and a few statuses (docs/i18n.md,
// "Messages from Rust").
//
// The message id is the English text itself. Rust keeps writing plain
// English (logs, tests and the server stay as they are); the shell looks a
// message up here when the window is in Arabic (shell.ts `toast`). A message
// with no entry shows in English, left to right, as before.
//
// - EXACT: the whole message, character for character.
// - PATTERNS: a message with variable parts. `{name}` in the English matches
//   any text; in the Arabic it is filled with that text. The names say how:
//     {reason} {e} {m}  another message, translated in turn when it has an entry
//     {role} {need}     a role label (Owner, Accountant…), translated
//     anything else     data from the books or the user, kept as written in
//                       its own direction (an FSI…PDI isolate)
//   An Arabic text may use ICU plural blocks for a count:
//   "{n, plural, one {…} two {…} few {# …} many {# …} other {# …}}".
//   Patterns are tried in order: put the more specific first.
//
// A Rust test (export.rs `the_arabic_message_table_matches_rust`) checks
// that every English text here still appears in the Rust sources, so a
// reworded Rust message shows up as a failing test, not a silent English
// toast.
import { fill } from "../i18n";

export const EXACT: Record<string, string> = {
  // ------------------------------------------------ company and sign-in
  "no company is open": "لا توجد شركة مفتوحة.",
  "sign in to this company first": "سجّل الدخول إلى هذه الشركة أولًا.",
  "pick the company's country first": "اختر دولة الشركة أولًا.",
  "the company was closed": "أُغلقت الشركة.",
  "company name is required": "اسم الشركة مطلوب.",
  "not a KhataOS company file": "هذا ليس ملف شركة ⁦KhataOS⁩.",
  "This company is open in another KhataOS window or on another user account on this computer. Close it there first.": "هذه الشركة مفتوحة في نافذة ⁦KhataOS⁩ أخرى أو في حساب مستخدم آخر على هذا الحاسوب. أغلقها هناك أولًا.",
  "the app data folder is unknown": "مجلد بيانات التطبيق غير معروف.",
  "that user name and password don't match": "اسم المستخدم وكلمة المرور غير متطابقين.",
  "passwords need at least 8 characters": "يجب أن تتكون كلمة المرور من 8 أحرف على الأقل.",
  "give the new user a password": "حدّد كلمة مرور للمستخدم الجديد.",
  "you can't lower or switch off your own access; ask another Owner": "لا يمكنك خفض صلاحيتك أو إيقافها بنفسك؛ اطلب ذلك من مالك آخر.",
  "you can't remove yourself; ask another Owner": "لا يمكنك حذف نفسك؛ اطلب ذلك من مالك آخر.",
  "only the company's owner can manage access; this computer was invited": "مالك الشركة وحده يدير الصلاحيات؛ هذا الحاسوب انضم بدعوة.",

  // ---------------------------------------------------------- records
  "unknown voucher": "سند غير معروف.",
  "unknown ledger": "حساب غير معروف.",
  "unknown group": "مجموعة غير معروفة.",
  "unknown party": "طرف غير معروف.",
  "unknown party ledger": "حساب طرف غير معروف.",
  "unknown unit": "وحدة غير معروفة.",
  "unknown stock group": "مجموعة مخزون غير معروفة.",
  "unknown stock item": "صنف مخزون غير معروف.",
  "unknown godown": "مستودع غير معروف.",
  "ledger name is required": "اسم الحساب مطلوب.",
  "group name is required": "اسم المجموعة مطلوب.",
  "unit symbol is required": "رمز الوحدة مطلوب.",
  "ledger is used by vouchers and cannot be deleted": "الحساب مستخدم في سندات ولا يمكن حذفه.",
  "group has sub-groups or ledgers, or is a primary group, and cannot be deleted": "لا يمكن حذف المجموعة: فيها مجموعات فرعية أو حسابات، أو هي مجموعة رئيسية.",
  "a group cannot be moved under itself or under a group of a different kind": "لا يمكن نقل المجموعة تحت نفسها أو تحت مجموعة من نوع مختلف.",
  "a primary group can be renamed but not moved under another group": "يمكن إعادة تسمية المجموعة الرئيسية، لكن لا يمكن نقلها تحت مجموعة أخرى.",
  "in use by stock items or vouchers, so it cannot be deleted": "مستخدم في أصناف مخزون أو سندات، فلا يمكن حذفه.",
  "a unit can have at most 4 decimal places": "للوحدة 4 منازل عشرية كحد أقصى.",
  "batch names can have at most 40 characters": "اسم الدفعة 40 حرفًا كحد أقصى.",

  // --------------------------------------------------------- vouchers
  "a voucher needs at least one debit and one credit entry": "يحتاج السند إلى قيد مدين واحد وقيد دائن واحد على الأقل.",
  "amounts are too large": "المبالغ كبيرة جدًا.",
  "a Stock Journal needs stock lines, and only a Stock Journal can have them": "قيد المخزون يحتاج إلى أسطر مخزون، ولا يحملها غيره.",
  "contra vouchers may only use cash and bank ledgers": "سندات التحويل تستخدم حسابات النقد والبنوك فقط.",
  "a cancelled voucher cannot be altered": "لا يمكن تعديل سند ملغى.",
  "stock items can only be on Sales, Purchase, Credit Note and Debit Note vouchers": "أصناف المخزون تكون فقط في فواتير المبيعات والمشتريات والإشعارات الدائنة والمدينة.",
  "a bank date can only be set on cash and bank ledgers, on or after the voucher date": "تاريخ البنك يُحدَّد لحسابات النقد والبنوك فقط، في تاريخ السند أو بعده.",
  "this voucher is cancelled": "هذا السند ملغى.",
  "the period ends before it starts": "تنتهي الفترة قبل أن تبدأ.",
  "the previous period ends before it starts": "تنتهي الفترة السابقة قبل أن تبدأ.",
  "give both dates of the previous period, or neither": "أدخل تاريخَي الفترة السابقة كليهما، أو اتركهما فارغين.",
  "the quarter must be 1 to 4": "يجب أن يكون الربع من 1 إلى 4.",

  // ----------------------------------------------------- print, export
  "cheques are printed from a saved, uncancelled Payment voucher": "تُطبع الشيكات من سند صرف محفوظ وغير ملغى.",
  "this payment wasn't made from a bank ledger": "لم تُدفع هذه الدفعة من حساب بنكي.",
  "Printing isn't available on phones yet": "الطباعة غير متاحة على الهواتف بعد.",
  "Printing isn't available on phones yet: use Export (CSV) here, or print from KhataOS on a computer": "الطباعة غير متاحة على الهواتف بعد: استخدم التصدير (⁦CSV⁩) هنا، أو اطبع من ⁦KhataOS⁩ على حاسوب.",
  "exports must be saved as .csv files": "يجب حفظ الملفات المصدَّرة بصيغة ⁦.csv⁩.",
  "export this report as a table": "صدّر هذا التقرير كجدول.",
  "choose at least one Tally XML file": "اختر ملف ⁦Tally XML⁩ واحدًا على الأقل.",
  "pick a .csv or .txt file": "اختر ملف ⁦.csv⁩ أو ⁦.txt⁩.",
  "the file is larger than 20 MB": "حجم الملف أكبر من 20 ميغابايت.",

  // ---------------------------------------------------------- banking
  "bank statements are matched against a bank ledger": "تُطابَق كشوف البنك مع حساب بنكي.",
  "bank reconciliation is for bank and cash ledgers": "التسوية البنكية لحسابات البنوك والنقد.",
  "no heading row with a date and amount columns was found in this file": "لم يُعثر في هذا الملف على صف عناوين فيه عمود للتاريخ وأعمدة للمبالغ.",
  "no transactions were found under the heading row": "لم يُعثر على حركات تحت صف العناوين.",
  "save the statement as a .txt file": "احفظ الكشف بصيغة ⁦.txt⁩.",

  // -------------------------------------------------------------- tax
  "only party ledgers (Sundry Debtors and Sundry Creditors) have a tax registration": "التسجيل الضريبي لحسابات الأطراف فقط (المدينون والدائنون).",
  "say whether the tax is on sales (output) or purchases (input)": "حدّد هل الضريبة على المبيعات (مخرجات) أم على المشتريات (مدخلات).",
  "tax is worked out on Sales, Purchase, Credit Note and Debit Note": "تُحسب الضريبة على فواتير المبيعات والمشتريات والإشعارات الدائنة والمدينة.",
  "type an HSN/SAC code or a tax category": "اكتب رمز ⁦HSN/SAC⁩ أو فئة ضريبية.",
  "set the company's GSTIN first (Company → Tax registration)": "أدخل ⁦GSTIN⁩ الشركة أولًا (الشركة ← التسجيل الضريبي).",
  "set the company's emirate first (Company → Tax registration)": "حدّد إمارة الشركة أولًا (الشركة ← التسجيل الضريبي).",
  "enter the company's VAT Reg No in Company → Tax registration first": "أدخل رقم التسجيل الضريبي للشركة في الشركة ← التسجيل الضريبي أولًا.",
  "VAT 201 is for companies in the United Arab Emirates": "إقرار ⁦VAT 201⁩ للشركات في الإمارات العربية المتحدة.",
  "ZATCA e-invoices and the KSA VAT return are for companies in Saudi Arabia": "فواتير ⁦ZATCA⁩ الإلكترونية وإقرار ضريبة القيمة المضافة السعودي للشركات في المملكة العربية السعودية.",
  "onboard this computer with ZATCA first (ZATCA → Onboarding)": "اربط هذا الحاسوب مع ⁦ZATCA⁩ أولًا (⁦ZATCA⁩ ← الربط).",
  "enter the OTP from the Fatoora portal (valid for one hour)": "أدخل رمز ⁦OTP⁩ من بوابة فاتورة (صالح لمدة ساعة).",
  "the OTP is the number shown on the Fatoora portal": "رمز ⁦OTP⁩ هو الرقم الظاهر في بوابة فاتورة.",
  "this device is already onboarded; choose Start again to onboard a new device": "هذا الجهاز مربوط مسبقًا؛ اختر «البدء من جديد» لربط جهاز جديد.",
  "the building number has 4 digits (BR-KSA-37)": "رقم المبنى من 4 أرقام (⁦BR-KSA-37⁩).",
  "e-invoices are made from Sales, Credit Note and Debit Note vouchers": "تُنشأ الفواتير الإلكترونية من فواتير المبيعات والإشعارات الدائنة والمدينة.",
  "e-invoices are made from Sales and Credit Note vouchers": "تُنشأ الفواتير الإلكترونية من فواتير المبيعات والإشعارات الدائنة.",
  "a cancelled voucher has no e-invoice": "لا فاتورة إلكترونية لسند ملغى.",
  "GST returns are for companies in India": "إقرارات ⁦GST⁩ للشركات في الهند.",
  "GST F5 is for companies in Singapore": "إقرار ⁦GST F5⁩ للشركات في سنغافورة.",
  "the UK VAT return is for companies in the United Kingdom": "إقرار ضريبة القيمة المضافة البريطاني للشركات في المملكة المتحدة.",

  // ------------------------------------------------- sharing and sync
  "sync via: choose Office server or KhataOS relay": "المزامنة عبر: اختر خادم المكتب أو خادم ⁦KhataOS⁩ الوسيط.",
  "sync every: enter 1 to 1440 minutes": "المزامنة كل: أدخل من 1 إلى 1440 دقيقة.",
  "invite as Accountant or Viewer": "ادعُ بصفة محاسب أو مشاهد.",
  "this invite code has no valid company name": "رمز الدعوة هذا لا يحمل اسم شركة صالحًا.",
  "this invite code has no valid access token": "رمز الدعوة هذا لا يحمل رمز وصول صالحًا.",
  "this file is too large to be a KhataOS accountant file": "هذا الملف أكبر من أن يكون ملف محاسب من ⁦KhataOS⁩.",
  "expiry: leave it blank for no expiry, or enter 1 or more days": "الانتهاء: اتركه فارغًا لعدم الانتهاء، أو أدخل يومًا واحدًا أو أكثر.",
  // ------------------------------------------------ accountant's desk
  "a note needs a title": "الملاحظة تحتاج إلى عنوان.",
  "that note doesn't exist any more": "هذه الملاحظة لم تعد موجودة.",
  "a link needs both its kind and its id": "الربط يحتاج إلى نوعه ومعرّفه معاً.",
  "a quarter ends in a month from 1 to 12": "ينتهي الربع في شهر من 1 إلى 12.",
  "the range ends before it starts": "تنتهي الفترة قبل أن تبدأ.",
  "the financial year ends in a month from 1 to 12": "تنتهي السنة المالية في شهر من 1 إلى 12.",
};

const CHANGES = "{n, plural, zero {لا تغييرات} one {تغيير واحد} two {تغييران} few {# تغييرات} many {# تغييرًا} other {# تغيير}}";

export const PATTERNS: [string, string][] = [
  // Roles and sign-in (security.rs)
  ["{name} is signed in as {role}, which can't do this; it needs {need}", "{name} مسجّل الدخول بصفة {role}، وهي لا تسمح بهذا؛ يتطلب ذلك صلاحية {need}."],
  ["{name} has been switched off by the company's owner", "أوقف مالك الشركة حساب {name}."],
  ['there is no user "{name}"', "لا يوجد مستخدم «{name}»."],
  // khata-core BookError and VoucherError
  ['a ledger named "{name}" already exists', "يوجد حساب باسم «{name}» مسبقًا."],
  ['a group named "{name}" already exists', "توجد مجموعة باسم «{name}» مسبقًا."],
  ['a unit "{name}" already exists', "توجد وحدة «{name}» مسبقًا."],
  ['a stock group or item named "{name}" already exists', "توجد مجموعة مخزون أو صنف باسم «{name}» مسبقًا."],
  ['a godown named "{name}" already exists', "يوجد مستودع باسم «{name}» مسبقًا."],
  ['voucher number "{number}" is already used for this voucher type', "رقم السند «{number}» مستخدم مسبقًا لهذا النوع من السندات."],
  ["entry {index} has a non-positive amount", "مبلغ القيد {index} ليس موجبًا."],
  ["debits ({dr}) do not equal credits ({cr})", "المدين ({dr}) لا يساوي الدائن ({cr})."],
  ["bill allocations on entry {index} do not add up to its amount", "توزيعات الفواتير في القيد {index} لا تساوي مبلغه."],
  ["stock items on entry {index} do not add up to its amount, or have a non-positive quantity", "أصناف المخزون في القيد {index} لا تساوي مبلغه، أو فيها كمية غير موجبة."],
  ["quantity {qty} has more decimal places than its unit allows", "الكمية {qty} فيها منازل عشرية أكثر مما تسمح به وحدتها."],
  ['"{code}" is not an ISO 3166-1 alpha-2 country code', "«{code}» ليس رمز دولة من حرفين (⁦ISO 3166-1⁩)."],
  ["audit trail is broken at entry {n}", "سجل التدقيق منقطع عند القيد {n}."],
  ["invalid numbering: {reason}", "ترقيم غير صالح: {reason}"],
  ["invalid tax setting: {reason}", "إعداد ضريبي غير صالح: {reason}"],
  ["unknown ledger {id}", "حساب غير معروف {id}."],
  // Input checks (api.rs)
  ['invalid date "{text}" (expected YYYY-MM-DD)', "تاريخ غير صالح «{text}» (المتوقع ⁦YYYY-MM-DD⁩)."],
  ['invalid amount "{text}"', "مبلغ غير صالح «{text}»."],
  ['unknown voucher type "{text}"', "نوع سند غير معروف «{text}»."],
  ['there is no country pack for "{code}"', "لا توجد حزمة دولة للرمز «{code}»."],
  ["a file already exists at {path}", "يوجد ملف مسبقًا في {path}."],
  ["no company file at {path}", "لا يوجد ملف شركة في {path}."],
  ["cannot read {path}: {e}", "تعذّرت قراءة {path}: {e}"],
  ["cannot write {path}: {e}", "تعذّرت الكتابة في {path}: {e}"],
  ["stock line {n}: {reason}", "سطر المخزون {n}: {reason}"],
  ["line {n}: {reason}", "السطر {n}: {reason}"],
  ["entry {n}: {reason}", "القيد {n}: {reason}"],
  // Tax (tax.rs, gst.rs)
  ["set the company's {label} and state first (Company → Tax registration)", "أدخل {label} الشركة وولايتها أولًا (الشركة ← التسجيل الضريبي)."],
  ["the {label} is registered in region {code}, not {region}", "{label} مسجّل في المنطقة {code}، لا في {region}."],
  ['"{region}" is not a known region', "«{region}» ليست منطقة معروفة."],
  ['tax class "{class}" is not valid', "الفئة الضريبية «{class}» غير صالحة."],
  ["rates for {class} must be between 0 and 100%", "يجب أن تكون نسب {class} بين 0 و100%."],
  ["{name} has no HSN/SAC code or tax category, so no tax was worked out for it", "لا رمز ⁦HSN/SAC⁩ ولا فئة ضريبية لـ {name}، فلم تُحسب له ضريبة."],
  ["{label} {id} is invalid: {reason}", "{label} {id} غير صالح: {reason}"],
  // Sync statuses (sync.rs)
  ["couldn't reach the sync server (network error: {m})", "تعذّر الوصول إلى خادم المزامنة (خطأ في الشبكة: {m})."],
  ["Received {n} change(s)", "استُلمت التغييرات: " + CHANGES + "."],
  ["Sent {n} change(s)", "أُرسلت التغييرات: " + CHANGES + "."],
  ["Received {n} update(s) to notes and settings", "استُلمت تحديثات الملاحظات والإعدادات: {n}."],
  ["Sent {n} update(s) to notes and settings", "أُرسلت تحديثات الملاحظات والإعدادات: {n}."],
  ["Conflict: {local} change(s) here, {remote} on the server", "تعارض: " + CHANGES.replace("{n,", "{local,") + " هنا، و" + CHANGES.replace("{n,", "{remote,") + " على الخادم."],
];

/** Role labels as security.rs writes them, in Arabic. */
const ROLES: Record<string, string> = {
  Owner: "المالك",
  Accountant: "محاسب",
  "Data Entry": "مدخل بيانات",
  "Auditor (view only)": "مدقق (عرض فقط)",
  Auditor: "مدقق",
};

const RECURSE = new Set(["reason", "e", "m"]);
const ROLE = new Set(["role", "need"]);

type Compiled = { re: RegExp; names: string[]; ar: string };
let compiled: Compiled[] | null = null;

function compile(): Compiled[] {
  return PATTERNS.map(([en, ar]) => {
    const names: string[] = [];
    const source = en
      .split(/(\{\w+\})/)
      .map((piece) => {
        const name = /^\{(\w+)\}$/.exec(piece)?.[1];
        if (name) {
          names.push(name);
          return "(.+?)";
        }
        return piece.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      })
      .join("");
    return { re: new RegExp(`^${source}\\.?$`, "s"), names, ar };
  });
}

/** Data in its own direction inside Arabic text (first strong isolate). */
function isolate(v: string): string {
  return /^[\d.,\s%-]*$/.test(v) ? v : `⁨${v}⁩`;
}

/**
 * The Arabic for a message from Rust, or null when there is none. `text` is
 * the English exactly as Rust wrote it.
 */
export function arabicMessage(text: string, depth = 0): string | null {
  const exact = EXACT[text] ?? EXACT[text.replace(/\.$/, "")];
  if (exact) return exact;
  if (depth > 3) return null;
  compiled ??= compile();
  for (const p of compiled) {
    const m = p.re.exec(text);
    if (!m) continue;
    const params: Record<string, string | number> = {};
    p.names.forEach((name, i) => {
      const v = m[i + 1];
      if (RECURSE.has(name)) params[name] = arabicMessage(v, depth + 1)?.replace(/\.$/, "") ?? isolate(v);
      else if (ROLE.has(name)) params[name] = ROLES[v] ?? isolate(v);
      else if (/^\d+$/.test(v)) params[name] = Number(v);
      else params[name] = isolate(v);
    });
    return fill(p.ar, params, "ar");
  }
  return null;
}
