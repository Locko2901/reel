/// The apps that have the foreground, or name a recording, but are no game; matched on an
/// executable's stem or a game's name, lowercased.
pub const NOT_GAMES: &[&str] = &[
    "explorer",
    "file explorer",
    "windows explorer",
    "desktop",
    "applicationframehost",
    "searchhost",
    "searchapp",
    "shellexperiencehost",
    "startmenuexperiencehost",
    "textinputhost",
    "systemsettings",
    "lockapp",
    "taskmgr",
    "chrome",
    "google chrome",
    "msedge",
    "microsoft edge",
    "msedgewebview2",
    "firefox",
    "zen",
    "zen browser",
    "brave",
    "brave browser",
    "opera",
    "opera gx",
    "vivaldi",
    "arc",
    "librewolf",
    "waterfox",
    "floorp",
    "thorium",
    "iexplore",
    "code",
    "visual studio code",
    "cursor",
    "windsurf",
    "zed",
    "devenv",
    "notepad",
    "notepad++",
    "sublime_text",
    "idea64",
    "rider64",
    "pycharm64",
    "winword",
    "excel",
    "powerpnt",
    "outlook",
    "onenote",
    "acrobat",
    "windowsterminal",
    "cmd",
    "powershell",
    "pwsh",
    "discord",
    "slack",
    "teams",
    "ms-teams",
    "telegram",
    "whatsapp",
    "signal",
    "skype",
    "spotify",
    "vlc",
    "mpc-hc64",
    "obs64",
    "obs32",
    "reel-capture",
    "obs",
    "obs studio",
    "streamlabs obs",
    "streamlabs desktop",
    "medal",
    "nvidia app",
    "nvidia overlay",
    "nvidia share",
    "nvcontainer",
    "geforce experience",
    "steam",
    "steamwebhelper",
    "epicgameslauncher",
    "riotclientservices",
    "riotclientux",
    "battle.net",
    "eadesktop",
    "upc",
    "ubisoftconnect",
    "galaxyclient",
    "reel-desktop",
    "reel",
];

/// Whether an executable (with or without its path and `.exe`) or a game's name is no game.
pub fn is_not_a_game(name: &str) -> bool {
    let stem = name.rsplit(['\\', '/']).next().unwrap_or(name).trim();
    let key = stem.to_lowercase();
    let key = key.strip_suffix(".exe").unwrap_or(&key);
    NOT_GAMES.contains(&key)
}

/// Whether a name in front of a title's moment is an app that is no game and so says nothing
/// about the clip: any of the list but ShadowPlay's "Desktop", which names a desktop capture.
pub fn drops_from_title(name: &str) -> bool {
    is_not_a_game(name) && !name.trim().eq_ignore_ascii_case("desktop")
}

/// A game as a clip keeps it: trimmed, and none for an empty name or an app that is no game.
pub fn clean(game: Option<&str>) -> Option<String> {
    let g = game?.trim();
    if g.is_empty() || is_not_a_game(g) {
        return None;
    }
    Some(g.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsers_the_shell_editors_chat_and_obs_are_no_game() {
        for app in [
            "Zen",
            "zen.exe",
            r"C:\Program Files\Zen Browser\zen.exe",
            "Google Chrome",
            "msedge.exe",
            "Firefox",
            "Explorer",
            "Code",
            "Discord",
            "OBS Studio",
            "obs64.exe",
            "reel-capture.exe",
            "Desktop",
        ] {
            assert!(is_not_a_game(app), "{app}");
        }
        for game in [
            "VRChat",
            "Valorant",
            "Ember Pass",
            "Beat Saber",
            "Overwatch 2",
            "Zenless Zone Zero",
        ] {
            assert!(!is_not_a_game(game), "{game}");
        }
        assert_eq!(clean(Some(" Valorant ")), Some("Valorant".into()));
        assert_eq!(clean(Some("Zen")), None);
        assert_eq!(clean(Some("  ")), None);
        assert_eq!(clean(None), None);
    }
}
