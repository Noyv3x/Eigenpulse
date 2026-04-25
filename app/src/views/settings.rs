use ep_ui::{Card, PageHead, StatRow};
use leptos::prelude::*;

#[component]
pub fn SettingsIndex() -> impl IntoView {
    view! {
        <div class="view">
            <PageHead
                code="CFG-01".to_string()
                module="SETTINGS · 系统设置".to_string()
                title="Settings".to_string()
                title_cn="系统设置"
            />
            <div class="grid-2">
                <Card title="账户 & 身份" code="CFG-ACC">
                    <div class="vstack" style="gap:0">
                        <StatRow label="用户" value="Leo Chen · UID-0001".to_string()/>
                        <StatRow label="角色" value="OWNER".to_string()/>
                        <StatRow label="数据条数" value="—".to_string()/>
                    </div>
                </Card>
                <Card title="数据 & 同步" code="CFG-DATA">
                    <div class="vstack" style="gap:0">
                        <StatRow label="本地存储" value="data/eigenpulse.db".to_string()/>
                        <StatRow label="上次备份" value="尚未配置".to_string()/>
                        <StatRow label="同步状态" value="本地".to_string()/>
                    </div>
                </Card>
                <Card title="通知 · 通道" code="CFG-NOT" sub="SMTP / Bark / Telegram / Discord">
                    <p class="muted">"在 " <a href="/settings/notifications">"通道管理"</a> " 中配置外部推送。"</p>
                </Card>
                <Card title="API · Personal Access Tokens" code="CFG-SEC" sub="/api/v1/* 端点鉴权">
                    <p class="muted">"在 " <a href="/settings/security">"安全管理"</a> " 中生成 PAT。"</p>
                </Card>
            </div>
        </div>
    }
}
