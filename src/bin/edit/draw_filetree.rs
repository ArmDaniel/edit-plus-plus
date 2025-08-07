// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::path::PathBuf;

use edit::tui::*;
use crate::state::*;

#[derive(Clone, Debug)]
pub struct FileTreeNode {
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<FileTreeNode>,
    pub expanded: bool,
}

pub fn draw_file_tree(ctx: &mut Context, state: &mut State) {
    if !state.file_tree.visible {
        return;
    }

    ctx.block_begin("file_tree");
    ctx.attr_background_rgba(ctx.indexed(edit::framebuffer::IndexedColor::Black));
    ctx.attr_foreground_rgba(ctx.indexed(edit::framebuffer::IndexedColor::White));
    ctx.inherit_focus();

    let flattened_nodes = flatten_tree(&state.file_tree.nodes);
    let mut activated_path = None;

    ctx.list_begin("tree_list");
    ctx.inherit_focus();

    for (i, (node, depth)) in flattened_nodes.iter().enumerate() {
        let mut prefix = " ".repeat(*depth * 2);
        if node.is_dir {
            if node.expanded {
                prefix.push_str("- ");
            } else {
                prefix.push_str("+ ");
            }
        } else {
            prefix.push_str("  ");
        }

        let filename = node.path.file_name().unwrap_or_default().to_string_lossy();
        let label = format!("{}{}", prefix, filename);
        ctx.next_block_id_mixin(i as u64);
        let selection = ctx.list_item(
            state.file_tree.selected_node == Some(i),
            &label,
        );

        match selection {
            ListSelection::Selected => {
                state.file_tree.selected_node = Some(i);
            }
            ListSelection::Activated => {
                activated_path = Some(node.path.clone());
            }
            _ => {}
        }
    }

    if let Some(path) = activated_path {
        if path.is_dir() {
            toggle_expanded(&mut state.file_tree.nodes, &path);
        } else {
            state.documents.add_file_path(&path).ok();
        }
    }

    ctx.list_end();

    ctx.block_end();
}

fn flatten_tree<'a>(nodes: &'a [FileTreeNode]) -> Vec<(&'a FileTreeNode, usize)> {
    let mut flattened = vec![];
    for node in nodes {
        flatten_recursive(node, 0, &mut flattened);
    }
    flattened
}

fn flatten_recursive<'a>(
    node: &'a FileTreeNode,
    depth: usize,
    flattened: &mut Vec<(&'a FileTreeNode, usize)>,
) {
    flattened.push((node, depth));
    if node.expanded {
        for child in &node.children {
            flatten_recursive(child, depth + 1, flattened);
        }
    }
}

fn toggle_expanded(nodes: &mut [FileTreeNode], path: &PathBuf) -> bool {
    for node in nodes {
        if &node.path == path {
            node.expanded = !node.expanded;
            return true;
        }
        if toggle_expanded(&mut node.children, path) {
            return true;
        }
    }
    false
}

pub fn build_file_tree(path: &PathBuf) -> Vec<FileTreeNode> {
    let mut nodes = vec![];
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if let Some(file_name) = path.file_name() {
                    if file_name.to_string_lossy().starts_with('.') {
                        continue;
                    }
                }
                let is_dir = path.is_dir();
                let children = if is_dir {
                    build_file_tree(&path)
                } else {
                    vec![]
                };
                nodes.push(FileTreeNode {
                    path,
                    is_dir,
                    children,
                    expanded: false,
                });
            }
        }
    }
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    nodes
}
