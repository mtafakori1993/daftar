//! Interface language support (English / Persian).
//!
//! The application is authored in English; every user-visible string is looked up in this
//! table when the interface language is Persian. Missing entries fall back to English, so a
//! partially translated interface is always coherent.

use std::sync::atomic::{AtomicBool, Ordering};

static PERSIAN: AtomicBool = AtomicBool::new(false);

/// Set the active interface language from a settings code (`en` or `fa`).
pub fn set_language(code: &str) {
    PERSIAN.store(code.trim().eq_ignore_ascii_case("fa"), Ordering::Relaxed);
}

/// Whether the interface is currently rendered in Persian.
pub fn is_persian() -> bool {
    PERSIAN.load(Ordering::Relaxed)
}

/// Normalise a stored language code to one of the supported values.
pub fn normalize(code: &str) -> &'static str {
    if code.trim().eq_ignore_ascii_case("fa") {
        "fa"
    } else {
        "en"
    }
}

/// The display name of a language code, in that language.
pub fn language_name(code: &str) -> &'static str {
    if normalize(code) == "fa" {
        "فارسی"
    } else {
        "English"
    }
}

/// Translate a user-visible string. Unknown strings are returned unchanged.
pub fn t(en: &str) -> &str {
    if !is_persian() {
        return en;
    }
    match FA.iter().find(|(source, _)| *source == en) {
        Some((_, persian)) => persian,
        None => en,
    }
}

/// Translate a `{}`-template and substitute the arguments in order.
pub fn tf(en: &str, args: &[&str]) -> String {
    let mut text = t(en).to_string();
    for arg in args {
        match text.find("{}") {
            Some(pos) => text.replace_range(pos..pos + 2, arg),
            None => break,
        }
    }
    text
}

/// English → Persian lookup table.
#[rustfmt::skip]
const FA: &[(&str, &str)] = &[
    ("A highlight with your note will be saved at this selection.", "یک برجسته همراه با یادداشت شما برای این انتخاب ذخیره میشود."),
    ("A native Rust desktop Markdown reader and knowledge studio. No JavaScript, WebView, Electron or Tauri.", "خوانندهای رومیزی و استودیوی دانش برای Markdown، نوشتهشده با Rust بومی. بدون JavaScript، WebView، Electron یا Tauri."),
    ("Actual minutes", "دقایق واقعی"),
    ("Add", "افزودن"),
    ("Add folder", "افزودن پوشه"),
    ("Add library", "افزودن کتابخانه"),
    ("Add milestone", "افزودن نقطهٔ عطف"),
    ("Add note", "افزودن یادداشت"),
    ("Add section", "افزودن بخش"),
    ("Add tag", "افزودن برچسب"),
    ("Add task", "افزودن کار"),
    ("Add URL", "افزودن نشانی"),
    ("Anchors survive file edits; unresolved anchors are kept, never silently deleted.", "لنگرها پس از ویرایش فایل باقی میمانند؛ لنگرهای حلنشده حفظ میشوند و هرگز بیدرخواست حذف نمیشوند."),
    ("Appearance", "ظاهر"),
    ("Archive", "بایگانی"),
    ("Archives contain ordinary Markdown files, attachments, a SQLite snapshot, and a versioned manifest.", "بایگانیها شامل فایلهای معمولی Markdown، پیوستها، یک تصویر SQLite و یک مانیفست نسخهدار هستند."),
    ("Back", "پشت"),
    ("Back up now", "اکنون پشتیبانگیری"),
    ("Backups", "پشتیبانها"),
    ("Bookmark saved.", "نشانک ذخیره شد."),
    ("Bookmark selection", "نشانکگذاری انتخاب"),
    ("Bookmarks", "نشانکها"),
    ("Cancel", "انصراف"),
    ("Categories", "دستهها"),
    ("Change library…", "تغییر کتابخانه…"),
    ("Clear", "پاک کردن"),
    ("Code copied to clipboard.", "کد در بریدهدان رونوشت شد."),
    ("Code font", "قلم کد"),
    ("Collections", "مجموعهها"),
    ("Confirm destructive actions", "تأیید کارهای مخرب"),
    ("Copy quote", "رونوشت نقلقول"),
    ("Copy selection", "رونوشت انتخاب"),
    ("Could not save the highlight.", "ذخیرهٔ برجسته ممکن نشد."),
    ("Create flashcard", "ساخت فلشکارت"),
    ("Data", "داده"),
    ("Daftar", "دفتر"),
    ("Daftar is MIT licensed. Bundled fonts: Inter, Vazirmatn, JetBrains Mono (SIL OFL). See assets/fonts license files and LICENSE.", "دفتر با مجوز MIT منتشر شده است. قلمهای همراه: Inter، Vazirmatn و JetBrains Mono (SIL OFL). پروندههای مجوز را در assets/fonts و LICENSE ببینید."),
    ("Default direction", "جهت پیشفرض"),
    ("Delete theme", "حذف پوسته"),
    ("Description and learning objectives…", "توضیح و اهداف یادگیری…"),
    ("Diagram rendering", "رندر نمودار"),
    ("Document", "سند"),
    ("Duplicate documents", "اسناد تکراری"),
    ("Duplicate theme", "تکثیر پوسته"),
    ("Edit note", "ویرایش یادداشت"),
    ("English", "English"),
    ("Estimated minutes", "دقایق تخمینی"),
    ("Explanation (optional)", "توضیح (اختیاری)"),
    ("Export as image", "برونریزی بهصورت تصویر"),
    ("Export library…", "برونریزی کتابخانه…"),
    ("Export PDF", "برونریزی PDF"),
    ("Export PNG", "برونریزی PNG"),
    ("Export PNG ZIP", "برونریزی ZIP تصویر PNG"),
    ("Flashcards", "فلشکارتها"),
    ("Folder", "پوشه"),
    ("Front", "رو"),
    ("Highlight color updated.", "رنگ برجسته بهروزرسانی شد."),
    ("Highlight removed.", "برجسته حذف شد."),
    ("Highlight saved.", "برجسته ذخیره شد."),
    ("Highlights", "برجستهها"),
    ("Highlights & Bookmarks", "برجستهها و نشانکها"),
    ("Honest limitations", "محدودیتهای صادقانه"),
    ("Implementation", "پیادهسازی"),
    ("Import", "درونریزی"),
    ("Import archive…", "درونریزی بایگانی…"),
    ("Import files", "درونریزی فایلها"),
    ("Import font…", "درونریزی قلم…"),
    ("Info", "اطلاعات"),
    ("Interface", "رابط"),
    ("Keyboard shortcuts", "کلیدهای میانبر"),
    ("Kind", "نوع"),
    ("Knowledge", "دانش"),
    ("Language", "زبان"),
    ("Last view", "آخرین نما"),
    ("Learned", "یادگرفته"),
    ("Library", "کتابخانه"),
    ("Library folder", "پوشهٔ کتابخانه"),
    ("Licenses", "مجوزها"),
    ("Mark diagram settings dirty", "علامتگذاری تنظیمات نمودار بهعنوان تغییرکرده"),
    ("Marks", "نشانهها"),
    ("Monitoring", "پایش"),
    ("Native Mermaid subset: flowchart/graph, sequence, class, state, ER, gantt, pie, mindmap and journey. Unsupported directives show an explicit error instead of a fake diagram.", "پشتیبانی بومی از زیرمجموعهای از Mermaid: فلوچارت/گراف، توالی، کلاس، حالت، ER، گانت، دایرهای، نقشهٔ ذهنی و سفر. دستورهای پشتیبانینشده بهجای نمایش نمودار جعلی، خطای صریح نشان میدهند."),
    ("Needs review", "نیازمند مرور"),
    ("New card", "کارت جدید"),
    ("New category", "دستهٔ جدید"),
    ("New collection", "مجموعهٔ جدید"),
    ("New folder", "پوشهٔ جدید"),
    ("New roadmap", "نقشهٔ راه جدید"),
    ("Next", "بعدی"),
    ("No bookmarks yet", "هنوز نشانکی وجود ندارد"),
    ("No cards", "کارتی وجود ندارد"),
    ("No collections yet.", "هنوز مجموعهای وجود ندارد."),
    ("No highlights yet", "هنوز برجستهای وجود ندارد"),
    ("No results", "نتیجهای یافت نشد"),
    ("No roadmaps yet", "هنوز نقشهٔ راهی وجود ندارد"),
    ("Note saved.", "یادداشت ذخیره شد."),
    ("Notes", "یادداشتها"),
    ("Open", "باز کردن"),
    ("Open another library…", "باز کردن کتابخانهٔ دیگر…"),
    ("Outline", "فهرست"),
    ("Plan technical learning with sections, tasks, milestones, prerequisites and progress.", "برنامهریزی یادگیری فنی با بخشها، کارها، نقاط عطف، پیشنیازها و پیشرفت."),
    ("Portable", "قابلحمل"),
    ("Portable mode · data beside executable", "حالت قابلحمل · دادهها کنار فایل اجرایی"),
    ("Previous", "قبلی"),
    ("Quote copied.", "نقلقول رونوشت شد."),
    ("Reading font", "قلم مطالعه"),
    ("Reading presets", "پیشتنظیمهای مطالعه"),
    ("Reading theme", "پوستهٔ مطالعه"),
    ("Recent", "اخیر"),
    ("Recent documents", "اسناد اخیر"),
    ("Reference", "مرجع"),
    ("Remember last document", "بهخاطر سپردن آخرین سند"),
    ("Remove from library", "حذف از کتابخانه"),
    ("Remove highlight", "حذف برجسته"),
    ("Rename", "تغییر نام"),
    ("Reset shortcuts", "بازنشانی میانبرها"),
    ("Roadmaps", "نقشههای راه"),
    ("Save", "ذخیره"),
    ("Save card", "ذخیرهٔ کارت"),
    ("Save note", "ذخیرهٔ یادداشت"),
    ("Save preset", "ذخیرهٔ پیشتنظیم"),
    ("Search", "جستوجو"),
    ("Search documents, headings, highlights, bookmarks, flashcards and roadmaps.", "جستوجو در اسناد، سرفصلها، برجستهها، نشانکها، فلشکارتها و نقشههای راه."),
    ("Search your knowledge", "جستوجو در دانش شما"),
    ("Select all", "انتخاب همه"),
    ("Select text and choose Bookmark.", "متن را انتخاب کنید و «نشانک» را بزنید."),
    ("Select text in the document to create a highlight.", "برای ساخت برجسته، متنی را در سند انتخاب کنید."),
    ("Selection copied.", "انتخاب رونوشت شد."),
    ("Settings", "تنظیمات"),
    ("Show hidden", "نمایش پنهانها"),
    ("Some anchors could not be relocated after the file changed. They are kept in the database and shown here for review.", "برخی لنگرها پس از تغییر فایل جابهجا نشدند. آنها در پایگاه داده نگه داشته و برای بازبینی در همینجا نمایش داده میشوند."),
    ("Space flip · ←/→ navigate · L learned · R review · Esc exit", "Space ورقزدن · ←/→ جابهجایی · L یادگرفته · R مرور · Esc خروج"),
    ("Standard data directory", "پوشهٔ دادهٔ استاندارد"),
    ("Could not open {}: {}", "باز کردن {} ممکن نشد: {}"),
    ("Keep both", "نگهداشتن هر دو"),
    ("No heading or footnote with anchor `{}`.", "سرفصل یا پانوشتی با لنگر «{}» یافت نشد."),
    ("Replace", "جایگزینی"),
    ("Restore application settings", "بازگردانی تنظیمات برنامه"),
    ("Restore flashcards", "بازگردانی فلشکارتها"),
    ("Restore study roadmaps", "بازگردانی نقشههای راه"),
    ("Skip existing", "نادیده گرفتن موارد موجود"),
    ("Unsupported link target: {}", "مقصد پیوند پشتیبانینشده: {}"),
    ("Linked document `{}` is not in this library.", "سند پیوندیافتهٔ «{}» در این کتابخانه نیست."),
    ("Source · {}", "منبع · {}"),
    ("Start your roadmap", "نقشهٔ راه خود را آغاز کنید"),
    ("Startup", "آغاز"),
    ("Status", "وضعیت"),
    ("Study all", "مطالعهٔ همه"),
    ("Study roadmaps", "نقشههای راه یادگیری"),
    ("Study selected", "مطالعهٔ انتخابشده"),
    ("Tags", "برچسبها"),
    ("Tags…", "برچسبها…"),
    ("Text alignment", "ترازبندی متن"),
    ("The interface is currently English. Markdown reading, selection, highlights, flashcards and exports fully support Persian/Arabic and mixed RTL content.", "رابط اکنون فارسی است. خواندن، انتخاب، برجستهها، فلشکارتها و برونریزیها بهطور کامل از محتوای فارسی/عربی و دوجهته پشتیبانی میکنند."),
    ("Theme", "پوسته"),
    ("This document has no headings.", "این سند سرفصلی ندارد."),
    ("Title", "عنوان"),
    ("Typography", "تایپوگرافی"),
    ("Unresolved anchors", "لنگرهای حلنشده"),
    ("Use Ctrl, Shift, Alt and a key name, for example `Ctrl+Shift+H`.", "از Ctrl، Shift، Alt و نام یک کلید استفاده کنید؛ مثلاً `Ctrl+Shift+H`."),
    ("Your note…", "یادداشت شما…"),
    ("egui/eframe · cosmic-text (HarfBuzz-class shaping + Unicode bidi) · rustybuzz · SQLite/FTS5 · syntect · tiny-skia · pdf-writer with embedded TrueType fonts.", "egui/eframe · cosmic-text (ترکیببندی در کلاس HarfBuzz + دوجهتهٔ یونیکد) · rustybuzz · SQLite/FTS5 · syntect · tiny-skia · pdf-writer با قلمهای TrueType جاسازیشده."),
    ("{:.0}% complete", "{}٪ کامل"),
    ("{:.0}% read · {} min", "{}٪ خواندهشده · {} دقیقه"),
    ("{:.0}% · {}/{} tasks · {} estimated", "{}٪ · {}/{} کار · {} تخمینی"),
    ("{} completed · {} in progress · {} not started · {} skipped", "{} انجامشده · {} در حال انجام · {} شروعنشده · {} ردشده"),
    ("{} docs · {} highlights · {} cards", "{} سند · {} برجسته · {} کارت"),
    ("{} est", "{} تخمینی"),
    ("{} cards · {} learned · {} marked for review", "{} کارت · {} یادگرفته · {} برای مرور علامتخورده"),
    ("{} reviewed · {} learned", "{} مرور شد · {} یادگرفته شد"),
    ("Data directory: {}", "پوشهٔ داده: {}"),
    ("Database integrity: {}", "یکپارچگی پایگاه داده: {}"),
    ("Estimated {} · actual {}", "تخمینی {} · واقعی {}"),
    ("Headings: {}", "سرفصلها: {}"),
    ("Highlights: {} · Bookmarks: {}", "برجستهها: {} · نشانکها: {}"),
    ("Library location: {}", "مکان کتابخانه: {}"),
    ("Version {}", "نسخهٔ {}"),
    ("Backup written to {}", "پشتیبان در {} نوشته شد"),
    ("Backup failed: {error}", "پشتیبانگیری شکست خورد: {}"),
    ("Could not serialise settings: {error}", "سریالسازی تنظیمات ممکن نشد: {}"),
    ("{message}  ({done}/{total})", "{}  ({}/{})"),
    ("Indexing finished with {} warning(s).", "نمایهسازی با {} هشدار پایان یافت."),
    ("Export complete: {}", "برونریزی کامل شد: {}"),
    ("Export failed: {error}", "برونریزی شکست خورد: {}"),
    ("Import failed: {error}", "درونریزی شکست خورد: {}"),
    ("{job} failed: {error}", "{} شکست خورد: {}"),
    ("{job} failed", "{} شکست خورد"),
    ("Could not create Imported folder: {error}", "ساخت پوشهٔ Imported ممکن نشد: {}"),
    ("Could not import {name}: {error}", "درونریزی {} ممکن نشد: {}"),
    ("Imported {copied} Markdown file(s). Indexing…", "{} فایل Markdown درونریزی شد. در حال نمایهسازی…"),
    ("Cannot open folder: {error}", "باز کردن پوشه ممکن نشد: {}"),
    ("Added {count} file(s) from `{name}` into the library.", "{} فایل از «{}» به کتابخانه افزوده شد."),
    ("Could not copy folder: {error}", "رونوشت پوشه ممکن نشد: {}"),
    ("Could not add library: {error}", "افزودن کتابخانه ممکن نشد: {}"),
    ("Image exported to {}", "تصویر در {} برونریزی شد"),
    ("Could not write image: {error}", "نوشتن تصویر ممکن نشد: {}"),
    ("Image export failed: {error}", "برونریزی تصویر شکست خورد: {}"),
    ("Diagram exported to {}", "نمودار در {} برونریزی شد"),
    ("Font `{name}` imported.", "قلم «{}» درونریزی شد."),
    ("Invalid archive: {error}", "بایگانی نامعتبر: {}"),
    ("Could not create collection: {error}", "ساخت مجموعه ممکن نشد: {}"),
    ("Could not create category: {error}", "ساخت دسته ممکن نشد: {}"),
    ("Could not create roadmap: {error}", "ساخت نقشهٔ راه ممکن نشد: {}"),
    ("Could not save card: {error}", "ذخیرهٔ کارت ممکن نشد: {}"),
    ("Cannot read folder: {error}", "خواندن پوشه ممکن نشد: {}"),
    ("New folder {n}", "پوشهٔ جدید {}"),
    ("Could not create folder: {error}", "ساخت پوشه ممکن نشد: {}"),
    ("Document not found.", "سند یافت نشد."),
    ("Folder is already inside the library. Re-indexing…", "پوشه از قبل داخل کتابخانه است؛ نمایهسازی دوباره…"),
    ("Library added.", "کتابخانه افزوده شد."),
    ("Another background task is running.", "کار پسزمینهٔ دیگری در حال اجراست."),
    ("Diagram is no longer available.", "نمودار دیگر در دسترس نیست."),
    ("Diagram could not be rendered.", "رندر نمودار ممکن نشد."),
    ("Nothing selected to export.", "چیزی برای برونریزی انتخاب نشده است."),
    ("Archive has validation errors and cannot be imported.", "بایگانی خطاهای اعتبارسنجی دارد و درونریزی نمیشود."),
    ("Document removed from the library. The file was not deleted.", "سند از کتابخانه حذف شد؛ فایل پاک نشد."),
    ("Flashcard saved.", "فلشکارت ذخیره شد."),
    ("Import library archive", "درونریزی بایگانی کتابخانه"),
    ("Delete flashcard", "حذف فلشکارت"),
    ("Delete this flashcard?", "این فلشکارت حذف شود؟"),
    ("Delete", "حذف"),
    ("Delete highlight", "حذف برجسته"),
    ("Delete this highlight? The source document is not modified.", "این برجسته حذف شود؟ سند اصلی تغییر نمیکند."),
    ("Delete bookmark", "حذف نشانک"),
    ("Delete this bookmark?", "این نشانک حذف شود؟"),
    ("Collection name", "نام مجموعه"),
    ("Category name", "نام دسته"),
    ("Rename document", "تغییر نام سند"),
    ("Tag", "برچسب"),
    ("Remove document", "حذف سند"),
    ("Remove", "حذف"),
    ("Roadmap title", "عنوان نقشهٔ راه"),
    ("Delete roadmap", "حذف نقشهٔ راه"),
    ("Delete roadmap item", "حذف مورد نقشهٔ راه"),
    ("Delete this item and its children?", "این مورد و زیرمجموعههایش حذف شوند؟"),
    ("Linked document", "سند پیوندیافته"),
    ("This archive cannot be imported:", "این بایگانی درونریزی نمیشود:"),
    ("Fit diagrams to the reading column", "جایدادن نمودارها در ستون مطالعه"),
    ("Use dark diagram palette in dark mode", "استفاده از پالت تیرهٔ نمودار در حالت تیره"),
    ("Show edge labels where available", "نمایش برچسب یالها در صورت وجود"),
    ("Watch library files for external edits", "پایش فایلهای کتابخانه برای ویرایشهای بیرونی"),
    ("Index new and changed Markdown files", "نمایهسازی فایلهای جدید و تغییرکردهٔ Markdown"),
    ("Back up the database on startup", "پشتیبانگیری از پایگاه داده در آغاز"),
    ("File name", "نام فایل"),
    ("Ready", "آماده"),
    ("Export failed", "برونریزی شکست خورد"),
    ("Import complete", "درونریزی کامل شد"),
    ("Import failed", "درونریزی شکست خورد"),
    ("Exporting library…", "در حال برونریزی کتابخانه…"),
    ("Importing library…", "در حال درونریزی کتابخانه…"),
    ("Library indexed: {} added, {} updated, {} missing", "نمایهسازی کتابخانه: {} افزوده، {} بهروزشده، {} گمشده"),
    ("Library exported ({} documents, {} attachments).", "کتابخانه برونریزی شد ({} سند، {} پیوست)."),
    ("Imported {} documents, {} highlights, {} flashcards, {} roadmaps.", "{} سند، {} برجسته، {} فلشکارت و {} نقشهٔ راه درونریزی شد."),
    ("Daftar {} · archive format v{} · {} documents · {} attachments · {} words", "دفتر {} · قالب بایگانی نسخهٔ {} · {} سند · {} پیوست · {} واژه"),
    ("{} new document(s), {} already present. Archive size {}.", "{} سند جدید، {} از قبل موجود. حجم بایگانی {}."),
    ("{} documents · {} collections · {} categories · {} words indexed", "{} سند · {} مجموعه · {} دسته · {} واژه نمایهشده"),
    ("Direction: {} · Progress: {:.0}%", "جهت: {} · پیشرفت: {}٪"),
    ("Selection", "انتخاب"),
    ("All cards", "همهٔ کارتها"),
    ("BACK", "پشت"),
    ("FRONT", "رو"),
    ("Missing file", "فایل گمشده"),
    ("Cycle status", "چرخش وضعیت"),
    ("Link a document", "پیوند یک سند"),
    ("Title cannot be empty.", "عنوان نمیتواند خالی باشد."),
    ("Could not save roadmap item.", "ذخیرهٔ مورد نقشهٔ راه ممکن نشد."),
    ("No library", "کتابخانهای نیست"),
    ("Custom theme", "پوستهٔ سفارشی"),
    ("Background", "پسزمینه"),
    ("Text", "متن"),
    ("Muted", "کمرنگ"),
    ("Headings", "سرفصلها"),
    ("Links", "پیوندها"),
    ("Code background", "پسزمینهٔ کد"),
    ("Code text", "متن کد"),
    ("Quote background", "پسزمینهٔ نقلقول"),
    ("Quote border", "حاشیهٔ نقلقول"),
    ("Table border", "حاشیهٔ جدول"),
    ("My preset", "پیشتنظیم من"),
    ("Cycle text direction (automatic, RTL, LTR)", "چرخش جهت متن (خودکار، راستبهچپ، چپبهراست)"),
    ("Copy code", "رونوشت کد"),
    ("Diagram could not be rendered", "رندر نمودار ممکن نشد"),
    ("Open diagram viewer", "باز کردن نمایشگر نمودار"),
    ("Mermaid diagram", "نمودار Mermaid"),
    ("Add a folder to the library", "افزودن پوشه به کتابخانه"),
    ("Exported {} card(s), {} KB.", "{} کارت برونریزی شد، {} KB."),
    ("Front and back cannot be empty.", "رو و پشت نمیتوانند خالی باشند."),
    ("My Library", "کتابخانهٔ من"),
    ("Home", "خانه"),
    ("Documents", "اسناد"),
    ("Desktop", "میزکار"),
    ("Downloads", "بارگیریها"),
    ("Working folder", "پوشهٔ کاری"),
    ("Select", "انتخاب"),
    ("That file already exists. Press Save again to replace it.", "این فایل از قبل وجود دارد. برای جایگزینی دوباره «ذخیره» را بزنید."),
    ("Export selected cards as PNG ZIP", "برونریزی کارتهای انتخابشده بهصورت ZIP تصویر PNG"),
    ("Export selected cards as PDF", "برونریزی کارتهای انتخابشده بهصورت PDF"),
    ("Choose a folder for PNG cards", "انتخاب پوشه برای کارتهای PNG"),
    ("Export selection as PNG", "برونریزی انتخاب بهصورت PNG"),
    ("Export diagram as PNG", "برونریزی نمودار بهصورت PNG"),
    ("This archive cannot be imported", "این بایگانی درونریزی نمیشود"),
    ("• {}", "• {}"),
    ("Mermaid support is a native subset; unsupported directives display an explicit error.", "پشتیبانی Mermaid یک زیرمجموعهٔ بومی است؛ دستورهای پشتیبانینشده خطای صریح نشان میدهند."),
    ("Mathematical expressions use a practical TeX-to-Unicode fallback rather than a full MathJax engine.", "عبارتهای ریاضی با تبدیل کاربردی TeX به یونیکد نمایش داده میشوند، نه با موتور کامل MathJax."),
    ("The interface language can be English or Persian; document content always keeps full RTL/LTR support.", "زبان رابط میتواند انگلیسی یا فارسی باشد؛ محتوای سند همیشه پشتیبانی کامل راستبهچپ/چپبهراست را حفظ میکند."),
    ("PDF export embeds full OFL fonts rather than subsetting them (larger files, correct shaping).", "برونریزی PDF قلمهای کامل OFL را جاسازی میکند و آنها را زیرمجموعه نمیکند (فایلهای بزرگتر، ترکیببندی درست)."),
    ("Table text is rendered but selection/highlighting inside tables is not supported.", "متن جدول نمایش داده میشود، اما انتخاب/برجستهسازی درون جدولها پشتیبانی نمیشود."),
    ("Filter cards…", "پالایش کارتها…"),
    ("Filter documents by title or tag…", "پالایش اسناد بر پایهٔ عنوان یا برچسب…"),
    ("Filter quotes and notes…", "پالایش نقلقولها و یادداشتها…"),
    ("Search the library…", "جستوجو در کتابخانه…"),
    ("Search the library…  Ctrl+F", "جستوجو در کتابخانه…  Ctrl+F"),
    ("All", "همه"),
    ("Preset name", "نام پیشتنظیم"),
    ("comma separated", "جداشده با کاما"),
    ("https://…", "https://…"),
    ("Could not load image: {}", "بارگذاری تصویر ممکن نشد: {}"),
    ("Could not start file watcher: {}", "راهاندازی پایشگر فایل ممکن نشد: {}"),
    ("Could not watch library folder: {}", "پایش پوشهٔ کتابخانه ممکن نشد: {}"),
    ("Footnotes", "پانوشتها"),
    ("Images from the web are not loaded. Save the image locally and use a relative path.", "تصاویر وب بارگذاری نمیشوند. تصویر را بهصورت محلی ذخیره کنید و از مسیر نسبی استفاده کنید."),
    ("Importing library", "در حال درونریزی کتابخانه"),
    ("Indexing", "نمایهسازی"),
    ("Indexing {}", "نمایهسازی {}"),
    ("Indexing cancelled.", "نمایهسازی لغو شد."),
    ("Library folder is missing: {}", "پوشهٔ کتابخانه یافت نشد: {}"),
    ("Marked as learned.", "بهعنوان یادگرفته علامت خورد."),
    ("Marked for review.", "برای مرور علامت خورد."),
    ("No cards to study.", "کارتی برای مطالعه وجود ندارد."),
    ("Updating library", "بهروزرسانی کتابخانه"),
    ("Use folder", "استفاده از پوشه"),
    ("Add library folder", "افزودن پوشهٔ کتابخانه"),
    ("Exporting library", "در حال برونریزی کتابخانه"),
    ("Open Markdown file", "باز کردن فایل Markdown"),
    ("Add folder to library", "افزودن پوشه به کتابخانه"),
    ("Search library", "جستوجو در کتابخانه"),
    ("Command palette", "پالت فرمان"),
    ("Toggle library panel", "باز/بستن پنل کتابخانه"),
    ("Toggle outline panel", "باز/بستن پنل فهرست"),
    ("Toggle light/dark theme", "تغییر پوستهٔ روشن/تیره"),
    ("Increase reading font size", "افزایش اندازهٔ قلم مطالعه"),
    ("Decrease reading font size", "کاهش اندازهٔ قلم مطالعه"),
    ("Cycle document direction", "چرخش جهت سند"),
    ("Highlight selection", "برجستهسازی انتخاب"),
    ("Bookmark position", "نشانکگذاری موقعیت"),
    ("Study: flip card", "مطالعه: ورقزدن کارت"),
    ("Study: next card", "مطالعه: کارت بعدی"),
    ("Study: previous card", "مطالعه: کارت قبلی"),
    ("Study: mark learned", "مطالعه: علامت یادگرفته"),
    ("Export current item", "برونریزی مورد جاری"),
    ("General", "عمومی"),
    ("Reading", "خواندن"),
    ("Diagrams", "نمودارها"),
    ("Keyboard", "صفحهکلید"),
    ("About", "درباره"),
    ("Light", "روشن"),
    ("Dark", "تیره"),
    ("Monospace", "تکعرض"),
    ("Proportional", "متناسب"),
    ("Left", "چپ"),
    ("Center", "وسط"),
    ("Right", "راست"),
    ("Justified", "همتراز"),
    ("Auto", "خودکار"),
    ("Font size", "اندازهٔ قلم"),
    ("Font weight", "وزن قلم"),
    ("Line height", "ارتفاع خط"),
    ("Paragraph spacing", "فاصلهٔ بندها"),
    ("Letter spacing", "فاصلهٔ حروف"),
    ("Heading spacing", "فاصلهٔ سرفصلها"),
    ("Reading width", "عرض مطالعه"),
    ("Code font size", "اندازهٔ قلم کد"),
    ("Maximum diagram height", "بیشینهٔ ارتفاع نمودار"),
    ("Default zoom", "بزرگنمایی پیشفرض"),
    ("Pan speed", "سرعت جابهجایی"),
    ("File watcher debounce (ms)", "تأخیر پایشگر فایل (میلیثانیه)"),
    ("Recent documents shown", "تعداد اسناد اخیر نمایشدادهشده"),
    ("Backups to keep", "تعداد پشتیبانهای نگهداشتهشده"),
    ("Match system", "هماهنگ با سامانه"),
    ("Document direction", "جهت سند"),
    ("Justify", "همتراز"),
    ("Portable mode", "حالت قابلحمل"),
    ("Standard mode", "حالت استاندارد"),
    ("Import Markdown files", "درونریزی فایلهای Markdown"),
    ("Choose library folder", "انتخاب پوشهٔ کتابخانه"),
    ("Import a font file", "درونریزی فایل قلم"),
    ("Export library as ZIP", "برونریزی کتابخانه بهصورت ZIP"),
    ("Import Daftar library ZIP", "درونریزی ZIP کتابخانهٔ دفتر"),
    ("{} selected", "{} انتخابشده"),
    ("{} words · {} min read", "{} واژه · {} دقیقه مطالعه"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default_and_passthrough() {
        set_language("en");
        assert!(!is_persian());
        assert_eq!(t("Library"), "Library");
        assert_eq!(t("Not translated at all"), "Not translated at all");
    }

    #[test]
    fn persian_translates_known_strings() {
        set_language("fa");
        assert!(is_persian());
        assert_eq!(t("Library"), "کتابخانه");
        assert_eq!(t("Settings"), "تنظیمات");
        assert_eq!(t("Unknown section"), "Unknown section");
        set_language("en");
    }

    #[test]
    fn templates_substitute_positionally() {
        set_language("fa");
        assert_eq!(tf("{} selected", &["3"]), "3 انتخابشده");
        assert_eq!(tf("{} docs · {} highlights · {} cards", &["4", "2", "7"]), "4 سند · 2 برجسته · 7 کارت");
        set_language("en");
        assert_eq!(tf("{} docs · {} highlights · {} cards", &["4", "2", "7"]), "4 docs · 2 highlights · 7 cards");
    }

    #[test]
    fn language_codes_normalise() {
        assert_eq!(normalize("fa"), "fa");
        assert_eq!(normalize("FA"), "fa");
        assert_eq!(normalize("fa-IR"), "en");
        assert_eq!(normalize(""), "en");
        assert_eq!(language_name("fa"), "فارسی");
        assert_eq!(language_name("en"), "English");
    }

    #[test]
    fn table_has_no_duplicate_or_missing_keys() {
        for (source, persian) in FA {
            assert!(!source.is_empty(), "empty source key");
            assert!(!persian.is_empty(), "empty translation for {source}");
        }
        let mut keys: Vec<&str> = FA.iter().map(|(source, _)| *source).collect();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate keys in the translation table");
    }
}
