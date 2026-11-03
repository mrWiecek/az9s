use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// The help text. Kept in one place so it can be tested for coverage.
pub fn help_lines() -> Vec<Line<'static>> {
    let heading = |text: &'static str| {
        Line::from(Span::styled(
            text,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
    };
    let dim =
        |text: &'static str| Line::from(Span::styled(text, Style::default().fg(Color::DarkGray)));
    let l = Line::from;
    vec![
        Line::from(Span::styled(
            "az9s help  ·  j/k or ↑/↓ scroll  ·  Esc, q or ? close",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        l(""),
        heading("Go to anything (Ctrl+P)"),
        l("  Ctrl+P    fuzzy search commands, subscriptions, management groups, and the"),
        l("            loaded resource groups and resources: type a few letters (kvprod"),
        l("            finds kv-prod-weu), ↑/↓ choose, Enter goes there, Esc closes."),
        l("            Only loaded data is searched: after :res all, every resource."),
        l(""),
        heading("Commands (type : then the command, Enter runs it)"),
        l("  Suggestions appear as you type: commands, scopes, filter keys, and values"),
        l("  from what's loaded (type=, loc=, tag=key=…). Tab completes, ↑/↓ choose."),
        l("  :mg                                Management group tree (the start view)"),
        l("  :sub                               Subscriptions"),
        l("  :rg   [scope] [loc=…] [tag=…]      Resource groups"),
        l("  :res  [scope] [type=…] [loc=…] [tag=…]   Resources"),
        l("  :type [scope] [type=…] [loc=…] [tag=…]   How many resources of each type"),
        l("  :tag  [scope] [type=…] [loc=…] [tag=…]   Tags: key, value, how many resources"),
        l(""),
        l("  scope   (nothing)   where you are: the highlighted or current subscription"),
        l("                      or management group; all subscriptions if neither"),
        l("          all         every subscription"),
        l("          <name|ID>   one subscription, e.g. :rg prod-connectivity"),
        l(""),
        heading("Filters (after :rg, :res or :type; space-separated)"),
        l("  type=vm          a short alias (vm, vnet, nsg, aks, keyvault or key-vault…), the last"),
        l("                   part (virtualMachines), the full type, or a provider (Network,"),
        l("                   Microsoft.Network)"),
        l("  loc=westeurope   location"),
        l("  tag=env=prod     tag with that value (tag=env:prod also works)"),
        l("  tag=env          tag with any value"),
        l("  a,b              either value: type=vm,disk   loc=westeurope,northeurope"),
        l("  Several filters must all match. Case doesn't matter. :rg has no type=."),
        l("  Active filters show in the list title; a new command without them clears them."),
        l(""),
        heading("Examples"),
        l("  :res all type=vm loc=westeurope     VMs in West Europe, every subscription"),
        l("  :res type=vnet                      VNets where you are"),
        l("  :res all type=keyvault tag=environment=prod"),
        l("  :res all tag=env=prod,env=production   either value"),
        l("  :rg all loc=northeurope             Resource groups in North Europe"),
        l("  :rg tag=owner                       Resource groups with an owner tag"),
        l("  :type all                           What exists, most common type first"),
        l("  :type all loc=westeurope            ...only in West Europe; Enter lists a type"),
        l("  :tag all                            Every tag in use; Enter lists its resources"),
        l("  :tag all type=keyvault              Tags on key vaults"),
        l(""),
        heading("Text filter"),
        l("  /text     in a list: narrow it by name, type (or alias) or location"),
        l("            in the resource detail: search, see Resource detail below"),
        l("            Enter keeps it, Esc clears it; combines with the filters above"),
        l(""),
        heading("Keys"),
        l("  j/k ↑/↓   move          g/G   first/last        Ctrl+d/u, PgDn/PgUp   page"),
        l("  Enter     open          Esc   back / clear filter / close popup"),
        l("  r         refresh       ?     this help         q, Ctrl+c   quit"),
        l("  y         copy the selected item's Azure ID"),
        l("  c         copy the current view (see below)"),
        l("  o         open the selected item in the Azure Portal"),
        l("            (no browser, e.g. WSL without wslview: the link is copied instead)"),
        l(""),
        heading("Copy the view (c)"),
        l("  Lists      every row that passes the filters, tab-separated with a header"),
        l("             (pastes into a spreadsheet), not only the rows on screen"),
        l("  Detail     the current tab: Overview, Tags or JSON"),
        l("  Tree, :type, :tag  their rows"),
        dim("  Copying goes through the terminal (OSC 52); in tmux enable set-clipboard."),
        l(""),
        heading("Management group tree"),
        l("  Space     open/close a group     l/→ open     h/← close (on a closed row: parent)"),
        l("  Enter     resource groups of the group (all its subscriptions) or subscription"),
        l(""),
        heading("Resource detail"),
        l("  Tab/l, Shift+Tab/h, or 1 2 3    Overview, Tags, JSON      j/k, g/G   scroll"),
        l("  /text     search the current tab, like Vim: jumps as you type, all matches"),
        l("            highlighted; Enter keeps it, Esc cancels. n / N next / previous"),
        l("            match (wraps around). Lowercase ignores case; any capital: exact case."),
        l("            Esc again clears the highlights."),
        l("  VNets list their subnets: prefixes, delegations, NSG, route table, NAT gateway"),
        l(""),
        heading("Status bar"),
        l("  ● Azure · 2m ago    fresh from Azure"),
        l("  ◐ Cached · 3h ago   from the local cache while Azure is asked"),
    ]
}

/// Render the help screen as a modal overlay, scrolled by `scroll` lines
/// (clamped and written back).
pub fn render_help(f: &mut Frame, area: Rect, scroll: &mut usize) {
    let modal_area = centered_rect(90, 90, area);
    f.render_widget(Clear, modal_area);

    let lines = help_lines();
    let viewport = modal_area.height.saturating_sub(2) as usize;
    *scroll = (*scroll).min(lines.len().saturating_sub(viewport));
    let help_paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left)
        .scroll((*scroll as u16, 0));

    f.render_widget(help_paragraph, modal_area);
}

/// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_documents_every_command_filter_and_key() {
        let text: String = help_lines().iter().map(|l| l.to_string() + "\n").collect();
        for needle in [
            ":mg",
            ":sub",
            ":rg",
            ":res",
            ":type",
            ":tag",
            "type=",
            "loc=",
            "tag=",
            "Examples",
            "Ctrl+P",
            "Tab completes",
            "  y ",
            "  c ",
            "  o ",
            "/text",
        ] {
            assert!(text.contains(needle), "help lacks {needle:?}");
        }
    }

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 100);
        let centered = centered_rect(80, 80, area);

        // Should be centered
        assert!(centered.x > 0);
        assert!(centered.y > 0);
        assert!(centered.width < area.width);
        assert!(centered.height < area.height);
    }

    #[test]
    fn test_centered_rect_full_size() {
        let area = Rect::new(0, 0, 100, 100);
        let centered = centered_rect(100, 100, area);

        // Should be same as original
        assert_eq!(centered.width, area.width);
        assert_eq!(centered.height, area.height);
    }
}
