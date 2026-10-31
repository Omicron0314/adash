//! All user-facing copy. Components never hard-code strings.
//!
//! Chinese first; the English column is reserved for M5. Dynamic text (names,
//! money, counts) is formatted by the call site and wrapped here where it is
//! reusable.

/// App name, as shown in the title bar.
pub const APP_NAME: &str = "adash";
/// One-line role under the app name.
pub const TAGLINE: &str = "UCloud 对话控制台";

/// Navigation labels.
pub mod nav {
    /// Agent conversation.
    pub const HOME: &str = "对话";
    /// Cloud resources.
    pub const RESOURCES: &str = "云资源";
    /// Project flow graphs.
    pub const FLOWS: &str = "AI 流程";
    /// Settings.
    pub const SETTINGS: &str = "设置";
    /// Group header above host sessions.
    pub const HOSTS: &str = "主机";
    /// SSH identities (M6).
    pub const IDENTITIES: &str = "身份";
    /// Reusable command snippets (M6).
    pub const SNIPPETS: &str = "片段";
    /// Port forwarding (M6).
    pub const PORT_FORWARD: &str = "端口转发";
    /// UCloud account and AI service configuration.
    pub const ACCOUNT: &str = "账户";
}

/// Overview card titles.
pub mod overview {
    /// Balance.
    pub const BALANCE: &str = "余额";
    /// This month's spend.
    pub const SPEND: &str = "本月花费";
    /// Budget progress.
    pub const BUDGET: &str = "预算";
    /// Resource counts.
    pub const RESOURCES: &str = "资源";
    /// Waste findings.
    pub const FINDINGS: &str = "待处理";
}

/// Buttons and one-shot actions.
pub mod action {
    /// Refresh data.
    pub const REFRESH: &str = "刷新";
    /// Send a message.
    pub const SEND: &str = "发送";
    /// Confirm and execute a plan.
    pub const CONFIRM: &str = "确认并执行";
    /// Dismiss a plan.
    pub const CANCEL: &str = "取消";
    /// Replay a flow.
    pub const REPLAY: &str = "回放";
    /// Stop a replay.
    pub const STOP: &str = "停止";
    /// Fit the graph to the canvas.
    pub const FIT: &str = "适应窗口";
    /// Open the host terminal.
    pub const OPEN_TERMINAL: &str = "打开终端";
    /// Copy to clipboard.
    pub const COPY: &str = "复制";
    /// Generate an optimization plan.
    pub const OPTIMIZE: &str = "生成省钱计划";
    /// Clear filters.
    pub const CLEAR_FILTERS: &str = "清除筛选";
}

/// Empty states.
pub mod empty {
    /// No resources yet.
    pub const NO_RESOURCES: &str = "这个账户还没有资源";
    /// No project selected on the flow page.
    pub const NO_FLOW: &str = "这个项目还没有资源";
    /// No chat history.
    pub const NO_CHAT: &str = "说一句话就能开始：查资源、看账单、生成部署计划。";
    /// No host session open.
    pub const NO_SESSION: &str = "从「云资源」里选一台主机，打开终端。";
}

/// Home welcome and quick questions.
pub mod home {
    pub const EYEBROW: &str = "ADASH · 云端工作空间";
    pub const TITLE: &str = "云上的事，交给我";
    pub const SUBTITLE: &str = "查看资源、梳理服务，或为下一个项目做好计划。";
    pub const INVENTORY: &str = "盘点云资源";
    pub const SAVINGS: &str = "寻找省钱机会";
    pub const FLOW: &str = "查看项目流程 →";
    pub const INVENTORY_QUESTION: &str = "有哪些云资源？";
    pub const SAVINGS_QUESTION: &str = "有浪费的钱吗？";
}

/// Flow inspector and connection legend.
pub mod flow {
    pub const NODE_DETAILS: &str = "节点详情";
    pub const PROJECT_OVERVIEW: &str = "项目概览";
    pub const VIEW_RESOURCE: &str = "在云资源中查看";
    pub const NODES: &str = "节点";
    pub const CONNECTIONS: &str = "连接";
    pub const RUNS: &str = "执行记录";
    pub const SELECT_HINT: &str = "点击画布中的服务，查看节点详情。";
    pub const LEGEND: &str = "实线：资源绑定或声明\n虚线：推断关系\n紫色：当前回放路径";
}

/// Chat placeholders.
pub const INPUT_PLACEHOLDER: &str = "问我：这个项目用了哪些服务？";
/// Offline-mode banner.
pub const OFFLINE_PREFIX: &str = "离线模式";
