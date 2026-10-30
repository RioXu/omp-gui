use gpui_kit::Context;
use gpui_kit::component::tree::{TreeItem, TreeState};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    file: bool,
}

pub(crate) fn update(tree: &mut TreeState, paths: &[PathBuf], cx: &mut Context<TreeState>) {
    fn remember(item: &TreeItem, expanded: &mut std::collections::HashSet<String>) {
        if item.is_expanded() {
            expanded.insert(item.id.to_string());
        }
        for child in &item.children {
            remember(child, expanded);
        }
    }
    fn restore(mut item: TreeItem, expanded: &std::collections::HashSet<String>) -> TreeItem {
        let children = item
            .children
            .iter()
            .cloned()
            .map(|c| restore(c, expanded))
            .collect::<Vec<_>>();
        let open = expanded.contains(item.id.as_ref());
        item.children = children;
        item.expanded(open)
    }
    let selected = tree.selected_item().map(|i| i.id.clone());
    let mut expanded = std::collections::HashSet::new();
    for ix in 0.. {
        let Some(entry) = tree.entry(ix) else {
            break;
        };
        if entry.depth() == 0 {
            remember(entry.item(), &mut expanded);
        }
    }
    tree.set_items(
        items(paths)
            .into_iter()
            .map(|i| restore(i, &expanded))
            .collect::<Vec<_>>(),
        cx,
    );
    if let Some(id) = selected {
        tree.set_selected_index(tree.index_of(&id), cx);
    }
}
pub(crate) fn items(paths: &[PathBuf]) -> Vec<TreeItem> {
    let mut root = Node::default();
    for path in paths {
        let mut node = &mut root;
        for part in path.components() {
            node = node
                .children
                .entry(part.as_os_str().to_string_lossy().into_owned())
                .or_default();
        }
        node.file = true;
    }
    fn convert(node: Node, prefix: &str) -> Vec<TreeItem> {
        let mut children = node.children.into_iter().collect::<Vec<_>>();
        children.sort_by(|(an, a), (bn, b)| {
            a.file
                .cmp(&b.file)
                .then_with(|| an.to_lowercase().cmp(&bn.to_lowercase()))
        });
        children
            .into_iter()
            .map(|(name, node)| {
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                let children = convert(node, &path);
                TreeItem::new(path, name).children(children)
            })
            .collect()
    }
    convert(root, "")
}
#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{AppContext, TestAppContext};

    #[gpui_kit::test]
    fn refresh_preserves_nested_expansion_and_selection_without_duplicate_rows(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let initial = items(&["src/nested/a.rs".into(), "src/b.rs".into()]);
        initial[0].clone().expanded(true);
        initial[0].children[0].clone().expanded(true);
        let tree = cx.new(|cx| TreeState::new(cx).items(initial));
        tree.update(cx, |tree, cx| {
            tree.set_selected_index(tree.index_of(&"src/nested/a.rs".into()), cx);
            update(
                tree,
                &[
                    "src/nested/a.rs".into(),
                    "src/nested/new.rs".into(),
                    "src/b.rs".into(),
                ],
                cx,
            );
            let entries = (0..).map_while(|i| tree.entry(i)).collect::<Vec<_>>();
            assert_eq!(entries.len(), 5);
            let unique = entries
                .iter()
                .map(|e| &e.item().id)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(unique.len(), entries.len());
            assert_eq!(tree.selected_item().unwrap().id.as_ref(), "src/nested/a.rs");
            assert!(tree.entry(0).unwrap().is_expanded());
            assert!(tree.entry(1).unwrap().is_expanded());
            update(tree, &["src/nested/new.rs".into()], cx);
            assert!(tree.selected_item().is_none());
            assert_eq!(
                tree.entry(2).unwrap().item().id.as_ref(),
                "src/nested/new.rs"
            );
        });
    }
    #[test]
    fn nested_directories_have_stable_paths_and_sort_before_files() {
        let list = items(&["z.rs".into(), "src/b.rs".into(), "src/nested/a.rs".into()]);
        assert_eq!(list[0].id.as_ref(), "src");
        assert_eq!(list[0].children[0].id.as_ref(), "src/nested");
        assert_eq!(
            list[0].children[0].children[0].id.as_ref(),
            "src/nested/a.rs"
        );
    }
}
