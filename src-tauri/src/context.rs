//! Which app the text is going into. The cleanup step uses this to pick a
//! writing style: a chat message, an email and a code comment should not all
//! read the same.

use objc2_app_kit::NSWorkspace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Chat,
    Email,
    Code,
    Notes,
    Assistant,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContext {
    pub name: String,
    pub bundle_id: String,
    pub category: Category,
}

impl Default for AppContext {
    fn default() -> Self {
        Self::unknown()
    }
}

impl AppContext {
    pub fn unknown() -> Self {
        Self {
            name: "Unknown app".into(),
            bundle_id: String::new(),
            category: Category::Other,
        }
    }
}

/// The app that currently has keyboard focus. Needs no special permission.
pub fn frontmost() -> AppContext {
    let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return AppContext::unknown();
    };
    let bundle_id = app
        .bundleIdentifier()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let name = app
        .localizedName()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown app".into());
    AppContext {
        category: categorise(&bundle_id),
        name,
        bundle_id,
    }
}

const BROWSERS: &[&str] = &[
    "com.apple.Safari",
    "com.apple.SafariTechnologyPreview",
    "com.google.Chrome",
    "com.google.Chrome.beta",
    "com.google.Chrome.canary",
    "company.thebrowser.Browser",
    "company.thebrowser.dia",
    "com.brave.Browser",
    "com.microsoft.edgemac",
    "org.mozilla.firefox",
    "com.operasoftware.Opera",
    "com.vivaldi.Vivaldi",
    "app.zen-browser.zen",
];

/// Is this app a web browser? Then the site open in it says more about the
/// writing style than the browser does.
pub fn is_browser(bundle_id: &str) -> bool {
    BROWSERS.iter().any(|id| id.eq_ignore_ascii_case(bundle_id))
}

/// Sites recognised in a browser: the end of the address, the name to use,
/// and how to write for it.
const SITES: &[(&str, &str, Category)] = &[
    ("mail.google.com", "Gmail", Category::Email),
    ("outlook.live.com", "Outlook", Category::Email),
    ("outlook.office.com", "Outlook", Category::Email),
    ("outlook.office365.com", "Outlook", Category::Email),
    ("mail.proton.me", "Proton Mail", Category::Email),
    ("app.fastmail.com", "Fastmail", Category::Email),
    ("mail.superhuman.com", "Superhuman", Category::Email),
    ("mail.yahoo.com", "Yahoo Mail", Category::Email),
    ("app.slack.com", "Slack", Category::Chat),
    ("discord.com", "Discord", Category::Chat),
    ("web.whatsapp.com", "WhatsApp", Category::Chat),
    ("teams.microsoft.com", "Microsoft Teams", Category::Chat),
    ("teams.live.com", "Microsoft Teams", Category::Chat),
    ("messenger.com", "Messenger", Category::Chat),
    ("web.telegram.org", "Telegram", Category::Chat),
    ("chat.google.com", "Google Chat", Category::Chat),
    ("claude.ai", "Claude", Category::Assistant),
    ("chatgpt.com", "ChatGPT", Category::Assistant),
    ("chat.openai.com", "ChatGPT", Category::Assistant),
    ("gemini.google.com", "Gemini", Category::Assistant),
    ("aistudio.google.com", "Google AI Studio", Category::Assistant),
    ("perplexity.ai", "Perplexity", Category::Assistant),
    ("copilot.microsoft.com", "Copilot", Category::Assistant),
    ("grok.com", "Grok", Category::Assistant),
    ("chat.mistral.ai", "Le Chat", Category::Assistant),
    ("notion.so", "Notion", Category::Notes),
    ("notion.site", "Notion", Category::Notes),
    ("docs.google.com", "Google Docs", Category::Notes),
    ("keep.google.com", "Google Keep", Category::Notes),
];

/// The names those sites put at the end of their page titles, for when the
/// address cannot be read.
const TITLE_NAMES: &[(&str, &str, Category)] = &[
    ("gmail", "Gmail", Category::Email),
    ("outlook", "Outlook", Category::Email),
    ("proton mail", "Proton Mail", Category::Email),
    ("slack", "Slack", Category::Chat),
    ("discord", "Discord", Category::Chat),
    ("whatsapp", "WhatsApp", Category::Chat),
    ("messenger", "Messenger", Category::Chat),
    ("microsoft teams", "Microsoft Teams", Category::Chat),
    ("telegram", "Telegram", Category::Chat),
    ("google chat", "Google Chat", Category::Chat),
    ("claude", "Claude", Category::Assistant),
    ("chatgpt", "ChatGPT", Category::Assistant),
    ("gemini", "Gemini", Category::Assistant),
    ("perplexity", "Perplexity", Category::Assistant),
    ("notion", "Notion", Category::Notes),
    ("google docs", "Google Docs", Category::Notes),
    ("google keep", "Google Keep", Category::Notes),
];

/// Browser names some browsers add to the end of a window title.
const BROWSER_NAMES: &[&str] = &[
    "google chrome",
    "safari",
    "arc",
    "brave",
    "microsoft edge",
    "mozilla firefox",
    "firefox",
    "opera",
    "vivaldi",
];

/// Which known site a browser page is, from its address or, failing that,
/// its title. Only the site's name ever leaves this function: never the
/// address or the title, which can hold an email subject or a person's name.
pub fn site(url: Option<&str>, title: Option<&str>) -> Option<(&'static str, Category)> {
    if let Some(host) = url.and_then(host_of) {
        let found = SITES
            .iter()
            .find(|(domain, ..)| host == *domain || host.ends_with(&format!(".{domain}")));
        if let Some((_, name, category)) = found {
            return Some((name, *category));
        }
    }
    let mut title = title?.trim().to_lowercase();
    for separator in [" - ", " – ", " — ", " · "] {
        title = title.replace(separator, " | ");
    }
    let mut parts: Vec<&str> = title
        .split(" | ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.last().is_some_and(|last| BROWSER_NAMES.contains(last)) {
        parts.pop();
    }
    // Sites put their own name last, or the whole title is just the name.
    let last = parts.last()?;
    TITLE_NAMES
        .iter()
        .find(|(name, ..)| last == name)
        .map(|(_, name, category)| (*name, *category))
}

/// The host of a web address, lower case: "https://Mail.Google.com/x" gives
/// "mail.google.com". Anything that is not a web address gives nothing.
fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_lowercase())
}

/// Map a bundle identifier to a writing context. Browsers land in `Other`
/// here because the bundle id alone does not say which site is open; the
/// pipeline asks the browser for that separately (see `site`).
pub fn categorise(bundle_id: &str) -> Category {
    const CHAT: &[&str] = &[
        "com.tinyspeck.slackmacgap",
        "com.hnc.Discord",
        "com.apple.MobileSMS",
        "net.whatsapp.WhatsApp",
        "ru.keepcoder.Telegram",
        "org.whispersystems.signal-desktop",
        "com.microsoft.teams2",
        "com.facebook.archon",
    ];
    const EMAIL: &[&str] = &[
        "com.apple.mail",
        "com.microsoft.Outlook",
        "com.readdle.smartemail-Mac",
        "com.mimestream.Mimestream",
        "com.superhuman.electron",
        "it.bloop.airmail2",
    ];
    const CODE: &[&str] = &[
        "com.microsoft.VSCode",
        "com.microsoft.VSCodeInsiders",
        "com.apple.dt.Xcode",
        "dev.zed.Zed",
        "com.todesktop.230313mzl4w4u92",
        "com.exafunction.windsurf",
        "com.vscodium",
        "com.visualstudio.code.oss",
        "com.google.android.studio",
        "com.panic.Nova",
        "com.barebones.bbedit",
        "com.sublimetext.3",
        "com.sublimetext.4",
        "com.apple.Terminal",
        "com.googlecode.iterm2",
        "dev.warp.Warp-Stable",
        "com.mitchellh.ghostty",
        "net.kovidgoyal.kitty",
        "com.github.wez.wezterm",
        "org.alacritty",
    ];
    const NOTES: &[&str] = &[
        "com.apple.Notes",
        "md.obsidian",
        "notion.id",
        "com.apple.TextEdit",
        "com.apple.iWork.Pages",
        "com.microsoft.Word",
        "net.shinyfrog.bear",
        "com.lukilabs.lukiapp",
    ];
    const ASSISTANT: &[&str] = &["com.anthropic.claudefordesktop", "com.openai.chat"];

    let is = |list: &[&str]| list.iter().any(|id| id.eq_ignore_ascii_case(bundle_id));

    if is(CHAT) {
        Category::Chat
    } else if is(EMAIL) {
        Category::Email
    } else if is(CODE) || bundle_id.starts_with("com.jetbrains.") {
        Category::Code
    } else if is(NOTES) {
        Category::Notes
    } else if is(ASSISTANT) {
        Category::Assistant
    } else {
        Category::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_apps_map_to_their_category() {
        assert_eq!(categorise("com.tinyspeck.slackmacgap"), Category::Chat);
        assert_eq!(categorise("com.apple.mail"), Category::Email);
        assert_eq!(categorise("com.microsoft.VSCode"), Category::Code);
        assert_eq!(categorise("com.apple.Notes"), Category::Notes);
        assert_eq!(categorise("com.anthropic.claudefordesktop"), Category::Assistant);
    }

    #[test]
    fn jetbrains_ides_match_by_prefix() {
        assert_eq!(categorise("com.jetbrains.intellij"), Category::Code);
        assert_eq!(categorise("com.jetbrains.pycharm"), Category::Code);
    }

    #[test]
    fn sites_are_known_by_address_and_by_title() {
        let gmail = site(Some("https://mail.google.com/mail/u/0/#inbox?compose=new"), None);
        assert_eq!(gmail, Some(("Gmail", Category::Email)));
        assert_eq!(site(Some("https://claude.ai/new"), None), Some(("Claude", Category::Assistant)));
        assert_eq!(site(Some("https://acme.slack.com/x"), None), None, "the web app is app.slack.com");
        assert_eq!(site(Some("https://app.slack.com/client/T1/C2"), None), Some(("Slack", Category::Chat)));
        assert_eq!(site(Some("https://www.notion.so/page"), None), Some(("Notion", Category::Notes)));
        // A look-alike host is not the site.
        assert_eq!(site(Some("https://notclaude.ai/"), None), None);
        assert_eq!(site(Some("https://claude.ai.evil.com/"), None), None);
        // No readable address: the title's last part names the site.
        assert_eq!(
            site(None, Some("Inbox (3) - someone@example.com - Gmail")),
            Some(("Gmail", Category::Email))
        );
        assert_eq!(site(Some("about:blank"), Some("Trip plans - Claude")), Some(("Claude", Category::Assistant)));
        assert_eq!(site(None, Some("general (Channel) - Acme - Slack - Google Chrome")), Some(("Slack", Category::Chat)));
        // A page that only mentions a site's name is not that site.
        assert_eq!(site(None, Some("Why I stopped using Gmail - My Blog")), None);
        assert_eq!(site(None, Some("")), None);
        assert_eq!(site(None, None), None);
        assert!(is_browser("com.google.Chrome") && !is_browser("com.apple.mail"));
    }

    #[test]
    fn browsers_and_unknown_apps_are_other() {
        assert_eq!(categorise("com.apple.Safari"), Category::Other);
        assert_eq!(categorise(""), Category::Other);
    }
}
