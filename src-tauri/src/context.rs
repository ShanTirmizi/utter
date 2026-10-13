//! Which app the text is going into. The cleanup step uses this to pick a
//! writing style: a chat message, an email and a code comment should not all
//! read the same.

use objc2_app_kit::NSWorkspace;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Chat,
    Email,
    Code,
    Notes,
    Assistant,
    Other,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContext {
    pub name: String,
    pub bundle_id: String,
    pub category: Category,
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

/// Map a bundle identifier to a writing context. Browsers land in `Other`
/// because the bundle id alone does not say which site is open.
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
    fn browsers_and_unknown_apps_are_other() {
        assert_eq!(categorise("com.apple.Safari"), Category::Other);
        assert_eq!(categorise(""), Category::Other);
    }
}
