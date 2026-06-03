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
| `?` | Help | Toggle help screen ✅ |
| `Ctrl+P` | Command Palette | Open command palette (future) |
| `/` | Filter | Enter filter mode ✅ |
| `r` | Refresh | Refresh current view |
| `Ctrl+R` | Force Refresh | Force refresh from Azure API |

## Navigation Shortcuts

These shortcuts work in list views (Subscriptions, Resource Groups, Resources):

### Basic Navigation

| Key | Action | Description |
|-----|--------|-------------|
| `↓` or `j` | Down | Move selection down one item |
| `↑` or `k` | Up | Move selection up one item |
| `Enter` | Select | Select the highlighted item |
| `Backspace` | Back | Go back to previous view |

### Jump Navigation

| Key | Action | Description |
|-----|--------|-------------|
| `g` | First | Jump to first item in list |
| `G` | Last | Jump to last item in list |
| `Ctrl+D` | Page Down | Move down 10 items |
| `Ctrl+U` | Page Up | Move up 10 items |
| `PgDn` | Page Down | Move down 10 items |
| `PgUp` | Page Up | Move up 10 items |

### Mouse Support

| Action | Description |
|--------|-------------|
| Scroll Up | Move selection up |
| Scroll Down | Move selection down |

## Filter Mode ✅

Filter mode allows you to quickly find items by typing:

| Key | Action | Description |
|-----|--------|-------------|
| `/` | Enter Filter | Start filter mode |
| `Type` | Filter | Type to filter list in real-time |
| `Enter` | Apply | Apply filter and exit filter mode |
| `Esc` | Cancel | Clear filter and exit filter mode |
| `Backspace` | Delete | Remove last character from filter |

**Filter Behavior:**
- Case-insensitive matching
- Searches across multiple fields (name, type, location)
- Real-time results as you type
- Filter persists after applying (until cleared with Esc)

## Help Screen ✅

The help screen provides comprehensive documentation:

| Key | Action | Description |
|-----|--------|-------------|
| `?` | Toggle Help | Open or close help screen |
| `Esc` | Close Help | Close help screen |
| `q` | Close Help | Close help screen |

**Help Content:**
- All keyboard shortcuts
- Navigation guide
- Filter mode instructions
- View hierarchy
- Data source information
- Tips and tricks

## View-Specific Shortcuts

### Subscription List

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | Select Subscription | Navigate to resources |
| `r` | Refresh | Reload subscriptions from Azure |
| `/` | Filter | Filter subscriptions by name or ID |

### Resource Group List

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | Select Resource Group | Navigate to resources in group |
| `Esc` | Back | Return to subscription list |
| `/` | Filter | Filter resource groups |

### Resource List

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | View Details | Show resource details |
| `Esc` | Back | Return to previous view |
| `/` | Filter | Filter resources by name, type, or location |

### Resource Detail

| Key | Action | Description |
|-----|--------|-------------|
| `Esc` | Back | Return to resource list |
| `j` | JSON View | Toggle JSON view (future) |
| `c` | Copy | Copy resource ID to clipboard (future) |

## Future Shortcuts

These shortcuts are planned for future iterations:

### Command Palette (Iteration 10)

| Key | Action | Description |
|-----|--------|-------------|
| `Ctrl+P` | Open Palette | Open command palette |
| `Esc` | Close Palette | Close command palette |
| `↑↓` | Navigate | Navigate through commands |
| `Enter` | Execute | Execute selected command |

### Multi-Pane (Iteration 15)

| Key | Action | Description |
|-----|--------|-------------|
| `Ctrl+W` `h` | Focus Left | Move focus to left pane |
| `Ctrl+W` `l` | Focus Right | Move focus to right pane |
| `Ctrl+W` `j` | Focus Down | Move focus to pane below |
| `Ctrl+W` `k` | Focus Up | Move focus to pane above |
| `Ctrl+W` `s` | Split Horizontal | Split current pane horizontally |
| `Ctrl+W` `v` | Split Vertical | Split current pane vertically |
| `Ctrl+W` `q` | Close Pane | Close current pane |

### Resource Actions (Iteration 13)

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
- `/` for search/filter (like vim)

### Quick Navigation

- Use `g` to jump to the top of a long list
- Use `G` to jump to the bottom
- Use `Ctrl+D`/`Ctrl+U` for faster scrolling through medium lists
- Use `/` to filter and find specific items quickly

### Filter Tips

1. **Quick Find**: Press `/`, type a few characters, press Enter
2. **Persistent Filter**: Filter stays active until you press Esc
3. **Multi-Field**: Filter searches name, type, and location
4. **Case-Insensitive**: No need to match case exactly

### Error Handling

- Press `Esc` to dismiss error messages
- Errors don't block the UI - you can continue working
- Check the status bar for data source (Azure vs Cache)

### Efficiency Tips

1. **Learn the j/k keys**: Faster than reaching for arrow keys
2. **Use g/G for long lists**: Jump to ends instead of scrolling
3. **Press r to refresh**: Keep data up-to-date
4. **Use Esc liberally**: It's your "back" button
5. **Use / for quick find**: Faster than scrolling through long lists
6. **Press ? when stuck**: Help is always available

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
filter = "/"
help = "?"
```

## Accessibility

### Alternative Keys

Most actions have multiple key options:

- Navigation: Both arrow keys and vim keys (j/k)
- Page navigation: Both Ctrl+D/U and PgDn/PgUp
- Quit: Both `q` and `Ctrl+Q`
- Back: Both `Esc` and `Backspace`

### Mouse Support

- Scroll wheel for navigation
- Future: Click to select items

### Future Accessibility Features

- Full mouse support (click to select)
- Screen reader compatibility
- High contrast themes
- Configurable key repeat rates

## Platform-Specific Notes

### macOS

- `Ctrl` key works as expected
- `Cmd` key not currently used (may be added in future)
- Works in Terminal.app, iTerm2, and other emulators

### Linux

- All shortcuts work in most terminal emulators
- Some terminals may intercept certain key combinations
- Tested on GNOME Terminal, Konsole, and Alacritty

### Windows

- Works in Windows Terminal, PowerShell, and CMD
- Some legacy terminals may have limited key support
- Windows Terminal recommended for best experience

## Troubleshooting

### Keys Not Working

1. **Check terminal emulator**: Some terminals intercept certain keys
2. **Try alternative keys**: Most actions have multiple key options
3. **Check for conflicts**: Disable terminal shortcuts that conflict
4. **Press ? for help**: Verify which keys should work

### Filter Not Working

1. **Press / to enter filter mode**: Look for "Filter:" in title
2. **Type to filter**: Results update in real-time
3. **Press Esc to clear**: Clears filter and exits filter mode

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
5. `?` - Help

### Intermediate

Add these for efficiency:
1. `j/k` - Vim-style navigation
2. `g/G` - Jump to first/last
3. `r` - Refresh
4. `/` - Filter

### Advanced

Master these for power use:
1. `Ctrl+D/U` - Page navigation
2. Filter mode workflow
3. `Ctrl+P` - Command palette (future)
4. Multi-pane shortcuts (future)

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
│   ?          Help                        ✅     │
│   /          Filter                      ✅     │
│                                                 │
│ NAVIGATION                                      │
│   ↑↓ / jk    Up/Down                           │
│   g / G      First/Last                         │
│   Ctrl+D/U   Page Up/Down                       │
│   Enter      Select                             │
│   Backspace  Back                               │
│                                                 │
│ FILTER MODE                              ✅     │
│   /          Enter filter mode                  │
│   Type       Filter in real-time                │
│   Enter      Apply filter                       │
│   Esc        Clear filter                       │
│   Backspace  Delete character                   │
│                                                 │
│ HELP SCREEN                              ✅     │
│   ?          Toggle help                        │
│   Esc/q      Close help                         │
│                                                 │
│ FUTURE                                          │
│   Ctrl+P     Command Palette                    │
└─────────────────────────────────────────────────┘
```

## Status Legend

- ✅ Implemented and working
- ⚠️ Partially implemented
- ❌ Not yet implemented
- 🔮 Planned for future

## Feedback

Have suggestions for keyboard shortcuts? Please open an issue on GitHub!
