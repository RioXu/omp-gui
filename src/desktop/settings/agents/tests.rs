use super::*;
use core::prelude::v1::test;
use gpui_kit::{TestAppContext, VisualTestContext};
use omp_core::{environment::HostPath, host::ExecutionHost};
use serde_json::json;

fn draw(visual: &mut VisualTestContext) {
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
}
fn click(visual: &mut VisualTestContext, selector: &'static str) {
    let bounds = visual.debug_bounds(selector).unwrap();
    visual.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::default());
    draw(visual);
}
fn fixture(window: &mut Window, cx: &mut Context<SettingsView>) -> SettingsView {
    let root = std::env::temp_dir().join(format!("omp-settings-agents-{}", uuid::Uuid::new_v4()));
    let global = root.join("global");
    let project = root.join(".omp");
    for (base, name, description) in [
        (&global, "reviewer", "检查代码"),
        (&project, "builder", "修改实现"),
    ] {
        std::fs::create_dir_all(base.join("agents")).unwrap();
        std::fs::write(base.join("agents").join(format!("{name}.md")), format!("---\nname: {name}\ndescription: {description}\nmodel: '@default'\n---\n\nTask instructions\n")).unwrap();
    }
    let configuration = Configuration {
        host: ExecutionHost::Local,
        omp: "omp-gui-nonexistent-settings-fixture".into(),
        cwd: HostPath::Local(root.clone()),
    };
    let mut view = SettingsView::new(configuration, Some(root), window, cx);
    view.loading = false;
    view.category = "子智能体";
    view.catalog = Catalog {
        root: global.clone(),
        ..Default::default()
    };
    view.agent_imports.insert(global.clone());
    view.agent_imports.insert(project.clone());
    view.catalog.agent_files.insert(
        "global/agents".into(),
        vec![AgentEntry::load(&global.join("agents/reviewer.md"))],
    );
    view.catalog.agent_files.insert(
        "project/agents".into(),
        vec![AgentEntry::load(&project.join("agents/builder.md"))],
    );
    view.catalog.entries.insert(
        "eval.js".into(),
        json!({"type":"boolean","value":true,"description":"JavaScript eval"}),
    );
    view.catalog.entries.insert(
        "eval.py".into(),
        json!({"type":"boolean","value":true,"description":"Python eval"}),
    );
    view
}

#[cfg(windows)]
#[gpui_kit::test]
fn builtin_import_runs_once_per_scope_and_preserves_existing_agents(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut this = fixture(window, cx);
                let script = this.cwd.join("omp-fixture.cmd");
                let log = this.cwd.join("imports.log");
                let global = this.catalog.root.display().to_string();
                std::fs::write(
                    &script,
                    format!(
                        r#"@echo off
if "%1"=="agents" goto agents
if "%2"=="path" echo {global}
if "%2"=="list" echo {{}}
exit /b 0
:agents
echo imported>>"{}"
if not exist "%~4" mkdir "%~4"
if exist "%~4\reviewer.md" goto bundled
echo --->"%~4\reviewer.md"
echo name: reviewer>>"%~4\reviewer.md"
echo description: Builtin reviewer>>"%~4\reviewer.md"
echo --->>"%~4\reviewer.md"
echo Review code>>"%~4\reviewer.md"
:bundled
if exist "%~4\bundled.md" goto done
echo --->"%~4\bundled.md"
echo name: bundled>>"%~4\bundled.md"
echo description: Bundled helper>>"%~4\bundled.md"
echo --->>"%~4\bundled.md"
echo Help the user>>"%~4\bundled.md"
:done
echo {{}}
exit /b 0
"#,
                        log.display()
                    ),
                )
                .unwrap();
                this.omp = script.to_string_lossy().into_owned();
                this.configuration.omp = this.omp.clone();
                this.agent_imports.clear();
                this.ensure_builtin_agents(cx);
                this
            })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1180.), px(850.)));
    fn finish(visual: &mut VisualTestContext, view: &Entity<SettingsView>) {
        for _ in 0..300 {
            std::thread::sleep(Duration::from_millis(5));
            visual
                .background_executor
                .advance_clock(Duration::from_millis(200));
            draw(visual);
            if view.read_with(visual, |this, _| !this.busy && !this.loading) {
                return;
            }
        }
        panic!("automatic import did not finish");
    }
    finish(&mut visual, &view);
    assert!(visual.debug_bounds("managed-agent-bundled").is_some());
    view.read_with(&visual, |this, _| {
        assert!(
            std::fs::read_to_string(this.catalog.root.join("agents/reviewer.md"))
                .unwrap()
                .contains("检查代码")
        );
        assert_eq!(
            std::fs::read_to_string(this.cwd.join("imports.log"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    });
    click(&mut visual, "settings-scope-project");
    finish(&mut visual, &view);
    assert!(visual.debug_bounds("managed-agent-bundled").is_some());
    assert!(visual.debug_bounds("managed-agent-builder").is_some());
    click(&mut visual, "settings-scope-global");
    draw(&mut visual);
    view.read_with(&visual, |this, _| {
        assert_eq!(
            std::fs::read_to_string(this.cwd.join("imports.log"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    });
}

#[gpui_kit::test]
fn manager_filters_and_scopes_existing_agents_and_opens_their_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1180.), px(850.)));
    draw(&mut visual);
    assert!(visual.debug_bounds("managed-agent-reviewer").is_some());
    assert!(visual.debug_bounds("managed-agent-builder").is_none());
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.search
                .update(cx, |input, cx| input.set_value("no matches", window, cx));
            cx.notify();
        })
    });
    draw(&mut visual);
    assert!(visual.debug_bounds("managed-agent-reviewer").is_none());
    assert!(visual.debug_bounds("managed-agents-empty").is_some());
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.search
                .update(cx, |input, cx| input.set_value("", window, cx));
            cx.notify();
        })
    });
    draw(&mut visual);
    click(&mut visual, "settings-scope-project");
    assert!(visual.debug_bounds("managed-agent-reviewer").is_none());
    assert!(visual.debug_bounds("managed-agent-builder").is_some());
    click(&mut visual, "edit-managed-agent-builder");
    for _ in 0..100 {
        if view.read_with(&visual, |this, _| this.agent_editor.is_some()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
        visual
            .background_executor
            .advance_clock(Duration::from_millis(200));
        draw(&mut visual);
    }
    assert!(
        view.read_with(&visual, |this, _| this.agent_editor.is_some()),
        "existing project agent must open its edit form"
    );
}

#[gpui_kit::test]
fn eval_shortcut_reveals_native_execution_switches_without_changing_values(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut this = fixture(window, cx);
                this.category = "工具调用";
                this
            })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1180.), px(850.)));
    draw(&mut visual);
    click(&mut visual, "find-eval-settings");
    assert!(visual.debug_bounds("setting-row-eval.js").is_some());
    assert!(visual.debug_bounds("setting-row-eval.py").is_some());
    view.read_with(&visual, |this, cx| {
        assert_eq!(this.search.read(cx).value().as_ref(), "eval.");
        assert_eq!(this.catalog.entries["eval.js"]["value"], true);
        assert_eq!(this.catalog.entries["eval.py"]["value"], true);
        assert!(this.pending.is_empty());
    });
}
