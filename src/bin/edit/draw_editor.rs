// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::num::ParseIntError;

use edit::framebuffer::IndexedColor;
use edit::helpers::*;
use edit::icu;
use edit::input::{kbmod, vk};
use edit::lsp::LspMessage;
use edit::tui::{ButtonStyle, Context, FloatSpec, Anchor, Position as TuiPosition};
use log::error;
use lsp_types::{Position, Url};
use tokio::sync::mpsc;

use crate::localization::*;
use crate::state::*;

pub async fn draw_editor(
    ctx: &mut Context<'_, '_>,
    state: &mut State,
    tx: &mpsc::Sender<LspMessage>,
) {
    if !matches!(state.wants_search.kind, StateSearchKind::Hidden | StateSearchKind::Disabled) {
        draw_search(ctx, state);
    }

    let size = ctx.size();
    // TODO: The layout code should be able to just figure out the height on its own.
    let height_reduction = match state.wants_search.kind {
        StateSearchKind::Search => 4,
        StateSearchKind::Replace => 5,
        _ => 2,
    };

    if let Some(doc) = state.documents.active() {
        if doc.language.is_some() {
            draw_highlighted_editor(ctx, state, tx).await;
        } else {
            ctx.textarea("textarea", doc.buffer.clone());
            ctx.inherit_focus();
        }
        if state.completion_items.is_some() {
            draw_suggestions_popup(ctx, state);
        }
    } else {
        ctx.block_begin("empty");
        ctx.block_end();
    }

    ctx.attr_intrinsic_size(Size { width: 0, height: size.height - height_reduction });
}

async fn draw_highlighted_editor(
    ctx: &mut Context<'_, '_>,
    state: &mut State,
    tx: &mpsc::Sender<LspMessage>,
) {
    let doc = state.documents.active_mut().unwrap();
    let lang = doc.language.unwrap();

    let buffer_generation = doc.buffer.borrow().generation();
    let generation_changed = doc.buffer_generation != buffer_generation;

    if generation_changed {
        let mut code = String::new();
        doc.buffer.borrow_mut().save_as_string(&mut code);

        doc.syntax_tree = state.syntax.parse(&code, lang);
        doc.highlights = state.syntax.highlight(&code, lang).collect();
        doc.buffer_generation = buffer_generation;
    }

    let highlights = doc.highlights.clone();
    doc.buffer.borrow_mut().set_highlights(highlights);

    ctx.textarea("textarea", doc.buffer.clone());

    if generation_changed {
        if let Some(path) = &doc.path {
            let uri = Url::from_file_path(path).unwrap();
            let mut buffer = doc.buffer.borrow_mut();
            let mut code = String::new();
            buffer.save_as_string(&mut code);
            if let Err(e) = tx.send(LspMessage::DidChange(uri.clone(), code, 1)).await {
                error!("Failed to send DidChange message: {}", e);
            }

            let cursor = buffer.cursor_logical_pos();
            let position = Position::new(cursor.y as u32, cursor.x as u32);
            if let Err(e) = tx.send(LspMessage::Completion(uri, position)).await {
                error!("Failed to send Completion message: {}", e);
            }
        }
    }
    ctx.inherit_focus();
}

pub fn draw_suggestions_popup(ctx: &mut Context, state: &mut State) {
    if let Some(items) = &state.completion_items {
        let doc = state.documents.active().unwrap();
        let cursor_pos = doc.buffer.borrow().cursor_visual_pos();

        ctx.list_begin("suggestions");
        ctx.attr_float(FloatSpec {
            anchor: Anchor::Last,
            gravity_x: 0.0,
            gravity_y: 1.0,
            offset_x: cursor_pos.x as f32,
            offset_y: cursor_pos.y as f32,
        });

        for (i, item) in items.iter().enumerate() {
            ctx.list_item(i == state.selected_completion_item, &item.label);
        }

        ctx.list_end();
    }
}

fn draw_search(ctx: &mut Context, state: &mut State) {
    if let Err(err) = icu::init() {
        error_log_add(ctx, state, err);
        state.wants_search.kind = StateSearchKind::Disabled;
        return;
    }

    let Some(doc) = state.documents.active() else {
        state.wants_search.kind = StateSearchKind::Hidden;
        return;
    };

    let mut action = None;
    let mut focus = StateSearchKind::Hidden;

    if state.wants_search.focus {
        state.wants_search.focus = false;
        focus = StateSearchKind::Search;

        // If the selection is empty, focus the search input field.
        // Otherwise, focus the replace input field, if it exists.
        if let Some(selection) = doc.buffer.borrow_mut().extract_user_selection(false) {
            state.search_needle = String::from_utf8_lossy_owned(selection);
            focus = state.wants_search.kind;
        }
    }

    ctx.block_begin("search");
    ctx.attr_focus_well();
    ctx.attr_background_rgba(ctx.indexed(IndexedColor::White));
    ctx.attr_foreground_rgba(ctx.indexed(IndexedColor::Black));
    {
        if ctx.contains_focus() && ctx.consume_shortcut(vk::ESCAPE) {
            state.wants_search.kind = StateSearchKind::Hidden;
        }

        ctx.table_begin("needle");
        ctx.table_set_cell_gap(Size { width: 1, height: 0 });
        {
            {
                ctx.table_next_row();
                ctx.label("label", loc(LocId::SearchNeedleLabel));

                if ctx.editline("needle", &mut state.search_needle) {
                    action = Some(SearchAction::Search);
                }
                if !state.search_success {
                    ctx.attr_background_rgba(ctx.indexed(IndexedColor::Red));
                    ctx.attr_foreground_rgba(ctx.indexed(IndexedColor::BrightWhite));
                }
                ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
                if focus == StateSearchKind::Search {
                    ctx.steal_focus();
                }
                if ctx.is_focused() && ctx.consume_shortcut(vk::RETURN) {
                    action = Some(SearchAction::Search);
                }
            }

            if state.wants_search.kind == StateSearchKind::Replace {
                ctx.table_next_row();
                ctx.label("label", loc(LocId::SearchReplacementLabel));

                ctx.editline("replacement", &mut state.search_replacement);
                ctx.attr_intrinsic_size(Size { width: COORD_TYPE_SAFE_MAX, height: 1 });
                if focus == StateSearchKind::Replace {
                    ctx.steal_focus();
                }
                if ctx.is_focused() {
                    if ctx.consume_shortcut(vk::RETURN) {
                        action = Some(SearchAction::Replace);
                    } else if ctx.consume_shortcut(kbmod::CTRL_ALT | vk::RETURN) {
                        action = Some(SearchAction::ReplaceAll);
                    }
                }
            }
        }
        ctx.table_end();

        ctx.table_begin("options");
        ctx.table_set_cell_gap(Size { width: 2, height: 0 });
        {
            let mut change = false;
            let mut change_action = Some(SearchAction::Search);

            ctx.table_next_row();

            change |= ctx.checkbox(
                "match-case",
                loc(LocId::SearchMatchCase),
                &mut state.search_options.match_case,
            );
            change |= ctx.checkbox(
                "whole-word",
                loc(LocId::SearchWholeWord),
                &mut state.search_options.whole_word,
            );
            change |= ctx.checkbox(
                "use-regex",
                loc(LocId::SearchUseRegex),
                &mut state.search_options.use_regex,
            );
            if state.wants_search.kind == StateSearchKind::Replace
                && ctx.button("replace-all", loc(LocId::SearchReplaceAll), ButtonStyle::default())
            {
                change = true;
                change_action = Some(SearchAction::ReplaceAll);
            }
            if ctx.button("close", loc(LocId::SearchClose), ButtonStyle::default()) {
                state.wants_search.kind = StateSearchKind::Hidden;
            }

            if change {
                action = change_action;
                state.wants_search.focus = true;
                ctx.needs_rerender();
            }
        }
        ctx.table_end();
    }
    ctx.block_end();

    if let Some(action) = action {
        search_execute(ctx, state, action);
    }
}

pub enum SearchAction {
    Search,
    Replace,
    ReplaceAll,
}

pub fn search_execute(ctx: &mut Context, state: &mut State, action: SearchAction) {
    let Some(doc) = state.documents.active_mut() else {
        return;
    };

    state.search_success = match action {
        SearchAction::Search => {
            doc.buffer.borrow_mut().find_and_select(&state.search_needle, state.search_options)
        }
        SearchAction::Replace => doc.buffer.borrow_mut().find_and_replace(
            &state.search_needle,
            state.search_options,
            state.search_replacement.as_bytes(),
        ),
        SearchAction::ReplaceAll => doc.buffer.borrow_mut().find_and_replace_all(
            &state.search_needle,
            state.search_options,
            state.search_replacement.as_bytes(),
        ),
    }
    .is_ok();

    ctx.needs_rerender();
}

pub fn draw_handle_save(ctx: &mut Context, state: &mut State) {
    if let Some(doc) = state.documents.active_mut() {
        if doc.path.is_some() {
            if let Err(err) = doc.save(None) {
                error_log_add(ctx, state, err);
            }
        } else {
            // No path? Show the file picker.
            state.wants_file_picker = StateFilePicker::SaveAs;
            state.wants_save = false;
            ctx.needs_rerender();
        }
    }

    state.wants_save = false;
}

pub fn draw_handle_wants_close(ctx: &mut Context, state: &mut State) {
    let Some(doc) = state.documents.active() else {
        state.wants_close = false;
        return;
    };

    if !doc.buffer.borrow().is_dirty() {
        state.documents.remove_active();
        state.wants_close = false;
        ctx.needs_rerender();
        return;
    }

    enum Action {
        None,
        Save,
        Discard,
        Cancel,
    }
    let mut action = Action::None;

    ctx.modal_begin("unsaved-changes", loc(LocId::UnsavedChangesDialogTitle));
    ctx.attr_background_rgba(ctx.indexed(IndexedColor::Red));
    ctx.attr_foreground_rgba(ctx.indexed(IndexedColor::BrightWhite));
    {
        let contains_focus = ctx.contains_focus();

        ctx.label("description", loc(LocId::UnsavedChangesDialogDescription));
        ctx.attr_padding(Rect::three(1, 2, 1));

        ctx.table_begin("choices");
        ctx.inherit_focus();
        ctx.attr_padding(Rect::three(0, 2, 1));
        ctx.attr_position(TuiPosition::Center);
        ctx.table_set_cell_gap(Size { width: 2, height: 0 });
        {
            ctx.table_next_row();
            ctx.inherit_focus();

            if ctx.button(
                "yes",
                loc(LocId::UnsavedChangesDialogYes),
                ButtonStyle::default().accelerator('S'),
            ) {
                action = Action::Save;
            }
            ctx.inherit_focus();
            if ctx.button(
                "no",
                loc(LocId::UnsavedChangesDialogNo),
                ButtonStyle::default().accelerator('N'),
            ) {
                action = Action::Discard;
            }
            if ctx.button("cancel", loc(LocId::Cancel), ButtonStyle::default()) {
                action = Action::Cancel;
            }

            // Handle accelerator shortcuts
            if contains_focus {
                if ctx.consume_shortcut(vk::S) {
                    action = Action::Save;
                } else if ctx.consume_shortcut(vk::N) {
                    action = Action::Discard;
                }
            }
        }
        ctx.table_end();
    }
    if ctx.modal_end() {
        action = Action::Cancel;
    }

    match action {
        Action::None => return,
        Action::Save => {
            state.wants_save = true;
        }
        Action::Discard => {
            state.documents.remove_active();
            state.wants_close = false;
        }
        Action::Cancel => {
            state.wants_exit = false;
            state.wants_close = false;
        }
    }

    ctx.needs_rerender();
}

pub fn draw_goto_menu(ctx: &mut Context, state: &mut State) {
    let mut done = false;

    if let Some(doc) = state.documents.active_mut() {
        ctx.modal_begin("goto", loc(LocId::FileGoto));
        {
            if ctx.editline("goto-line", &mut state.goto_target) {
                state.goto_invalid = false;
            }
            if state.goto_invalid {
                ctx.attr_background_rgba(ctx.indexed(IndexedColor::Red));
                ctx.attr_foreground_rgba(ctx.indexed(IndexedColor::BrightWhite));
            }

            ctx.attr_intrinsic_size(Size { width: 24, height: 1 });
            ctx.steal_focus();

            if ctx.consume_shortcut(vk::RETURN) {
                match validate_goto_point(&state.goto_target) {
                    Ok(point) => {
                        let mut buf = doc.buffer.borrow_mut();
                        buf.cursor_move_to_logical(point);
                        buf.make_cursor_visible();
                        done = true;
                    }
                    Err(_) => state.goto_invalid = true,
                }
                ctx.needs_rerender();
            }
        }
        done |= ctx.modal_end();
    } else {
        done = true;
    }

    if done {
        state.wants_goto = false;
        state.goto_target.clear();
        state.goto_invalid = false;
        ctx.needs_rerender();
    }
}

fn validate_goto_point(line: &str) -> Result<Point, ParseIntError> {
    let mut coords = [0; 2];
    let (y, x) = line.split_once(':').unwrap_or((line, "0"));
    // Using a loop here avoids 2 copies of the str->int code.
    // This makes the binary more compact.
    for (i, s) in [x, y].iter().enumerate() {
        coords[i] = s.parse::<CoordType>()?.saturating_sub(1);
    }
    Ok(Point { x: coords[0], y: coords[1] })
}
