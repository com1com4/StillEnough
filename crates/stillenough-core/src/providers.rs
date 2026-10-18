use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub supported: bool,
    pub auth_method: &'static str,
    pub base_urls: Vec<&'static str>,
    pub capability: &'static str,
    pub status: &'static str,
    pub limitation: &'static str,
    pub setup_instructions: &'static str,
    pub authorization_label: &'static str,
    pub network_note: &'static str,
}
fn deepseek() -> ProviderDescriptor {
    ProviderDescriptor {
        id: "deepseek_api",
        name: "DeepSeek API",
        supported: true,
        auth_method: "api_key",
        base_urls: vec!["https://api.deepseek.com"],
        capability: "API 账号可用余额（按接口返回币种）",
        status: "实验性 / 待真实账号验证",
        setup_instructions: "",
        authorization_label: "",
        network_note: "",
        limitation: "只读取钱包余额，不读取编程套餐额度；接口不返回账号身份，按所配置的 Key 绑定",
    }
}
const MINI: &[&str] = &["https://www.minimaxi.com", "https://www.minimax.io"];
const WALLET: &[&str] = &["https://business.aliyuncs.com"];
const MINI_WALLET: &[&str] = &["https://api.minimax.cn", "https://api.minimax.io"];
fn plans(id: &'static str) -> ProviderDescriptor {
    let (name, urls, auth, capability, limitation) = match id {
            "minimax_api" => ("MiniMax API", MINI_WALLET, "api_key", "API 钱包可用金额 / 现金余额", "使用 sk-api- 普通 API Key；查询源为官方 CLI 使用的账户接口，可能变化。响应未给币种时明确显示币种未知，不推定 CNY 或 USD。"),
            "minimax_token_plan" => ("MiniMax Token Plan", MINI, "api_key", "Token Plan 通用额度 · 5 小时 / 周窗口", "中国/国际区域分别使用对应套餐 Key；只显示 general 额度，不把计数为零解释为耗尽或不限量。"),
            _ => ("阿里百炼 API（阿里云账户余额）", WALLET, "access_key_pair", "阿里云账户现金余额 / 可用额度", "需要 AccessKey ID / Secret 及 bss:DescribeAcccount 只读权限；这是整个阿里云账户余额，不是百炼专属余额或 Token Plan 额度。"),
        };
    ProviderDescriptor {
        id,
        name,
        supported: true,
        auth_method: auth,
        base_urls: urls.to_vec(),
        capability,
        status: "实验性 / 待真实账号验证",
        setup_instructions: "",
        authorization_label: "",
        network_note: "",
        limitation,
    }
}
fn bailian_cli() -> ProviderDescriptor {
    ProviderDescriptor { setup_instructions: "先在 PowerShell 安装并登录官方 CLI：\nnpm install -g bailian-cli\nbl auth login --console\n安装后重新启动本应用。切换登录后请重新测试。", authorization_label: "我授权应用调用官方百炼 CLI 查询当前登录账号的额度", network_note: "此 CLI 仅沿用自身环境；暂不支持应用指定的直连或自定义代理。", id: "bailian_token_plan", name: "阿里百炼 Token Plan", supported: true, auth_method: "cli", base_urls: vec!["cli://bailian/token-plan"], capability: "官方 CLI · 个人 Token Plan 月额度 / 已报告窗口", status: "实验性 / 待真实账号验证", limitation: "使用本机官方 bl CLI 当前登录账号。请先在 PowerShell 执行 bl auth login --console；仅查询额度，不读取或复制认证文件。共用一个 CLI 登录来源，切换登录账号后请重新测试；团队席位暂不支持。" }
}
fn codex_http() -> ProviderDescriptor {
    ProviderDescriptor { id: "codex_http", name: "Codex · HTTP", supported: true,
            auth_method: "local_oauth", base_urls: vec!["local://codex/subscription-http"],
            capability: "ChatGPT 订阅额度 · 直接 HTTP 查询", status: "实验性 / 本机 HTTP 查询已实测",
            limitation: "只读 auth.json 的访问令牌和账号标识，不使用刷新令牌、不改写文件。不支持 API Key、系统凭据库存储或浏览器凭据。过期请通过官方 CLI 重新登录；切换账号后重新测试。服务端限制时停止更新并保留旧观测。",
            setup_instructions: "先通过官方 Codex CLI 登录 ChatGPT：\ncodex login\n尊重 CODEX_HOME，默认用户 .codex/auth.json。查询不启动 CLI；若登录只存在系统凭据库，请选择 Codex · app-server。当前来源身份未核验。",
            authorization_label: "我授权只读 Codex 本地登录文件的访问令牌和账号标识，仅发送到 https://chatgpt.com/backend-api/wham/usage 查询额度；不刷新令牌或改写文件",
            network_note: "使用应用全局/账号网络设置，支持 HTTP、SOCKS5 和 SOCKS5h；不跟随重定向，不自动切换 app-server。" }
}
fn codex() -> ProviderDescriptor {
    ProviderDescriptor { id: "codex", name: "Codex · app-server", supported: true, auth_method: "cli", base_urls: vec!["cli://codex/subscription"],
            capability: "ChatGPT 订阅额度 · 客户端实际返回的窗口", status: "实验性 / 待真实账号验证",
            limitation: "使用本机 Codex app-server 的只读 account/read 和 account/rateLimits/read。不读取 auth.json、不主动刷新令牌、不启动模型任务。仅支持 ChatGPT 订阅登录，同一来源限一个账号；切换登录后请重新测试。",
            setup_instructions: "先安装并使用官方 Codex CLI 登录 ChatGPT 账号：\nnpm install -g @openai/codex\ncodex login\n尊重 CODEX_HOME。仅安装桌面版而没有 CLI 时需另行安装 CLI。",
            authorization_label: "我授权应用启动本机 Codex app-server，只读查询当前登录账号的订阅额度",
            network_note: "支持直连、自定义 HTTP 代理及系统环境/Windows 静态代理。Codex 子进程暂不支持应用指定的 SOCKS 代理。" }
}
fn claude() -> ProviderDescriptor {
    ProviderDescriptor { id: "claude_code", name: "Claude Code", supported: true, auth_method: "local_oauth",
            base_urls: vec!["local://claude-code/subscription"], capability: "订阅额度 · 5 小时 / 周 / 模型窗口", status: "实验性 / 待真实账号验证",
            limitation: "只读本机 Claude Code 订阅登录，使用非公开用量接口，可能变化。不支持 API Key、第三方中转或浏览器登录；不刷新令牌。当前登录身份未核验，切换账号后请重新测试。",
            setup_instructions: "先使用官方 Claude Code 登录订阅账号：\nclaude auth login\n凭据读取尊重 CLAUDE_CONFIG_DIR；默认当前用户 .claude 目录。每个本地来源仅添加一个账号。",
            authorization_label: "我授权只读 Claude Code 本地订阅登录凭据，仅用于查询用量；不刷新令牌或改写文件",
            network_note: "查询使用应用的网络设置，支持 HTTP、SOCKS5 和 SOCKS5h 代理。" }
}
fn regional(id: &'static str) -> ProviderDescriptor {
    let (name, url, capability, limitation) = match id {
        "kimi_api_cn" => (
            "Kimi / Moonshot API（国内）",
            "https://api.moonshot.cn",
            "API 钱包余额",
            "使用国内 Moonshot 开放平台 API Key；与 Kimi Code 套餐独立，币种仅按返回值确认。",
        ),
        "kimi_api_intl" => (
            "Kimi / Moonshot API（海外）",
            "https://api.moonshot.ai",
            "API 钱包余额（USD）",
            "使用海外 Moonshot 开放平台 API Key；与 Kimi Code 套餐独立。",
        ),
        "minimax_api_intl" => (
            "MiniMax API（海外）",
            "https://api.minimax.io",
            "API 钱包可用金额 / 现金余额",
            "使用海外普通 API Key；币种未返回时显示未知，不推定 USD。",
        ),
        "minimax_token_plan_intl" => (
            "MiniMax Token Plan（海外）",
            "https://www.minimax.io",
            "Token Plan 通用额度 · 5 小时 / 周窗口",
            "使用海外 Token Plan Key；仅显示接口明确返回的有效窗口。",
        ),
        "glm_coding_cn" => (
            "GLM Coding Plan（国内）",
            "https://open.bigmodel.cn",
            "编程套餐额度 / 工具调用额度",
            "使用智谱国内 Coding Plan Key；只读官方插件查询接口，不代表 API 钱包余额。",
        ),
        "glm_coding_intl" => (
            "GLM Coding Plan（海外 Z.ai）",
            "https://api.z.ai",
            "编程套餐额度 / 工具调用额度",
            "使用 Z.ai Coding Plan Key；接口可能变化，未报告的窗口和重置时间不推测。",
        ),
        "kimi_coding_cn" => (
            "Kimi Code（国内）",
            "https://api.kimi.com",
            "编程订阅额度 · 接口报告窗口",
            "使用 Kimi Code 订阅 Key，不支持 Moonshot API 钱包 Key；接口可能变化，月额度可能缺失。",
        ),
        "kimi_coding_intl" => (
            "Kimi Code（海外）",
            "https://api.kimi.ai",
            "编程订阅额度 · 接口报告窗口",
            "使用对应海外 Kimi Code Key；地区、套餐和窗口待实测，不将订阅额度当作 API 余额。",
        ),
        _ => unreachable!(),
    };
    ProviderDescriptor {
        id,
        name,
        supported: true,
        auth_method: "api_key",
        base_urls: vec![url],
        capability,
        status: "实验性 / 待真实账号验证",
        limitation,
        setup_instructions: "请填写对应地区的套餐或 API Key，然后测试连接；不同地区凭据不可混用。",
        authorization_label: "",
        network_note: "使用应用全局或账号网络设置；不跟随重定向。",
    }
}
pub const PROVIDER_IDS: &[&str] = &[
    "kimi_api_cn",
    "kimi_api_intl",
    "minimax_api_intl",
    "minimax_token_plan_intl",
    "glm_coding_cn",
    "glm_coding_intl",
    "kimi_coding_cn",
    "kimi_coding_intl",
    "deepseek_api",
    "minimax_api",
    "minimax_token_plan",
    "bailian_api",
    "bailian_token_plan",
    "codex_http",
    "codex",
    "claude_code",
];
pub fn descriptor(id: &str) -> Option<ProviderDescriptor> {
    Some(match id {
        "kimi_api_cn"
        | "kimi_api_intl"
        | "minimax_api_intl"
        | "minimax_token_plan_intl"
        | "glm_coding_cn"
        | "glm_coding_intl"
        | "kimi_coding_cn"
        | "kimi_coding_intl" => regional(PROVIDER_IDS.iter().copied().find(|entry| *entry == id)?),
        "deepseek_api" => deepseek(),
        "minimax_api" => plans("minimax_api"),
        "minimax_token_plan" => plans("minimax_token_plan"),
        "bailian_api" => plans("bailian_api"),
        "bailian_token_plan" => bailian_cli(),
        "codex_http" => codex_http(),
        "codex" => codex(),
        "claude_code" => claude(),
        _ => return None,
    })
}
pub fn descriptors() -> Vec<ProviderDescriptor> {
    PROVIDER_IDS
        .iter()
        .filter_map(|id| descriptor(id))
        .collect()
}
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct ProviderProjection {
    pub id: String,
    pub name: String,
    pub supported: bool,
    pub capability: String,
    pub status: String,
    pub limitation: String,
}
impl From<ProviderDescriptor> for ProviderProjection {
    fn from(p: ProviderDescriptor) -> Self {
        Self {
            id: p.id.into(),
            name: p.name.into(),
            supported: p.supported,
            capability: p.capability.into(),
            status: p.status.into(),
            limitation: p.limitation.into(),
        }
    }
}
