//! All user-facing copy. Components never hard-code strings.

/// A string the UI can show; Chinese first, English reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Text {
    /// `adash`
    AppName,
    /// `对话`
    NavHome,
    /// `云资源`
    NavResources,
    /// `AI 流程`
    NavFlows,
    /// `设置`
    NavSettings,
}

impl Text {
    /// Copy for the current locale. Chinese only until M4 adds the switch.
    #[must_use]
    pub fn zh(self) -> &'static str {
        match self {
            Self::AppName => "adash",
            Self::NavHome => "对话",
            Self::NavResources => "云资源",
            Self::NavFlows => "AI 流程",
            Self::NavSettings => "设置",
        }
    }
}
