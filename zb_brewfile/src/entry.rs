#[derive(Debug, Clone)]
pub enum BrewfileEntry {
    Tap {
        name: String,
        url: Option<String>,
    },
    Brew(BrewEntry),
    Cask {
        name: String,
    },
    Mas {
        name: String,
        id: u64,
    },
    Vscode {
        name: String,
    },
    Go {
        name: String,
    },
    Cargo {
        name: String,
    },
    Flatpak {
        name: String,
        remote: Option<String>,
        url: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct BrewEntry {
    pub name: String,
    pub args: Vec<String>,
    pub restart_service: Option<RestartService>,
    pub link: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartService {
    Always,  // true
    Changed, // :changed
}

impl BrewfileEntry {
    pub fn is_brew(&self) -> bool {
        matches!(self, BrewfileEntry::Brew(_))
    }

    pub fn is_supported(&self) -> bool {
        matches!(self, BrewfileEntry::Brew(_) | BrewfileEntry::Tap { .. })
    }

    pub fn entry_type(&self) -> &str {
        match self {
            BrewfileEntry::Tap { .. } => "tap",
            BrewfileEntry::Brew(_) => "brew",
            BrewfileEntry::Cask { .. } => "cask",
            BrewfileEntry::Mas { .. } => "mas",
            BrewfileEntry::Vscode { .. } => "vscode",
            BrewfileEntry::Go { .. } => "go",
            BrewfileEntry::Cargo { .. } => "cargo",
            BrewfileEntry::Flatpak { .. } => "flatpak",
        }
    }
}
