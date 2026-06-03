# aztui Keyboard Shortcuts

Quick reference for all keyboard shortcuts in aztui.

## Global Shortcuts

These shortcuts work in all views:

| Key | Action | Description |
|-----|--------|-------------|
| `q` | Quit | Exit the application |
| `Ctrl+C` | Force Quit | Immediately exit (emergency) |
| `Ctrl+Q` | Quit | Alternative quit shortcut |
| `Esc` | Back/Close | Go back, close dialog, or dismiss error |
| `?` | Help | Show help screen (future) |
| `Ctrl+P` | Command Palette | Open command palette (future) |
| `/` | Filter | Start filtering (future) |
| `r` | Refresh | Refresh current view |

## Navigation Shortcuts

These shortcuts work in list views (Subscriptions, Resource Groups, Resources):

### Basic Navigation

| Key | Action | Description |
|-----|--------|-------------|
| `↓` or `j` | Down | Move selection down one item |
| `↑` or `k` | Up | Move selection up one item |
| `Enter` | Select | Select the highlighted item |

### Jump Navigation

| Key | Action | Description |
|-----|--------|-------------|
| `g` | First | Jump to first item in list |
| `G` | Last | Jump to last item in list |
| `PgDn` | Page Down | Move down 10 items |
| `PgUp` | Page Up | Move up 10 items |

## View-Specific Shortcuts

### Subscription List

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | Select Subscription | Navigate to resource groups |
| `r` | Refresh | Reload subscriptions from Azure |

### Resource Group List

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | Select Resource Group | Navigate to resources in group |
| `Esc` | Back | Return to subscription list |

### Resource List

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | View Details | Show resource details |
| `Esc` | Back | Return to previous view |

### Resource Detail

| Key | Action | Description |
|-----|--------|-------------|
| `Esc` | Back | Return to resource list |
| `j` | JSON View | Toggle JSON view (future) |
| `c` | Copy | Copy resource ID to clipboard (future) |

## Future Shortcuts

These shortcuts are planned for future iterations:

### Command Palette (Iteration 8)

| Key | Action | Description |
|-----|--------|-------------|
| `Ctrl+P` | Open Palette | Open command palette |
| `Esc` | Close Palette | Close command palette |
| `↑↓` | Navigate | Navigate through commands |
| `Enter` | Execute | Execute selected command |

### Search and Filter (Iteration 7)

| Key | Action | Description |
|-----|--------|-------------|
| `/` | Start Filter | Enter filter mode |
| `Esc` | Clear Filter | Clear current filter |
| `Ctrl+F` | Advanced Filter | Open filter dialog |

### Multi-Pane (Iteration 11)

| Key | Action | Description |
|-----|--------|-------------|
| `Ctrl+W` `h` | Focus Left | Move focus to left pane |
| `Ctrl+W` `l` | Focus Right | Move focus to right pane |
| `Ctrl+W` `j` | Focus Down | Move focus to pane below |
| `Ctrl+W` `k` | Focus Up | Move focus to pane above |
| `Ctrl+W` `s` | Split Horizontal | Split current pane horizontally |
| `Ctrl+W` `v` | Split Vertical | Split current pane vertically |
| `Ctrl+W` `q` | Close Pane | Close current pane |

### Resource Actions (Iteration 14)

| Key | Action | Description |
|-----|--------|-------------|
| `a` | Actions Menu | Open actions menu for resource |
| `s` | Start | Start resource (e.g., VM) |
| `S` | Stop | Stop resource (e.g., VM) |
| `d` | Delete | Delete resource (with confirmation) |
| `t` | Tags | Edit resource tags |

## Tips and Tricks

### Vim-Style Navigation

aztui uses vim-style navigation for efficiency:

- `j` and `k` for up/down (like vim)
- `g` and `G` for first/last (like vim)
- `Esc` to go back (like vim's normal mode)

### Quick Navigation

- Use `g` to jump to the top of a long list
- Use `G` to jump to the bottom
- Use `PgUp`/`PgDn` for faster scrolling through medium lists

### Error Handling

- Press `Esc` to dismiss error messages
- Errors don't block the UI - you can continue working
- Check the header for loading indicators (⟳)

### Efficiency Tips

1. **Learn the j/k keys**: Faster than reaching for arrow keys
2. **Use g/G for long lists**: Jump to ends instead of scrolling
3. **Press r to refresh**: Keep data up-to-date
4. **Use Esc liberally**: It's your "back" button

## Customization

Future versions will support custom keybindings through configuration files.

Example configuration (future):

```toml
[keybindings]
quit = "q"
refresh = "r"
navigate_down = ["j", "Down"]
navigate_up = ["k", "Up"]
select = "Enter"
```

## Accessibility

### Alternative Keys

Most actions have multiple key options:

- Navigation: Both arrow keys and vim keys (j/k)
- Quit: Both `q` and `Ctrl+Q`
- Back: `Esc` key

### Future Accessibility Features

- Mouse support for navigation
- Screen reader compatibility
- High contrast themes
- Configurable key repeat rates

## Platform-Specific Notes

### macOS

- `Ctrl` key works as expected
- `Cmd` key not currently used (may be added in future)

### Linux

- All shortcuts work in most terminal emulators
- Some terminals may intercept certain key combinations

### Windows

- Works in Windows Terminal, PowerShell, and CMD
- Some legacy terminals may have limited key support

## Troubleshooting

### Keys Not Working

1. **Check terminal emulator**: Some terminals intercept certain keys
2. **Try alternative keys**: Most actions have multiple key options
3. **Check for conflicts**: Disable terminal shortcuts that conflict

### Accidental Quit

- Use `Ctrl+C` only for emergency exit
- Regular quit with `q` is safer
- Future versions will add quit confirmation

## Learning Path

### Beginner

Start with these essential keys:
1. `↑↓` - Navigate
2. `Enter` - Select
3. `Esc` - Go back
4. `q` - Quit

### Intermediate

Add these for efficiency:
1. `j/k` - Vim-style navigation
2. `g/G` - Jump to first/last
3. `r` - Refresh

### Advanced

Master these for power use:
1. `Ctrl+P` - Command palette (future)
2. `/` - Quick filter (future)
3. Multi-pane shortcuts (future)

## Reference Card

Print-friendly quick reference:

```
┌─────────────────────────────────────────────────┐
│ aztui Keyboard Shortcuts Quick Reference        │
├─────────────────────────────────────────────────┤
│ GLOBAL                                          │
│   q          Quit                               │
│   Esc        Back/Close                         │
│   r          Refresh                            │
│   ?          Help                               │
│                                                 │
│ NAVIGATION                                      │
│   ↑↓ / jk    Up/Down                           │
│   g / G      First/Last                         │
│   PgUp/PgDn  Page Up/Down                       │
│   Enter      Select                             │
│                                                 │
│ FUTURE                                          │
│   Ctrl+P     Command Palette                    │
│   /          Filter                             │
└─────────────────────────────────────────────────┘
```

## Feedback

Have suggestions for keyboard shortcuts? Please open an issue on GitHub!
