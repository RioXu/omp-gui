use gpui_kit::component::tree::TreeItem;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    file: bool,
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
