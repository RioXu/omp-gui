//! One component-based directory picker for local and remote filesystems.
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, Icon, IconName, Sizable,
        button::Button,
        input::{Input, InputEvent, InputState},
        list::ListItem,
    },
    *,
};
use omp_core::{
    environment::HostPath,
    host::{Directory, DirectoryBrowser, DirectoryEntry},
};

fn path_label(path: &HostPath) -> String {
    match path {
        HostPath::Local(path) => omp_core::infrastructure::omp::sessions::plain_path(path)
            .to_string_lossy()
            .into_owned(),
        _ => path.text(),
    }
}
fn directory_item(entry: &DirectoryEntry, index: usize, busy: bool) -> ListItem {
    ListItem::new(("project-directory", index))
        .w_full()
        .h_8()
        .py_0()
        .disabled(busy)
        .accessibility_label(entry.name.clone())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .min_w_0()
                .child(
                    div()
                        .debug_selector(|| "project-directory-icon".into())
                        .flex_shrink_0()
                        .child(Icon::new(IconName::Folder).small()),
                )
                .child(
                    div()
                        .debug_selector(|| "project-directory-name".into())
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(entry.name.clone()),
                ),
        )
}
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

pub(crate) struct ProjectPicker {
    browser: Arc<dyn DirectoryBrowser>,
    path: Entity<InputState>,
    directory: Option<Directory>,
    recent: Vec<HostPath>,
    generation: u64,
    busy: bool,
    error: Option<String>,
    tx: mpsc::Sender<(u64, Result<Directory, String>)>,
    rx: mpsc::Receiver<(u64, Result<Directory, String>)>,
    _subscriptions: Vec<Subscription>,
}

impl ProjectPicker {
    pub(crate) fn new(
        browser: Arc<dyn DirectoryBrowser>,
        recent: Vec<HostPath>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let path = cx.new(|cx| InputState::new(window, cx).placeholder("输入绝对目录路径"));
        let subscription = cx.subscribe_in(&path, window, |this, _, event, _, cx| match event {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } => this.enter_path(cx),
            _ => {}
        });
        let (tx, rx) = mpsc::channel();
        let mut this = Self {
            browser,
            path,
            directory: None,
            recent,
            generation: 0,
            busy: false,
            error: None,
            tx,
            rx,
            _subscriptions: vec![subscription],
        };
        this.browse(None, cx);
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                if view
                    .update_in(cx, |this, w, cx| {
                        for (generation, result) in this.rx.try_iter().collect::<Vec<_>>() {
                            if generation != this.generation {
                                continue;
                            }
                            this.busy = false;
                            match result {
                                Ok(directory) => {
                                    this.path.update(cx, |path, cx| {
                                        path.set_value(directory.path.text(), w, cx)
                                    });
                                    this.directory = Some(directory);
                                    this.error = None;
                                }
                                Err(error) => this.error = Some(error),
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        this
    }

    pub(crate) fn selected(&self, cx: &App) -> Option<HostPath> {
        let directory = self.directory.as_ref()?;
        (!self.busy
            && self.error.is_none()
            && self.path.read(cx).value().trim() == directory.path.text())
        .then(|| directory.path.clone())
    }

    fn browse(&mut self, path: Option<HostPath>, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.busy = true;
        self.error = None;
        let generation = self.generation;
        let browser = self.browser.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = browser.browse(path.as_ref()).map_err(|e| format!("{e:#}"));
            let _ = tx.send((generation, result));
        });
        cx.notify();
    }

    fn enter_path(&mut self, cx: &mut Context<Self>) {
        match self.browser.entered_path(self.path.read(cx).value().trim()) {
            Ok(path) => self.browse(Some(path), cx),
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
            }
        }
    }
}

impl Render for ProjectPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let parent = self.directory.as_ref().and_then(|d| d.parent.clone());
        let mut body = div().flex().flex_col().gap_2().w_full().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("parent-directory")
                        .small()
                        .icon(IconName::ArrowUp)
                        .tooltip("上级目录")
                        .disabled(parent.is_none() || self.busy)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.browse(parent.clone(), cx)),
                        ),
                )
                .child(div().flex_1().min_w_0().child(Input::new(&self.path)))
                .child(
                    Button::new("browse-directory")
                        .small()
                        .label("前往")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.enter_path(cx);
                        })),
                ),
        );
        if !self.recent.is_empty() {
            let mut recent = div().flex().flex_wrap().gap_1();
            for (ix, path) in self.recent.iter().take(6).enumerate() {
                let path = path.clone();
                recent = recent.child(
                    Button::new(("recent-project", ix))
                        .small()
                        .label(path_label(&path))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.browse(Some(path.clone()), cx)),
                        ),
                );
            }
            body = body
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("最近项目"),
                )
                .child(recent);
        }
        if let Some(error) = &self.error {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }
        if self.busy {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("正在读取目录…"),
            );
        }
        let count = self.directory.as_ref().map_or(0, |d| d.entries.len());
        body.child(
            uniform_list(
                "project-directories",
                count,
                cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|ix| {
                            let entry = &this.directory.as_ref().unwrap().entries[ix];
                            let path = entry.path.clone();
                            div().h_8().w_full().child(
                                directory_item(entry, ix, this.busy).on_click(cx.listener(
                                    move |this, _, _, cx| this.browse(Some(path.clone()), cx),
                                )),
                            )
                        })
                        .collect()
                }),
            )
            .h(px(300.))
            .w_full(),
        )
    }
}

#[cfg(test)]
mod tests;
