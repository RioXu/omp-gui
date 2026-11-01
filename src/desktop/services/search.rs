use super::*;

impl RegistryView {
    pub(super) fn save_web(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let service = self
            .search_service
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or(DEFAULT_SEARCH.into());
        let mut roles = self.roles.clone();
        roles["web"] = json!(service);
        let mut chains = self.chains.clone();
        chains["web"] = json!(self.fallbacks);
        let key = self.value("search_key", cx);
        let url = self.value("search_url", cx);
        let token = self.value("search_token", cx);
        if service == "web/searxng"
            && reqwest::Url::parse(&url)
                .ok()
                .is_none_or(|u| !["http", "https"].contains(&u.scheme()))
        {
            self.notice = "SearXNG 需要有效的服务地址".into();
            cx.notify();
            return;
        }
        let configuration = self.configuration.clone();
        let root = self.root.clone();
        let project = self.project;
        let enabled = self.web_enabled;
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                if !key.is_empty() {
                    let provider = service
                        .strip_prefix("web/")
                        .unwrap_or_else(|| service.split('/').next().unwrap_or(&service));
                    configuration.call::<()>(ConfigAction::ImportKey {
                        omp: configuration.omp.clone(),
                        cwd: configuration.cwd.text(),
                        provider: provider.into(),
                        key: key.clone(),
                    })?;
                }
                if service == "web/searxng" {
                    configuration.save_scoped(&root, project, "searxng.endpoint", json!(url))?;
                    if !token.is_empty() {
                        configuration.save_scoped(&root, project, "searxng.token", json!(token))?;
                    }
                }
                configuration.save_scoped(&root, project, "modelRoles", roles)?;
                configuration.save_scoped(&root, project, "retry.fallbackChains", chains)?;
                configuration.save_scoped(&root, project, "web_search.enabled", json!(enabled))
            })();
            let _ = tx.send(ResultEvent::WebSaved(result.map_err(|e| format!("{e:#}"))));
        });
        cx.notify();
    }
}
