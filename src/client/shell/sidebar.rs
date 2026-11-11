use super::*;
use ratatui::{
    text::Line,
    widgets::{Paragraph, Widget},
};

fn workspace_selection_background(palette: &Palette) -> ratatui::style::Color {
    if palette.selection_bg == ratatui::style::Color::Reset {
        palette.active_row_bg
    } else {
        palette.selection_bg
    }
}

pub(in crate::client::shell) fn workspace_active_background(
    palette: &Palette,
    navigating: bool,
) -> ratatui::style::Color {
    // The fallback cursor shares the active-row color; only fill the cursor while navigating.
    if navigating && palette.selection_bg == ratatui::style::Color::Reset {
        palette.sidebar_bg
    } else {
        palette.active_row_bg
    }
}

pub(in crate::client::shell) fn collapsed_sidebar_sections(
    area: Rect,
) -> (Rect, Option<u16>, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.is_empty() {
        return (Rect::default(), None, Rect::default());
    }
    if content.height < 7 {
        return (content, None, Rect::default());
    }
    let workspace_height = content.height.div_ceil(2);
    let divider_y = content.y + workspace_height;
    let detail_height = content.height.saturating_sub(workspace_height + 1);
    (
        Rect::new(content.x, content.y, content.width, workspace_height),
        Some(divider_y),
        Rect::new(content.x, divider_y + 1, content.width, detail_height),
    )
}

/// The first letters of a name, lowercased, as the collapsed sidebar abbreviates it.
fn initials(name: &str, count: usize) -> String {
    name.chars()
        .filter(|letter| letter.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .take(count)
        .collect()
}

/// The name an agent's line shows: its first text token.
fn agent_name(row: &super::agent_sidebar::AgentRow) -> &str {
    use crate::ui::ResolvedTokenKind as Kind;
    row.rows
        .iter()
        .flatten()
        .find_map(|token| match &token.kind {
            Kind::Machine(text)
            | Kind::Tab(text)
            | Kind::Pane(text)
            | Kind::Agent(text)
            | Kind::TerminalTitle(text)
            | Kind::Custom(text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or_default()
}

/// The expanded list in miniature: a space's first letter and, under an expanded space, an agent's
/// first two, each with its state mark on the right.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_collapsed_sidebar(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    selected_workspace_id: Option<&str>,
    collapsed_workspaces: &HashSet<String>,
    working_frame: usize,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    let selection_background = workspace_selection_background(palette);
    let active_background = workspace_active_background(palette, selected_workspace_id.is_some());
    render_sidebar_background(buffer, area, palette);
    // the last row keeps the sidebar toggle
    let content = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(1),
        area.height.saturating_sub(1),
    );
    // every space shows here, worktree groups included, as before
    let collapsed_groups = &HashSet::new();
    let items = sidebar_items(snapshot, config, collapsed_groups, collapsed_workspaces);
    let mark_x = content.x.saturating_add(2);
    for (index, item) in items.iter().take(content.height as usize).enumerate() {
        let rect = Rect::new(content.x, content.y + index as u16, content.width, 1);
        match item {
            SidebarItem::Workspace {
                entry,
                has_agents,
                collapsed,
                agent,
            } => {
                let workspace = &snapshot.workspaces[entry.index];
                let letter = initials(&workspace.label, 1);
                let selected = selected_workspace_id == Some(workspace.workspace_id.as_str());
                if selected {
                    buffer.set_style(rect, Style::default().bg(selection_background));
                } else if workspace.focused {
                    buffer.set_style(rect, Style::default().bg(active_background));
                }
                let letter_style = if selected {
                    Style::default()
                        .fg(palette.overlay1)
                        .bg(selection_background)
                } else if workspace.focused {
                    Style::default().fg(palette.text).bg(active_background)
                } else {
                    Style::default().fg(palette.overlay0)
                };
                put_text(
                    buffer,
                    rect.x,
                    rect.y,
                    rect.width.min(1),
                    &letter,
                    letter_style,
                );
                // as in the expanded list: the state, or else the expand toggle
                let status = displayed_workspace_status(snapshot, workspace, collapsed_groups);
                let mark = match agent {
                    Some(agent) => {
                        status_mark(agent.status, working_frame).map(|mark| (mark, agent.status))
                    }
                    None if *has_agents && !*collapsed => None,
                    None => status_mark(status, working_frame).map(|mark| (mark, status)),
                };
                let symbol = match mark {
                    Some((mark, mark_status)) => Some((
                        mark,
                        Style::default().fg(status_color(mark_status, palette)),
                    )),
                    None if *has_agents => Some((
                        if *collapsed { "⏵" } else { "⏷" },
                        Style::default().fg(palette.overlay0),
                    )),
                    None => None,
                };
                if let Some((symbol, style)) = symbol {
                    put_text(buffer, mark_x, rect.y, 1, symbol, style);
                }
                hits.workspaces.push(WorkspaceHit {
                    rect,
                    endpoint_id: ClientEndpointId::Local,
                    workspace_id: workspace.workspace_id.clone(),
                    indented: false,
                    group_toggle: None,
                    block_bottom: rect.bottom(),
                    pane_id: agent.as_ref().map(|agent| agent.pane_id.clone()),
                    toggles: *has_agents,
                });
            }
            SidebarItem::Agent { row, .. } => {
                if row.focused {
                    buffer.set_style(rect, Style::default().bg(palette.active_row_bg));
                }
                put_text(
                    buffer,
                    rect.x,
                    rect.y,
                    mark_x.saturating_sub(rect.x),
                    &initials(agent_name(row), 2),
                    Style::default().fg(if row.focused {
                        palette.text
                    } else {
                        palette.overlay0
                    }),
                );
                if let Some(mark) = status_mark(row.status, working_frame) {
                    put_text(
                        buffer,
                        mark_x,
                        rect.y,
                        1,
                        mark,
                        Style::default().fg(status_color(row.status, palette)),
                    );
                }
                hits.agents.push((rect, row.pane_id.clone()));
                if let Some(space) = hits.workspaces.last_mut() {
                    space.block_bottom = rect.bottom();
                }
            }
        }
    }
    hits.sidebar_toggle = if area.is_empty() || content.width == 0 {
        Rect::default()
    } else {
        Rect::new(
            content.x + content.width / 2,
            area.bottom().saturating_sub(1),
            1,
            1,
        )
    };
    put_text(
        buffer,
        hits.sidebar_toggle.x,
        hits.sidebar_toggle.y,
        hits.sidebar_toggle.width,
        "»",
        if super::super::global_menu::global_menu_attention(snapshot) {
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.overlay0)
        },
    );
}

/// One line of the expanded sidebar: a space, or an agent listed under its space.
pub(in crate::client::shell) enum SidebarItem {
    Workspace {
        entry: WorkspaceEntry,
        /// More than one agent, listed under the space.
        has_agents: bool,
        collapsed: bool,
        /// The space's only agent, shown by the space's own line.
        agent: Option<super::agent_sidebar::AgentRow>,
    },
    Agent {
        indented: bool,
        row: super::agent_sidebar::AgentRow,
    },
}

/// The spaces in sidebar order, each followed by its agents unless it is collapsed; a lone agent
/// stays on its space's line.
pub(in crate::client::shell) fn sidebar_items(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    collapsed_groups: &HashSet<String>,
    collapsed_workspaces: &HashSet<String>,
) -> Vec<SidebarItem> {
    let mut agents = HashMap::<String, Vec<super::agent_sidebar::AgentRow>>::new();
    for mut agent in super::agent_sidebar::agent_rows(snapshot, config, None) {
        // the space above already names it, and the state shows on the right
        for row in &mut agent.rows {
            row.retain(|token| {
                !matches!(
                    token.kind,
                    crate::ui::ResolvedTokenKind::Workspace(_)
                        | crate::ui::ResolvedTokenKind::StateIcon
                        | crate::ui::ResolvedTokenKind::StateText(_)
                )
            });
        }
        agent.rows.retain(|row| !row.is_empty());
        agents
            .entry(agent.workspace_id.clone())
            .or_default()
            .push(agent);
    }
    let mut items = Vec::new();
    for entry in workspace_entries(snapshot, collapsed_groups) {
        let workspace_id = &snapshot.workspaces[entry.index].workspace_id;
        let mut rows = agents.remove(workspace_id).unwrap_or_default();
        let agent = (rows.len() == 1).then(|| rows.remove(0));
        let collapsed = collapsed_workspaces.contains(workspace_id);
        items.push(SidebarItem::Workspace {
            entry,
            has_agents: !rows.is_empty(),
            collapsed,
            agent,
        });
        if !collapsed {
            items.extend(rows.into_iter().map(|row| SidebarItem::Agent {
                indented: entry.indented,
                row,
            }));
        }
    }
    items
}

/// The rows of a space's line, without the state icon: its state shows on the right.
fn sidebar_workspace_rows(
    snapshot: &ClientShellSnapshot,
    workspace: &ClientShellWorkspace,
    entry: &WorkspaceEntry,
    collapsed_groups: &HashSet<String>,
    config: &ClientShellConfig,
) -> (
    crate::api::schema::AgentStatus,
    Vec<Vec<crate::ui::ResolvedToken>>,
) {
    let status = displayed_workspace_status(snapshot, workspace, collapsed_groups);
    let mut rows = workspace_rows(workspace, status, entry.indented, &config.spaces);
    for row in &mut rows {
        row.retain(|token| {
            !matches!(
                token.kind,
                crate::ui::ResolvedTokenKind::StateIcon
                    | crate::ui::ResolvedTokenKind::StateText(_)
            )
        });
    }
    rows.retain(|row| !row.is_empty());
    (status, rows)
}

pub(in crate::client::shell) fn sidebar_item_height(
    item: &SidebarItem,
    snapshot: &ClientShellSnapshot,
    collapsed_groups: &HashSet<String>,
    config: &ClientShellConfig,
) -> u16 {
    let lines = match item {
        SidebarItem::Workspace { entry, .. } => {
            snapshot.workspaces.get(entry.index).map_or(1, |workspace| {
                sidebar_workspace_rows(snapshot, workspace, entry, collapsed_groups, config)
                    .1
                    .len()
            })
        }
        SidebarItem::Agent { row, .. } => row.rows.len(),
    };
    lines.max(1).min(u16::MAX as usize) as u16
}

pub(in crate::client::shell) fn sidebar_item_gap(
    current: &SidebarItem,
    next: Option<&SidebarItem>,
    config: &ClientShellConfig,
) -> u16 {
    match (current, next) {
        (_, Some(SidebarItem::Workspace { entry, .. })) => {
            u16::from(!entry.indented) * config.spaces.row_gap
        }
        (SidebarItem::Agent { .. }, Some(SidebarItem::Agent { .. })) => config.agents.row_gap,
        _ => 0,
    }
}

const WORKING_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The state mark of a sidebar line: a braille spinner while working, a check when done,
/// a dot while asking; idle needs no mark.
pub(in crate::client::shell) fn status_mark(
    status: crate::api::schema::AgentStatus,
    frame: usize,
) -> Option<&'static str> {
    use crate::api::schema::AgentStatus;
    match status {
        AgentStatus::Blocked => Some("●"),
        AgentStatus::Done => Some("✓"),
        AgentStatus::Working => Some(WORKING_FRAMES[frame % WORKING_FRAMES.len()]),
        AgentStatus::Idle | AgentStatus::Unknown => None,
    }
}

/// The position of a space in the expanded sidebar list.
pub(in crate::client::shell) fn sidebar_item_position(
    items: &[SidebarItem],
    snapshot: &ClientShellSnapshot,
    workspace_id: &str,
) -> Option<usize> {
    items.iter().position(|item| {
        matches!(item, SidebarItem::Workspace { entry, .. }
            if snapshot.workspaces[entry.index].workspace_id == workspace_id)
    })
}

/// Draws one line of the expanded list: a space, or an agent under it. Without `collapsed_groups`
/// worktree groups cannot fold, and their parents get no toggle; the toggle drawn is returned.
#[allow(clippy::too_many_arguments)]
pub(in crate::client::shell) fn render_sidebar_item(
    buffer: &mut Buffer,
    rect: Rect,
    item: &SidebarItem,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    collapsed_groups: Option<&HashSet<String>>,
    selected: bool,
    dragged: bool,
    navigating: bool,
    working_frame: usize,
) -> Option<(Rect, String)> {
    let palette = &config.palette;
    match item {
        SidebarItem::Workspace {
            entry,
            has_agents,
            collapsed,
            agent,
        } => {
            let workspace = snapshot.workspaces.get(entry.index)?;
            let no_groups = HashSet::new();
            let (status, rows) = sidebar_workspace_rows(
                snapshot,
                workspace,
                entry,
                collapsed_groups.unwrap_or(&no_groups),
                config,
            );
            // the focused agent marks where you are; the space itself is not highlighted
            if selected {
                buffer.set_style(rect, Style::default().bg(palette.selection_bg));
            } else if dragged {
                buffer.set_style(rect, Style::default().bg(palette.surface1));
            }
            // an expanded space leaves its state to its agents; a lone agent's is the space's
            let mark = match agent {
                Some(agent) => {
                    status_mark(agent.status, working_frame).map(|mark| (mark, agent.status))
                }
                None if *has_agents && !*collapsed => None,
                None => status_mark(status, working_frame).map(|mark| (mark, status)),
            };
            let agent_focused = agent.as_ref().is_some_and(|agent| agent.focused);
            // one column on the right, a column clear of the separator and before a worktree
            // group's toggle: the state, or else the expand toggle of a space with agents under it
            let slot = rect.right().saturating_sub(2).saturating_sub(
                if collapsed_groups.is_some() && parent_group_key(snapshot, entry.index).is_some() {
                    2
                } else {
                    0
                },
            );
            let nested = Rect::new(
                rect.x,
                rect.y,
                slot.saturating_sub(1).saturating_sub(rect.x),
                rect.height,
            );
            render_workspace_rows(
                buffer,
                nested,
                status,
                config.status_indicators,
                entry,
                rows,
                agent_focused,
                selected,
                navigating,
                dragged,
                palette,
            );
            // the right columns share the row's background
            if !nested.is_empty() {
                for y in rect.y..rect.bottom() {
                    let background = buffer[(nested.x, y)].bg;
                    for x in nested.right()..rect.right() {
                        buffer[(x, y)].set_bg(background);
                    }
                }
            }
            let symbol = match mark {
                Some((mark, mark_status)) => Some((
                    mark,
                    Style::default().fg(status_color(mark_status, palette)),
                )),
                None if *has_agents => Some((
                    if *collapsed { "⏵" } else { "⏷" },
                    Style::default().fg(palette.overlay0),
                )),
                None => None,
            };
            if let Some((symbol, style)) = symbol {
                put_text(buffer, slot, rect.y, 1, symbol, style);
            }
            collapsed_groups.and_then(|groups| {
                render_parent_group_toggle(buffer, rect, snapshot, entry.index, groups, palette)
            })
        }
        SidebarItem::Agent { indented, row } => {
            if row.focused {
                buffer.set_style(rect, Style::default().bg(palette.active_row_bg));
            }
            // three columns in from the space, or past a worktree child's tree lines
            let indent = if *indented { 7 } else { 2 };
            super::agent_sidebar::render_agent_row(
                buffer,
                Rect::new(
                    rect.x.saturating_add(indent),
                    rect.y,
                    rect.width.saturating_sub(indent + 3),
                    rect.height,
                ),
                row,
                config,
            );
            // the state lines up with the spaces' column on the right
            if let Some(mark) = status_mark(row.status, working_frame) {
                put_text(
                    buffer,
                    rect.right().saturating_sub(2),
                    rect.y,
                    1,
                    mark,
                    Style::default().fg(status_color(row.status, palette)),
                );
            }
            None
        }
    }
}

pub(crate) fn render_sidebar(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    state: &mut ShellRenderState<'_>,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    render_sidebar_background(buffer, area, palette);
    hits.sidebar_divider = if area.is_empty() {
        Rect::default()
    } else {
        Rect::new(area.right().saturating_sub(1), area.y, 1, area.height)
    };
    // one list for spaces and their agents; the last row keeps the sidebar toggle
    let workspace_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(1),
        area.height.saturating_sub(1),
    );
    hits.sidebar_section_divider = Rect::default();
    put_text(
        buffer,
        workspace_area.x,
        workspace_area.y,
        workspace_area.width,
        " spaces",
        Style::default()
            .fg(palette.overlay0)
            .add_modifier(Modifier::BOLD),
    );

    let items = sidebar_items(
        snapshot,
        config,
        state.collapsed_groups,
        state.collapsed_workspaces,
    );
    let body = Rect::new(
        workspace_area.x,
        workspace_area.y.saturating_add(WORKSPACE_HEADER_ROWS),
        workspace_area.width,
        workspace_area
            .height
            .saturating_sub(WORKSPACE_HEADER_ROWS + 1),
    );
    hits.workspace_body = body;
    let row_heights = items
        .iter()
        .map(|item| sidebar_item_height(item, snapshot, state.collapsed_groups, config))
        .collect::<Vec<_>>();
    let gaps = items
        .iter()
        .enumerate()
        .map(|(index, item)| sidebar_item_gap(item, items.get(index + 1), config))
        .collect::<Vec<_>>();
    let mut metrics = super::scroll::list_scroll_metrics(
        &row_heights,
        &gaps,
        body.height,
        *state.workspace_scroll,
    );
    if !body.is_empty() && std::mem::take(state.reveal_focused_workspace) {
        if let Some(target) = snapshot
            .focused_workspace_id
            .as_deref()
            .and_then(|workspace_id| sidebar_item_position(&items, snapshot, workspace_id))
        {
            *state.workspace_scroll = super::scroll::list_scroll_start_to_reveal(
                &row_heights,
                &gaps,
                body.height,
                *state.workspace_scroll,
                target,
            );
            metrics = super::scroll::list_scroll_metrics(
                &row_heights,
                &gaps,
                body.height,
                *state.workspace_scroll,
            );
        }
    }
    hits.workspace_max_scroll = metrics.max_offset_from_bottom;
    hits.workspace_scroll_metrics = Some(metrics);
    *state.workspace_scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let show_scrollbar = metrics.max_offset_from_bottom > 0 && body.width > 1;
    let content_width = body.width.saturating_sub(u16::from(show_scrollbar));
    let mut y = body.y;
    let mut list_end = Some(y);
    for (item_position, item) in items.iter().enumerate().skip(*state.workspace_scroll) {
        let row_height = row_heights[item_position].min(body.height);
        if y.saturating_add(row_height) > body.bottom() {
            list_end = None;
            break;
        }
        let rect = Rect::new(body.x, y, content_width, row_height);
        match item {
            SidebarItem::Workspace {
                entry,
                has_agents,
                agent,
                ..
            } => {
                let Some(workspace) = snapshot.workspaces.get(entry.index) else {
                    continue;
                };
                let selected = state.selected_workspace_id.is_some_and(|target| {
                    target.matches(state.active_endpoint_id, &workspace.workspace_id)
                });
                let dragged = state.dragged_workspace_id == Some(workspace.workspace_id.as_str());
                let group_toggle = render_sidebar_item(
                    buffer,
                    rect,
                    item,
                    snapshot,
                    config,
                    Some(state.collapsed_groups),
                    selected,
                    dragged,
                    state.selected_workspace_id.is_some(),
                    state.working_frame,
                );
                hits.workspaces.push(WorkspaceHit {
                    rect,
                    endpoint_id: ClientEndpointId::Local,
                    workspace_id: workspace.workspace_id.clone(),
                    indented: entry.indented,
                    group_toggle,
                    block_bottom: rect.bottom(),
                    pane_id: agent.as_ref().map(|agent| agent.pane_id.clone()),
                    toggles: *has_agents,
                });
            }
            SidebarItem::Agent { row, .. } => {
                render_sidebar_item(
                    buffer,
                    rect,
                    item,
                    snapshot,
                    config,
                    Some(state.collapsed_groups),
                    false,
                    false,
                    false,
                    state.working_frame,
                );
                hits.agents.push((rect, row.pane_id.clone()));
                if let Some(space) = hits.workspaces.last_mut() {
                    space.block_bottom = rect.bottom();
                }
            }
        }
        y = y.saturating_add(row_height + gaps[item_position]);
        list_end = Some(y);
    }

    if show_scrollbar {
        let track = Rect::new(body.right().saturating_sub(1), body.y, 1, body.height);
        hits.workspace_scrollbar = track;
        super::scroll::render_list_scrollbar(buffer, track, metrics, palette);
    }

    if let Some(row) = state.workspace_drop_indicator_row.filter(|row| {
        *row >= workspace_area.y.saturating_add(1)
            && *row < workspace_area.bottom().saturating_sub(1)
    }) {
        put_text(
            buffer,
            body.x,
            row,
            body.width,
            &"─".repeat(body.width as usize),
            Style::default().fg(palette.accent),
        );
    }

    let footer_y = workspace_area.bottom().saturating_sub(1);
    if config.mouse_capture {
        // "new" follows the last space, or waits at the bottom while the list scrolls on
        let new_y = list_end.map_or(footer_y, |end| end.min(footer_y));
        hits.new_workspace = Rect::new(
            workspace_area.x,
            new_y,
            5.min(workspace_area.width),
            u16::from(workspace_area.height > 0),
        );
        put_text(
            buffer,
            workspace_area.x,
            new_y,
            workspace_area.width,
            " new",
            Style::default().fg(palette.overlay0),
        );
        // the menu sits on the title row, a column clear of the separator
        let attention = super::super::global_menu::global_menu_attention(snapshot);
        let menu_area = Rect::new(
            workspace_area.x,
            workspace_area.y,
            workspace_area.width.saturating_sub(1),
            1,
        );
        let launcher_width = if attention { 8 } else { 6 }.min(menu_area.width);
        hits.global_launcher = Rect::new(
            menu_area.right().saturating_sub(launcher_width),
            menu_area.y,
            launcher_width,
            u16::from(workspace_area.height > 0),
        );
        if attention {
            let start_x = menu_area.right().saturating_sub(6);
            put_text(
                buffer,
                start_x,
                menu_area.y,
                2,
                "● ",
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            );
            put_text(
                buffer,
                start_x.saturating_add(2),
                menu_area.y,
                4,
                "menu",
                Style::default().fg(palette.overlay0),
            );
        } else {
            put_right_text(
                buffer,
                menu_area,
                menu_area.y,
                "menu",
                Style::default().fg(palette.overlay0),
            );
        }
    }

    hits.sidebar_toggle = Rect::new(
        area.right().saturating_sub(2),
        area.bottom().saturating_sub(1),
        u16::from(area.width > 1),
        u16::from(area.height > 0),
    );
    put_text(
        buffer,
        hits.sidebar_toggle.x,
        hits.sidebar_toggle.y,
        hits.sidebar_toggle.width,
        "«",
        Style::default().fg(palette.overlay0),
    );
}

pub(crate) fn workspace_entries(
    snapshot: &ClientShellSnapshot,
    collapsed_groups: &HashSet<String>,
) -> Vec<WorkspaceEntry> {
    let mut members = HashMap::<&str, Vec<usize>>::new();
    for (index, workspace) in snapshot.workspaces.iter().enumerate() {
        if let Some(worktree) = &workspace.worktree {
            members.entry(&worktree.key).or_default().push(index);
        }
    }
    let grouped = members
        .iter()
        .filter(|(_, indices)| {
            indices.iter().any(|index| {
                snapshot.workspaces[*index]
                    .worktree
                    .as_ref()
                    .is_some_and(|worktree| worktree.is_linked_worktree)
            }) && indices.iter().any(|index| {
                snapshot.workspaces[*index]
                    .worktree
                    .as_ref()
                    .is_some_and(|worktree| !worktree.is_linked_worktree)
            })
        })
        .map(|(key, _)| *key)
        .collect::<HashSet<_>>();
    let mut emitted = HashSet::<&str>::new();
    let mut entries = Vec::new();
    for (index, workspace) in snapshot.workspaces.iter().enumerate() {
        let Some(worktree) = workspace
            .worktree
            .as_ref()
            .filter(|worktree| grouped.contains(worktree.key.as_str()))
        else {
            entries.push(WorkspaceEntry {
                index,
                indented: false,
                last_child: false,
            });
            continue;
        };
        if !emitted.insert(&worktree.key) {
            continue;
        }
        let Some(group_members) = members.get(worktree.key.as_str()) else {
            continue;
        };
        for parent in group_members.iter().copied().filter(|member| {
            snapshot.workspaces[*member]
                .worktree
                .as_ref()
                .is_some_and(|worktree| !worktree.is_linked_worktree)
        }) {
            entries.push(WorkspaceEntry {
                index: parent,
                indented: false,
                last_child: false,
            });
        }
        if collapsed_groups.contains(&worktree.key) {
            if let Some(active) = group_members.iter().copied().find(|member| {
                let workspace = &snapshot.workspaces[*member];
                workspace.focused
                    && workspace
                        .worktree
                        .as_ref()
                        .is_some_and(|worktree| worktree.is_linked_worktree)
            }) {
                entries.push(WorkspaceEntry {
                    index: active,
                    indented: true,
                    last_child: true,
                });
            }
            continue;
        }
        let children = group_members
            .iter()
            .copied()
            .filter(|member| {
                snapshot.workspaces[*member]
                    .worktree
                    .as_ref()
                    .is_some_and(|worktree| worktree.is_linked_worktree)
            })
            .collect::<Vec<_>>();
        for (child_index, child) in children.iter().enumerate() {
            entries.push(WorkspaceEntry {
                index: *child,
                indented: true,
                last_child: child_index + 1 == children.len(),
            });
        }
    }
    entries
}

fn parent_group_key(snapshot: &ClientShellSnapshot, index: usize) -> Option<String> {
    let workspace = snapshot.workspaces.get(index)?;
    let worktree = workspace.worktree.as_ref()?;
    if worktree.is_linked_worktree {
        return None;
    }
    snapshot
        .workspaces
        .iter()
        .any(|candidate| {
            candidate.worktree.as_ref().is_some_and(|candidate| {
                candidate.key == worktree.key && candidate.is_linked_worktree
            })
        })
        .then(|| worktree.key.clone())
}

pub(in crate::client::shell) fn workspace_close_is_group(
    snapshot: &ClientShellSnapshot,
    workspace: &ClientShellWorkspace,
) -> bool {
    let Some(worktree) = workspace
        .worktree
        .as_ref()
        .filter(|worktree| !worktree.is_linked_worktree)
    else {
        return false;
    };
    let mut has_child = false;
    for member in &snapshot.workspaces {
        if member.workspace_id == workspace.workspace_id {
            continue;
        }
        if let Some(candidate) = member
            .worktree
            .as_ref()
            .filter(|candidate| candidate.key == worktree.key)
        {
            if !candidate.is_linked_worktree {
                return false;
            }
            has_child = true;
        }
    }
    has_child
}

pub(in crate::client::shell) fn render_parent_group_toggle(
    buffer: &mut Buffer,
    workspace_rect: Rect,
    snapshot: &ClientShellSnapshot,
    workspace_index: usize,
    collapsed_groups: &HashSet<String>,
    palette: &Palette,
) -> Option<(Rect, String)> {
    let key = parent_group_key(snapshot, workspace_index)?;
    let toggle = Rect::new(
        workspace_rect.right().saturating_sub(1),
        workspace_rect.y,
        1,
        1,
    );
    put_text(
        buffer,
        toggle.x,
        toggle.y,
        toggle.width,
        if collapsed_groups.contains(&key) {
            "▸"
        } else {
            "▾"
        },
        Style::default().fg(palette.accent),
    );
    Some((toggle, key))
}

pub(in crate::client::shell) fn displayed_workspace_status(
    snapshot: &ClientShellSnapshot,
    workspace: &ClientShellWorkspace,
    collapsed_groups: &HashSet<String>,
) -> crate::api::schema::AgentStatus {
    let Some(worktree) = workspace
        .worktree
        .as_ref()
        .filter(|worktree| !worktree.is_linked_worktree)
    else {
        return workspace.agent_status;
    };
    if !collapsed_groups.contains(&worktree.key) {
        return workspace.agent_status;
    }
    snapshot
        .workspaces
        .iter()
        .filter(|candidate| {
            candidate.worktree.as_ref().is_some_and(|member| {
                member.key == worktree.key
                    && (member.is_linked_worktree
                        || candidate.workspace_id == workspace.workspace_id)
            })
        })
        .map(|candidate| candidate.agent_status)
        .max_by_key(|status| status_priority(*status))
        .unwrap_or(workspace.agent_status)
}

pub(in crate::client::shell) fn workspace_rows(
    workspace: &ClientShellWorkspace,
    status: crate::api::schema::AgentStatus,
    indented: bool,
    config: &SpacesSidebarConfig,
) -> Vec<Vec<crate::ui::ResolvedToken>> {
    let label = if indented && !workspace.custom_label {
        workspace
            .branch
            .as_deref()
            .and_then(|branch| branch.strip_prefix("worktree/").or(Some(branch)))
            .unwrap_or(&workspace.label)
    } else {
        &workspace.label
    };
    let token_values = workspace.tokens.iter().cloned().collect::<HashMap<_, _>>();
    crate::ui::sidebar_space_rows(
        config,
        crate::ui::SpaceTokenContext {
            workspace: label,
            branch: workspace.branch.as_deref(),
            state_text: status_text(status),
            ahead_behind: workspace.git_ahead_behind,
            tokens: &token_values,
            suppress_git_details: indented,
        },
    )
}

pub(in crate::client::shell) fn render_workspace_rows(
    buffer: &mut Buffer,
    area: Rect,
    status: crate::api::schema::AgentStatus,
    indicators: crate::config::StatusIndicatorStyle,
    entry: &WorkspaceEntry,
    rows: Vec<Vec<crate::ui::ResolvedToken>>,
    focused: bool,
    selected: bool,
    navigating: bool,
    dragged: bool,
    palette: &Palette,
) {
    for (row_index, row) in rows.iter().enumerate() {
        let y = area.y + row_index as u16;
        if y >= area.bottom() {
            break;
        }
        let mut x = area.x;
        if entry.indented {
            let prefix = if row_index == 0 {
                if entry.last_child {
                    "   └─ "
                } else {
                    "   ├─ "
                }
            } else if entry.last_child {
                "        "
            } else {
                "   │    "
            };
            x = put_segment(
                buffer,
                x,
                y,
                area.right(),
                prefix,
                Style::default().fg(palette.overlay0),
            );
        } else if row_index == 0 {
            x = x.saturating_add(1);
        } else {
            x = x.saturating_add(3);
        }
        let highlighted = focused || dragged;
        let workspace_style = Style::default()
            .fg(if highlighted {
                palette.text
            } else {
                palette.subtext0
            })
            .add_modifier(if highlighted {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        let secondary_style = Style::default().fg(if focused {
            palette.mauve
        } else {
            palette.overlay0
        });
        let spans = crate::ui::resolved_token_spans(
            row,
            (
                status_icon(status, indicators),
                Style::default().fg(status_color(status, palette)),
            ),
            Style::default().fg(status_color(status, palette)),
            workspace_style,
            secondary_style,
            Style::default().fg(palette.overlay1),
            palette,
            area.right().saturating_sub(2).saturating_sub(x) as usize,
        );
        Paragraph::new(Line::from(spans)).render(
            Rect::new(x, y, area.right().saturating_sub(2).saturating_sub(x), 1),
            buffer,
        );
    }

    let background = if selected {
        Some(workspace_selection_background(palette))
    } else if dragged {
        Some(palette.surface1)
    } else if focused {
        Some(workspace_active_background(palette, navigating))
    } else {
        None
    };
    if let Some(background) = background {
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buffer[(x, y)].set_bg(background);
            }
        }
    }
}
